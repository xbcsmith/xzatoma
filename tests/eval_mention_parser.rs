//! Eval tests for the mention parser.
//!
//! Loads scenario definitions from `evals/mention_parser/scenarios.yaml` and
//! exercises each scenario:
//!
//! - `parse_only` scenarios call `parse_mentions()` and assert mention count
//!   and mention types.
//! - `augment` scenarios create a temporary directory, copy fixture files,
//!   call `augment_prompt_with_mentions()`, and assert output content.
//!
//! Every scenario is deterministic and offline. URL mentions that would
//! require a live network connection are not included.

use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use tempfile::tempdir;
use xzatoma::mention_parser::{
    augment_prompt_with_mentions, parse_mentions, Mention, MentionCache,
};

// ---------------------------------------------------------------------------
// Scenario schema
// ---------------------------------------------------------------------------

/// Top-level container for the scenarios file.
#[derive(Debug, Deserialize)]
struct ScenarioFile {
    scenarios: Vec<Scenario>,
}

/// A single mention parser eval scenario.
#[derive(Debug, Deserialize)]
struct Scenario {
    /// Unique identifier used in test output.
    id: String,
    /// Human-readable description of what is being tested.
    description: String,
    /// Test mode: "parse_only" or "augment".
    test_mode: String,
    /// Scenario inputs.
    input: ScenarioInput,
    /// Assertions on the scenario outcome.
    expect: ScenarioExpect,
}

/// Inputs supplied to a scenario.
#[derive(Debug, Deserialize)]
struct ScenarioInput {
    /// The prompt text to parse or augment.
    prompt: String,
    /// Files to place in the temporary working directory before augmentation.
    ///
    /// Maps destination filename to source fixture name inside
    /// `evals/mention_parser/files/`. The special source value `"__binary__"`
    /// writes programmatic binary content (null bytes) instead of copying a
    /// fixture file.
    #[serde(default)]
    working_dir_files: BTreeMap<String, String>,
}

/// Assertions applied after running the scenario.
#[derive(Debug, Deserialize, Default)]
struct ScenarioExpect {
    /// Expected number of parsed mentions (parse_only scenarios).
    mention_count: Option<usize>,
    /// Expected ordered list of mention type names (parse_only scenarios).
    /// Valid values: "file", "search", "grep", "url".
    mention_types: Option<Vec<String>>,
    /// Substrings that must appear in the augmented output (augment scenarios).
    output_contains: Option<Vec<String>>,
    /// Substrings that must NOT appear in the augmented output (augment
    /// scenarios).
    output_not_contains: Option<Vec<String>>,
    /// When true the errors list returned by augmentation must be non-empty
    /// (augment scenarios).
    errors_nonempty: Option<bool>,
}

// ---------------------------------------------------------------------------
// Path helpers
// ---------------------------------------------------------------------------

/// Returns the absolute path to the `evals/mention_parser/` directory.
fn evals_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("evals")
        .join("mention_parser")
}

/// Returns the absolute path to the `evals/mention_parser/files/` directory.
fn fixtures_dir() -> PathBuf {
    evals_dir().join("files")
}

// ---------------------------------------------------------------------------
// Mention type helper
// ---------------------------------------------------------------------------

/// Returns a lowercase string label for a `Mention` variant.
fn mention_type_str(mention: &Mention) -> &'static str {
    match mention {
        Mention::File(_) => "file",
        Mention::Search(_) => "search",
        Mention::Grep(_) => "grep",
        Mention::Url(_) => "url",
    }
}

// ---------------------------------------------------------------------------
// Eval runner
// ---------------------------------------------------------------------------

#[tokio::test]
async fn eval_mention_parser_scenarios() {
    let scenarios_path = evals_dir().join("scenarios.yaml");
    let yaml = std::fs::read_to_string(&scenarios_path).unwrap_or_else(|e| {
        panic!(
            "Failed to read scenarios.yaml at {:?}: {}",
            scenarios_path, e
        )
    });

    let scenario_file: ScenarioFile = serde_yaml::from_str(&yaml)
        .unwrap_or_else(|e| panic!("Failed to parse scenarios.yaml: {}", e));

    assert!(
        !scenario_file.scenarios.is_empty(),
        "scenarios.yaml must contain at least one scenario"
    );

    let mut passed = 0usize;
    let mut failed = 0usize;
    let mut failures: Vec<String> = Vec::new();

    for scenario in &scenario_file.scenarios {
        let result = run_scenario(scenario).await;
        match result {
            Ok(()) => {
                passed += 1;
                println!("[PASS] {} -- {}", scenario.id, scenario.description);
            }
            Err(msg) => {
                failed += 1;
                let failure = format!(
                    "[FAIL] {} -- {}: {}",
                    scenario.id, scenario.description, msg
                );
                println!("{}", failure);
                failures.push(failure);
            }
        }
    }

    println!(
        "\nEval results: {} passed, {} failed out of {} scenarios",
        passed,
        failed,
        scenario_file.scenarios.len()
    );

    if !failures.is_empty() {
        panic!(
            "{} scenario(s) failed:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }
}

/// Dispatch a scenario to the appropriate runner based on `test_mode`.
async fn run_scenario(scenario: &Scenario) -> Result<(), String> {
    match scenario.test_mode.as_str() {
        "parse_only" => run_parse_only(scenario),
        "augment" => run_augment(scenario).await,
        other => Err(format!("unknown test_mode '{}' in scenarios.yaml", other)),
    }
}

/// Run a parse_only scenario.
///
/// Calls `parse_mentions` on the prompt and asserts the returned mention count
/// and type sequence against the expected values.
fn run_parse_only(scenario: &Scenario) -> Result<(), String> {
    let (mentions, _cleaned) = parse_mentions(&scenario.input.prompt)
        .map_err(|e| format!("parse_mentions failed unexpectedly: {}", e))?;

    if let Some(expected_count) = scenario.expect.mention_count {
        if mentions.len() != expected_count {
            return Err(format!(
                "expected {} mention(s) but got {}; mentions: {:?}",
                expected_count,
                mentions.len(),
                mentions.iter().map(mention_type_str).collect::<Vec<_>>()
            ));
        }
    }

    if let Some(ref expected_types) = scenario.expect.mention_types {
        let actual_types: Vec<&str> = mentions.iter().map(mention_type_str).collect();
        let expected_refs: Vec<&str> = expected_types.iter().map(String::as_str).collect();
        if actual_types != expected_refs {
            return Err(format!(
                "expected mention types {:?} but got {:?}",
                expected_refs, actual_types
            ));
        }
    }

    Ok(())
}

/// Run an augment scenario.
///
/// Creates a temporary directory, installs fixture files, calls
/// `augment_prompt_with_mentions`, and asserts output content and error state.
async fn run_augment(scenario: &Scenario) -> Result<(), String> {
    let temp_dir = tempdir().map_err(|e| format!("failed to create temp dir: {}", e))?;

    // Install working directory files before augmentation.
    for (dest, source) in &scenario.input.working_dir_files {
        let dest_path = temp_dir.path().join(dest);
        if source == "__binary__" {
            // Write programmatic binary content.  Null bytes are valid UTF-8
            // so read_to_string succeeds, but the binary-detection check in
            // load_file_content rejects the file with a FileBinary error.
            fs::write(&dest_path, b"test\x00binary")
                .map_err(|e| format!("failed to write binary fixture '{}': {}", dest, e))?;
        } else {
            let src_path = fixtures_dir().join(source);
            // Use fs::copy so binary content is preserved byte-for-byte.
            fs::copy(&src_path, &dest_path)
                .map_err(|e| format!("failed to copy fixture '{}' to '{}': {}", source, dest, e))?;
        }
    }

    // Parse mentions from the prompt so augment receives the correct list.
    let (mentions, _cleaned) = parse_mentions(&scenario.input.prompt)
        .map_err(|e| format!("parse_mentions failed: {}", e))?;

    // Augment the prompt with content resolved from the temporary directory.
    let mut cache = MentionCache::new();
    let (augmented, errors, _successes) = augment_prompt_with_mentions(
        &mentions,
        &scenario.input.prompt,
        temp_dir.path(),
        1024 * 1024,
        &mut cache,
    )
    .await;

    // Assert that required substrings are present.
    if let Some(ref expected_strings) = scenario.expect.output_contains {
        for expected in expected_strings {
            if !augmented.contains(expected.as_str()) {
                return Err(format!(
                    "expected output to contain {:?} but it did not.\nFull output:\n{}",
                    expected, augmented
                ));
            }
        }
    }

    // Assert that forbidden substrings are absent.
    if let Some(ref not_expected_strings) = scenario.expect.output_not_contains {
        for not_expected in not_expected_strings {
            if augmented.contains(not_expected.as_str()) {
                return Err(format!(
                    "expected output NOT to contain {:?} but it did.\nFull output:\n{}",
                    not_expected, augmented
                ));
            }
        }
    }

    // Assert that the errors list is non-empty when the scenario requires it.
    if let Some(true) = scenario.expect.errors_nonempty {
        if errors.is_empty() {
            return Err(format!(
                "expected errors to be non-empty but the list was empty.\nFull output:\n{}",
                augmented
            ));
        }
    }

    Ok(())
}

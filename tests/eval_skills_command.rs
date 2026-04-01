//! Eval tests for skills command operations.
//!
//! Loads scenario definitions from `evals/skills_command/scenarios.yaml` and
//! exercises each scenario by calling the appropriate skills command function
//! directly with a temporary directory structure.
//!
//! Every scenario is deterministic and offline -- no network calls, no provider
//! instantiation. Temporary directories are created and cleaned up automatically
//! after each scenario.

use serde::Deserialize;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::tempdir;
use xzatoma::cli::{SkillsTrustCommand, SkillsTrustPathArgs};
use xzatoma::commands::skills::{
    build_visible_skill_catalog, handle_trust, list_skills, show_paths, show_skill, validate_skills,
};
use xzatoma::config::{Config, SkillsConfig};
use xzatoma::error::{Result as XResult, XzatomaError};

// ---------------------------------------------------------------------------
// Scenario schema
// ---------------------------------------------------------------------------

/// Top-level scenario file structure.
#[derive(Debug, Deserialize)]
struct ScenarioFile {
    scenarios: Vec<Scenario>,
}

/// A single skills command eval scenario.
#[derive(Debug, Deserialize)]
struct Scenario {
    /// Unique identifier used in test output.
    id: String,
    /// Human-readable description of what is being tested.
    description: String,
    /// Command to execute: build_catalog, list, validate, show, paths,
    /// trust_show, trust_add, trust_remove, or trust_round_trip.
    command: String,
    /// Optional setup configuration for this scenario.
    #[serde(default)]
    setup: ScenarioSetup,
    /// Optional inputs for commands that require them.
    #[serde(default)]
    input: ScenarioInput,
    /// Assertions on the scenario outcome.
    expect: ScenarioExpect,
}

/// Setup configuration applied before running a scenario.
#[derive(Debug, Deserialize)]
struct ScenarioSetup {
    /// Global skills feature flag.
    #[serde(default = "default_true")]
    skills_enabled: bool,
    /// Skip trust checks for additional_paths discovery roots.
    #[serde(default = "default_true")]
    allow_custom_paths_without_trust: bool,
    /// Skill fixtures to install as `<skills_root>/<dir>/SKILL.md`.
    #[serde(default)]
    fixtures: Vec<SkillFixture>,
    /// Extra files to place verbatim under the skills root.
    #[serde(default)]
    extra_files: Vec<ExtraFile>,
}

impl Default for ScenarioSetup {
    fn default() -> Self {
        Self {
            skills_enabled: true,
            allow_custom_paths_without_trust: true,
            fixtures: Vec::new(),
            extra_files: Vec::new(),
        }
    }
}

fn default_true() -> bool {
    true
}

/// A skill fixture to be installed into the temporary skills root.
#[derive(Debug, Deserialize)]
struct SkillFixture {
    /// Filename inside `evals/skills_command/skills/`.
    fixture: String,
    /// Subdirectory name under `skills_root` where the file is written as `SKILL.md`.
    dir: String,
}

/// An extra (non-SKILL.md) file to place in the temporary skills root.
#[derive(Debug, Deserialize)]
struct ExtraFile {
    /// Filename inside `evals/skills_command/skills/`.
    file: String,
    /// Destination path relative to `skills_root`.
    dest: String,
}

/// Optional command inputs.
#[derive(Debug, Deserialize, Default)]
struct ScenarioInput {
    /// Skill name for the `show` command.
    skill_name: Option<String>,
    /// Path relative to the scenario temp dir for trust operations.
    trust_path: Option<String>,
}

/// Assertions applied after running the scenario.
#[derive(Debug, Deserialize)]
struct ScenarioExpect {
    /// Expected outcome: "ok" or "error".
    outcome: String,
    /// Optional substring that must appear in the error message.
    error_contains: Option<String>,
    /// Optional expected catalog entry count (build_catalog command only).
    catalog_size: Option<usize>,
}

// ---------------------------------------------------------------------------
// Path helpers
// ---------------------------------------------------------------------------

/// Returns the absolute path to the `evals/skills_command/` directory.
fn evals_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("evals")
        .join("skills_command")
}

/// Returns the absolute path to the `evals/skills_command/skills/` directory.
fn fixtures_dir() -> PathBuf {
    evals_dir().join("skills")
}

// ---------------------------------------------------------------------------
// Config builder
// ---------------------------------------------------------------------------

/// Build a `Config` for a scenario using the provided skills root.
///
/// Project and user discovery are disabled. Only the additional_paths root is
/// used so results are fully deterministic and independent of HOME or the real
/// working directory.
fn build_scenario_config(setup: &ScenarioSetup, skills_root: &Path, temp_dir: &Path) -> Config {
    let trust_store_path = temp_dir.join("skills_trust.yaml");
    Config {
        skills: SkillsConfig {
            enabled: setup.skills_enabled,
            project_enabled: false,
            user_enabled: false,
            additional_paths: vec![skills_root.to_string_lossy().into_owned()],
            allow_custom_paths_without_trust: setup.allow_custom_paths_without_trust,
            trust_store_path: Some(trust_store_path.to_string_lossy().into_owned()),
            ..SkillsConfig::default()
        },
        ..Config::default()
    }
}

// ---------------------------------------------------------------------------
// Eval runner
// ---------------------------------------------------------------------------

#[test]
fn eval_skills_command_scenarios() {
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
        let result = run_scenario(scenario);
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

/// Run a single scenario and return `Ok(())` on pass or `Err(message)` on failure.
fn run_scenario(scenario: &Scenario) -> Result<(), String> {
    let temp_dir = tempdir().map_err(|e| format!("failed to create temp dir: {}", e))?;
    let skills_root = temp_dir.path().join("skills_root");
    fs::create_dir_all(&skills_root).map_err(|e| format!("failed to create skills_root: {}", e))?;

    // Install skill fixture files as <skills_root>/<dir>/SKILL.md
    for fixture in &scenario.setup.fixtures {
        let fixture_path = fixtures_dir().join(&fixture.fixture);
        let skill_dir = skills_root.join(&fixture.dir);
        fs::create_dir_all(&skill_dir)
            .map_err(|e| format!("failed to create skill dir '{}': {}", fixture.dir, e))?;
        let content = fs::read_to_string(&fixture_path)
            .map_err(|e| format!("failed to read fixture '{}': {}", fixture.fixture, e))?;
        fs::write(skill_dir.join("SKILL.md"), &content)
            .map_err(|e| format!("failed to write SKILL.md for '{}': {}", fixture.dir, e))?;
    }

    // Install extra (non-SKILL.md) files verbatim
    for extra in &scenario.setup.extra_files {
        let file_path = fixtures_dir().join(&extra.file);
        let dest_path = skills_root.join(&extra.dest);
        if let Some(parent) = dest_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("failed to create parent dir for '{}': {}", extra.dest, e))?;
        }
        let content = fs::read_to_string(&file_path)
            .map_err(|e| format!("failed to read extra file '{}': {}", extra.file, e))?;
        fs::write(&dest_path, &content)
            .map_err(|e| format!("failed to write extra file '{}': {}", extra.dest, e))?;
    }

    let config = build_scenario_config(&scenario.setup, &skills_root, temp_dir.path());
    let result = run_command(scenario, &config, temp_dir.path());
    check_result(result, &scenario.expect)
}

/// Execute the command for a scenario and return the library result.
fn run_command(scenario: &Scenario, config: &Config, temp_dir: &Path) -> XResult<()> {
    match scenario.command.as_str() {
        "build_catalog" => run_build_catalog(scenario, config),

        "list" => list_skills(config.clone()),

        "validate" => validate_skills(config.clone()),

        "show" => {
            let name = scenario.input.skill_name.as_deref().ok_or_else(|| {
                XzatomaError::Config("show command requires input.skill_name".to_string())
            })?;
            show_skill(config.clone(), name)
        }

        "paths" => show_paths(config.clone()),

        "trust_show" => handle_trust(SkillsTrustCommand::Show, config.clone()),

        "trust_add" => {
            let rel_path = require_trust_path(scenario)?;
            let path = temp_dir.join(rel_path);
            fs::create_dir_all(&path).map_err(XzatomaError::Io)?;
            handle_trust(
                SkillsTrustCommand::Add(SkillsTrustPathArgs { path }),
                config.clone(),
            )
        }

        "trust_remove" => {
            let rel_path = require_trust_path(scenario)?;
            let path = temp_dir.join(rel_path);
            handle_trust(
                SkillsTrustCommand::Remove(SkillsTrustPathArgs { path }),
                config.clone(),
            )
        }

        "trust_round_trip" => {
            let rel_path = require_trust_path(scenario)?;
            let path = temp_dir.join(rel_path);
            fs::create_dir_all(&path).map_err(XzatomaError::Io)?;
            handle_trust(
                SkillsTrustCommand::Add(SkillsTrustPathArgs { path: path.clone() }),
                config.clone(),
            )?;
            handle_trust(
                SkillsTrustCommand::Remove(SkillsTrustPathArgs { path }),
                config.clone(),
            )
        }

        other => Err(XzatomaError::Config(format!(
            "unknown command '{}' in scenarios.yaml",
            other
        ))),
    }
}

/// Run the `build_catalog` command and assert the catalog size when specified.
fn run_build_catalog(scenario: &Scenario, config: &Config) -> XResult<()> {
    let working_dir = std::env::current_dir().map_err(|e| XzatomaError::Config(e.to_string()))?;
    let trusted_paths: BTreeSet<PathBuf> = BTreeSet::new();
    let catalog = build_visible_skill_catalog(config, &working_dir, &trusted_paths)?;

    if let Some(expected_size) = scenario.expect.catalog_size {
        if catalog.len() != expected_size {
            return Err(XzatomaError::Config(format!(
                "expected catalog size {} but got {}",
                expected_size,
                catalog.len()
            )));
        }
    }

    Ok(())
}

/// Extract the trust_path input field or return a configuration error.
fn require_trust_path(scenario: &Scenario) -> XResult<&str> {
    scenario.input.trust_path.as_deref().ok_or_else(|| {
        XzatomaError::Config(format!(
            "'{}' command requires input.trust_path",
            scenario.command
        ))
    })
}

/// Assert that a library result matches the expected outcome and error substring.
fn check_result(result: XResult<()>, expect: &ScenarioExpect) -> Result<(), String> {
    match expect.outcome.as_str() {
        "ok" => match result {
            Ok(()) => Ok(()),
            Err(e) => Err(format!("expected Ok but got error: {}", e)),
        },

        "error" => match result {
            Err(e) => {
                if let Some(ref expected_substr) = expect.error_contains {
                    let err_str = e.to_string();
                    if err_str.contains(expected_substr.as_str()) {
                        Ok(())
                    } else {
                        Err(format!(
                            "expected error containing {:?} but got: {}",
                            expected_substr, err_str
                        ))
                    }
                } else {
                    // Any error is acceptable when no substring is required.
                    Ok(())
                }
            }
            Ok(()) => Err("expected an error but the call succeeded".to_string()),
        },

        other => Err(format!(
            "unknown expected outcome {:?} in scenarios.yaml",
            other
        )),
    }
}

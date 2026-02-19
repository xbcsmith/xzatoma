//! Eval tests for the `xzatoma run` command.
//!
//! Loads scenario definitions from `evals/run_command/scenarios.yaml` and
//! exercises each scenario at the appropriate layer:
//!
//! - Default scenarios call `commands::run::run_plan_with_options` with an
//!   intentionally invalid provider — deterministic and offline.
//! - Scenarios with `test_mode: parse_only` call `tools::plan::PlanParser`
//!   directly, testing plan parsing and validation independent of the provider.

use serde::Deserialize;
use std::path::PathBuf;
use xzatoma::commands::r#run::run_plan_with_options;
use xzatoma::config::Config;
use xzatoma::tools::plan::PlanParser;

// ---------------------------------------------------------------------------
// Scenario schema
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct ScenarioFile {
    scenarios: Vec<Scenario>,
}

#[derive(Debug, Deserialize)]
struct Scenario {
    id: String,
    description: String,
    /// Optional test mode. "parse_only" calls PlanParser directly.
    #[serde(default)]
    test_mode: Option<String>,
    input: ScenarioInput,
    expect: ScenarioExpect,
}

#[derive(Debug, Deserialize, Default)]
struct ScenarioInput {
    /// Path to a plan file, relative to `evals/run_command/plans/`
    plan_file: Option<String>,
    /// Direct prompt text
    prompt: Option<String>,
    /// Whether to pass `allow_dangerous = true`
    #[serde(default)]
    allow_dangerous: bool,
}

#[derive(Debug, Deserialize)]
struct ScenarioExpect {
    /// "error" or "ok"
    outcome: String,
    /// Optional substring that must appear in the error message
    error_contains: Option<String>,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Returns the absolute path to the `evals/run_command/` directory.
fn evals_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("evals")
        .join("run_command")
}

/// Returns the absolute path to the `evals/run_command/plans/` directory.
fn plans_dir() -> PathBuf {
    evals_dir().join("plans")
}

/// Build a `Config` that uses an invalid provider so execution is offline.
fn offline_config() -> Config {
    let mut cfg = Config::default();
    cfg.provider.provider_type = "invalid_provider".to_string();
    cfg
}

// ---------------------------------------------------------------------------
// Eval runner
// ---------------------------------------------------------------------------

#[tokio::test]
async fn eval_run_command_scenarios() {
    // Load scenarios.yaml
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
                println!("[PASS] {} — {}", scenario.id, scenario.description);
            }
            Err(msg) => {
                failed += 1;
                let failure = format!("[FAIL] {} — {}: {}", scenario.id, scenario.description, msg);
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

/// Run a single scenario and return Ok(()) on pass or Err(message) on failure.
async fn run_scenario(scenario: &Scenario) -> Result<(), String> {
    match scenario.test_mode.as_deref() {
        Some("parse_only") => run_parse_only_scenario(scenario),
        _ => run_full_scenario(scenario).await,
    }
}

/// Scenarios that test plan parsing/validation directly via `PlanParser`.
///
/// These bypass provider instantiation, which happens after plan parsing in
/// the actual `run_plan_with_options` call chain.
fn run_parse_only_scenario(scenario: &Scenario) -> Result<(), String> {
    let plan_file = scenario
        .input
        .plan_file
        .as_ref()
        .ok_or_else(|| "parse_only scenario must specify a plan_file".to_string())?;

    let plan_path = plans_dir().join(plan_file);
    let result = PlanParser::from_file(&plan_path);

    check_result(result.map(|_| ()), &scenario.expect)
}

/// Scenarios that exercise the full `run_plan_with_options` path.
///
/// Uses an invalid provider so execution is deterministic and offline.
async fn run_full_scenario(scenario: &Scenario) -> Result<(), String> {
    let cfg = offline_config();

    let plan_path: Option<String> = match &scenario.input.plan_file {
        Some(rel) => {
            let abs = plans_dir().join(rel);
            Some(abs.to_string_lossy().into_owned())
        }
        None => None,
    };

    let prompt = scenario.input.prompt.clone();
    let allow_dangerous = scenario.input.allow_dangerous;

    let result = run_plan_with_options(cfg, plan_path, prompt, allow_dangerous).await;
    check_result(result, &scenario.expect)
}

/// Assert that a result matches the expected outcome and error substring.
fn check_result(result: anyhow::Result<()>, expect: &ScenarioExpect) -> Result<(), String> {
    match expect.outcome.as_str() {
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
                    Ok(())
                }
            }
            Ok(()) => Err("expected an error but the call succeeded".to_string()),
        },
        "ok" => match result {
            Ok(()) => Ok(()),
            Err(e) => Err(format!("expected Ok but got error: {}", e)),
        },
        other => Err(format!(
            "unknown expected outcome {:?} in scenarios.yaml",
            other
        )),
    }
}

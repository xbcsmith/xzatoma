//! Eval tests for configuration validation.
//!
//! Loads scenario definitions from `evals/config_validation/scenarios.yaml` and
//! exercises `Config::validate()` against each fixture config file.
//!
//! Every scenario is deterministic and offline -- no network calls, no provider
//! instantiation, just pure config parsing and validation logic.

use serde::Deserialize;
use std::path::PathBuf;
use xzatoma::config::Config;

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
    /// Path to a config fixture file, relative to `configs/`
    config_file: String,
    expect: ScenarioExpect,
}

#[derive(Debug, Deserialize)]
struct ScenarioExpect {
    /// "valid" or "invalid"
    outcome: String,
    /// Optional substring that must appear in the error message
    error_contains: Option<String>,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Returns the absolute path to the `evals/config_validation/` directory.
fn evals_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("evals")
        .join("config_validation")
}

/// Returns the absolute path to the `evals/config_validation/configs/` directory.
fn configs_dir() -> PathBuf {
    evals_dir().join("configs")
}

// ---------------------------------------------------------------------------
// Eval runner
// ---------------------------------------------------------------------------

#[test]
fn eval_config_validation_scenarios() {
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

/// Run a single config validation scenario.
///
/// 1. Read the fixture YAML file.
/// 2. Deserialize it into `Config`.
/// 3. Call `config.validate()`.
/// 4. Assert the outcome matches expectations.
fn run_scenario(scenario: &Scenario) -> Result<(), String> {
    let config_path = configs_dir().join(&scenario.config_file);
    let yaml = std::fs::read_to_string(&config_path)
        .map_err(|e| format!("failed to read config fixture {:?}: {}", config_path, e))?;

    // Attempt to deserialize the config
    let config: Config = match serde_yaml::from_str(&yaml) {
        Ok(c) => c,
        Err(e) => {
            // Deserialization failure counts as invalid
            return check_invalid_result(Err(e.to_string()), &scenario.expect, "deserialization");
        }
    };

    // Run validation
    let validation_result = config.validate();

    match scenario.expect.outcome.as_str() {
        "valid" => match validation_result {
            Ok(()) => Ok(()),
            Err(e) => Err(format!("expected valid config but got error: {}", e)),
        },
        "invalid" => {
            let result = validation_result.map_err(|e| e.to_string());
            check_invalid_result(result, &scenario.expect, "validation")
        }
        other => Err(format!(
            "unknown expected outcome {:?} in scenarios.yaml",
            other
        )),
    }
}

/// Assert that an invalid result matches expectations.
///
/// `stage` is used in error messages to indicate whether the failure
/// came from deserialization or validation.
fn check_invalid_result(
    result: Result<(), String>,
    expect: &ScenarioExpect,
    stage: &str,
) -> Result<(), String> {
    if expect.outcome != "invalid" {
        if let Err(e) = result {
            return Err(format!("expected valid config but {} failed: {}", stage, e));
        }
        return Ok(());
    }

    match result {
        Err(err_str) => {
            if let Some(ref expected_substr) = expect.error_contains {
                if err_str.contains(expected_substr.as_str()) {
                    Ok(())
                } else {
                    Err(format!(
                        "expected {} error containing {:?} but got: {}",
                        stage, expected_substr, err_str
                    ))
                }
            } else {
                // Any error is acceptable
                Ok(())
            }
        }
        Ok(()) => Err(format!(
            "expected an invalid config but {} succeeded",
            stage
        )),
    }
}

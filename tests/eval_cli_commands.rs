//! Eval tests for CLI command argument parsing and basic command dispatch.
//!
//! Loads scenario definitions from `evals/cli_commands/scenarios.yaml` and for
//! each scenario invokes the compiled `xzatoma` binary via `assert_cmd`,
//! asserting the exit status and optional output substrings.
//!
//! The harness distinguishes two classes of scenario:
//!
//! - **Help / clap-only** (`needs_config: false`): clap handles `--help`,
//!   `--version`, and missing-subcommand errors before any config loading.
//!   These scenarios receive no `--config` argument.
//!
//! - **Execution** (`needs_config: true`): commands that must pass
//!   `Config::load` and `Config::validate` before dispatch. The harness writes
//!   a minimal config file to a per-scenario temp directory and prepends
//!   `--config <path>` to the argument list. `XZATOMA_HISTORY_DB` and
//!   `XZATOMA_SKILLS_TRUST_STORE_PATH` are also pointed at isolated temp paths
//!   so every scenario is fully hermetic.
//!
//! The special token `__TEMP_DIR__` anywhere in `args` is replaced at runtime
//! with the absolute path of the per-scenario temp directory, enabling
//! subcommands like `replay --db-path __TEMP_DIR__/replay.db`.

use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

mod common;

// ---------------------------------------------------------------------------
// Scenario schema
// ---------------------------------------------------------------------------

/// Top-level structure of `evals/cli_commands/scenarios.yaml`.
#[derive(Debug, Deserialize)]
struct ScenarioFile {
    scenarios: Vec<Scenario>,
}

/// A single CLI eval scenario.
#[derive(Debug, Deserialize)]
struct Scenario {
    /// Unique identifier used in test output.
    id: String,
    /// Human-readable description of what is being tested.
    description: String,
    /// CLI arguments to pass to the binary (after any harness-injected flags).
    args: Vec<String>,
    /// Optional environment variable overrides applied to the subprocess.
    #[serde(default)]
    env: HashMap<String, String>,
    /// When true the harness provides a minimal `--config` file and sets
    /// `XZATOMA_HISTORY_DB` / `XZATOMA_SKILLS_TRUST_STORE_PATH` to isolated
    /// temp paths.  When false the scenario is expected to be handled entirely
    /// by clap before any config loading takes place.
    #[serde(default)]
    needs_config: bool,
    /// Assertions applied after the process exits.
    expect: ScenarioExpect,
}

/// Assertions on the process result.
#[derive(Debug, Deserialize)]
struct ScenarioExpect {
    /// Whether the process must exit 0 (`true`) or non-zero (`false`).
    success: bool,
    /// Optional substring that must appear somewhere in stdout.
    stdout_contains: Option<String>,
    /// Optional substring that must appear somewhere in stderr.
    stderr_contains: Option<String>,
}

// ---------------------------------------------------------------------------
// Path helpers
// ---------------------------------------------------------------------------

/// Returns the absolute path to `evals/cli_commands/`.
fn evals_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("evals")
        .join("cli_commands")
}

// ---------------------------------------------------------------------------
// Minimal config file
// ---------------------------------------------------------------------------

/// Returns a minimal valid `config.yaml` content suitable for execution
/// scenarios that must pass `Config::load` and `Config::validate`.
///
/// Skills project and user discovery are disabled so the binary does not scan
/// the real project directory or the developer's home directory during tests.
fn minimal_config_yaml() -> String {
    r#"provider:
  type: ollama
  ollama:
    host: "http://localhost:11434"
    model: "llama3.2:latest"
agent:
  max_turns: 50
  timeout_seconds: 300
skills:
  enabled: true
  project_enabled: false
  user_enabled: false
  additional_paths: []
"#
    .to_string()
}

// ---------------------------------------------------------------------------
// Main test entry point
// ---------------------------------------------------------------------------

#[test]
fn eval_cli_commands_scenarios() {
    let scenarios_path = evals_dir().join("scenarios.yaml");
    let yaml = fs::read_to_string(&scenarios_path).unwrap_or_else(|e| {
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

// ---------------------------------------------------------------------------
// Per-scenario runner
// ---------------------------------------------------------------------------

/// Run a single scenario and return `Ok(())` on pass or `Err(message)` on
/// failure.
fn run_scenario(scenario: &Scenario) -> Result<(), String> {
    let temp_dir = TempDir::new().map_err(|e| format!("failed to create temp dir: {}", e))?;
    let temp_path = temp_dir.path();
    let temp_path_str = temp_path.to_string_lossy().into_owned();

    // Build the final argument list: harness-injected flags come first so they
    // appear before the subcommand name, which is where clap expects them.
    let mut final_args: Vec<String> = Vec::new();
    let mut extra_env: Vec<(String, String)> = Vec::new();

    if scenario.needs_config {
        // Write minimal config to temp dir.
        let config_path = temp_path.join("config.yaml");
        fs::write(&config_path, minimal_config_yaml())
            .map_err(|e| format!("failed to write config file: {}", e))?;

        final_args.push("--config".to_string());
        final_args.push(config_path.to_string_lossy().into_owned());

        // Provide an isolated history database so history commands do not
        // touch the developer's real storage.
        let history_db = temp_path.join("history.db");
        extra_env.push((
            "XZATOMA_HISTORY_DB".to_string(),
            history_db.to_string_lossy().into_owned(),
        ));

        // Provide an isolated skills trust store so skills commands do not
        // read from or write to ~/.xzatoma/.
        let trust_store = temp_path.join("skills_trust.yaml");
        extra_env.push((
            "XZATOMA_SKILLS_TRUST_STORE_PATH".to_string(),
            trust_store.to_string_lossy().into_owned(),
        ));
    }

    // Append scenario args, substituting __TEMP_DIR__ where present.
    for arg in &scenario.args {
        final_args.push(arg.replace("__TEMP_DIR__", &temp_path_str));
    }

    // Build the assert_cmd Command.
    let mut cmd =
        common::xzatoma_command().map_err(|e| format!("failed to locate binary: {}", e))?;

    cmd.args(&final_args);

    // Scenario-defined environment overrides.
    for (k, v) in &scenario.env {
        cmd.env(k, v);
    }

    // Harness-injected environment overrides.
    for (k, v) in &extra_env {
        cmd.env(k, v);
    }

    // Execute and collect output.
    let output = cmd
        .output()
        .map_err(|e| format!("failed to execute binary: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

    check_output(&output.status, &stdout, &stderr, &scenario.expect)
}

// ---------------------------------------------------------------------------
// Result checker
// ---------------------------------------------------------------------------

/// Assert that a process result matches the scenario expectations.
fn check_output(
    status: &std::process::ExitStatus,
    stdout: &str,
    stderr: &str,
    expect: &ScenarioExpect,
) -> Result<(), String> {
    // Exit status check.
    if expect.success && !status.success() {
        return Err(format!(
            "expected exit 0 but got {:?}; stderr: {}",
            status.code(),
            stderr.trim()
        ));
    }
    if !expect.success && status.success() {
        return Err(format!(
            "expected non-zero exit but got 0; stdout: {}",
            stdout.trim()
        ));
    }

    // stdout substring check.
    if let Some(ref needle) = expect.stdout_contains {
        if !stdout.contains(needle.as_str()) {
            return Err(format!(
                "expected stdout to contain {:?} but got:\n{}",
                needle,
                stdout.trim()
            ));
        }
    }

    // stderr substring check.
    if let Some(ref needle) = expect.stderr_contains {
        if !stderr.contains(needle.as_str()) {
            return Err(format!(
                "expected stderr to contain {:?} but got:\n{}",
                needle,
                stderr.trim()
            ));
        }
    }

    Ok(())
}

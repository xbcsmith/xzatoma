#![allow(deprecated)]

use assert_cmd::Command;
use predicates::prelude::*;
use serial_test::serial;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

mod common;

struct HomeEnvGuard {
    original_home: Option<String>,
}

impl HomeEnvGuard {
    fn set(temp_home: &Path) -> Self {
        let original_home = std::env::var("HOME").ok();
        // SAFETY: Test-only environment mutation scoped by guard lifetime.
        unsafe {
            std::env::set_var("HOME", temp_home);
        }
        Self { original_home }
    }
}

impl Drop for HomeEnvGuard {
    fn drop(&mut self) {
        match &self.original_home {
            Some(value) => {
                // SAFETY: Test-only environment mutation scoped by guard lifetime.
                unsafe {
                    std::env::set_var("HOME", value);
                }
            }
            None => {
                // SAFETY: Test-only environment mutation scoped by guard lifetime.
                unsafe {
                    std::env::remove_var("HOME");
                }
            }
        }
    }
}

fn write_skill(
    root: &Path,
    relative_dir: &str,
    name: &str,
    description: &str,
    body: &str,
) -> PathBuf {
    let skill_dir = root.join(relative_dir);
    fs::create_dir_all(&skill_dir).expect("skill directory should be created");

    let skill_file = skill_dir.join("SKILL.md");
    let contents = format!(
        "---\nname: {name}\ndescription: {description}\nallowed-tools: read_file, grep\n---\n{body}\n"
    );

    fs::write(&skill_file, contents).expect("skill file should be written");
    skill_file
}

fn write_invalid_skill(root: &Path, relative_dir: &str, contents: &str) -> PathBuf {
    let skill_dir = root.join(relative_dir);
    fs::create_dir_all(&skill_dir).expect("skill directory should be created");

    let skill_file = skill_dir.join("SKILL.md");
    fs::write(&skill_file, contents).expect("invalid skill file should be written");
    skill_file
}

fn default_config_yaml(trust_store_path: &Path) -> String {
    format!(
        r#"provider:
  type: ollama

agent:
  max_turns: 50
  timeout_seconds: 300

skills:
  enabled: true
  project_enabled: true
  user_enabled: true
  additional_paths: []
  max_discovered_skills: 256
  max_scan_directories: 2000
  max_scan_depth: 6
  catalog_max_entries: 128
  activation_tool_enabled: true
  project_trust_required: true
  trust_store_path: "{}"
  allow_custom_paths_without_trust: false
  strict_frontmatter: true
"#,
        trust_store_path.display()
    )
}

fn config_with_additional_paths(
    trust_store_path: &Path,
    additional_paths: &[&Path],
    allow_custom_paths_without_trust: bool,
) -> String {
    let additional_paths_yaml = if additional_paths.is_empty() {
        "[]".to_string()
    } else {
        let entries = additional_paths
            .iter()
            .map(|path| format!("\n    - {}", path.display()))
            .collect::<String>();
        entries
    };

    format!(
        r#"provider:
  type: ollama

agent:
  max_turns: 50
  timeout_seconds: 300

skills:
  enabled: true
  project_enabled: true
  user_enabled: true
  additional_paths:{additional_paths_yaml}
  max_discovered_skills: 256
  max_scan_directories: 2000
  max_scan_depth: 6
  catalog_max_entries: 128
  activation_tool_enabled: true
  project_trust_required: true
  trust_store_path: "{}"
  allow_custom_paths_without_trust: {}
  strict_frontmatter: true
"#,
        trust_store_path.display(),
        allow_custom_paths_without_trust,
    )
}

fn trust_store_contents(path: &Path) -> String {
    fs::read_to_string(path).expect("trust store should be readable")
}

#[test]
#[serial]
fn test_skills_list_output_shows_only_valid_visible_skills() {
    let project_dir = TempDir::new().expect("project temp dir should exist");
    let home_dir = TempDir::new().expect("home temp dir should exist");
    let _home_guard = HomeEnvGuard::set(home_dir.path());

    write_skill(
        &project_dir.path().join(".xzatoma").join("skills"),
        "visible_project_skill",
        "visible_project_skill",
        "Visible project skill",
        "# Visible project skill",
    );
    write_skill(
        &home_dir.path().join(".xzatoma").join("skills"),
        "visible_user_skill",
        "visible_user_skill",
        "Visible user skill",
        "# Visible user skill",
    );
    write_invalid_skill(
        &project_dir.path().join(".xzatoma").join("skills"),
        "invalid_skill",
        "---\nname: Invalid-Skill\ndescription: Invalid skill\n---\n# Invalid\n",
    );

    let trust_store_path = project_dir.path().join("skills_trust.yaml");
    let config_yaml = default_config_yaml(&trust_store_path);
    let (_config_dir, config_path) = common::temp_config_file(&config_yaml);

    let mut cmd = Command::cargo_bin("xzatoma").expect("binary should build");
    cmd.current_dir(project_dir.path())
        .arg("--config")
        .arg(&config_path)
        .arg("skills")
        .arg("list");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("visible_user_skill"))
        .stdout(predicate::str::contains("Visible user skill"))
        .stdout(predicate::str::contains("visible_project_skill").not())
        .stdout(predicate::str::contains("invalid_skill").not())
        .stdout(predicate::str::contains("Invalid-Skill").not());
}

#[test]
#[serial]
fn test_skills_validate_output_shows_valid_invalid_and_shadowed_diagnostics() {
    let project_dir = TempDir::new().expect("project temp dir should exist");
    let home_dir = TempDir::new().expect("home temp dir should exist");
    let _home_guard = HomeEnvGuard::set(home_dir.path());

    write_skill(
        &project_dir.path().join(".xzatoma").join("skills"),
        "shared_skill",
        "shared_skill",
        "Project version",
        "# Project version",
    );
    write_skill(
        &home_dir.path().join(".xzatoma").join("skills"),
        "shared_skill",
        "shared_skill",
        "User version",
        "# User version",
    );
    write_skill(
        &home_dir.path().join(".xzatoma").join("skills"),
        "visible_user_skill",
        "visible_user_skill",
        "Visible user skill",
        "# User visible skill",
    );
    write_invalid_skill(
        &project_dir.path().join(".xzatoma").join("skills"),
        "invalid_skill",
        "---\nname: Invalid-Skill\ndescription: Invalid skill\n---\n# Invalid\n",
    );

    let trust_store_path = project_dir.path().join("skills_trust.yaml");
    let config_yaml = default_config_yaml(&trust_store_path);
    let (_config_dir, config_path) = common::temp_config_file(&config_yaml);

    let mut cmd = Command::cargo_bin("xzatoma").expect("binary should build");
    cmd.current_dir(project_dir.path())
        .arg("--config")
        .arg(&config_path)
        .arg("skills")
        .arg("validate");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Valid visible skills:"))
        .stdout(predicate::str::contains("visible_user_skill"))
        .stdout(predicate::str::contains("Invalid skill diagnostics:"))
        .stdout(predicate::str::contains("[invalid_name]"))
        .stdout(predicate::str::contains("must match"))
        .stdout(predicate::str::contains("Shadowed skill diagnostics:"))
        .stdout(predicate::str::contains("shared_skill"))
        .stdout(predicate::str::contains("shadowed_by="));
}

#[test]
#[serial]
fn test_skills_show_output_displays_visible_skill_metadata() {
    let project_dir = TempDir::new().expect("project temp dir should exist");
    let home_dir = TempDir::new().expect("home temp dir should exist");
    let _home_guard = HomeEnvGuard::set(home_dir.path());

    let skill_root = home_dir.path().join(".xzatoma").join("skills");
    let skill_dir = skill_root.join("visible_user_skill");
    fs::create_dir_all(&skill_dir).expect("skill dir should exist");
    fs::write(
        skill_dir.join("SKILL.md"),
        concat!(
            "---\n",
            "name: visible_user_skill\n",
            "description: Visible user skill\n",
            "license: MIT\n",
            "compatibility: xzatoma >=0.2.0\n",
            "allowed-tools: read_file, grep\n",
            "metadata:\n",
            "  owner: docs\n",
            "  team: platform\n",
            "---\n",
            "# Visible user skill\n"
        ),
    )
    .expect("skill file should be written");

    let trust_store_path = project_dir.path().join("skills_trust.yaml");
    let config_yaml = default_config_yaml(&trust_store_path);
    let (_config_dir, config_path) = common::temp_config_file(&config_yaml);

    let mut cmd = Command::cargo_bin("xzatoma").expect("binary should build");
    cmd.current_dir(project_dir.path())
        .arg("--config")
        .arg(&config_path)
        .arg("skills")
        .arg("show")
        .arg("visible_user_skill");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("name: visible_user_skill"))
        .stdout(predicate::str::contains("description: Visible user skill"))
        .stdout(predicate::str::contains("scope: user_client_specific"))
        .stdout(predicate::str::contains("license: MIT"))
        .stdout(predicate::str::contains("compatibility: xzatoma >=0.2.0"))
        .stdout(predicate::str::contains("allowed_tools: read_file, grep"))
        .stdout(predicate::str::contains("metadata:"))
        .stdout(predicate::str::contains("owner: docs"))
        .stdout(predicate::str::contains("team: platform"));
}

#[test]
#[serial]
fn test_skills_show_errors_on_hidden_project_skill() {
    let project_dir = TempDir::new().expect("project temp dir should exist");
    let home_dir = TempDir::new().expect("home temp dir should exist");
    let _home_guard = HomeEnvGuard::set(home_dir.path());

    write_skill(
        &project_dir.path().join(".xzatoma").join("skills"),
        "hidden_project_skill",
        "hidden_project_skill",
        "Hidden project skill",
        "# Hidden project skill",
    );

    let trust_store_path = project_dir.path().join("skills_trust.yaml");
    let config_yaml = default_config_yaml(&trust_store_path);
    let (_config_dir, config_path) = common::temp_config_file(&config_yaml);

    let mut cmd = Command::cargo_bin("xzatoma").expect("binary should build");
    cmd.current_dir(project_dir.path())
        .arg("--config")
        .arg(&config_path)
        .arg("skills")
        .arg("show")
        .arg("hidden_project_skill");

    cmd.assert().failure().stderr(predicate::str::contains(
        "Skill 'hidden_project_skill' is missing, invalid, or not visible",
    ));
}

#[test]
#[serial]
fn test_skills_paths_output_prints_effective_roots_and_trust_status() {
    let project_dir = TempDir::new().expect("project temp dir should exist");
    let home_dir = TempDir::new().expect("home temp dir should exist");
    let custom_dir = TempDir::new().expect("custom temp dir should exist");
    let _home_guard = HomeEnvGuard::set(home_dir.path());

    let trust_store_path = project_dir.path().join("skills_trust.yaml");
    let config_yaml = config_with_additional_paths(&trust_store_path, &[custom_dir.path()], false);
    let (_config_dir, config_path) = common::temp_config_file(&config_yaml);

    let mut add_cmd = Command::cargo_bin("xzatoma").expect("binary should build");
    add_cmd
        .current_dir(project_dir.path())
        .arg("--config")
        .arg(&config_path)
        .arg("skills")
        .arg("trust")
        .arg("add")
        .arg(project_dir.path());

    add_cmd.assert().success();

    let mut cmd = Command::cargo_bin("xzatoma").expect("binary should build");
    cmd.current_dir(project_dir.path())
        .arg("--config")
        .arg(&config_path)
        .arg("skills")
        .arg("paths");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("working_dir:"))
        .stdout(predicate::str::contains("trust_store:"))
        .stdout(predicate::str::contains("project_trust_required: true"))
        .stdout(predicate::str::contains(
            "allow_custom_paths_without_trust: false",
        ))
        .stdout(predicate::str::contains("configured discovery roots:"))
        .stdout(predicate::str::contains("project_client_specific:"))
        .stdout(predicate::str::contains("project_shared_convention:"))
        .stdout(predicate::str::contains("user_client_specific:"))
        .stdout(predicate::str::contains("user_shared_convention:"))
        .stdout(predicate::str::contains("custom_0:"))
        .stdout(predicate::str::contains("trusted paths:"))
        .stdout(predicate::str::contains(
            project_dir.path().display().to_string(),
        ));
}

#[test]
#[serial]
fn test_skills_trust_show_output_prints_trusted_paths() {
    let project_dir = TempDir::new().expect("project temp dir should exist");
    let home_dir = TempDir::new().expect("home temp dir should exist");
    let _home_guard = HomeEnvGuard::set(home_dir.path());

    let trust_store_path = project_dir.path().join("skills_trust.yaml");
    let config_yaml = default_config_yaml(&trust_store_path);
    let (_config_dir, config_path) = common::temp_config_file(&config_yaml);

    let mut add_cmd = Command::cargo_bin("xzatoma").expect("binary should build");
    add_cmd
        .current_dir(project_dir.path())
        .arg("--config")
        .arg(&config_path)
        .arg("skills")
        .arg("trust")
        .arg("add")
        .arg(project_dir.path());

    add_cmd.assert().success();

    let mut show_cmd = Command::cargo_bin("xzatoma").expect("binary should build");
    show_cmd
        .current_dir(project_dir.path())
        .arg("--config")
        .arg(&config_path)
        .arg("skills")
        .arg("trust")
        .arg("show");

    show_cmd
        .assert()
        .success()
        .stdout(predicate::str::contains("trust_store:"))
        .stdout(predicate::str::contains("project_trust_required: true"))
        .stdout(predicate::str::contains(
            "allow_custom_paths_without_trust: false",
        ))
        .stdout(predicate::str::contains("trusted_paths:"))
        .stdout(predicate::str::contains(
            project_dir.path().display().to_string(),
        ));
}

#[test]
#[serial]
fn test_skills_trust_add_output_updates_trust_store_deterministically() {
    let project_dir = TempDir::new().expect("project temp dir should exist");
    let home_dir = TempDir::new().expect("home temp dir should exist");
    let custom_dir = TempDir::new().expect("custom temp dir should exist");
    let _home_guard = HomeEnvGuard::set(home_dir.path());

    let trust_store_path = project_dir.path().join("skills_trust.yaml");
    let config_yaml = default_config_yaml(&trust_store_path);
    let (_config_dir, config_path) = common::temp_config_file(&config_yaml);

    let mut first_add = Command::cargo_bin("xzatoma").expect("binary should build");
    first_add
        .current_dir(project_dir.path())
        .arg("--config")
        .arg(&config_path)
        .arg("skills")
        .arg("trust")
        .arg("add")
        .arg(custom_dir.path());

    first_add
        .assert()
        .success()
        .stdout(predicate::str::contains("Trusted path added:"))
        .stdout(predicate::str::contains(
            custom_dir.path().display().to_string(),
        ));

    let initial_contents = trust_store_contents(&trust_store_path);

    let mut second_add = Command::cargo_bin("xzatoma").expect("binary should build");
    second_add
        .current_dir(project_dir.path())
        .arg("--config")
        .arg(&config_path)
        .arg("skills")
        .arg("trust")
        .arg("add")
        .arg(custom_dir.path());

    second_add.assert().success();

    let second_contents = trust_store_contents(&trust_store_path);
    assert_eq!(initial_contents, second_contents);
    assert_eq!(second_contents.matches("trusted_paths:").count(), 1);
}

#[test]
#[serial]
fn test_skills_trust_remove_output_updates_trust_store_deterministically() {
    let project_dir = TempDir::new().expect("project temp dir should exist");
    let home_dir = TempDir::new().expect("home temp dir should exist");
    let custom_dir = TempDir::new().expect("custom temp dir should exist");
    let _home_guard = HomeEnvGuard::set(home_dir.path());

    let trust_store_path = project_dir.path().join("skills_trust.yaml");
    let config_yaml = default_config_yaml(&trust_store_path);
    let (_config_dir, config_path) = common::temp_config_file(&config_yaml);

    let mut add_cmd = Command::cargo_bin("xzatoma").expect("binary should build");
    add_cmd
        .current_dir(project_dir.path())
        .arg("--config")
        .arg(&config_path)
        .arg("skills")
        .arg("trust")
        .arg("add")
        .arg(custom_dir.path());

    add_cmd.assert().success();

    let mut remove_cmd = Command::cargo_bin("xzatoma").expect("binary should build");
    remove_cmd
        .current_dir(project_dir.path())
        .arg("--config")
        .arg(&config_path)
        .arg("skills")
        .arg("trust")
        .arg("remove")
        .arg(custom_dir.path());

    remove_cmd
        .assert()
        .success()
        .stdout(predicate::str::contains("Trusted path removed:"))
        .stdout(predicate::str::contains(
            custom_dir.path().display().to_string(),
        ));

    let contents_after_remove = trust_store_contents(&trust_store_path);
    assert!(contents_after_remove.contains("trusted_paths:"));
    assert!(!contents_after_remove.contains(&custom_dir.path().display().to_string()));

    let mut remove_again_cmd = Command::cargo_bin("xzatoma").expect("binary should build");
    remove_again_cmd
        .current_dir(project_dir.path())
        .arg("--config")
        .arg(&config_path)
        .arg("skills")
        .arg("trust")
        .arg("remove")
        .arg(custom_dir.path());

    remove_again_cmd
        .assert()
        .success()
        .stdout(predicate::str::contains("Path was not trusted:"))
        .stdout(predicate::str::contains(
            custom_dir.path().display().to_string(),
        ));

    let contents_after_second_remove = trust_store_contents(&trust_store_path);
    assert_eq!(contents_after_remove, contents_after_second_remove);
}

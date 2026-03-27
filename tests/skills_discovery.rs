use serial_test::serial;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;
use xzatoma::config::SkillsConfig;
use xzatoma::skills::{discover_skills, SkillDiagnosticKind, SkillSourceScope};

struct HomeEnvGuard {
    original_home: Option<String>,
}

impl HomeEnvGuard {
    fn set(temp_home: &Path) -> Self {
        let original_home = env::var("HOME").ok();
        // SAFETY: Test-only environment mutation scoped by guard lifetime.
        unsafe {
            env::set_var("HOME", temp_home);
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
                    env::set_var("HOME", value);
                }
            }
            None => {
                // SAFETY: Test-only environment mutation scoped by guard lifetime.
                unsafe {
                    env::remove_var("HOME");
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
    fs::create_dir_all(&skill_dir).expect("failed to create skill directory");

    let skill_file = skill_dir.join("SKILL.md");
    let contents = format!(
        "---\nname: {name}\ndescription: {description}\nallowed-tools: read_file, grep\n---\n{body}\n"
    );

    fs::write(&skill_file, contents).expect("failed to write skill file");
    skill_file
}

fn write_raw_skill(root: &Path, relative_dir: &str, contents: &str) -> PathBuf {
    let skill_dir = root.join(relative_dir);
    fs::create_dir_all(&skill_dir).expect("failed to create skill directory");

    let skill_file = skill_dir.join("SKILL.md");
    fs::write(&skill_file, contents).expect("failed to write raw skill file");
    skill_file
}

fn default_skills_config() -> SkillsConfig {
    SkillsConfig::default()
}

fn canonicalize_for_assertion(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

#[test]
#[serial]
fn test_valid_project_skill_discovery() {
    let project_dir = TempDir::new().expect("failed to create project temp dir");
    let home_dir = TempDir::new().expect("failed to create home temp dir");
    let _home_guard = HomeEnvGuard::set(home_dir.path());

    write_skill(
        &project_dir.path().join(".xzatoma").join("skills"),
        "project_skill",
        "project_skill",
        "Project skill description",
        "# Project skill",
    );

    let config = default_skills_config();
    let result = discover_skills(&config, project_dir.path()).expect("discovery should succeed");

    assert_eq!(result.valid_skills.len(), 1);
    assert_eq!(result.catalog.len(), 1);
    assert!(result.invalid_diagnostics.is_empty());
    assert!(result.shadowed_diagnostics.is_empty());

    let record = result
        .catalog
        .get("project_skill")
        .expect("project skill should be loaded");
    assert_eq!(record.metadata.description, "Project skill description");
    assert_eq!(record.source_scope, SkillSourceScope::ProjectClientSpecific);
}

#[test]
#[serial]
fn test_valid_user_skill_discovery() {
    let project_dir = TempDir::new().expect("failed to create project temp dir");
    let home_dir = TempDir::new().expect("failed to create home temp dir");
    let _home_guard = HomeEnvGuard::set(home_dir.path());

    write_skill(
        &home_dir.path().join(".xzatoma").join("skills"),
        "user_skill",
        "user_skill",
        "User skill description",
        "# User skill",
    );

    let config = SkillsConfig {
        project_enabled: false,
        ..default_skills_config()
    };

    let result = discover_skills(&config, project_dir.path()).expect("discovery should succeed");

    assert_eq!(result.valid_skills.len(), 1);
    let record = result
        .catalog
        .get("user_skill")
        .expect("user skill should be loaded");

    assert_eq!(record.metadata.description, "User skill description");
    assert_eq!(record.source_scope, SkillSourceScope::UserClientSpecific);
}

#[test]
#[serial]
fn test_custom_path_discovery() {
    let project_dir = TempDir::new().expect("failed to create project temp dir");
    let home_dir = TempDir::new().expect("failed to create home temp dir");
    let custom_dir = TempDir::new().expect("failed to create custom temp dir");
    let _home_guard = HomeEnvGuard::set(home_dir.path());

    write_skill(
        custom_dir.path(),
        "custom_skill",
        "custom_skill",
        "Custom skill description",
        "# Custom skill",
    );

    let config = SkillsConfig {
        project_enabled: false,
        user_enabled: false,
        additional_paths: vec![custom_dir.path().display().to_string()],
        ..default_skills_config()
    };

    let result = discover_skills(&config, project_dir.path()).expect("discovery should succeed");

    assert_eq!(result.valid_skills.len(), 1);
    let record = result
        .catalog
        .get("custom_skill")
        .expect("custom skill should be loaded");

    assert_eq!(record.source_scope, SkillSourceScope::CustomConfigured);
    assert_eq!(record.source_order, 0);
}

#[test]
#[serial]
fn test_collision_precedence_prefers_project_over_user() {
    let project_dir = TempDir::new().expect("failed to create project temp dir");
    let home_dir = TempDir::new().expect("failed to create home temp dir");
    let _home_guard = HomeEnvGuard::set(home_dir.path());

    let project_skill_path = write_skill(
        &project_dir.path().join(".xzatoma").join("skills"),
        "shared_skill",
        "shared_skill",
        "Project version",
        "# Project",
    );

    let user_skill_path = write_skill(
        &home_dir.path().join(".xzatoma").join("skills"),
        "shared_skill",
        "shared_skill",
        "User version",
        "# User",
    );

    let config = default_skills_config();
    let result = discover_skills(&config, project_dir.path()).expect("discovery should succeed");

    assert_eq!(result.valid_skills.len(), 1);

    let winner = result
        .catalog
        .get("shared_skill")
        .expect("shared skill should be loaded");
    assert_eq!(winner.metadata.description, "Project version");
    assert_eq!(
        canonicalize_for_assertion(&winner.skill_file),
        canonicalize_for_assertion(&project_skill_path)
    );

    assert_eq!(result.shadowed_diagnostics.len(), 1);
    let diagnostic = &result.shadowed_diagnostics[0];
    assert_eq!(diagnostic.kind, SkillDiagnosticKind::ShadowedSkill);
    assert_eq!(diagnostic.skill_name.as_deref(), Some("shared_skill"));
    assert_eq!(
        canonicalize_for_assertion(&diagnostic.skill_file_path),
        canonicalize_for_assertion(&user_skill_path)
    );
    assert_eq!(
        diagnostic
            .overshadowed_by
            .as_ref()
            .map(|path| canonicalize_for_assertion(path)),
        Some(canonicalize_for_assertion(&project_skill_path))
    );
}

#[test]
#[serial]
fn test_invalid_name_is_excluded_from_catalog() {
    let project_dir = TempDir::new().expect("failed to create project temp dir");
    let home_dir = TempDir::new().expect("failed to create home temp dir");
    let _home_guard = HomeEnvGuard::set(home_dir.path());

    write_raw_skill(
        &project_dir.path().join(".xzatoma").join("skills"),
        "invalid_skill",
        "---\nname: Invalid-Skill\ndescription: Invalid name\n---\n# Invalid\n",
    );

    let config = default_skills_config();
    let result = discover_skills(&config, project_dir.path()).expect("discovery should succeed");

    assert!(result.valid_skills.is_empty());
    assert!(result.catalog.is_empty());
    assert_eq!(result.invalid_diagnostics.len(), 1);
    assert_eq!(
        result.invalid_diagnostics[0].kind,
        SkillDiagnosticKind::InvalidName
    );
}

#[test]
#[serial]
fn test_invalid_description_is_excluded_from_catalog() {
    let project_dir = TempDir::new().expect("failed to create project temp dir");
    let home_dir = TempDir::new().expect("failed to create home temp dir");
    let _home_guard = HomeEnvGuard::set(home_dir.path());

    write_raw_skill(
        &project_dir.path().join(".xzatoma").join("skills"),
        "bad_description",
        "---\nname: bad_description\ndescription:   \n---\n# Invalid\n",
    );

    let config = default_skills_config();
    let result = discover_skills(&config, project_dir.path()).expect("discovery should succeed");

    assert!(result.valid_skills.is_empty());
    assert!(result.catalog.is_empty());
    assert_eq!(result.invalid_diagnostics.len(), 1);
    assert_eq!(
        result.invalid_diagnostics[0].kind,
        SkillDiagnosticKind::InvalidDescription
    );
}

#[test]
#[serial]
fn test_missing_frontmatter_is_diagnosed() {
    let project_dir = TempDir::new().expect("failed to create project temp dir");
    let home_dir = TempDir::new().expect("failed to create home temp dir");
    let _home_guard = HomeEnvGuard::set(home_dir.path());

    write_raw_skill(
        &project_dir.path().join(".xzatoma").join("skills"),
        "missing_frontmatter",
        "# Missing frontmatter\n",
    );

    let config = default_skills_config();
    let result = discover_skills(&config, project_dir.path()).expect("discovery should succeed");

    assert!(result.catalog.is_empty());
    assert_eq!(result.invalid_diagnostics.len(), 1);
    assert_eq!(
        result.invalid_diagnostics[0].kind,
        SkillDiagnosticKind::MissingFrontmatter
    );
}

#[test]
#[serial]
fn test_malformed_frontmatter_is_diagnosed() {
    let project_dir = TempDir::new().expect("failed to create project temp dir");
    let home_dir = TempDir::new().expect("failed to create home temp dir");
    let _home_guard = HomeEnvGuard::set(home_dir.path());

    write_raw_skill(
        &project_dir.path().join(".xzatoma").join("skills"),
        "malformed_frontmatter",
        "---\nname: malformed_frontmatter\ndescription: valid\nmetadata:\n  broken: [\n---\n# Invalid\n",
    );

    let config = default_skills_config();
    let result = discover_skills(&config, project_dir.path()).expect("discovery should succeed");

    assert!(result.catalog.is_empty());
    assert_eq!(result.invalid_diagnostics.len(), 1);
    assert_eq!(
        result.invalid_diagnostics[0].kind,
        SkillDiagnosticKind::MalformedFrontmatter
    );
}

#[test]
#[serial]
fn test_shadowed_valid_skill_diagnostic_is_recorded() {
    let project_dir = TempDir::new().expect("failed to create project temp dir");
    let home_dir = TempDir::new().expect("failed to create home temp dir");
    let custom_a = TempDir::new().expect("failed to create custom dir a");
    let custom_b = TempDir::new().expect("failed to create custom dir b");
    let _home_guard = HomeEnvGuard::set(home_dir.path());

    let winner_path = write_skill(
        custom_a.path(),
        "duplicate_skill",
        "duplicate_skill",
        "Preferred custom skill",
        "# Winner",
    );

    let loser_path = write_skill(
        custom_b.path(),
        "duplicate_skill",
        "duplicate_skill",
        "Later custom skill",
        "# Loser",
    );

    let config = SkillsConfig {
        project_enabled: false,
        user_enabled: false,
        additional_paths: vec![
            custom_a.path().display().to_string(),
            custom_b.path().display().to_string(),
        ],
        ..default_skills_config()
    };

    let result = discover_skills(&config, project_dir.path()).expect("discovery should succeed");

    let winner = result
        .catalog
        .get("duplicate_skill")
        .expect("duplicate skill should be loaded");
    assert_eq!(
        canonicalize_for_assertion(&winner.skill_file),
        canonicalize_for_assertion(&winner_path)
    );
    assert_eq!(winner.metadata.description, "Preferred custom skill");

    assert_eq!(result.shadowed_diagnostics.len(), 1);
    let diagnostic = &result.shadowed_diagnostics[0];
    assert_eq!(diagnostic.kind, SkillDiagnosticKind::ShadowedSkill);
    assert_eq!(
        canonicalize_for_assertion(&diagnostic.skill_file_path),
        canonicalize_for_assertion(&loser_path)
    );
    assert_eq!(
        diagnostic
            .overshadowed_by
            .as_ref()
            .map(|path| canonicalize_for_assertion(path)),
        Some(canonicalize_for_assertion(&winner_path))
    );
}

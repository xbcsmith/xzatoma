use serial_test::serial;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;
use xzatoma::config::SkillsConfig;
use xzatoma::skills::catalog::SkillCatalog;
use xzatoma::skills::disclosure::{build_skill_disclosure_section, render_skill_catalog};
use xzatoma::skills::parser::parse_skill_file;
use xzatoma::skills::types::{
    SkillDiagnostic, SkillDiagnosticKind, SkillMetadata, SkillRecord, SkillSourceScope,
    SkillValidationOutcome,
};
use xzatoma::skills::validation::validate_parsed_skill;

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

fn default_skills_config() -> SkillsConfig {
    SkillsConfig::default()
}

fn canonicalize_for_assertion(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
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

fn load_valid_skill(
    skill_file: &Path,
    source_scope: SkillSourceScope,
    source_order: usize,
) -> SkillRecord {
    let parsed =
        parse_skill_file(skill_file, source_scope).expect("expected test skill to parse correctly");

    match validate_parsed_skill(parsed) {
        SkillValidationOutcome::Valid(document) => SkillRecord {
            metadata: SkillMetadata {
                name: document.name.expect("validated skill should have name"),
                description: document
                    .description
                    .expect("validated skill should have description"),
                license: document.license,
                compatibility: document.compatibility,
                metadata: document.metadata.unwrap_or_default(),
                allowed_tools_raw: document.allowed_tools_raw.clone(),
                allowed_tools: document
                    .allowed_tools_raw
                    .as_deref()
                    .map(|raw| {
                        raw.split([',', '\n'])
                            .map(str::trim)
                            .filter(|value| !value.is_empty())
                            .map(ToOwned::to_owned)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default(),
            },
            skill_dir: canonicalize_for_assertion(
                document
                    .skill_file_path
                    .parent()
                    .expect("skill file should have parent directory"),
            ),
            skill_file: canonicalize_for_assertion(&document.skill_file_path),
            source_scope,
            source_order,
            body: document.body_markdown,
        },
        SkillValidationOutcome::Invalid(diagnostics) => {
            panic!("expected valid skill, got diagnostics: {:?}", diagnostics)
        }
    }
}

fn invalid_diagnostic(
    path: &Path,
    scope: SkillSourceScope,
    kind: SkillDiagnosticKind,
    skill_name: Option<&str>,
) -> SkillDiagnostic {
    SkillDiagnostic::new(
        kind,
        "invalid skill for disclosure testing",
        skill_name.map(ToOwned::to_owned),
        canonicalize_for_assertion(path),
        Some(scope),
    )
}

fn catalog_from_records(records: Vec<SkillRecord>) -> SkillCatalog {
    SkillCatalog::from_records(records).expect("catalog construction should succeed")
}

#[test]
#[serial]
fn test_disclosure_contains_only_valid_visible_skills() {
    let project_dir = TempDir::new().expect("failed to create project temp dir");
    let home_dir = TempDir::new().expect("failed to create home temp dir");
    let _home_guard = HomeEnvGuard::set(home_dir.path());

    let project_skill = write_skill(
        &project_dir.path().join(".xzatoma").join("skills"),
        "project_visible_skill",
        "project_visible_skill",
        "Visible project skill",
        "# Project",
    );
    let user_skill = write_skill(
        &home_dir.path().join(".xzatoma").join("skills"),
        "user_visible_skill",
        "user_visible_skill",
        "Visible user skill",
        "# User",
    );

    let catalog = catalog_from_records(vec![
        load_valid_skill(&project_skill, SkillSourceScope::ProjectClientSpecific, 0),
        load_valid_skill(&user_skill, SkillSourceScope::UserClientSpecific, 0),
    ]);

    let config = SkillsConfig {
        project_trust_required: false,
        ..default_skills_config()
    };

    let disclosure = build_skill_disclosure_section(
        &catalog,
        &[],
        &config,
        project_dir.path(),
        &std::collections::BTreeSet::new(),
    )
    .expect("expected disclosure to be present");

    assert!(disclosure.contains("project_visible_skill"));
    assert!(disclosure.contains("Visible project skill"));
    assert!(disclosure.contains("user_visible_skill"));
    assert!(disclosure.contains("Visible user skill"));
    assert!(!disclosure.contains("# Project"));
    assert!(!disclosure.contains("# User"));
}

#[test]
#[serial]
fn test_untrusted_project_skills_are_omitted() {
    let project_dir = TempDir::new().expect("failed to create project temp dir");
    let home_dir = TempDir::new().expect("failed to create home temp dir");
    let _home_guard = HomeEnvGuard::set(home_dir.path());

    let project_skill = write_skill(
        &project_dir.path().join(".xzatoma").join("skills"),
        "project_hidden_skill",
        "project_hidden_skill",
        "Hidden project skill",
        "# Hidden",
    );
    let user_skill = write_skill(
        &home_dir.path().join(".xzatoma").join("skills"),
        "user_visible_skill",
        "user_visible_skill",
        "Visible user skill",
        "# User",
    );

    let catalog = catalog_from_records(vec![
        load_valid_skill(&project_skill, SkillSourceScope::ProjectClientSpecific, 0),
        load_valid_skill(&user_skill, SkillSourceScope::UserClientSpecific, 0),
    ]);

    let config = SkillsConfig {
        project_trust_required: true,
        ..default_skills_config()
    };

    let trusted_paths = std::collections::BTreeSet::new();
    let disclosure =
        build_skill_disclosure_section(&catalog, &[], &config, project_dir.path(), &trusted_paths)
            .expect("expected disclosure to be present because user skill remains visible");

    assert!(!disclosure.contains("project_hidden_skill"));
    assert!(!disclosure.contains("Hidden project skill"));
    assert!(disclosure.contains("user_visible_skill"));
    assert!(disclosure.contains("Visible user skill"));
}

#[test]
#[serial]
fn test_invalid_skills_are_omitted() {
    let project_dir = TempDir::new().expect("failed to create project temp dir");
    let home_dir = TempDir::new().expect("failed to create home temp dir");
    let _home_guard = HomeEnvGuard::set(home_dir.path());

    let valid_skill = write_skill(
        &home_dir.path().join(".xzatoma").join("skills"),
        "valid_visible_skill",
        "valid_visible_skill",
        "Visible user skill",
        "# User",
    );
    let invalid_skill = write_raw_skill(
        &project_dir.path().join(".xzatoma").join("skills"),
        "invalid_hidden_skill",
        "---\nname: Invalid-Skill\ndescription: Invalid\n---\n# Invalid\n",
    );

    let catalog = catalog_from_records(vec![load_valid_skill(
        &valid_skill,
        SkillSourceScope::UserClientSpecific,
        0,
    )]);

    let invalid_diagnostics = vec![invalid_diagnostic(
        &invalid_skill,
        SkillSourceScope::ProjectClientSpecific,
        SkillDiagnosticKind::InvalidName,
        Some("Invalid-Skill"),
    )];

    let config = SkillsConfig {
        project_trust_required: false,
        ..default_skills_config()
    };

    let disclosure = build_skill_disclosure_section(
        &catalog,
        &invalid_diagnostics,
        &config,
        project_dir.path(),
        &std::collections::BTreeSet::new(),
    )
    .expect("expected disclosure to be present");

    assert!(disclosure.contains("valid_visible_skill"));
    assert!(!disclosure.contains("Invalid-Skill"));
    assert!(!disclosure.contains("invalid_hidden_skill"));
    assert!(!disclosure.contains("# Invalid"));
}

#[test]
#[serial]
fn test_empty_catalog_yields_no_disclosure_block() {
    let project_dir = TempDir::new().expect("failed to create project temp dir");
    let home_dir = TempDir::new().expect("failed to create home temp dir");
    let _home_guard = HomeEnvGuard::set(home_dir.path());

    let catalog = SkillCatalog::new();
    let disclosure = build_skill_disclosure_section(
        &catalog,
        &[],
        &default_skills_config(),
        project_dir.path(),
        &std::collections::BTreeSet::new(),
    );

    assert!(disclosure.is_none());
}

#[test]
#[serial]
fn test_disclosed_catalog_ordering_is_deterministic() {
    let project_dir = TempDir::new().expect("failed to create project temp dir");
    let home_dir = TempDir::new().expect("failed to create home temp dir");
    let _home_guard = HomeEnvGuard::set(home_dir.path());

    let skill_b = write_skill(
        &home_dir.path().join(".xzatoma").join("skills"),
        "beta_skill",
        "beta_skill",
        "Beta description",
        "# Beta",
    );
    let skill_a = write_skill(
        &home_dir.path().join(".xzatoma").join("skills"),
        "alpha_skill",
        "alpha_skill",
        "Alpha description",
        "# Alpha",
    );
    let skill_c = write_skill(
        &home_dir.path().join(".xzatoma").join("skills"),
        "gamma_skill",
        "gamma_skill",
        "Gamma description",
        "# Gamma",
    );

    let catalog = catalog_from_records(vec![
        load_valid_skill(&skill_b, SkillSourceScope::UserClientSpecific, 0),
        load_valid_skill(&skill_a, SkillSourceScope::UserClientSpecific, 0),
        load_valid_skill(&skill_c, SkillSourceScope::UserClientSpecific, 0),
    ]);

    let rendered = render_skill_catalog(
        &catalog,
        &default_skills_config(),
        project_dir.path(),
        &std::collections::BTreeSet::new(),
    );

    assert_eq!(rendered.len(), 3);

    let first = &rendered[0];
    let second = &rendered[1];
    let third = &rendered[2];

    assert!(first.contains("alpha_skill"));
    assert!(second.contains("beta_skill"));
    assert!(third.contains("gamma_skill"));
}

#[test]
#[serial]
fn test_catalog_entry_count_obeys_catalog_max_entries() {
    let project_dir = TempDir::new().expect("failed to create project temp dir");
    let home_dir = TempDir::new().expect("failed to create home temp dir");
    let _home_guard = HomeEnvGuard::set(home_dir.path());

    let alpha = write_skill(
        &home_dir.path().join(".xzatoma").join("skills"),
        "alpha_skill",
        "alpha_skill",
        "Alpha description",
        "# Alpha",
    );
    let beta = write_skill(
        &home_dir.path().join(".xzatoma").join("skills"),
        "beta_skill",
        "beta_skill",
        "Beta description",
        "# Beta",
    );
    let gamma = write_skill(
        &home_dir.path().join(".xzatoma").join("skills"),
        "gamma_skill",
        "gamma_skill",
        "Gamma description",
        "# Gamma",
    );

    let catalog = catalog_from_records(vec![
        load_valid_skill(&alpha, SkillSourceScope::UserClientSpecific, 0),
        load_valid_skill(&beta, SkillSourceScope::UserClientSpecific, 0),
        load_valid_skill(&gamma, SkillSourceScope::UserClientSpecific, 0),
    ]);

    let config = SkillsConfig {
        catalog_max_entries: 2,
        ..default_skills_config()
    };

    let rendered = render_skill_catalog(
        &catalog,
        &config,
        project_dir.path(),
        &std::collections::BTreeSet::new(),
    );

    assert_eq!(rendered.len(), 2);
    assert!(rendered[0].contains("alpha_skill"));
    assert!(rendered[1].contains("beta_skill"));
}

#[test]
#[serial]
fn test_untrusted_custom_skills_are_omitted_by_default() {
    let project_dir = TempDir::new().expect("failed to create project temp dir");
    let home_dir = TempDir::new().expect("failed to create home temp dir");
    let custom_dir = TempDir::new().expect("failed to create custom temp dir");
    let _home_guard = HomeEnvGuard::set(home_dir.path());

    let custom_skill = write_skill(
        custom_dir.path(),
        "custom_hidden_skill",
        "custom_hidden_skill",
        "Hidden custom skill",
        "# Custom",
    );
    let user_skill = write_skill(
        &home_dir.path().join(".xzatoma").join("skills"),
        "user_visible_skill",
        "user_visible_skill",
        "Visible user skill",
        "# User",
    );

    let catalog = catalog_from_records(vec![
        load_valid_skill(&custom_skill, SkillSourceScope::CustomConfigured, 0),
        load_valid_skill(&user_skill, SkillSourceScope::UserClientSpecific, 0),
    ]);

    let config = SkillsConfig {
        allow_custom_paths_without_trust: false,
        ..default_skills_config()
    };

    let disclosure = build_skill_disclosure_section(
        &catalog,
        &[],
        &config,
        project_dir.path(),
        &std::collections::BTreeSet::new(),
    )
    .expect("expected disclosure to be present because user skill remains visible");

    assert!(!disclosure.contains("custom_hidden_skill"));
    assert!(disclosure.contains("user_visible_skill"));
}

#[test]
#[serial]
fn test_trusted_project_skills_are_visible() {
    let project_dir = TempDir::new().expect("failed to create project temp dir");
    let home_dir = TempDir::new().expect("failed to create home temp dir");
    let _home_guard = HomeEnvGuard::set(home_dir.path());

    let project_skill = write_skill(
        &project_dir.path().join(".xzatoma").join("skills"),
        "trusted_project_skill",
        "trusted_project_skill",
        "Trusted project skill",
        "# Project",
    );

    let catalog = catalog_from_records(vec![load_valid_skill(
        &project_skill,
        SkillSourceScope::ProjectClientSpecific,
        0,
    )]);

    let config = SkillsConfig {
        project_trust_required: true,
        ..default_skills_config()
    };

    let mut trusted_paths = std::collections::BTreeSet::new();
    trusted_paths.insert(canonicalize_for_assertion(project_dir.path()));

    let disclosure =
        build_skill_disclosure_section(&catalog, &[], &config, project_dir.path(), &trusted_paths)
            .expect("expected disclosure for trusted project skill");

    assert!(disclosure.contains("trusted_project_skill"));
    assert!(disclosure.contains("Trusted project skill"));
    assert!(!disclosure.contains("# Project"));
}

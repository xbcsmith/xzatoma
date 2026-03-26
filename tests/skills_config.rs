use serial_test::serial;
use std::fs;
use tempfile::TempDir;
use xzatoma::cli::{Cli, Commands, SkillsCommand};
use xzatoma::config::{Config, SkillsConfig};

struct EnvVarGuard {
    key: &'static str,
    original: Option<String>,
}

impl EnvVarGuard {
    fn set(key: &'static str, value: &str) -> Self {
        let original = std::env::var(key).ok();
        // SAFETY: Test-scoped environment mutation restored by Drop.
        unsafe {
            std::env::set_var(key, value);
        }

        Self { key, original }
    }

    fn remove(key: &'static str) -> Self {
        let original = std::env::var(key).ok();
        // SAFETY: Test-scoped environment mutation restored by Drop.
        unsafe {
            std::env::remove_var(key);
        }

        Self { key, original }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        match &self.original {
            Some(value) => {
                // SAFETY: Test-scoped environment restoration.
                unsafe {
                    std::env::set_var(self.key, value);
                }
            }
            None => {
                // SAFETY: Test-scoped environment restoration.
                unsafe {
                    std::env::remove_var(self.key);
                }
            }
        }
    }
}

fn auth_cli() -> Cli {
    Cli {
        config: Some("config/config.yaml".to_string()),
        verbose: false,
        storage_path: None,
        command: Commands::Auth { provider: None },
    }
}

fn skills_cli(command: SkillsCommand) -> Cli {
    Cli {
        config: Some("config/config.yaml".to_string()),
        verbose: false,
        storage_path: None,
        command: Commands::Skills { command },
    }
}

fn write_temp_config(contents: &str) -> (TempDir, String) {
    let temp_dir = TempDir::new().expect("temp dir should be created");
    let config_path = temp_dir.path().join("config.yaml");
    fs::write(&config_path, contents).expect("config file should be written");
    (temp_dir, config_path.to_string_lossy().to_string())
}

#[test]
fn test_skills_config_default_values() {
    let config = SkillsConfig::default();

    assert!(config.enabled);
    assert!(config.project_enabled);
    assert!(config.user_enabled);
    assert!(config.additional_paths.is_empty());
    assert_eq!(config.max_discovered_skills, 256);
    assert_eq!(config.max_scan_directories, 2000);
    assert_eq!(config.max_scan_depth, 6);
    assert_eq!(config.catalog_max_entries, 128);
    assert!(config.activation_tool_enabled);
    assert!(config.project_trust_required);
    assert!(config.strict_frontmatter);
    assert!(!config.allow_custom_paths_without_trust);
    assert_eq!(config.trust_store_path, None);
}

#[test]
fn test_skills_config_deserialization_with_all_fields() {
    let yaml = r#"
provider:
  type: copilot
agent:
  max_turns: 50
skills:
  enabled: false
  project_enabled: false
  user_enabled: true
  additional_paths:
    - /opt/xzatoma/skills
    - ./custom_skills
  max_discovered_skills: 64
  max_scan_directories: 400
  max_scan_depth: 4
  catalog_max_entries: 32
  activation_tool_enabled: false
  project_trust_required: false
  trust_store_path: ~/.xzatoma/custom_skills_trust.yaml
  allow_custom_paths_without_trust: true
  strict_frontmatter: false
"#;

    let config: Config = serde_yaml::from_str(yaml).expect("config should deserialize");

    assert!(!config.skills.enabled);
    assert!(!config.skills.project_enabled);
    assert!(config.skills.user_enabled);
    assert_eq!(
        config.skills.additional_paths,
        vec![
            "/opt/xzatoma/skills".to_string(),
            "./custom_skills".to_string()
        ]
    );
    assert_eq!(config.skills.max_discovered_skills, 64);
    assert_eq!(config.skills.max_scan_directories, 400);
    assert_eq!(config.skills.max_scan_depth, 4);
    assert_eq!(config.skills.catalog_max_entries, 32);
    assert!(!config.skills.activation_tool_enabled);
    assert!(!config.skills.project_trust_required);
    assert_eq!(
        config.skills.trust_store_path.as_deref(),
        Some("~/.xzatoma/custom_skills_trust.yaml")
    );
    assert!(config.skills.allow_custom_paths_without_trust);
    assert!(!config.skills.strict_frontmatter);
}

#[test]
fn test_skills_config_deserialization_defaults_when_section_omitted() {
    let yaml = r#"
provider:
  type: ollama
agent:
  max_turns: 25
"#;

    let config: Config = serde_yaml::from_str(yaml).expect("config should deserialize");
    let defaults = SkillsConfig::default();

    assert_eq!(config.skills.enabled, defaults.enabled);
    assert_eq!(config.skills.project_enabled, defaults.project_enabled);
    assert_eq!(config.skills.user_enabled, defaults.user_enabled);
    assert_eq!(config.skills.additional_paths, defaults.additional_paths);
    assert_eq!(
        config.skills.max_discovered_skills,
        defaults.max_discovered_skills
    );
    assert_eq!(
        config.skills.max_scan_directories,
        defaults.max_scan_directories
    );
    assert_eq!(config.skills.max_scan_depth, defaults.max_scan_depth);
    assert_eq!(
        config.skills.catalog_max_entries,
        defaults.catalog_max_entries
    );
    assert_eq!(
        config.skills.activation_tool_enabled,
        defaults.activation_tool_enabled
    );
    assert_eq!(
        config.skills.project_trust_required,
        defaults.project_trust_required
    );
    assert_eq!(config.skills.trust_store_path, defaults.trust_store_path);
    assert_eq!(
        config.skills.allow_custom_paths_without_trust,
        defaults.allow_custom_paths_without_trust
    );
    assert_eq!(
        config.skills.strict_frontmatter,
        defaults.strict_frontmatter
    );
}

#[test]
fn test_skills_config_validation_accepts_valid_configuration() {
    let config = Config {
        skills: SkillsConfig {
            additional_paths: vec![
                "/opt/xzatoma/skills".to_string(),
                "./project_skills".to_string(),
            ],
            max_discovered_skills: 64,
            max_scan_directories: 500,
            max_scan_depth: 3,
            catalog_max_entries: 32,
            activation_tool_enabled: true,
            project_trust_required: true,
            trust_store_path: Some("~/.xzatoma/skills_trust.yaml".to_string()),
            allow_custom_paths_without_trust: false,
            strict_frontmatter: true,
            ..SkillsConfig::default()
        },
        ..Config::default()
    };

    assert!(config.validate().is_ok());
}

#[test]
fn test_skills_config_validation_rejects_zero_max_discovered_skills() {
    let config = Config {
        skills: SkillsConfig {
            max_discovered_skills: 0,
            ..SkillsConfig::default()
        },
        ..Config::default()
    };

    let error = config.validate().expect_err("validation should fail");
    assert!(error.to_string().contains("skills.max_discovered_skills"));
}

#[test]
fn test_skills_config_validation_rejects_zero_max_scan_directories() {
    let config = Config {
        skills: SkillsConfig {
            max_scan_directories: 0,
            ..SkillsConfig::default()
        },
        ..Config::default()
    };

    let error = config.validate().expect_err("validation should fail");
    assert!(error.to_string().contains("skills.max_scan_directories"));
}

#[test]
fn test_skills_config_validation_rejects_zero_max_scan_depth() {
    let config = Config {
        skills: SkillsConfig {
            max_scan_depth: 0,
            ..SkillsConfig::default()
        },
        ..Config::default()
    };

    let error = config.validate().expect_err("validation should fail");
    assert!(error.to_string().contains("skills.max_scan_depth"));
}

#[test]
fn test_skills_config_validation_rejects_zero_catalog_max_entries() {
    let config = Config {
        skills: SkillsConfig {
            catalog_max_entries: 0,
            ..SkillsConfig::default()
        },
        ..Config::default()
    };

    let error = config.validate().expect_err("validation should fail");
    assert!(error.to_string().contains("skills.catalog_max_entries"));
}

#[test]
fn test_skills_config_validation_rejects_catalog_max_entries_above_discovered_limit() {
    let config = Config {
        skills: SkillsConfig {
            max_discovered_skills: 10,
            catalog_max_entries: 11,
            ..SkillsConfig::default()
        },
        ..Config::default()
    };

    let error = config.validate().expect_err("validation should fail");
    assert!(error
        .to_string()
        .contains("skills.catalog_max_entries must be less than or equal"));
}

#[test]
fn test_skills_config_validation_rejects_empty_additional_path_entry() {
    let config = Config {
        skills: SkillsConfig {
            additional_paths: vec!["/valid/path".to_string(), "   ".to_string()],
            ..SkillsConfig::default()
        },
        ..Config::default()
    };

    let error = config.validate().expect_err("validation should fail");
    assert!(error
        .to_string()
        .contains("skills.additional_paths cannot contain empty entries"));
}

#[test]
fn test_skills_config_validation_rejects_empty_trust_store_path_when_set() {
    let config = Config {
        skills: SkillsConfig {
            trust_store_path: Some("   ".to_string()),
            ..SkillsConfig::default()
        },
        ..Config::default()
    };

    let error = config.validate().expect_err("validation should fail");
    assert!(error
        .to_string()
        .contains("skills.trust_store_path cannot be empty when set"));
}

#[test]
#[serial]
fn test_config_load_applies_skills_env_var_overrides() {
    let _enabled = EnvVarGuard::set("XZATOMA_SKILLS_ENABLED", "false");
    let _project_enabled = EnvVarGuard::set("XZATOMA_SKILLS_PROJECT_ENABLED", "false");
    let _user_enabled = EnvVarGuard::set("XZATOMA_SKILLS_USER_ENABLED", "false");
    let _activation = EnvVarGuard::set("XZATOMA_SKILLS_ACTIVATION_TOOL_ENABLED", "false");
    let _project_trust = EnvVarGuard::set("XZATOMA_SKILLS_PROJECT_TRUST_REQUIRED", "false");
    let _trust_store = EnvVarGuard::set(
        "XZATOMA_SKILLS_TRUST_STORE_PATH",
        "/tmp/xzatoma-skills-trust.yaml",
    );

    let (_temp_dir, config_path) = write_temp_config(
        r#"
provider:
  type: copilot
agent:
  max_turns: 50
skills:
  enabled: true
  project_enabled: true
  user_enabled: true
  activation_tool_enabled: true
  project_trust_required: true
  trust_store_path: ~/.xzatoma/skills_trust.yaml
"#,
    );

    let cli = auth_cli();
    let config = Config::load(&config_path, &cli).expect("config should load");

    assert!(!config.skills.enabled);
    assert!(!config.skills.project_enabled);
    assert!(!config.skills.user_enabled);
    assert!(!config.skills.activation_tool_enabled);
    assert!(!config.skills.project_trust_required);
    assert_eq!(
        config.skills.trust_store_path.as_deref(),
        Some("/tmp/xzatoma-skills-trust.yaml")
    );
}

#[test]
#[serial]
fn test_config_load_clears_trust_store_path_when_env_var_is_empty() {
    let _trust_store = EnvVarGuard::set("XZATOMA_SKILLS_TRUST_STORE_PATH", "");

    let (_temp_dir, config_path) = write_temp_config(
        r#"
provider:
  type: copilot
agent:
  max_turns: 50
skills:
  trust_store_path: ~/.xzatoma/skills_trust.yaml
"#,
    );

    let cli = auth_cli();
    let config = Config::load(&config_path, &cli).expect("config should load");

    assert_eq!(config.skills.trust_store_path, None);
}

#[test]
#[serial]
fn test_config_load_ignores_invalid_boolean_skills_env_values() {
    let _enabled = EnvVarGuard::set("XZATOMA_SKILLS_ENABLED", "not-a-bool");
    let _project_enabled = EnvVarGuard::set("XZATOMA_SKILLS_PROJECT_ENABLED", "invalid");
    let _user_enabled = EnvVarGuard::set("XZATOMA_SKILLS_USER_ENABLED", "maybe");
    let _activation = EnvVarGuard::set("XZATOMA_SKILLS_ACTIVATION_TOOL_ENABLED", "sometimes");
    let _project_trust = EnvVarGuard::set("XZATOMA_SKILLS_PROJECT_TRUST_REQUIRED", "unknown");

    let (_temp_dir, config_path) = write_temp_config(
        r#"
provider:
  type: copilot
agent:
  max_turns: 50
skills:
  enabled: false
  project_enabled: false
  user_enabled: true
  activation_tool_enabled: false
  project_trust_required: false
"#,
    );

    let cli = auth_cli();
    let config = Config::load(&config_path, &cli).expect("config should load");

    assert!(!config.skills.enabled);
    assert!(!config.skills.project_enabled);
    assert!(config.skills.user_enabled);
    assert!(!config.skills.activation_tool_enabled);
    assert!(!config.skills.project_trust_required);
}

#[test]
#[serial]
fn test_config_load_defaults_skills_when_file_missing() {
    let _enabled = EnvVarGuard::remove("XZATOMA_SKILLS_ENABLED");
    let _project_enabled = EnvVarGuard::remove("XZATOMA_SKILLS_PROJECT_ENABLED");
    let _user_enabled = EnvVarGuard::remove("XZATOMA_SKILLS_USER_ENABLED");
    let _activation = EnvVarGuard::remove("XZATOMA_SKILLS_ACTIVATION_TOOL_ENABLED");
    let _project_trust = EnvVarGuard::remove("XZATOMA_SKILLS_PROJECT_TRUST_REQUIRED");
    let _trust_store = EnvVarGuard::remove("XZATOMA_SKILLS_TRUST_STORE_PATH");

    let cli = skills_cli(SkillsCommand::List);
    let config = Config::load("this-file-should-not-exist.yaml", &cli).expect("config should load");

    assert_eq!(config.skills.enabled, SkillsConfig::default().enabled);
    assert_eq!(
        config.skills.project_enabled,
        SkillsConfig::default().project_enabled
    );
    assert_eq!(
        config.skills.user_enabled,
        SkillsConfig::default().user_enabled
    );
    assert_eq!(
        config.skills.activation_tool_enabled,
        SkillsConfig::default().activation_tool_enabled
    );
    assert_eq!(
        config.skills.project_trust_required,
        SkillsConfig::default().project_trust_required
    );
    assert_eq!(
        config.skills.trust_store_path,
        SkillsConfig::default().trust_store_path
    );
}

#[test]
fn test_skills_config_roundtrip_yaml_serialization() {
    let original = SkillsConfig {
        enabled: false,
        project_enabled: true,
        user_enabled: false,
        additional_paths: vec![
            "/opt/xzatoma/skills".to_string(),
            "./local_skills".to_string(),
        ],
        max_discovered_skills: 12,
        max_scan_directories: 99,
        max_scan_depth: 2,
        catalog_max_entries: 10,
        activation_tool_enabled: false,
        project_trust_required: false,
        trust_store_path: Some("~/.xzatoma/custom_trust.yaml".to_string()),
        allow_custom_paths_without_trust: true,
        strict_frontmatter: false,
    };

    let yaml = serde_yaml::to_string(&original).expect("serialization should succeed");
    let restored: SkillsConfig =
        serde_yaml::from_str(&yaml).expect("deserialization should succeed");

    assert_eq!(restored.enabled, original.enabled);
    assert_eq!(restored.project_enabled, original.project_enabled);
    assert_eq!(restored.user_enabled, original.user_enabled);
    assert_eq!(restored.additional_paths, original.additional_paths);
    assert_eq!(
        restored.max_discovered_skills,
        original.max_discovered_skills
    );
    assert_eq!(restored.max_scan_directories, original.max_scan_directories);
    assert_eq!(restored.max_scan_depth, original.max_scan_depth);
    assert_eq!(restored.catalog_max_entries, original.catalog_max_entries);
    assert_eq!(
        restored.activation_tool_enabled,
        original.activation_tool_enabled
    );
    assert_eq!(
        restored.project_trust_required,
        original.project_trust_required
    );
    assert_eq!(restored.trust_store_path, original.trust_store_path);
    assert_eq!(
        restored.allow_custom_paths_without_trust,
        original.allow_custom_paths_without_trust
    );
    assert_eq!(restored.strict_frontmatter, original.strict_frontmatter);
}

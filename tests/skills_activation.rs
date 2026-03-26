use serial_test::serial;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use tempfile::TempDir;

use xzatoma::agent::Conversation;
use xzatoma::commands::{
    build_active_skill_prompt_injection, build_visible_skill_catalog, register_activate_skill_tool,
};
use xzatoma::config::{Config, SkillsConfig};
use xzatoma::providers::Message;
use xzatoma::skills::activation::{ActivationStatus, ActiveSkillRegistry};
use xzatoma::skills::catalog::SkillCatalog;
use xzatoma::skills::disclosure::visible_skill_records;
use xzatoma::skills::types::{SkillMetadata, SkillRecord, SkillSourceScope};
use xzatoma::tools::activate_skill::ActivateSkillTool;
use xzatoma::tools::{ToolExecutor, ToolRegistry};

fn write_skill(
    root: &Path,
    relative_dir: &str,
    name: &str,
    description: &str,
    body: &str,
) -> PathBuf {
    let skill_dir = root.join(relative_dir);
    std::fs::create_dir_all(&skill_dir).expect("failed to create skill directory");

    let skill_file = skill_dir.join("SKILL.md");
    let contents = format!(
        "---\nname: {name}\ndescription: {description}\nallowed-tools: read_file, grep\n---\n{body}\n"
    );

    std::fs::write(&skill_file, contents).expect("failed to write skill file");
    skill_file
}

fn sample_record(
    name: &str,
    description: &str,
    scope: SkillSourceScope,
    dir: &str,
    file: &str,
    source_order: usize,
    body: &str,
) -> SkillRecord {
    SkillRecord {
        metadata: SkillMetadata {
            name: name.to_string(),
            description: description.to_string(),
            license: None,
            compatibility: None,
            metadata: BTreeMap::new(),
            allowed_tools_raw: Some("read_file, grep".to_string()),
            allowed_tools: vec!["read_file".to_string(), "grep".to_string()],
        },
        skill_dir: PathBuf::from(dir),
        skill_file: PathBuf::from(file),
        source_scope: scope,
        source_order,
        body: body.to_string(),
    }
}

fn catalog_from_records(records: Vec<SkillRecord>) -> SkillCatalog {
    SkillCatalog::from_records(records).expect("catalog construction should succeed")
}

fn activate_tool_from_catalog(
    catalog: SkillCatalog,
    visible_skill_names: Vec<&str>,
    registry: Arc<Mutex<ActiveSkillRegistry>>,
) -> ActivateSkillTool {
    ActivateSkillTool::new(
        Arc::new(catalog),
        registry,
        visible_skill_names
            .into_iter()
            .map(ToOwned::to_owned)
            .collect(),
    )
}

fn config_with_skills(skills: SkillsConfig) -> Config {
    Config {
        skills,
        ..Config::default()
    }
}

#[test]
fn test_active_skill_registry_activation_succeeds_for_valid_visible_skill() {
    let catalog = catalog_from_records(vec![sample_record(
        "example_skill",
        "Example description",
        SkillSourceScope::UserClientSpecific,
        "/tmp/example_skill",
        "/tmp/example_skill/SKILL.md",
        0,
        "Use this skill body",
    )]);

    let mut registry = ActiveSkillRegistry::new();
    let status = registry
        .activate(&catalog, "example_skill")
        .expect("activation should succeed");

    assert!(matches!(status, ActivationStatus::Activated(_)));
    assert!(registry.is_active("example_skill"));
    assert_eq!(registry.len(), 1);

    let active = registry
        .get("example_skill")
        .expect("activated skill should be present");
    assert_eq!(active.skill_name, "example_skill");
    assert_eq!(active.description, "Example description");
    assert_eq!(active.allowed_tools, vec!["read_file", "grep"]);
    assert_eq!(active.body_content, "Use this skill body");
}

#[test]
fn test_active_skill_registry_duplicate_activation_does_not_duplicate_registry_state() {
    let catalog = catalog_from_records(vec![sample_record(
        "example_skill",
        "Example description",
        SkillSourceScope::UserClientSpecific,
        "/tmp/example_skill",
        "/tmp/example_skill/SKILL.md",
        0,
        "Use this skill body",
    )]);

    let mut registry = ActiveSkillRegistry::new();

    let first = registry
        .activate(&catalog, "example_skill")
        .expect("first activation should succeed");
    let second = registry
        .activate(&catalog, "example_skill")
        .expect("second activation should succeed");

    assert!(matches!(first, ActivationStatus::Activated(_)));
    assert!(matches!(second, ActivationStatus::AlreadyActive(_)));
    assert_eq!(registry.len(), 1);

    let names = registry.names();
    assert_eq!(names, vec!["example_skill"]);
}

#[test]
fn test_active_skill_registry_activation_fails_for_missing_skill() {
    let catalog = catalog_from_records(vec![sample_record(
        "example_skill",
        "Example description",
        SkillSourceScope::UserClientSpecific,
        "/tmp/example_skill",
        "/tmp/example_skill/SKILL.md",
        0,
        "Use this skill body",
    )]);

    let mut registry = ActiveSkillRegistry::new();
    let result = registry.activate(&catalog, "missing_skill");

    assert!(result.is_err());
    assert!(registry.is_empty());
}

#[test]
fn test_active_skill_registry_activation_fails_for_invalid_skill_not_in_catalog() {
    let valid_catalog = catalog_from_records(vec![sample_record(
        "valid_skill",
        "Valid description",
        SkillSourceScope::UserClientSpecific,
        "/tmp/valid_skill",
        "/tmp/valid_skill/SKILL.md",
        0,
        "Valid body",
    )]);

    let mut registry = ActiveSkillRegistry::new();
    let result = registry.activate(&valid_catalog, "invalid_skill");

    assert!(result.is_err());
    assert!(!registry.is_active("invalid_skill"));
    assert_eq!(registry.len(), 0);
}

#[test]
#[serial]
fn test_active_skill_registry_activation_fails_for_untrusted_project_skill() {
    let project_dir = TempDir::new().expect("failed to create project temp dir");
    let skill_file = write_skill(
        &project_dir.path().join(".xzatoma").join("skills"),
        "project_skill",
        "project_skill",
        "Project description",
        "Project body",
    );

    let catalog = catalog_from_records(vec![sample_record(
        "project_skill",
        "Project description",
        SkillSourceScope::ProjectClientSpecific,
        skill_file
            .parent()
            .expect("skill file should have parent")
            .to_string_lossy()
            .as_ref(),
        skill_file.to_string_lossy().as_ref(),
        0,
        "Project body",
    )]);

    let skills_config = SkillsConfig {
        project_trust_required: true,
        ..SkillsConfig::default()
    };

    let visible = visible_skill_records(
        &catalog,
        &skills_config,
        project_dir.path(),
        &std::collections::BTreeSet::new(),
    );

    assert!(visible.is_empty());

    let registry = Arc::new(Mutex::new(ActiveSkillRegistry::new()));
    let tool = activate_tool_from_catalog(catalog, Vec::new(), Arc::clone(&registry));

    let result = futures::executor::block_on(tool.execute(serde_json::json!({
        "skill_name": "project_skill"
    })));

    assert!(result.is_err());
    assert!(registry
        .lock()
        .expect("registry lock should succeed")
        .is_empty());
}

#[test]
fn test_active_skill_registry_render_for_prompt_injection_contains_active_skill_content() {
    let catalog = catalog_from_records(vec![
        sample_record(
            "alpha_skill",
            "Alpha description",
            SkillSourceScope::UserClientSpecific,
            "/tmp/alpha_skill",
            "/tmp/alpha_skill/SKILL.md",
            0,
            "Alpha body",
        ),
        sample_record(
            "beta_skill",
            "Beta description",
            SkillSourceScope::UserClientSpecific,
            "/tmp/beta_skill",
            "/tmp/beta_skill/SKILL.md",
            0,
            "Beta body",
        ),
    ]);

    let mut registry = ActiveSkillRegistry::new();
    registry
        .activate(&catalog, "beta_skill")
        .expect("activation should succeed");
    registry
        .activate(&catalog, "alpha_skill")
        .expect("activation should succeed");

    let injected = registry
        .render_for_prompt_injection()
        .expect("injected prompt should exist");

    assert!(injected.contains("## Active Skills"));
    assert!(injected.contains("alpha_skill"));
    assert!(injected.contains("beta_skill"));
    assert!(injected.contains("Alpha body"));
    assert!(injected.contains("Beta body"));
}

#[test]
fn test_build_active_skill_prompt_injection_returns_none_when_registry_is_empty() {
    let registry = Arc::new(Mutex::new(ActiveSkillRegistry::new()));
    let prompt = build_active_skill_prompt_injection(&registry)
        .expect("prompt injection build should succeed");

    assert!(prompt.is_none());
}

#[test]
fn test_build_active_skill_prompt_injection_returns_prompt_after_activation() {
    let catalog = catalog_from_records(vec![sample_record(
        "example_skill",
        "Example description",
        SkillSourceScope::UserClientSpecific,
        "/tmp/example_skill",
        "/tmp/example_skill/SKILL.md",
        0,
        "Use this skill body",
    )]);

    let mut active_registry = ActiveSkillRegistry::new();
    active_registry
        .activate(&catalog, "example_skill")
        .expect("activation should succeed");

    let registry = Arc::new(Mutex::new(active_registry));
    let prompt = build_active_skill_prompt_injection(&registry)
        .expect("prompt injection build should succeed")
        .expect("prompt injection should exist");

    assert!(prompt.contains("## Active Skills"));
    assert!(prompt.contains("example_skill"));
    assert!(prompt.contains("Use this skill body"));
}

#[test]
#[serial]
fn test_register_activate_skill_tool_not_registered_when_no_valid_visible_skills() {
    let config = config_with_skills(SkillsConfig {
        activation_tool_enabled: true,
        ..SkillsConfig::default()
    });

    let visible_catalog = SkillCatalog::new();
    let active_registry = Arc::new(Mutex::new(ActiveSkillRegistry::new()));
    let mut tools = ToolRegistry::new();

    let registered = register_activate_skill_tool(
        &mut tools,
        &config,
        visible_catalog,
        Arc::clone(&active_registry),
    )
    .expect("tool registration should succeed");

    assert!(!registered);
    assert!(tools.get("activate_skill").is_none());
}

#[test]
#[serial]
fn test_register_activate_skill_tool_not_registered_when_feature_disabled() {
    let config = config_with_skills(SkillsConfig {
        activation_tool_enabled: false,
        ..SkillsConfig::default()
    });

    let visible_catalog = catalog_from_records(vec![sample_record(
        "example_skill",
        "Example description",
        SkillSourceScope::UserClientSpecific,
        "/tmp/example_skill",
        "/tmp/example_skill/SKILL.md",
        0,
        "Use this skill body",
    )]);

    let active_registry = Arc::new(Mutex::new(ActiveSkillRegistry::new()));
    let mut tools = ToolRegistry::new();

    let registered = register_activate_skill_tool(
        &mut tools,
        &config,
        visible_catalog,
        Arc::clone(&active_registry),
    )
    .expect("tool registration should succeed");

    assert!(!registered);
    assert!(tools.get("activate_skill").is_none());
}

#[test]
#[serial]
fn test_register_activate_skill_tool_registers_when_visible_skills_exist() {
    let config = config_with_skills(SkillsConfig {
        activation_tool_enabled: true,
        ..SkillsConfig::default()
    });

    let visible_catalog = catalog_from_records(vec![sample_record(
        "example_skill",
        "Example description",
        SkillSourceScope::UserClientSpecific,
        "/tmp/example_skill",
        "/tmp/example_skill/SKILL.md",
        0,
        "Use this skill body",
    )]);

    let active_registry = Arc::new(Mutex::new(ActiveSkillRegistry::new()));
    let mut tools = ToolRegistry::new();

    let registered = register_activate_skill_tool(
        &mut tools,
        &config,
        visible_catalog,
        Arc::clone(&active_registry),
    )
    .expect("tool registration should succeed");

    assert!(registered);
    assert!(tools.get("activate_skill").is_some());
}

#[test]
#[serial]
fn test_build_visible_skill_catalog_excludes_untrusted_project_skills() {
    let project_dir = TempDir::new().expect("failed to create project temp dir");
    let skill_file = write_skill(
        &project_dir.path().join(".xzatoma").join("skills"),
        "project_skill",
        "project_skill",
        "Project description",
        "Project body",
    );

    let config = Config {
        skills: SkillsConfig {
            project_trust_required: true,
            ..SkillsConfig::default()
        },
        ..Config::default()
    };

    let visible_catalog = build_visible_skill_catalog(&config, project_dir.path())
        .expect("catalog build should work");

    assert!(visible_catalog.is_empty());

    let expected_path = canonicalize_for_assertion(&skill_file);
    assert_ne!(
        visible_catalog
            .get("project_skill")
            .map(|record| &record.skill_file),
        Some(&expected_path)
    );
}

#[test]
#[serial]
fn test_build_visible_skill_catalog_includes_visible_user_skill() {
    let project_dir = TempDir::new().expect("failed to create project temp dir");
    let home_dir = TempDir::new().expect("failed to create home temp dir");

    let original_home = std::env::var("HOME").ok();
    // SAFETY: test-scoped env mutation
    unsafe {
        std::env::set_var("HOME", home_dir.path());
    }

    write_skill(
        &home_dir.path().join(".xzatoma").join("skills"),
        "user_skill",
        "user_skill",
        "User description",
        "User body",
    );

    let config = Config::default();
    let visible_catalog = build_visible_skill_catalog(&config, project_dir.path())
        .expect("catalog build should succeed");

    assert_eq!(visible_catalog.len(), 1);
    let skill = visible_catalog
        .get("user_skill")
        .expect("user skill should be visible");
    assert_eq!(skill.metadata.description, "User description");

    match original_home {
        Some(value) => {
            // SAFETY: test-scoped env restore
            unsafe {
                std::env::set_var("HOME", value);
            }
        }
        None => {
            // SAFETY: test-scoped env restore
            unsafe {
                std::env::remove_var("HOME");
            }
        }
    }
}

#[test]
fn test_transient_active_skill_prompt_injection_does_not_pollute_conversation_messages() {
    let catalog = catalog_from_records(vec![sample_record(
        "example_skill",
        "Example description",
        SkillSourceScope::UserClientSpecific,
        "/tmp/example_skill",
        "/tmp/example_skill/SKILL.md",
        0,
        "Use this skill body",
    )]);

    let mut active_registry = ActiveSkillRegistry::new();
    active_registry
        .activate(&catalog, "example_skill")
        .expect("activation should succeed");

    let registry = Arc::new(Mutex::new(active_registry));
    let injected_prompt = build_active_skill_prompt_injection(&registry)
        .expect("prompt injection build should succeed")
        .expect("prompt injection should exist");

    let mut conversation = Conversation::new(8000, 5, 0.8);
    conversation.add_system_message("Base system prompt");
    conversation.add_user_message("Solve the task");

    let original_messages = conversation.messages().to_vec();
    let transient_messages = {
        let mut messages = conversation.messages().to_vec();
        messages.insert(1, Message::system(injected_prompt.clone()));
        messages
    };

    assert_eq!(conversation.messages().len(), original_messages.len());
    assert_eq!(conversation.messages()[0].role, "system");
    assert_eq!(conversation.messages()[1].role, "user");
    assert!(!conversation
        .messages()
        .iter()
        .filter_map(|message| message.content.as_deref())
        .any(|content| content.contains("## Active Skills")));

    assert_eq!(transient_messages.len(), original_messages.len() + 1);
    assert_eq!(transient_messages[1].role, "system");
    assert!(transient_messages[1]
        .content
        .as_deref()
        .map(|content| content.contains("## Active Skills"))
        .unwrap_or(false));
    assert!(transient_messages[1]
        .content
        .as_deref()
        .map(|content| content.contains("example_skill"))
        .unwrap_or(false));
    assert_eq!(conversation.messages().len(), 2);
}

#[test]
fn test_transient_active_skill_prompt_injection_adds_only_one_synthetic_message() {
    let catalog = catalog_from_records(vec![
        sample_record(
            "alpha_skill",
            "Alpha description",
            SkillSourceScope::UserClientSpecific,
            "/tmp/alpha_skill",
            "/tmp/alpha_skill/SKILL.md",
            0,
            "Alpha body",
        ),
        sample_record(
            "beta_skill",
            "Beta description",
            SkillSourceScope::UserClientSpecific,
            "/tmp/beta_skill",
            "/tmp/beta_skill/SKILL.md",
            0,
            "Beta body",
        ),
    ]);

    let mut active_registry = ActiveSkillRegistry::new();
    active_registry
        .activate(&catalog, "alpha_skill")
        .expect("first activation should succeed");
    active_registry
        .activate(&catalog, "beta_skill")
        .expect("second activation should succeed");

    let registry = Arc::new(Mutex::new(active_registry));
    let injected_prompt = build_active_skill_prompt_injection(&registry)
        .expect("prompt injection build should succeed")
        .expect("prompt injection should exist");

    let mut conversation = Conversation::new(8000, 5, 0.8);
    conversation.add_system_message("Base system prompt");
    conversation.add_user_message("Solve the task");
    conversation.add_assistant_message("Initial response");

    let original_len = conversation.messages().len();

    let transient_messages = {
        let mut messages = conversation.messages().to_vec();
        messages.insert(1, Message::system(injected_prompt.clone()));
        messages
    };

    assert_eq!(original_len, 3);
    assert_eq!(conversation.messages().len(), original_len);
    assert_eq!(transient_messages.len(), original_len + 1);

    let synthetic_system_messages = transient_messages
        .iter()
        .filter(|message| message.role == "system")
        .count();
    assert_eq!(synthetic_system_messages, 2);

    assert!(transient_messages[1]
        .content
        .as_deref()
        .map(|content| content.contains("alpha_skill"))
        .unwrap_or(false));
    assert!(transient_messages[1]
        .content
        .as_deref()
        .map(|content| content.contains("beta_skill"))
        .unwrap_or(false));

    assert!(!conversation
        .messages()
        .iter()
        .filter_map(|message| message.content.as_deref())
        .any(|content| content.contains("alpha_skill") && content.contains("beta_skill")));
}

fn canonicalize_for_assertion(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

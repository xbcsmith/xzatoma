use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use xzatoma::commands::build_active_skill_prompt_injection;
use xzatoma::skills::activation::ActiveSkillRegistry;
use xzatoma::skills::catalog::SkillCatalog;
use xzatoma::skills::types::{SkillMetadata, SkillRecord, SkillSourceScope};

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

#[test]
fn test_active_skills_do_not_persist_across_restarted_runtime_instances() {
    let catalog = catalog_from_records(vec![sample_record(
        "example_skill",
        "Example description",
        SkillSourceScope::UserClientSpecific,
        "/tmp/example_skill",
        "/tmp/example_skill/SKILL.md",
        0,
        "Use this skill body",
    )]);

    let mut first_runtime_registry = ActiveSkillRegistry::new();
    first_runtime_registry
        .activate(&catalog, "example_skill")
        .expect("activation should succeed in first runtime");

    assert!(first_runtime_registry.is_active("example_skill"));
    assert_eq!(first_runtime_registry.len(), 1);

    let restarted_runtime_registry = ActiveSkillRegistry::new();

    assert!(!restarted_runtime_registry.is_active("example_skill"));
    assert!(restarted_runtime_registry.is_empty());
    assert_eq!(restarted_runtime_registry.len(), 0);
    assert!(restarted_runtime_registry
        .render_for_prompt_injection()
        .is_none());
}

#[test]
fn test_prompt_injection_repeats_correctly_across_multiple_turns_in_one_session() {
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

    let mut runtime_registry = ActiveSkillRegistry::new();
    runtime_registry
        .activate(&catalog, "alpha_skill")
        .expect("alpha activation should succeed");
    runtime_registry
        .activate(&catalog, "beta_skill")
        .expect("beta activation should succeed");

    let shared_registry = Arc::new(Mutex::new(runtime_registry));

    let first_turn_prompt = build_active_skill_prompt_injection(&shared_registry)
        .expect("first turn prompt build should succeed")
        .expect("first turn prompt should exist");

    let second_turn_prompt = build_active_skill_prompt_injection(&shared_registry)
        .expect("second turn prompt build should succeed")
        .expect("second turn prompt should exist");

    let third_turn_prompt = build_active_skill_prompt_injection(&shared_registry)
        .expect("third turn prompt build should succeed")
        .expect("third turn prompt should exist");

    assert_eq!(first_turn_prompt, second_turn_prompt);
    assert_eq!(second_turn_prompt, third_turn_prompt);

    assert!(first_turn_prompt.contains("## Active Skills"));
    assert!(first_turn_prompt.contains("alpha_skill"));
    assert!(first_turn_prompt.contains("beta_skill"));
    assert!(first_turn_prompt.contains("Alpha body"));
    assert!(first_turn_prompt.contains("Beta body"));
}

#[test]
fn test_prompt_injection_updates_after_additional_activation_in_same_session() {
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

    let mut runtime_registry = ActiveSkillRegistry::new();
    runtime_registry
        .activate(&catalog, "alpha_skill")
        .expect("alpha activation should succeed");

    let shared_registry = Arc::new(Mutex::new(runtime_registry));

    let initial_prompt = build_active_skill_prompt_injection(&shared_registry)
        .expect("initial prompt build should succeed")
        .expect("initial prompt should exist");

    assert!(initial_prompt.contains("alpha_skill"));
    assert!(!initial_prompt.contains("beta_skill"));
    assert!(initial_prompt.contains("Alpha body"));
    assert!(!initial_prompt.contains("Beta body"));

    {
        let mut locked = shared_registry
            .lock()
            .expect("registry lock should succeed for additional activation");
        locked
            .activate(&catalog, "beta_skill")
            .expect("beta activation should succeed");
    }

    let updated_prompt = build_active_skill_prompt_injection(&shared_registry)
        .expect("updated prompt build should succeed")
        .expect("updated prompt should exist");

    assert!(updated_prompt.contains("alpha_skill"));
    assert!(updated_prompt.contains("beta_skill"));
    assert!(updated_prompt.contains("Alpha body"));
    assert!(updated_prompt.contains("Beta body"));
    assert_ne!(initial_prompt, updated_prompt);
}

#[test]
fn test_prompt_injection_stops_after_registry_clear_in_same_session() {
    let catalog = catalog_from_records(vec![sample_record(
        "example_skill",
        "Example description",
        SkillSourceScope::UserClientSpecific,
        "/tmp/example_skill",
        "/tmp/example_skill/SKILL.md",
        0,
        "Use this skill body",
    )]);

    let mut runtime_registry = ActiveSkillRegistry::new();
    runtime_registry
        .activate(&catalog, "example_skill")
        .expect("activation should succeed");

    let shared_registry = Arc::new(Mutex::new(runtime_registry));

    let before_clear = build_active_skill_prompt_injection(&shared_registry)
        .expect("prompt build before clear should succeed")
        .expect("prompt should exist before clear");

    assert!(before_clear.contains("example_skill"));
    assert!(before_clear.contains("Use this skill body"));

    {
        let mut locked = shared_registry
            .lock()
            .expect("registry lock should succeed for clear");
        locked.clear();
    }

    let after_clear = build_active_skill_prompt_injection(&shared_registry)
        .expect("prompt build after clear should succeed");

    assert!(after_clear.is_none());
}

#[test]
fn test_resumed_conversation_state_does_not_restore_active_skills() {
    let catalog = catalog_from_records(vec![sample_record(
        "example_skill",
        "Example description",
        SkillSourceScope::UserClientSpecific,
        "/tmp/example_skill",
        "/tmp/example_skill/SKILL.md",
        0,
        "Use this skill body",
    )]);

    let mut original_session_registry = ActiveSkillRegistry::new();
    original_session_registry
        .activate(&catalog, "example_skill")
        .expect("activation should succeed in original session");

    let original_prompt = original_session_registry
        .render_for_prompt_injection()
        .expect("original session prompt should exist");
    assert!(original_prompt.contains("example_skill"));

    let resumed_session_registry = ActiveSkillRegistry::new();
    let resumed_prompt = resumed_session_registry.render_for_prompt_injection();

    assert!(resumed_prompt.is_none());
    assert!(!resumed_session_registry.is_active("example_skill"));
    assert!(resumed_session_registry.is_empty());
}

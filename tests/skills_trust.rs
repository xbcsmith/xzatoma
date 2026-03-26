use serial_test::serial;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;
use xzatoma::config::SkillsConfig;
use xzatoma::skills::trust::{
    default_trust_store_path, enumerate_skill_resources, expand_tilde_path, load_trusted_paths,
    resolve_skill_resource_path, resolve_trust_store_path, SkillTrustStore, SkillTrustStoreData,
};

fn write_file(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("parent directory should be created");
    }
    fs::write(path, contents).expect("file should be written");
}

fn canonicalize_for_assertion(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

#[test]
#[serial]
fn test_trust_add_show_remove() {
    let temp_dir = TempDir::new().expect("temp dir should exist");
    let trusted_dir = temp_dir.path().join("project");
    fs::create_dir_all(&trusted_dir).expect("trusted dir should exist");

    let store_path = temp_dir.path().join("skills_trust.yaml");
    let mut store = SkillTrustStore::new(store_path.clone()).expect("store should be created");

    assert!(store.trusted_paths().is_empty());

    let canonical = store
        .add_path(&trusted_dir)
        .expect("trust add should succeed");
    assert!(store.trusted_paths().contains(&canonical));
    assert!(store.path().ends_with("skills_trust.yaml"));

    let loaded = SkillTrustStore::load_or_create(store_path).expect("store should load");
    assert!(loaded.trusted_paths().contains(&canonical));
    assert!(loaded.is_trusted_path(&trusted_dir));

    let removed = store
        .remove_path(&trusted_dir)
        .expect("trust remove should succeed");
    assert!(removed);
    assert!(!store.is_trusted_path(&trusted_dir));
    assert!(store.trusted_paths().is_empty());
}

#[test]
#[serial]
fn test_trust_store_persists_and_replace_data_updates_store() {
    let temp_dir = TempDir::new().expect("temp dir should exist");
    let first_dir = temp_dir.path().join("first");
    let second_dir = temp_dir.path().join("second");
    fs::create_dir_all(&first_dir).expect("first dir should exist");
    fs::create_dir_all(&second_dir).expect("second dir should exist");

    let store_path = temp_dir.path().join("skills_trust.yaml");
    let mut store = SkillTrustStore::new(store_path.clone()).expect("store should be created");

    let first_canonical = store.add_path(&first_dir).expect("add should work");
    assert!(store.trusted_paths().contains(&first_canonical));

    let mut replacement = BTreeSet::new();
    replacement.insert(canonicalize_for_assertion(&second_dir));

    store
        .replace_data(SkillTrustStoreData {
            trusted_paths: replacement.clone(),
        })
        .expect("replace should succeed");

    let loaded = SkillTrustStore::load_or_create(store_path).expect("store should load");
    assert_eq!(loaded.trusted_paths(), &replacement);
    assert!(!loaded.is_trusted_path(&first_dir));
    assert!(loaded.is_trusted_path(&second_dir));
}

#[test]
#[serial]
fn test_trusted_project_path_inclusion_via_prefix_check() {
    let temp_dir = TempDir::new().expect("temp dir should exist");
    let project_root = temp_dir.path().join("project");
    let skill_path = project_root
        .join(".xzatoma")
        .join("skills")
        .join("demo_skill");
    fs::create_dir_all(&skill_path).expect("skill path should exist");

    let store_path = temp_dir.path().join("skills_trust.yaml");
    let mut store = SkillTrustStore::new(store_path).expect("store should be created");
    store
        .add_path(&project_root)
        .expect("project root should be trusted");

    assert!(store.is_trusted_path(&project_root));
    assert!(store.is_trusted_path(&skill_path));
}

#[test]
#[serial]
fn test_untrusted_project_path_omission_signal_via_load_trusted_paths() {
    let temp_dir = TempDir::new().expect("temp dir should exist");
    let project_root = temp_dir.path().join("project");
    fs::create_dir_all(&project_root).expect("project root should exist");

    let skills_config = SkillsConfig {
        trust_store_path: Some(
            temp_dir
                .path()
                .join("skills_trust.yaml")
                .to_string_lossy()
                .to_string(),
        ),
        project_trust_required: true,
        ..SkillsConfig::default()
    };

    let trusted_paths =
        load_trusted_paths(&skills_config, &project_root).expect("trusted paths should load");

    assert!(trusted_paths.is_empty());
    assert!(!trusted_paths.contains(&canonicalize_for_assertion(&project_root)));
}

#[test]
#[serial]
fn test_custom_path_trust_behavior() {
    let temp_dir = TempDir::new().expect("temp dir should exist");
    let custom_root = temp_dir.path().join("custom_skills");
    fs::create_dir_all(&custom_root).expect("custom root should exist");

    let store_path = temp_dir.path().join("skills_trust.yaml");
    let mut store = SkillTrustStore::new(store_path.clone()).expect("store should be created");

    assert!(!store.is_trusted_path(&custom_root));

    let trusted_custom = store
        .add_path(&custom_root)
        .expect("custom path should be trusted");
    assert!(store.is_trusted_path(&custom_root));

    let loaded = SkillTrustStore::load_or_create(store_path).expect("store should load");
    assert!(loaded.trusted_paths().contains(&trusted_custom));
    assert!(loaded.is_trusted_path(&custom_root));
}

#[test]
fn test_path_traversal_rejection() {
    let temp_dir = TempDir::new().expect("temp dir should exist");
    let skill_root = temp_dir.path().join("skill");
    fs::create_dir_all(skill_root.join("references")).expect("references dir should exist");

    let result = resolve_skill_resource_path(&skill_root, "../outside.txt");
    assert!(result.is_err());

    let absolute_result = resolve_skill_resource_path(&skill_root, "/etc/passwd");
    assert!(absolute_result.is_err());
}

#[test]
fn test_resource_enumeration_stays_within_skill_root() {
    let temp_dir = TempDir::new().expect("temp dir should exist");
    let skill_root = temp_dir.path().join("skill");

    write_file(&skill_root.join("scripts").join("build.sh"), "echo hi");
    write_file(&skill_root.join("references").join("guide.md"), "guide");
    write_file(&skill_root.join("assets").join("logo.png"), "png");
    write_file(&skill_root.join("other").join("ignored.txt"), "ignored");

    let resources = enumerate_skill_resources(&skill_root).expect("enumeration should succeed");
    let canonical_root = canonicalize_for_assertion(&skill_root);

    assert_eq!(resources.scripts.len(), 1);
    assert_eq!(resources.references.len(), 1);
    assert_eq!(resources.assets.len(), 1);

    let all = resources.all();
    assert_eq!(all.len(), 3);
    assert!(all.iter().all(|path| path.starts_with(&canonical_root)));
    assert!(!all.iter().any(|path| path.ends_with("ignored.txt")));
    assert!(!resources.is_empty());
}

#[test]
fn test_resource_resolution_from_skill_root_only() {
    let temp_dir = TempDir::new().expect("temp dir should exist");
    let skill_root = temp_dir.path().join("skill");
    let target = skill_root.join("references").join("guide.md");
    write_file(&target, "guide");

    let resolved = resolve_skill_resource_path(&skill_root, "references/guide.md")
        .expect("resource should resolve");
    assert_eq!(resolved, canonicalize_for_assertion(&target));
}

#[test]
#[serial]
fn test_default_trust_store_path_uses_home_directory() {
    let original_home = std::env::var("HOME").ok();
    let temp_dir = TempDir::new().expect("temp dir should exist");

    // SAFETY: test-scoped env mutation
    unsafe {
        std::env::set_var("HOME", temp_dir.path());
    }

    let result = default_trust_store_path().expect("default trust store path should resolve");

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

    assert!(result.starts_with(temp_dir.path()));
    assert!(result.to_string_lossy().contains(".xzatoma"));
    assert!(result.to_string_lossy().contains("skills_trust.yaml"));
}

#[test]
#[serial]
fn test_expand_tilde_and_resolve_trust_store_path() {
    let original_home = std::env::var("HOME").ok();
    let temp_dir = TempDir::new().expect("temp dir should exist");

    // SAFETY: test-scoped env mutation
    unsafe {
        std::env::set_var("HOME", temp_dir.path());
    }

    let expanded =
        expand_tilde_path("~/custom/skills_trust.yaml").expect("tilde expansion should work");
    let resolved = resolve_trust_store_path(Some("~/custom/skills_trust.yaml"))
        .expect("configured trust store path should resolve");
    let default_resolved =
        resolve_trust_store_path(None).expect("default trust store path should resolve");

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

    assert_eq!(
        expanded,
        temp_dir.path().join("custom").join("skills_trust.yaml")
    );
    assert_eq!(resolved, expanded);
    assert!(default_resolved.ends_with(".xzatoma/skills_trust.yaml"));
}

#[test]
#[serial]
fn test_load_trusted_paths_reads_persistent_store() {
    let temp_dir = TempDir::new().expect("temp dir should exist");
    let trusted_root = temp_dir.path().join("project");
    fs::create_dir_all(&trusted_root).expect("trusted root should exist");

    let store_path = temp_dir.path().join("skills_trust.yaml");
    let mut store = SkillTrustStore::new(store_path.clone()).expect("store should be created");
    let canonical = store
        .add_path(&trusted_root)
        .expect("trusted path should be added");

    let config = SkillsConfig {
        trust_store_path: Some(store_path.to_string_lossy().to_string()),
        ..SkillsConfig::default()
    };

    let loaded =
        load_trusted_paths(&config, temp_dir.path()).expect("trusted paths should load correctly");
    assert!(loaded.contains(&canonical));
}

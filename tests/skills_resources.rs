use std::fs;
use std::path::{Path, PathBuf};

use tempfile::TempDir;
use xzatoma::skills::trust::{
    enumerate_skill_resources, resolve_skill_resource_path, SkillResources,
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

fn create_skill_root(temp_dir: &TempDir) -> PathBuf {
    let skill_root = temp_dir.path().join("example_skill");
    fs::create_dir_all(&skill_root).expect("skill root should be created");
    skill_root
}

#[test]
fn test_resolve_skill_resource_path_with_valid_reference_file() {
    let temp_dir = TempDir::new().expect("temp dir should be created");
    let skill_root = create_skill_root(&temp_dir);
    let guide_path = skill_root.join("references").join("guide.md");
    write_file(&guide_path, "# Guide");

    let resolved = resolve_skill_resource_path(&skill_root, "references/guide.md")
        .expect("resource path should resolve");

    assert_eq!(resolved, canonicalize_for_assertion(&guide_path));
    assert!(resolved.starts_with(canonicalize_for_assertion(&skill_root)));
}

#[test]
fn test_resolve_skill_resource_path_with_valid_script_file() {
    let temp_dir = TempDir::new().expect("temp dir should be created");
    let skill_root = create_skill_root(&temp_dir);
    let script_path = skill_root.join("scripts").join("build.sh");
    write_file(&script_path, "echo build");

    let resolved = resolve_skill_resource_path(&skill_root, "scripts/build.sh")
        .expect("script resource path should resolve");

    assert_eq!(resolved, canonicalize_for_assertion(&script_path));
    assert!(resolved.starts_with(canonicalize_for_assertion(&skill_root)));
}

#[test]
fn test_resolve_skill_resource_path_rejects_absolute_path() {
    let temp_dir = TempDir::new().expect("temp dir should be created");
    let skill_root = create_skill_root(&temp_dir);

    let result = resolve_skill_resource_path(&skill_root, "/etc/passwd");

    assert!(result.is_err());
    let message = result
        .err()
        .map(|error| error.to_string())
        .unwrap_or_default();
    assert!(message.contains("relative") || message.contains("skill root"));
}

#[test]
fn test_resolve_skill_resource_path_rejects_parent_directory_traversal() {
    let temp_dir = TempDir::new().expect("temp dir should be created");
    let skill_root = create_skill_root(&temp_dir);

    let result = resolve_skill_resource_path(&skill_root, "../outside.txt");

    assert!(result.is_err());
    let message = result
        .err()
        .map(|error| error.to_string())
        .unwrap_or_default();
    assert!(message.contains("traversal") || message.contains("skill root"));
}

#[test]
fn test_resolve_skill_resource_path_rejects_nested_parent_directory_traversal() {
    let temp_dir = TempDir::new().expect("temp dir should be created");
    let skill_root = create_skill_root(&temp_dir);

    let result = resolve_skill_resource_path(&skill_root, "references/../../outside.txt");

    assert!(result.is_err());
    let message = result
        .err()
        .map(|error| error.to_string())
        .unwrap_or_default();
    assert!(message.contains("traversal") || message.contains("skill root"));
}

#[test]
fn test_enumerate_skill_resources_returns_empty_for_missing_supported_directories() {
    let temp_dir = TempDir::new().expect("temp dir should be created");
    let skill_root = create_skill_root(&temp_dir);

    let resources =
        enumerate_skill_resources(&skill_root).expect("resource enumeration should succeed");

    assert_eq!(resources, SkillResources::default());
    assert!(resources.is_empty());
    assert!(resources.all().is_empty());
}

#[test]
fn test_enumerate_skill_resources_collects_supported_directories_only() {
    let temp_dir = TempDir::new().expect("temp dir should be created");
    let skill_root = create_skill_root(&temp_dir);

    let script_file = skill_root.join("scripts").join("build.sh");
    let reference_file = skill_root.join("references").join("guide.md");
    let asset_file = skill_root.join("assets").join("logo.png");
    let ignored_file = skill_root.join("notes").join("ignored.txt");

    write_file(&script_file, "echo build");
    write_file(&reference_file, "# Guide");
    write_file(&asset_file, "png");
    write_file(&ignored_file, "ignore me");

    let resources =
        enumerate_skill_resources(&skill_root).expect("resource enumeration should succeed");

    assert_eq!(resources.scripts.len(), 1);
    assert_eq!(resources.references.len(), 1);
    assert_eq!(resources.assets.len(), 1);

    let all = resources.all();
    assert_eq!(all.len(), 3);

    assert!(all.contains(&canonicalize_for_assertion(&script_file)));
    assert!(all.contains(&canonicalize_for_assertion(&reference_file)));
    assert!(all.contains(&canonicalize_for_assertion(&asset_file)));
    assert!(!all.contains(&canonicalize_for_assertion(&ignored_file)));
}

#[test]
fn test_enumerate_skill_resources_recurses_within_supported_directories() {
    let temp_dir = TempDir::new().expect("temp dir should be created");
    let skill_root = create_skill_root(&temp_dir);

    let nested_script = skill_root.join("scripts").join("ci").join("build.sh");
    let nested_reference = skill_root.join("references").join("docs").join("guide.md");
    let nested_asset = skill_root.join("assets").join("images").join("logo.png");

    write_file(&nested_script, "echo build");
    write_file(&nested_reference, "# Guide");
    write_file(&nested_asset, "png");

    let resources =
        enumerate_skill_resources(&skill_root).expect("resource enumeration should succeed");

    assert_eq!(
        resources.scripts,
        vec![canonicalize_for_assertion(&nested_script)]
    );
    assert_eq!(
        resources.references,
        vec![canonicalize_for_assertion(&nested_reference)]
    );
    assert_eq!(
        resources.assets,
        vec![canonicalize_for_assertion(&nested_asset)]
    );
}

#[test]
fn test_enumerate_skill_resources_stays_within_skill_root() {
    let temp_dir = TempDir::new().expect("temp dir should be created");
    let skill_root = create_skill_root(&temp_dir);

    let script_file = skill_root.join("scripts").join("build.sh");
    let reference_file = skill_root.join("references").join("guide.md");
    let asset_file = skill_root.join("assets").join("logo.png");

    write_file(&script_file, "echo build");
    write_file(&reference_file, "# Guide");
    write_file(&asset_file, "png");

    let resources =
        enumerate_skill_resources(&skill_root).expect("resource enumeration should succeed");
    let canonical_root = canonicalize_for_assertion(&skill_root);

    for path in resources.all() {
        assert!(path.starts_with(&canonical_root));
    }
}

#[test]
fn test_skill_resources_all_returns_grouped_paths_in_order() {
    let resources = SkillResources {
        scripts: vec![PathBuf::from("/tmp/skill/scripts/build.sh")],
        references: vec![PathBuf::from("/tmp/skill/references/guide.md")],
        assets: vec![PathBuf::from("/tmp/skill/assets/logo.png")],
    };

    let all = resources.all();

    assert_eq!(
        all,
        vec![
            PathBuf::from("/tmp/skill/scripts/build.sh"),
            PathBuf::from("/tmp/skill/references/guide.md"),
            PathBuf::from("/tmp/skill/assets/logo.png"),
        ]
    );
}

#[test]
fn test_skill_resources_is_empty_reflects_group_contents() {
    let empty = SkillResources::default();
    assert!(empty.is_empty());

    let non_empty = SkillResources {
        scripts: vec![PathBuf::from("/tmp/skill/scripts/build.sh")],
        references: Vec::new(),
        assets: Vec::new(),
    };
    assert!(!non_empty.is_empty());
}

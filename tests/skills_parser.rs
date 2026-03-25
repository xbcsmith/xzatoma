use std::path::PathBuf;

use xzatoma::skills::parser::{parse_frontmatter_map, parse_skill_content, split_frontmatter};
use xzatoma::skills::types::SkillSourceScope;
use xzatoma::skills::validation::{is_valid_skill_name, normalize_allowed_tools};

#[test]
fn test_parse_skill_content_with_valid_frontmatter() {
    let skill_path = PathBuf::from("/tmp/example_skill/SKILL.md");
    let parsed = parse_skill_content(
        &skill_path,
        SkillSourceScope::ProjectClientSpecific,
        "---\nname: example_skill\ndescription: Example skill\nlicense: MIT\ncompatibility: xzatoma >=0.2\nallowed-tools: read_file, grep\nmetadata:\n  owner: team-a\n---\n# Example\nBody\n",
    )
    .expect("expected valid skill content to parse");

    assert!(parsed.frontmatter_present);
    assert_eq!(parsed.name.as_deref(), Some("example_skill"));
    assert_eq!(parsed.description.as_deref(), Some("Example skill"));
    assert_eq!(parsed.license.as_deref(), Some("MIT"));
    assert_eq!(parsed.compatibility.as_deref(), Some("xzatoma >=0.2"));
    assert_eq!(parsed.allowed_tools_raw.as_deref(), Some("read_file, grep"));

    let metadata = parsed.metadata.expect("expected metadata map");
    assert_eq!(metadata.get("owner").map(String::as_str), Some("team-a"));
    assert_eq!(parsed.body_markdown.trim(), "# Example\nBody");
}

#[test]
fn test_parse_skill_content_without_frontmatter() {
    let skill_path = PathBuf::from("/tmp/example_skill/SKILL.md");
    let parsed = parse_skill_content(
        &skill_path,
        SkillSourceScope::UserClientSpecific,
        "# Example\nBody without frontmatter\n",
    )
    .expect("content without frontmatter should still parse as raw document");

    assert!(!parsed.frontmatter_present);
    assert!(parsed.name.is_none());
    assert!(parsed.description.is_none());
    assert!(parsed.license.is_none());
    assert!(parsed.compatibility.is_none());
    assert!(parsed.metadata.is_none());
    assert!(parsed.allowed_tools_raw.is_none());
    assert_eq!(
        parsed.body_markdown,
        "# Example\nBody without frontmatter\n"
    );
}

#[test]
fn test_parse_skill_content_with_malformed_frontmatter() {
    let skill_path = PathBuf::from("/tmp/example_skill/SKILL.md");
    let result = parse_skill_content(
        &skill_path,
        SkillSourceScope::ProjectSharedConvention,
        "---\nname: example_skill\ndescription: [unterminated\n---\n# Example\n",
    );

    assert!(result.is_err());
}

#[test]
fn test_split_frontmatter_with_valid_document() {
    let (present, frontmatter, body) =
        split_frontmatter("---\nname: example_skill\ndescription: Example\n---\n# Body\n")
            .expect("frontmatter split should succeed");

    assert!(present);
    assert!(frontmatter.contains("name: example_skill"));
    assert_eq!(body.trim(), "# Body");
}

#[test]
fn test_split_frontmatter_without_frontmatter() {
    let (present, frontmatter, body) =
        split_frontmatter("# Body only\n").expect("split should succeed");

    assert!(!present);
    assert_eq!(frontmatter, "");
    assert_eq!(body, "# Body only\n");
}

#[test]
fn test_split_frontmatter_with_missing_closing_delimiter() {
    let result = split_frontmatter("---\nname: example_skill\ndescription: Example\n# Body\n");

    assert!(result.is_err());
}

#[test]
fn test_parse_frontmatter_map_with_scalar_fields() {
    let parsed = parse_frontmatter_map(
        "name: example_skill\ndescription: Example\nlicense: MIT\ncompatibility: xzatoma >=0.2\n",
    )
    .expect("frontmatter map should parse");

    assert_eq!(
        parsed.get("name").map(String::as_str),
        Some("example_skill")
    );
    assert_eq!(
        parsed.get("description").map(String::as_str),
        Some("Example")
    );
    assert_eq!(parsed.get("license").map(String::as_str), Some("MIT"));
    assert_eq!(
        parsed.get("compatibility").map(String::as_str),
        Some("xzatoma >=0.2")
    );
}

#[test]
fn test_parse_frontmatter_map_with_sequence_fields() {
    let parsed = parse_frontmatter_map(
        "description: Example\ncompatibility:\n  - xzatoma >=0.2\n  - rust >=1.70\nallowed-tools:\n  - read_file\n  - grep\n",
    )
    .expect("sequence-based frontmatter should parse");

    assert_eq!(
        parsed.get("compatibility").map(String::as_str),
        Some("xzatoma >=0.2, rust >=1.70")
    );
    assert_eq!(
        parsed.get("allowed-tools").map(String::as_str),
        Some("read_file, grep")
    );
}

#[test]
fn test_parse_frontmatter_map_with_metadata_mapping() {
    let parsed =
        parse_frontmatter_map("description: Example\nmetadata:\n  owner: team-a\n  tier: core\n")
            .expect("metadata mapping should parse");

    assert_eq!(
        parsed.get("metadata").map(String::as_str),
        Some("owner=team-a\ntier=core")
    );
}

#[test]
fn test_parse_frontmatter_map_rejects_non_mapping_root() {
    let result = parse_frontmatter_map("- item1\n- item2\n");

    assert!(result.is_err());
}

#[test]
fn test_normalize_allowed_tools_from_raw_string() {
    let tools = normalize_allowed_tools(Some("read_file, grep\nlist_directory"));

    assert_eq!(tools, vec!["read_file", "grep", "list_directory"]);
}

#[test]
fn test_normalize_allowed_tools_with_none() {
    let tools = normalize_allowed_tools(None);

    assert!(tools.is_empty());
}

#[test]
fn test_is_valid_skill_name_accepts_supported_format() {
    assert!(is_valid_skill_name("example_skill"));
    assert!(is_valid_skill_name("skill2"));
    assert!(is_valid_skill_name("a"));
}

#[test]
fn test_is_valid_skill_name_rejects_unsupported_format() {
    assert!(!is_valid_skill_name("ExampleSkill"));
    assert!(!is_valid_skill_name("example-skill"));
    assert!(!is_valid_skill_name("2skill"));
    assert!(!is_valid_skill_name(""));
}

#[test]
fn test_parse_skill_content_preserves_empty_metadata_as_empty_map() {
    let skill_path = PathBuf::from("/tmp/example_skill/SKILL.md");
    let parsed = parse_skill_content(
        &skill_path,
        SkillSourceScope::CustomConfigured,
        "---\nname: example_skill\ndescription: Example\nmetadata: {}\n---\nBody\n",
    )
    .expect("expected parse to succeed");

    let metadata = parsed.metadata.unwrap_or_default();
    assert!(metadata.is_empty());
    assert_eq!(parsed.body_markdown.trim(), "Body");
}

//! Skill document parser for `SKILL.md` files.
//!
//! This module provides the Phase 1 parsing foundation for agent skills.
//! It parses `SKILL.md` files into a lightweight raw document model that is
//! later validated by the skills validation layer.

use crate::error::{Result, XzatomaError};
use crate::skills::types::{RawSkillDocument, SkillSourceScope};
use std::collections::BTreeMap;
use std::path::Path;

/// Parse a `SKILL.md` file from disk.
///
/// # Arguments
///
/// * `skill_file_path` - Absolute path to the `SKILL.md` file
/// * `source_scope` - Discovery scope the skill came from
///
/// # Returns
///
/// Returns a raw parsed skill document containing frontmatter fields and the
/// Markdown body.
///
/// # Errors
///
/// Returns an error if the file cannot be read or if the frontmatter cannot be
/// parsed.
///
/// # Examples
///
/// ```
/// use std::fs;
/// use tempfile::tempdir;
/// use xzatoma::skills::parser::parse_skill_file;
/// use xzatoma::skills::SkillSourceScope;
///
/// let temp_dir = tempdir().unwrap();
/// let skill_dir = temp_dir.path().join("demo_skill");
/// fs::create_dir_all(&skill_dir).unwrap();
/// fs::write(
///     skill_dir.join("SKILL.md"),
///     "---\nname: demo_skill\ndescription: Demo\n---\n# Demo\n",
/// )
/// .unwrap();
///
/// let parsed = parse_skill_file(
///     &skill_dir.join("SKILL.md"),
///     SkillSourceScope::ProjectClientSpecific,
/// )
/// .unwrap();
///
/// assert!(parsed.frontmatter_present);
/// assert_eq!(parsed.name.as_deref(), Some("demo_skill"));
/// assert_eq!(parsed.description.as_deref(), Some("Demo"));
/// assert_eq!(parsed.body_markdown.trim(), "# Demo");
/// ```
pub fn parse_skill_file(
    skill_file_path: &Path,
    source_scope: SkillSourceScope,
) -> Result<RawSkillDocument> {
    let content = std::fs::read_to_string(skill_file_path).map_err(|error| {
        XzatomaError::Config(format!(
            "Failed to read skill file '{}': {}",
            skill_file_path.display(),
            error
        ))
    })?;

    parse_skill_content(skill_file_path, source_scope, &content)
}

/// Parse `SKILL.md` content that is already loaded in memory.
///
/// # Arguments
///
/// * `skill_file_path` - Absolute path to the `SKILL.md` file
/// * `source_scope` - Discovery scope the skill came from
/// * `content` - Full file content
///
/// # Returns
///
/// Returns a raw parsed skill document.
///
/// # Errors
///
/// Returns an error if the YAML frontmatter is malformed.
///
/// # Examples
///
/// ```
/// use std::path::Path;
/// use xzatoma::skills::parser::parse_skill_content;
/// use xzatoma::skills::SkillSourceScope;
///
/// let parsed = parse_skill_content(
///     Path::new("/tmp/demo_skill/SKILL.md"),
///     SkillSourceScope::UserClientSpecific,
///     "---\nname: demo_skill\ndescription: Demo\n---\nHello\n",
/// )
/// .unwrap();
///
/// assert_eq!(parsed.name.as_deref(), Some("demo_skill"));
/// assert_eq!(parsed.body_markdown.trim(), "Hello");
/// ```
pub fn parse_skill_content(
    skill_file_path: &Path,
    source_scope: SkillSourceScope,
    content: &str,
) -> Result<RawSkillDocument> {
    let normalized = content.strip_prefix('\u{feff}').unwrap_or(content);

    let (frontmatter_present, frontmatter, body_markdown) = split_frontmatter(normalized)?;
    let parsed_frontmatter = if frontmatter_present {
        Some(parse_frontmatter_map(frontmatter)?)
    } else {
        None
    };

    let name = parsed_frontmatter
        .as_ref()
        .and_then(|map| map.get("name"))
        .cloned();
    let description = parsed_frontmatter
        .as_ref()
        .and_then(|map| map.get("description"))
        .cloned();
    let license = parsed_frontmatter
        .as_ref()
        .and_then(|map| map.get("license"))
        .cloned();
    let compatibility = parsed_frontmatter
        .as_ref()
        .and_then(|map| map.get("compatibility"))
        .cloned();
    let metadata = match parsed_frontmatter.as_ref() {
        Some(map) => parse_metadata_from_map(map)?,
        None => None,
    };
    let allowed_tools_raw = parsed_frontmatter
        .as_ref()
        .and_then(|map| map.get("allowed-tools"))
        .cloned();

    Ok(RawSkillDocument {
        skill_file_path: skill_file_path.to_path_buf(),
        source_scope,
        frontmatter_present,
        name,
        description,
        license,
        compatibility,
        metadata,
        allowed_tools_raw,
        body_markdown: body_markdown.to_string(),
    })
}

/// Split content into optional frontmatter and Markdown body.
///
/// Phase 1 requires invalid skills to be diagnosed rather than promoted into the
/// valid catalog. This parser therefore accepts content without frontmatter and
/// marks it as such so validation can reject it deterministically.
///
/// # Arguments
///
/// * `content` - Full file content
///
/// # Returns
///
/// Returns a tuple of:
///
/// - `frontmatter_present`
/// - `frontmatter_content`
/// - `body_markdown`
///
/// # Errors
///
/// Returns an error if a frontmatter opening delimiter exists but the
/// frontmatter is malformed or not closed.
///
/// # Examples
///
/// ```
/// use xzatoma::skills::parser::split_frontmatter;
///
/// let (present, frontmatter, body) = split_frontmatter(
///     "---\nname: demo_skill\ndescription: Demo\n---\n# Body\n"
/// )
/// .unwrap();
///
/// assert!(present);
/// assert!(frontmatter.contains("name: demo_skill"));
/// assert_eq!(body.trim(), "# Body");
/// ```
pub fn split_frontmatter(content: &str) -> Result<(bool, &str, &str)> {
    if !starts_with_frontmatter_delimiter(content) {
        return Ok((false, "", content));
    }

    let after_open = if let Some(rest) = content.strip_prefix("---\r\n") {
        rest
    } else if let Some(rest) = content.strip_prefix("---\n") {
        rest
    } else if let Some(rest) = content.strip_prefix("---") {
        rest
    } else {
        return Ok((false, "", content));
    };

    let close_offset = find_frontmatter_close(after_open).ok_or_else(|| {
        XzatomaError::Config(
            "Skill file has a frontmatter opening delimiter but no closing delimiter".to_string(),
        )
    })?;

    let frontmatter = &after_open[..close_offset];
    let remainder = &after_open[close_offset..];
    let body_start = closing_delimiter_len(remainder);
    let body = &remainder[body_start..];

    Ok((true, frontmatter, body))
}

/// Parse YAML frontmatter into a flattened string map.
///
/// Supported Phase 1 fields:
///
/// - `name`
/// - `description`
/// - `license`
/// - `compatibility`
/// - `metadata`
/// - `allowed-tools`
///
/// Values that are not simple scalars are normalized as strings when possible.
///
/// # Arguments
///
/// * `frontmatter` - YAML frontmatter without delimiters
///
/// # Returns
///
/// Returns a flattened map of parsed string fields.
///
/// # Errors
///
/// Returns an error if the frontmatter is malformed YAML or not a mapping.
///
/// # Examples
///
/// ```
/// use xzatoma::skills::parser::parse_frontmatter_map;
///
/// let parsed = parse_frontmatter_map(
///     "name: demo_skill\ndescription: Demo\nlicense: MIT\n"
/// )
/// .unwrap();
///
/// assert_eq!(parsed.get("name").map(String::as_str), Some("demo_skill"));
/// assert_eq!(parsed.get("license").map(String::as_str), Some("MIT"));
/// ```
pub fn parse_frontmatter_map(frontmatter: &str) -> Result<BTreeMap<String, String>> {
    let yaml_value: serde_yaml::Value = serde_yaml::from_str(frontmatter).map_err(|error| {
        XzatomaError::Config(format!("Failed to parse skill frontmatter: {}", error))
    })?;

    let mapping = yaml_value.as_mapping().ok_or_else(|| {
        XzatomaError::Config("Skill frontmatter must be a YAML mapping".to_string())
    })?;

    let mut parsed = BTreeMap::new();

    if let Some(value) = mapping.get(serde_yaml::Value::String("name".to_string())) {
        if let Some(value) = yaml_value_to_string(value)? {
            parsed.insert("name".to_string(), value);
        }
    }

    if let Some(value) = mapping.get(serde_yaml::Value::String("description".to_string())) {
        if let Some(value) = yaml_value_to_string(value)? {
            parsed.insert("description".to_string(), value);
        }
    }

    if let Some(value) = mapping.get(serde_yaml::Value::String("license".to_string())) {
        if let Some(value) = yaml_value_to_string(value)? {
            parsed.insert("license".to_string(), value);
        }
    }

    if let Some(value) = mapping.get(serde_yaml::Value::String("compatibility".to_string())) {
        if let Some(value) = yaml_value_to_string_or_sequence(value, "compatibility")? {
            parsed.insert("compatibility".to_string(), value);
        }
    }

    if let Some(value) = mapping.get(serde_yaml::Value::String("allowed-tools".to_string())) {
        if let Some(value) = yaml_value_to_string_or_sequence(value, "allowed-tools")? {
            parsed.insert("allowed-tools".to_string(), value);
        }
    }

    if let Some(value) = mapping.get(serde_yaml::Value::String("metadata".to_string())) {
        let metadata_map = value.as_mapping().ok_or_else(|| {
            XzatomaError::Config(
                "Skill frontmatter field 'metadata' must be a YAML mapping".to_string(),
            )
        })?;

        let mut flattened = Vec::new();
        for (key, value) in metadata_map {
            let key = yaml_value_to_string(key)?.ok_or_else(|| {
                XzatomaError::Config(
                    "Skill frontmatter metadata keys must be scalar values".to_string(),
                )
            })?;
            let value = yaml_value_to_string(value)?.ok_or_else(|| {
                XzatomaError::Config(format!(
                    "Skill frontmatter metadata value for key '{}' must be a scalar value",
                    key
                ))
            })?;
            flattened.push((key, value));
        }

        flattened.sort_by(|left, right| left.0.cmp(&right.0));

        let raw_metadata = flattened
            .into_iter()
            .map(|(key, value)| format!("{}={}", key, value))
            .collect::<Vec<_>>()
            .join("\n");

        parsed.insert("metadata".to_string(), raw_metadata);
    }

    Ok(parsed)
}

fn starts_with_frontmatter_delimiter(content: &str) -> bool {
    content.starts_with("---\n") || content.starts_with("---\r\n") || content == "---"
}

fn find_frontmatter_close(content: &str) -> Option<usize> {
    let bytes = content.as_bytes();
    let mut index = 0usize;

    while index < bytes.len() {
        let line_end = bytes[index..]
            .iter()
            .position(|byte| *byte == b'\n')
            .map(|offset| index + offset)
            .unwrap_or(bytes.len());

        let line = content[index..line_end].trim_end_matches('\r');
        if line == "---" {
            return Some(index);
        }

        if line_end == bytes.len() {
            break;
        }

        index = line_end + 1;
    }

    None
}

fn closing_delimiter_len(content: &str) -> usize {
    if content.starts_with("---\r\n") {
        5
    } else if content.starts_with("---\n") {
        4
    } else if content.starts_with("---") {
        3
    } else {
        0
    }
}

fn parse_metadata_from_map(
    map: &BTreeMap<String, String>,
) -> Result<Option<BTreeMap<String, String>>> {
    let Some(raw_metadata) = map.get("metadata") else {
        return Ok(None);
    };

    if raw_metadata.trim().is_empty() {
        return Ok(Some(BTreeMap::new()));
    }

    let mut metadata = BTreeMap::new();

    for line in raw_metadata.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let (key, value) = trimmed.split_once('=').ok_or_else(|| {
            XzatomaError::Config(format!(
                "Invalid flattened metadata entry '{}'; expected key=value",
                trimmed
            ))
        })?;

        metadata.insert(key.trim().to_string(), value.trim().to_string());
    }

    Ok(Some(metadata))
}

fn yaml_value_to_string(value: &serde_yaml::Value) -> Result<Option<String>> {
    let result = match value {
        serde_yaml::Value::Null => None,
        serde_yaml::Value::Bool(value) => Some(value.to_string()),
        serde_yaml::Value::Number(value) => Some(value.to_string()),
        serde_yaml::Value::String(value) => Some(value.clone()),
        _ => {
            return Err(XzatomaError::Config(
                "Expected a scalar YAML value in skill frontmatter".to_string(),
            ))
        }
    };

    Ok(result)
}

fn yaml_value_to_string_or_sequence(
    value: &serde_yaml::Value,
    field_name: &str,
) -> Result<Option<String>> {
    match value {
        serde_yaml::Value::Sequence(values) => {
            let mut normalized = Vec::new();
            for value in values {
                let scalar = yaml_value_to_string(value)?.ok_or_else(|| {
                    XzatomaError::Config(format!(
                        "Skill frontmatter field '{}' cannot contain null sequence entries",
                        field_name
                    ))
                })?;
                normalized.push(scalar);
            }
            Ok(Some(normalized.join(", ")))
        }
        _ => yaml_value_to_string(value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_split_frontmatter_with_valid_frontmatter() {
        let (present, frontmatter, body) =
            split_frontmatter("---\nname: demo_skill\ndescription: Demo\n---\n# Body\n").unwrap();

        assert!(present);
        assert!(frontmatter.contains("name: demo_skill"));
        assert_eq!(body.trim(), "# Body");
    }

    #[test]
    fn test_split_frontmatter_without_frontmatter() {
        let (present, frontmatter, body) = split_frontmatter("# Body\nNo frontmatter\n").unwrap();

        assert!(!present);
        assert_eq!(frontmatter, "");
        assert_eq!(body, "# Body\nNo frontmatter\n");
    }

    #[test]
    fn test_split_frontmatter_with_missing_closing_delimiter_returns_error() {
        let result = split_frontmatter("---\nname: demo_skill\ndescription: Demo\n# Body\n");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_frontmatter_map_with_scalars() {
        let parsed = parse_frontmatter_map(
            "name: demo_skill\ndescription: Demo\nlicense: MIT\ncompatibility: xzatoma >=0.2\n",
        )
        .unwrap();

        assert_eq!(parsed.get("name").map(String::as_str), Some("demo_skill"));
        assert_eq!(parsed.get("description").map(String::as_str), Some("Demo"));
        assert_eq!(parsed.get("license").map(String::as_str), Some("MIT"));
        assert_eq!(
            parsed.get("compatibility").map(String::as_str),
            Some("xzatoma >=0.2")
        );
    }

    #[test]
    fn test_parse_frontmatter_map_with_allowed_tools_sequence() {
        let parsed =
            parse_frontmatter_map("description: Demo\nallowed-tools:\n  - read_file\n  - grep\n")
                .unwrap();

        assert_eq!(
            parsed.get("allowed-tools").map(String::as_str),
            Some("read_file, grep")
        );
    }

    #[test]
    fn test_parse_frontmatter_map_with_metadata_mapping() {
        let parsed =
            parse_frontmatter_map("description: Demo\nmetadata:\n  owner: team\n  tier: core\n")
                .unwrap();

        assert_eq!(
            parsed.get("metadata").map(String::as_str),
            Some("owner=team\ntier=core")
        );
    }

    #[test]
    fn test_parse_skill_content_with_full_document() {
        let path = PathBuf::from("/tmp/demo_skill/SKILL.md");
        let parsed = parse_skill_content(
            &path,
            SkillSourceScope::CustomConfigured,
            "---\nname: demo_skill\ndescription: Demo\nlicense: MIT\ncompatibility:\n  - xzatoma >=0.2\nallowed-tools:\n  - read_file\nmetadata:\n  owner: team\n---\n# Demo\nbody\n",
        )
        .unwrap();

        assert!(parsed.frontmatter_present);
        assert_eq!(parsed.name.as_deref(), Some("demo_skill"));
        assert_eq!(parsed.description.as_deref(), Some("Demo"));
        assert_eq!(parsed.license.as_deref(), Some("MIT"));
        assert_eq!(parsed.compatibility.as_deref(), Some("xzatoma >=0.2"));
        assert_eq!(parsed.allowed_tools_raw.as_deref(), Some("read_file"));
        assert_eq!(parsed.body_markdown.trim(), "# Demo\nbody");
        assert_eq!(
            parsed
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("owner"))
                .map(String::as_str),
            Some("team")
        );
    }

    #[test]
    fn test_parse_skill_content_without_frontmatter_marks_document_invalid_for_validation() {
        let path = PathBuf::from("/tmp/demo_skill/SKILL.md");
        let parsed = parse_skill_content(
            &path,
            SkillSourceScope::ProjectSharedConvention,
            "# Demo\nNo frontmatter\n",
        )
        .unwrap();

        assert!(!parsed.frontmatter_present);
        assert!(parsed.name.is_none());
        assert!(parsed.description.is_none());
        assert_eq!(parsed.body_markdown, "# Demo\nNo frontmatter\n");
    }
}

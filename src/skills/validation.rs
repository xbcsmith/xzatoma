//! Skill validation and diagnostics.
//!
//! This module validates parsed `SKILL.md` documents and converts them into
//! Loaded skill records and diagnostics.

use crate::skills::types::{RawSkillDocument, SkillDiagnostic, SkillValidationOutcome};
use regex::Regex;

/// Validate a parsed skill document.
///
/// This function enforces the validation rules:
///
/// - reject missing `description`
/// - reject empty `description`
/// - reject name mismatch with parent directory
/// - reject name values violating the spec format
/// - preserve `allowed-tools` as raw text plus normalized vector
///
/// # Arguments
///
/// * `document` - Parsed skill document from the parser
///
/// # Returns
///
/// Returns a `SkillValidationOutcome` containing either a valid `SkillRecord`
/// or one or more diagnostics.
///
/// # Examples
///
/// ```
/// use std::collections::BTreeMap;
/// use std::path::PathBuf;
/// use xzatoma::skills::types::{RawSkillDocument, SkillSourceScope, SkillValidationOutcome};
/// use xzatoma::skills::validation::validate_parsed_skill;
///
/// let document = RawSkillDocument {
///     skill_file_path: PathBuf::from("/tmp/example_skill/SKILL.md"),
///     source_scope: SkillSourceScope::ProjectClientSpecific,
///     frontmatter_present: true,
///     name: Some("example_skill".to_string()),
///     description: Some("Example description".to_string()),
///     license: None,
///     compatibility: None,
///     metadata: Some(BTreeMap::new()),
///     allowed_tools_raw: Some("read_file, grep".to_string()),
///     body_markdown: "# Example\n".to_string(),
/// };
///
/// match validate_parsed_skill(document) {
///     SkillValidationOutcome::Valid(document) => {
///         assert_eq!(document.name.as_deref(), Some("example_skill"));
///         assert_eq!(document.allowed_tools_raw.as_deref(), Some("read_file, grep"));
///     }
///     SkillValidationOutcome::Invalid(_) => panic!("expected valid skill"),
/// }
/// ```
pub fn validate_parsed_skill(document: RawSkillDocument) -> SkillValidationOutcome {
    let mut diagnostics = Vec::new();

    if !document.frontmatter_present {
        diagnostics.push(invalid_skill_diagnostic(
            "missing_frontmatter",
            "missing required YAML frontmatter",
            &document,
            None,
        ));
        return SkillValidationOutcome::Invalid(diagnostics);
    }

    let name = match validate_name_field(&document, &mut diagnostics) {
        Some(name) => name,
        None => {
            return SkillValidationOutcome::Invalid(diagnostics);
        }
    };

    validate_name_matches_directory(&document, &name, &mut diagnostics);
    let description = validate_description_field(&document, &mut diagnostics);

    if !diagnostics.is_empty() {
        return SkillValidationOutcome::Invalid(diagnostics);
    }

    let mut validated_document = document;
    validated_document.name = Some(name);
    validated_document.description = description;
    validated_document.license = optional_trimmed_field(&validated_document, "license");
    validated_document.compatibility = optional_trimmed_field(&validated_document, "compatibility");
    validated_document.metadata = Some(collect_metadata_fields(&validated_document));
    validated_document.allowed_tools_raw =
        optional_trimmed_field(&validated_document, "allowed-tools");

    SkillValidationOutcome::Valid(validated_document)
}

/// Normalize a raw `allowed-tools` field into a vector of tool names.
///
/// Supported separators:
///
/// - commas
/// - newlines
///
/// Empty entries are ignored and whitespace is trimmed.
///
/// # Arguments
///
/// * `raw` - Optional raw `allowed-tools` string
///
/// # Returns
///
/// Returns a normalized list of allowed tool names.
///
/// # Examples
///
/// ```
/// use xzatoma::skills::validation::normalize_allowed_tools;
///
/// let tools = normalize_allowed_tools(Some("read_file, grep\nlist_directory"));
/// assert_eq!(tools, vec!["read_file", "grep", "list_directory"]);
/// ```
pub fn normalize_allowed_tools(raw: Option<&str>) -> Vec<String> {
    raw.unwrap_or_default()
        .split([',', '\n'])
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

/// Validate a skill name against the expected format.
///
/// Valid names must match `^[a-z][a-z0-9_]*$`.
///
/// # Arguments
///
/// * `name` - Name to validate
///
/// # Returns
///
/// Returns `true` if the name matches the supported format.
///
/// # Examples
///
/// ```
/// use xzatoma::skills::validation::is_valid_skill_name;
///
/// assert!(is_valid_skill_name("example_skill"));
/// assert!(is_valid_skill_name("skill2"));
/// assert!(!is_valid_skill_name("ExampleSkill"));
/// assert!(!is_valid_skill_name("example-skill"));
/// ```
pub fn is_valid_skill_name(name: &str) -> bool {
    let regex = Regex::new(r"^[a-z][a-z0-9_]*$")
        .unwrap_or_else(|_| panic!("skill name regex must be valid"));
    regex.is_match(name)
}

/// Build a diagnostic for an invalid discovered skill.
///
/// # Arguments
///
/// * `code` - Stable diagnostic code
/// * `message` - Human-readable message
/// * `document` - Parsed document associated with the problem
/// * `skill_name` - Optional skill name if known
///
/// # Returns
///
/// Returns an error-severity `SkillDiagnostic`.
///
/// # Examples
///
/// ```
/// use std::path::PathBuf;
/// use xzatoma::skills::types::{
///     RawSkillDocument, SkillDiagnosticSeverity, SkillSourceScope,
/// };
/// use xzatoma::skills::validation::invalid_skill_diagnostic;
///
/// let document = RawSkillDocument {
///     skill_file_path: PathBuf::from("/tmp/example_skill/SKILL.md"),
///     source_scope: SkillSourceScope::ProjectClientSpecific,
///     frontmatter_present: true,
///     name: Some("example_skill".to_string()),
///     description: None,
///     license: None,
///     compatibility: None,
///     metadata: None,
///     allowed_tools_raw: None,
///     body_markdown: String::new(),
/// };
///
/// let diagnostic = invalid_skill_diagnostic(
///     "missing_description",
///     "missing required `description` field",
///     &document,
///     Some("example_skill".to_string()),
/// );
///
/// assert_eq!(diagnostic.severity(), SkillDiagnosticSeverity::Error);
/// assert_eq!(diagnostic.code(), "invalid_description");
/// ```
pub fn invalid_skill_diagnostic(
    code: &'static str,
    message: impl Into<String>,
    document: &RawSkillDocument,
    skill_name: Option<String>,
) -> SkillDiagnostic {
    let kind = match code {
        "missing_name" | "invalid_name" => crate::skills::types::SkillDiagnosticKind::InvalidName,
        "missing_description" | "empty_description" => {
            crate::skills::types::SkillDiagnosticKind::InvalidDescription
        }
        "name_directory_mismatch" => {
            crate::skills::types::SkillDiagnosticKind::NameDirectoryMismatch
        }
        "missing_frontmatter" => crate::skills::types::SkillDiagnosticKind::MissingFrontmatter,
        _ => crate::skills::types::SkillDiagnosticKind::InternalError,
    };

    SkillDiagnostic::new(
        kind,
        message,
        skill_name,
        document.skill_file_path.clone(),
        Some(document.source_scope),
    )
}

fn validate_name_field(
    document: &RawSkillDocument,
    diagnostics: &mut Vec<SkillDiagnostic>,
) -> Option<String> {
    let Some(raw_name) = optional_trimmed_field(document, "name") else {
        diagnostics.push(invalid_skill_diagnostic(
            "missing_name",
            "missing required `name` field",
            document,
            None,
        ));
        return None;
    };

    if !is_valid_skill_name(&raw_name) {
        diagnostics.push(invalid_skill_diagnostic(
            "invalid_name",
            "skill `name` must match `^[a-z][a-z0-9_]*$`",
            document,
            Some(raw_name),
        ));
        return None;
    }

    Some(raw_name)
}

fn validate_name_matches_directory(
    document: &RawSkillDocument,
    name: &str,
    diagnostics: &mut Vec<SkillDiagnostic>,
) {
    let directory_name = document
        .skill_file_path
        .parent()
        .and_then(|path| path.file_name())
        .and_then(|value| value.to_str())
        .unwrap_or_default();

    if directory_name != name {
        diagnostics.push(invalid_skill_diagnostic(
            "name_directory_mismatch",
            format!(
                "skill `name` `{}` does not match parent directory `{}`",
                name, directory_name
            ),
            document,
            Some(name.to_string()),
        ));
    }
}

fn validate_description_field(
    document: &RawSkillDocument,
    diagnostics: &mut Vec<SkillDiagnostic>,
) -> Option<String> {
    let Some(description) = document.description.as_deref() else {
        diagnostics.push(invalid_skill_diagnostic(
            "missing_description",
            "missing required `description` field",
            document,
            optional_trimmed_field(document, "name"),
        ));
        return None;
    };

    let trimmed = description.trim();
    if trimmed.is_empty() {
        diagnostics.push(invalid_skill_diagnostic(
            "empty_description",
            "`description` must not be empty",
            document,
            optional_trimmed_field(document, "name"),
        ));
        return None;
    }

    Some(trimmed.to_string())
}

fn optional_trimmed_field(document: &RawSkillDocument, key: &str) -> Option<String> {
    let value = match key {
        "name" => document.name.as_deref(),
        "description" => document.description.as_deref(),
        "license" => document.license.as_deref(),
        "compatibility" => document.compatibility.as_deref(),
        "allowed-tools" => document.allowed_tools_raw.as_deref(),
        _ => None,
    }?;

    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn collect_metadata_fields(
    document: &RawSkillDocument,
) -> std::collections::BTreeMap<String, String> {
    document.metadata.clone().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::types::SkillSourceScope;
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    fn parsed_document(
        name: Option<&str>,
        description: Option<&str>,
        license: Option<&str>,
        compatibility: Option<&str>,
        allowed_tools_raw: Option<&str>,
        metadata: Option<BTreeMap<String, String>>,
    ) -> RawSkillDocument {
        RawSkillDocument {
            skill_file_path: PathBuf::from("/tmp/example_skill/SKILL.md"),
            source_scope: SkillSourceScope::ProjectClientSpecific,
            frontmatter_present: true,
            name: name.map(ToOwned::to_owned),
            description: description.map(ToOwned::to_owned),
            license: license.map(ToOwned::to_owned),
            compatibility: compatibility.map(ToOwned::to_owned),
            metadata,
            allowed_tools_raw: allowed_tools_raw.map(ToOwned::to_owned),
            body_markdown: "# Body\n".to_string(),
        }
    }

    #[test]
    fn test_validate_parsed_skill_with_valid_document() {
        match validate_parsed_skill(parsed_document(
            Some("example_skill"),
            Some("Example description"),
            Some("MIT"),
            Some("xzatoma >=0.2"),
            Some("read_file, grep"),
            Some(BTreeMap::new()),
        )) {
            SkillValidationOutcome::Valid(document) => {
                assert_eq!(document.name.as_deref(), Some("example_skill"));
                assert_eq!(document.description.as_deref(), Some("Example description"));
                assert_eq!(document.license.as_deref(), Some("MIT"));
                assert_eq!(document.compatibility.as_deref(), Some("xzatoma >=0.2"));
                assert_eq!(
                    document.allowed_tools_raw.as_deref(),
                    Some("read_file, grep")
                );
            }
            SkillValidationOutcome::Invalid(_) => panic!("expected valid skill"),
        }
    }

    #[test]
    fn test_validate_parsed_skill_rejects_missing_name() {
        match validate_parsed_skill(parsed_document(
            None,
            Some("Example description"),
            None,
            None,
            None,
            None,
        )) {
            SkillValidationOutcome::Valid(_) => panic!("expected invalid skill"),
            SkillValidationOutcome::Invalid(diagnostics) => {
                assert_eq!(diagnostics.len(), 1);
                assert_eq!(diagnostics[0].code(), "invalid_name");
                assert_eq!(
                    diagnostics[0].severity(),
                    crate::skills::types::SkillDiagnosticSeverity::Error
                );
            }
        }
    }

    #[test]
    fn test_validate_parsed_skill_rejects_invalid_name() {
        match validate_parsed_skill(parsed_document(
            Some("Example-Skill"),
            Some("Example description"),
            None,
            None,
            None,
            None,
        )) {
            SkillValidationOutcome::Valid(_) => panic!("expected invalid skill"),
            SkillValidationOutcome::Invalid(diagnostics) => {
                assert_eq!(diagnostics.len(), 1);
                assert_eq!(diagnostics[0].code(), "invalid_name");
            }
        }
    }

    #[test]
    fn test_validate_parsed_skill_rejects_missing_description() {
        match validate_parsed_skill(parsed_document(
            Some("example_skill"),
            None,
            None,
            None,
            None,
            None,
        )) {
            SkillValidationOutcome::Valid(_) => panic!("expected invalid skill"),
            SkillValidationOutcome::Invalid(diagnostics) => {
                assert_eq!(diagnostics.len(), 1);
                assert_eq!(diagnostics[0].code(), "invalid_description");
            }
        }
    }

    #[test]
    fn test_validate_parsed_skill_rejects_empty_description() {
        match validate_parsed_skill(parsed_document(
            Some("example_skill"),
            Some("   "),
            None,
            None,
            None,
            None,
        )) {
            SkillValidationOutcome::Valid(_) => panic!("expected invalid skill"),
            SkillValidationOutcome::Invalid(diagnostics) => {
                assert_eq!(diagnostics.len(), 1);
                assert_eq!(diagnostics[0].code(), "invalid_description");
            }
        }
    }

    #[test]
    fn test_validate_parsed_skill_rejects_name_directory_mismatch() {
        match validate_parsed_skill(parsed_document(
            Some("other_skill"),
            Some("Example description"),
            None,
            None,
            None,
            None,
        )) {
            SkillValidationOutcome::Valid(_) => panic!("expected invalid skill"),
            SkillValidationOutcome::Invalid(diagnostics) => {
                assert_eq!(diagnostics.len(), 1);
                assert_eq!(diagnostics[0].code(), "name_directory_mismatch");
            }
        }
    }

    #[test]
    fn test_normalize_allowed_tools_with_none_returns_empty() {
        let tools = normalize_allowed_tools(None);
        assert!(tools.is_empty());
    }

    #[test]
    fn test_normalize_allowed_tools_splits_commas_and_newlines() {
        let tools = normalize_allowed_tools(Some("read_file, grep\nlist_directory"));
        assert_eq!(tools, vec!["read_file", "grep", "list_directory"]);
    }

    #[test]
    fn test_is_valid_skill_name_accepts_supported_format() {
        assert!(is_valid_skill_name("example_skill"));
        assert!(is_valid_skill_name("skill2"));
    }

    #[test]
    fn test_is_valid_skill_name_rejects_unsupported_format() {
        assert!(!is_valid_skill_name("ExampleSkill"));
        assert!(!is_valid_skill_name("example-skill"));
        assert!(!is_valid_skill_name("2skill"));
    }

    #[test]
    fn test_collect_metadata_fields_excludes_reserved_keys() {
        let mut metadata_map = BTreeMap::new();
        metadata_map.insert("metadata_owner".to_string(), "team-a".to_string());
        metadata_map.insert("custom".to_string(), "value".to_string());

        let metadata = collect_metadata_fields(&parsed_document(
            Some("example_skill"),
            Some("Example"),
            None,
            None,
            None,
            Some(metadata_map),
        ));
        assert_eq!(
            metadata.get("metadata_owner").map(String::as_str),
            Some("team-a")
        );
        assert_eq!(metadata.get("custom").map(String::as_str), Some("value"));
    }
}

//! Core type definitions for agent skills support.
//!
//! This module defines the shared data model used by discovery, parsing,
//! validation, and catalog construction.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Discovery scope for a skill root.
///
/// Lower precedence rank values win during collision resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SkillSourceScope {
    /// `<working_dir>/.xzatoma/skills/`
    ProjectClientSpecific,
    /// `<working_dir>/.agents/skills/`
    ProjectSharedConvention,
    /// `~/.xzatoma/skills/`
    UserClientSpecific,
    /// `~/.agents/skills/`
    UserSharedConvention,
    /// Configured entry from `skills.additional_paths`
    CustomConfigured,
}

impl SkillSourceScope {
    /// Returns the deterministic precedence rank for the scope.
    ///
    /// Lower numbers have higher priority.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::skills::SkillSourceScope;
    ///
    /// assert_eq!(SkillSourceScope::ProjectClientSpecific.precedence_rank(), 1);
    /// assert_eq!(SkillSourceScope::CustomConfigured.precedence_rank(), 5);
    /// ```
    pub fn precedence_rank(self) -> u8 {
        match self {
            Self::ProjectClientSpecific => 1,
            Self::ProjectSharedConvention => 2,
            Self::UserClientSpecific => 3,
            Self::UserSharedConvention => 4,
            Self::CustomConfigured => 5,
        }
    }

    /// Returns a stable string identifier for the scope.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::skills::SkillSourceScope;
    ///
    /// assert_eq!(
    ///     SkillSourceScope::ProjectSharedConvention.as_str(),
    ///     "project_shared_convention"
    /// );
    /// ```
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ProjectClientSpecific => "project_client_specific",
            Self::ProjectSharedConvention => "project_shared_convention",
            Self::UserClientSpecific => "user_client_specific",
            Self::UserSharedConvention => "user_shared_convention",
            Self::CustomConfigured => "custom_configured",
        }
    }
}

/// Startup-visible metadata for a valid skill.
///
/// This structure intentionally separates metadata from activation content so
/// startup disclosure can remain lightweight.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillMetadata {
    /// Canonical skill name.
    pub name: String,
    /// Non-empty human-readable description.
    pub description: String,
    /// Optional license identifier or free-form value.
    pub license: Option<String>,
    /// Optional compatibility declaration.
    pub compatibility: Option<String>,
    /// Arbitrary scalar metadata preserved from frontmatter.
    pub metadata: BTreeMap<String, String>,
    /// Raw `allowed-tools` value as encountered in frontmatter.
    pub allowed_tools_raw: Option<String>,
    /// Normalized `allowed-tools` entries.
    pub allowed_tools: Vec<String>,
}

/// Fully loaded valid skill record.
///
/// This record combines startup metadata with the markdown body and fully
/// qualified filesystem location information.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillRecord {
    /// Parsed and validated metadata.
    pub metadata: SkillMetadata,
    /// Absolute path to the skill directory.
    pub skill_dir: PathBuf,
    /// Absolute path to `SKILL.md`.
    pub skill_file: PathBuf,
    /// Discovery scope for the winning root.
    pub source_scope: SkillSourceScope,
    /// Ordering of the root within its precedence class.
    pub source_order: usize,
    /// Markdown body content after frontmatter.
    pub body: String,
}

impl SkillRecord {
    /// Returns the canonical skill name.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::collections::BTreeMap;
    /// use std::path::PathBuf;
    /// use xzatoma::skills::{SkillMetadata, SkillRecord, SkillSourceScope};
    ///
    /// let record = SkillRecord {
    ///     metadata: SkillMetadata {
    ///         name: "example_skill".to_string(),
    ///         description: "Example".to_string(),
    ///         license: None,
    ///         compatibility: None,
    ///         metadata: BTreeMap::new(),
    ///         allowed_tools_raw: None,
    ///         allowed_tools: Vec::new(),
    ///     },
    ///     skill_dir: PathBuf::from("/tmp/example_skill"),
    ///     skill_file: PathBuf::from("/tmp/example_skill/SKILL.md"),
    ///     source_scope: SkillSourceScope::ProjectClientSpecific,
    ///     source_order: 0,
    ///     body: "Body".to_string(),
    /// };
    ///
    /// assert_eq!(record.name(), "example_skill");
    /// ```
    pub fn name(&self) -> &str {
        &self.metadata.name
    }

    /// Returns the absolute `SKILL.md` path.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::collections::BTreeMap;
    /// use std::path::{Path, PathBuf};
    /// use xzatoma::skills::{SkillMetadata, SkillRecord, SkillSourceScope};
    ///
    /// let record = SkillRecord {
    ///     metadata: SkillMetadata {
    ///         name: "example_skill".to_string(),
    ///         description: "Example".to_string(),
    ///         license: None,
    ///         compatibility: None,
    ///         metadata: BTreeMap::new(),
    ///         allowed_tools_raw: None,
    ///         allowed_tools: Vec::new(),
    ///     },
    ///     skill_dir: PathBuf::from("/tmp/example_skill"),
    ///     skill_file: PathBuf::from("/tmp/example_skill/SKILL.md"),
    ///     source_scope: SkillSourceScope::ProjectClientSpecific,
    ///     source_order: 0,
    ///     body: "Body".to_string(),
    /// };
    ///
    /// assert_eq!(record.skill_path(), Path::new("/tmp/example_skill/SKILL.md"));
    /// ```
    pub fn skill_path(&self) -> &Path {
        &self.skill_file
    }

    /// Returns the precedence key used for deterministic collision handling.
    ///
    /// Ordering rules:
    ///
    /// 1. lower scope rank wins
    /// 2. lower `source_order` wins
    /// 3. lexicographically smaller absolute `SKILL.md` path wins
    ///
    /// # Examples
    ///
    /// ```
    /// use std::collections::BTreeMap;
    /// use std::path::PathBuf;
    /// use xzatoma::skills::{SkillMetadata, SkillRecord, SkillSourceScope};
    ///
    /// fn record(path: &str, scope: SkillSourceScope, order: usize) -> SkillRecord {
    ///     SkillRecord {
    ///         metadata: SkillMetadata {
    ///             name: "example_skill".to_string(),
    ///             description: "Example".to_string(),
    ///             license: None,
    ///             compatibility: None,
    ///             metadata: BTreeMap::new(),
    ///             allowed_tools_raw: None,
    ///             allowed_tools: Vec::new(),
    ///         },
    ///         skill_dir: PathBuf::from(path).parent().unwrap().to_path_buf(),
    ///         skill_file: PathBuf::from(path),
    ///         source_scope: scope,
    ///         source_order: order,
    ///         body: "Body".to_string(),
    ///     }
    /// }
    ///
    /// let a = record("/tmp/a/SKILL.md", SkillSourceScope::ProjectClientSpecific, 0);
    /// let b = record("/tmp/b/SKILL.md", SkillSourceScope::UserClientSpecific, 0);
    ///
    /// assert!(a.precedence_key() < b.precedence_key());
    /// ```
    pub fn precedence_key(&self) -> (u8, usize, &Path) {
        (
            self.source_scope.precedence_rank(),
            self.source_order,
            self.skill_file.as_path(),
        )
    }

    /// Returns `true` if this record outranks `other`.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::collections::BTreeMap;
    /// use std::path::PathBuf;
    /// use xzatoma::skills::{SkillMetadata, SkillRecord, SkillSourceScope};
    ///
    /// fn record(path: &str, scope: SkillSourceScope, order: usize) -> SkillRecord {
    ///     SkillRecord {
    ///         metadata: SkillMetadata {
    ///             name: "example_skill".to_string(),
    ///             description: "Example".to_string(),
    ///             license: None,
    ///             compatibility: None,
    ///             metadata: BTreeMap::new(),
    ///             allowed_tools_raw: None,
    ///             allowed_tools: Vec::new(),
    ///         },
    ///         skill_dir: PathBuf::from(path).parent().unwrap().to_path_buf(),
    ///         skill_file: PathBuf::from(path),
    ///         source_scope: scope,
    ///         source_order: order,
    ///         body: "Body".to_string(),
    ///     }
    /// }
    ///
    /// let winner = record("/tmp/a/SKILL.md", SkillSourceScope::ProjectClientSpecific, 0);
    /// let loser = record("/tmp/b/SKILL.md", SkillSourceScope::UserClientSpecific, 0);
    ///
    /// assert!(winner.precedes(&loser));
    /// assert!(!loser.precedes(&winner));
    /// ```
    pub fn precedes(&self, other: &Self) -> bool {
        self.precedence_key() < other.precedence_key()
    }
}

/// Diagnostic severity for skill processing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SkillDiagnosticSeverity {
    /// Informational message.
    Info,
    /// Warning that does not invalidate the winning catalog entry.
    Warning,
    /// Error that invalidates a discovered candidate.
    Error,
}

impl SkillDiagnosticSeverity {
    /// Returns a stable string identifier for the severity.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::skills::types::SkillDiagnosticSeverity;
    ///
    /// assert_eq!(SkillDiagnosticSeverity::Error.as_str(), "error");
    /// ```
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
}

/// Machine-readable diagnostic kinds emitted during skill validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SkillDiagnosticKind {
    /// A candidate did not contain YAML frontmatter.
    MissingFrontmatter,
    /// Frontmatter existed but could not be parsed.
    MalformedFrontmatter,
    /// The skill `name` field was invalid.
    InvalidName,
    /// The skill `description` field was invalid.
    InvalidDescription,
    /// The skill `name` did not match the parent directory.
    NameDirectoryMismatch,
    /// The candidate path layout was invalid.
    InvalidPath,
    /// A valid skill was shadowed by a higher-precedence skill.
    ShadowedSkill,
    /// A scan limit was reached.
    ScanLimitReached,
    /// An internal validation or plumbing error occurred.
    InternalError,
}

impl SkillDiagnosticKind {
    /// Returns a stable string code for the diagnostic kind.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::skills::types::SkillDiagnosticKind;
    ///
    /// assert_eq!(SkillDiagnosticKind::InvalidName.code(), "invalid_name");
    /// ```
    pub fn code(self) -> &'static str {
        match self {
            Self::MissingFrontmatter => "missing_frontmatter",
            Self::MalformedFrontmatter => "malformed_frontmatter",
            Self::InvalidName => "invalid_name",
            Self::InvalidDescription => "invalid_description",
            Self::NameDirectoryMismatch => "name_directory_mismatch",
            Self::InvalidPath => "invalid_path",
            Self::ShadowedSkill => "shadowed_skill",
            Self::ScanLimitReached => "scan_limit_reached",
            Self::InternalError => "internal_error",
        }
    }

    /// Returns the default severity for the diagnostic kind.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::skills::types::{SkillDiagnosticKind, SkillDiagnosticSeverity};
    ///
    /// assert_eq!(
    ///     SkillDiagnosticKind::ShadowedSkill.default_severity(),
    ///     SkillDiagnosticSeverity::Warning
    /// );
    /// ```
    pub fn default_severity(self) -> SkillDiagnosticSeverity {
        match self {
            Self::ShadowedSkill | Self::ScanLimitReached => SkillDiagnosticSeverity::Warning,
            Self::MissingFrontmatter
            | Self::MalformedFrontmatter
            | Self::InvalidName
            | Self::InvalidDescription
            | Self::NameDirectoryMismatch
            | Self::InvalidPath
            | Self::InternalError => SkillDiagnosticSeverity::Error,
        }
    }
}

/// Diagnostic emitted during discovery, parsing, or validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillDiagnostic {
    /// Machine-readable kind.
    pub kind: SkillDiagnosticKind,
    /// Human-readable message.
    pub message: String,
    /// Optional skill name when known.
    pub skill_name: Option<String>,
    /// Absolute path to the relevant `SKILL.md`.
    pub skill_file_path: PathBuf,
    /// Discovery scope when known.
    pub source_scope: Option<SkillSourceScope>,
    /// Optional path to the winning skill for shadowed diagnostics.
    pub overshadowed_by: Option<PathBuf>,
}

impl SkillDiagnostic {
    /// Creates a diagnostic with the default severity derived from `kind`.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::PathBuf;
    /// use xzatoma::skills::types::{SkillDiagnostic, SkillDiagnosticKind, SkillSourceScope};
    ///
    /// let diagnostic = SkillDiagnostic::new(
    ///     SkillDiagnosticKind::InvalidName,
    ///     "invalid name",
    ///     Some("bad-name".to_string()),
    ///     PathBuf::from("/tmp/bad-name/SKILL.md"),
    ///     Some(SkillSourceScope::ProjectClientSpecific),
    /// );
    ///
    /// assert_eq!(diagnostic.kind, SkillDiagnosticKind::InvalidName);
    /// ```
    pub fn new(
        kind: SkillDiagnosticKind,
        message: impl Into<String>,
        skill_name: Option<String>,
        skill_file_path: PathBuf,
        source_scope: Option<SkillSourceScope>,
    ) -> Self {
        Self {
            kind,
            message: message.into(),
            skill_name,
            skill_file_path,
            source_scope,
            overshadowed_by: None,
        }
    }

    /// Creates a shadowed-skill diagnostic.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::PathBuf;
    /// use xzatoma::skills::types::{SkillDiagnostic, SkillDiagnosticKind, SkillSourceScope};
    ///
    /// let diagnostic = SkillDiagnostic::shadowed(
    ///     "example_skill",
    ///     PathBuf::from("/tmp/loser/SKILL.md"),
    ///     SkillSourceScope::UserClientSpecific,
    ///     PathBuf::from("/tmp/winner/SKILL.md"),
    /// );
    ///
    /// assert_eq!(diagnostic.kind, SkillDiagnosticKind::ShadowedSkill);
    /// assert!(diagnostic.overshadowed_by.is_some());
    /// ```
    pub fn shadowed(
        skill_name: impl Into<String>,
        skill_file_path: PathBuf,
        source_scope: SkillSourceScope,
        overshadowed_by: PathBuf,
    ) -> Self {
        let skill_name = skill_name.into();
        Self {
            kind: SkillDiagnosticKind::ShadowedSkill,
            message: format!(
                "skill '{}' is shadowed by {}",
                skill_name,
                overshadowed_by.display()
            ),
            skill_name: Some(skill_name),
            skill_file_path,
            source_scope: Some(source_scope),
            overshadowed_by: Some(overshadowed_by),
        }
    }

    /// Returns the severity implied by the diagnostic kind.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::PathBuf;
    /// use xzatoma::skills::types::{
    ///     SkillDiagnostic, SkillDiagnosticKind, SkillDiagnosticSeverity, SkillSourceScope,
    /// };
    ///
    /// let diagnostic = SkillDiagnostic::new(
    ///     SkillDiagnosticKind::InvalidDescription,
    ///     "bad description",
    ///     None,
    ///     PathBuf::from("/tmp/example/SKILL.md"),
    ///     Some(SkillSourceScope::ProjectClientSpecific),
    /// );
    ///
    /// assert_eq!(diagnostic.severity(), SkillDiagnosticSeverity::Error);
    /// ```
    pub fn severity(&self) -> SkillDiagnosticSeverity {
        self.kind.default_severity()
    }

    /// Returns the stable diagnostic code.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::PathBuf;
    /// use xzatoma::skills::types::{SkillDiagnostic, SkillDiagnosticKind};
    ///
    /// let diagnostic = SkillDiagnostic::new(
    ///     SkillDiagnosticKind::MalformedFrontmatter,
    ///     "bad yaml",
    ///     None,
    ///     PathBuf::from("/tmp/example/SKILL.md"),
    ///     None,
    /// );
    ///
    /// assert_eq!(diagnostic.code(), "malformed_frontmatter");
    /// ```
    pub fn code(&self) -> &'static str {
        self.kind.code()
    }
}

/// Raw parsed frontmatter plus markdown body prior to validation.
///
/// This preserves startup parse results separately from validated catalog data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawSkillDocument {
    /// Absolute path to `SKILL.md`.
    pub skill_file_path: PathBuf,
    /// Discovery scope of the candidate.
    pub source_scope: SkillSourceScope,
    /// Whether YAML frontmatter delimiters were present.
    pub frontmatter_present: bool,
    /// Parsed optional `name` field.
    pub name: Option<String>,
    /// Parsed optional `description` field.
    pub description: Option<String>,
    /// Parsed optional `license` field.
    pub license: Option<String>,
    /// Parsed optional `compatibility` field.
    pub compatibility: Option<String>,
    /// Parsed optional `metadata` map.
    pub metadata: Option<BTreeMap<String, String>>,
    /// Raw `allowed-tools` representation.
    pub allowed_tools_raw: Option<String>,
    /// Markdown body after frontmatter.
    pub body_markdown: String,
}

/// Outcome from validating a parsed skill document.
///
/// This keeps valid loaded records separate from invalid discovered candidates
/// and their diagnostics.
///
/// # Examples
///
/// ```
/// use std::collections::BTreeMap;
/// use std::path::PathBuf;
/// use xzatoma::skills::types::{
///     RawSkillDocument, SkillDiagnostic, SkillDiagnosticKind, SkillSourceScope,
///     SkillValidationOutcome,
/// };
///
/// let valid_document = RawSkillDocument {
///     skill_file_path: PathBuf::from("/tmp/example_skill/SKILL.md"),
///     source_scope: SkillSourceScope::ProjectClientSpecific,
///     frontmatter_present: true,
///     name: Some("example_skill".to_string()),
///     description: Some("Example".to_string()),
///     license: None,
///     compatibility: None,
///     metadata: Some(BTreeMap::new()),
///     allowed_tools_raw: None,
///     body_markdown: "Body".to_string(),
/// };
///
/// let invalid_document = RawSkillDocument {
///     skill_file_path: PathBuf::from("/tmp/bad_skill/SKILL.md"),
///     source_scope: SkillSourceScope::UserClientSpecific,
///     frontmatter_present: false,
///     name: None,
///     description: None,
///     license: None,
///     compatibility: None,
///     metadata: None,
///     allowed_tools_raw: None,
///     body_markdown: "# Missing frontmatter".to_string(),
/// };
///
/// let valid = SkillValidationOutcome::from_valid_document(valid_document);
/// assert!(valid.is_valid());
///
/// let invalid = SkillValidationOutcome::from_invalid_diagnostic(SkillDiagnostic::new(
///     SkillDiagnosticKind::MissingFrontmatter,
///     "missing frontmatter",
///     None,
///     PathBuf::from("/tmp/bad_skill/SKILL.md"),
///     Some(SkillSourceScope::UserClientSpecific),
/// ));
/// assert!(invalid.is_invalid());
///
/// // Prevent unused variable warning in doc tests.
/// let _ = invalid_document;
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkillValidationOutcome {
    /// The parsed skill was valid and can be loaded into the catalog.
    Valid(RawSkillDocument),
    /// The parsed skill was invalid and must not be loaded.
    Invalid(Vec<SkillDiagnostic>),
}

impl SkillValidationOutcome {
    /// Returns `true` when the outcome is valid.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::collections::BTreeMap;
    /// use std::path::PathBuf;
    /// use xzatoma::skills::types::{RawSkillDocument, SkillSourceScope, SkillValidationOutcome};
    ///
    /// let outcome = SkillValidationOutcome::Valid(RawSkillDocument {
    ///     skill_file_path: PathBuf::from("/tmp/example_skill/SKILL.md"),
    ///     source_scope: SkillSourceScope::ProjectClientSpecific,
    ///     frontmatter_present: true,
    ///     name: Some("example_skill".to_string()),
    ///     description: Some("Example".to_string()),
    ///     license: None,
    ///     compatibility: None,
    ///     metadata: Some(BTreeMap::new()),
    ///     allowed_tools_raw: None,
    ///     body_markdown: "Body".to_string(),
    /// });
    ///
    /// assert!(outcome.is_valid());
    /// assert!(!outcome.is_invalid());
    /// ```
    pub fn is_valid(&self) -> bool {
        matches!(self, Self::Valid(_))
    }

    /// Returns `true` when the outcome is invalid.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::PathBuf;
    /// use xzatoma::skills::types::{
    ///     SkillDiagnostic, SkillDiagnosticKind, SkillSourceScope, SkillValidationOutcome,
    /// };
    ///
    /// let outcome = SkillValidationOutcome::Invalid(vec![SkillDiagnostic::new(
    ///     SkillDiagnosticKind::InvalidName,
    ///     "invalid name",
    ///     Some("Bad-Name".to_string()),
    ///     PathBuf::from("/tmp/bad_name/SKILL.md"),
    ///     Some(SkillSourceScope::UserClientSpecific),
    /// )]);
    ///
    /// assert!(outcome.is_invalid());
    /// assert!(!outcome.is_valid());
    /// ```
    pub fn is_invalid(&self) -> bool {
        matches!(self, Self::Invalid(_))
    }

    /// Builds a valid outcome from a parsed raw skill document.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::collections::BTreeMap;
    /// use std::path::PathBuf;
    /// use xzatoma::skills::types::{RawSkillDocument, SkillSourceScope, SkillValidationOutcome};
    ///
    /// let document = RawSkillDocument {
    ///     skill_file_path: PathBuf::from("/tmp/example_skill/SKILL.md"),
    ///     source_scope: SkillSourceScope::ProjectClientSpecific,
    ///     frontmatter_present: true,
    ///     name: Some("example_skill".to_string()),
    ///     description: Some("Example".to_string()),
    ///     license: None,
    ///     compatibility: None,
    ///     metadata: Some(BTreeMap::new()),
    ///     allowed_tools_raw: None,
    ///     body_markdown: "Body".to_string(),
    /// };
    ///
    /// let outcome = SkillValidationOutcome::from_valid_document(document);
    /// assert!(outcome.is_valid());
    /// ```
    pub fn from_valid_document(document: RawSkillDocument) -> Self {
        Self::Valid(document)
    }

    /// Builds an invalid outcome from a single diagnostic.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::PathBuf;
    /// use xzatoma::skills::types::{
    ///     SkillDiagnostic, SkillDiagnosticKind, SkillSourceScope, SkillValidationOutcome,
    /// };
    ///
    /// let diagnostic = SkillDiagnostic::new(
    ///     SkillDiagnosticKind::InvalidDescription,
    ///     "description is required",
    ///     Some("example_skill".to_string()),
    ///     PathBuf::from("/tmp/example_skill/SKILL.md"),
    ///     Some(SkillSourceScope::ProjectClientSpecific),
    /// );
    ///
    /// let outcome = SkillValidationOutcome::from_invalid_diagnostic(diagnostic);
    /// assert!(outcome.is_invalid());
    /// ```
    pub fn from_invalid_diagnostic(diagnostic: SkillDiagnostic) -> Self {
        Self::Invalid(vec![diagnostic])
    }
}

impl RawSkillDocument {
    /// Returns the parent directory path for the skill candidate.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::{Path, PathBuf};
    /// use xzatoma::skills::types::{RawSkillDocument, SkillSourceScope};
    ///
    /// let document = RawSkillDocument {
    ///     skill_file_path: PathBuf::from("/tmp/example_skill/SKILL.md"),
    ///     source_scope: SkillSourceScope::ProjectClientSpecific,
    ///     frontmatter_present: true,
    ///     name: Some("example_skill".to_string()),
    ///     description: Some("Example".to_string()),
    ///     license: None,
    ///     compatibility: None,
    ///     metadata: None,
    ///     allowed_tools_raw: None,
    ///     body_markdown: "Body".to_string(),
    /// };
    ///
    /// assert_eq!(document.skill_dir_path(), Path::new("/tmp/example_skill"));
    /// ```
    pub fn skill_dir_path(&self) -> &Path {
        self.skill_file_path
            .parent()
            .unwrap_or_else(|| Path::new(""))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_metadata(name: &str) -> SkillMetadata {
        SkillMetadata {
            name: name.to_string(),
            description: "Sample description".to_string(),
            license: None,
            compatibility: None,
            metadata: BTreeMap::new(),
            allowed_tools_raw: Some("read_file, grep".to_string()),
            allowed_tools: vec!["read_file".to_string(), "grep".to_string()],
        }
    }

    fn sample_record(name: &str, scope: SkillSourceScope, order: usize, path: &str) -> SkillRecord {
        let skill_file = PathBuf::from(path);
        SkillRecord {
            metadata: sample_metadata(name),
            skill_dir: skill_file
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from("/")),
            skill_file,
            source_scope: scope,
            source_order: order,
            body: "Body".to_string(),
        }
    }

    #[test]
    fn test_skill_source_scope_precedence_rank() {
        assert_eq!(SkillSourceScope::ProjectClientSpecific.precedence_rank(), 1);
        assert_eq!(
            SkillSourceScope::ProjectSharedConvention.precedence_rank(),
            2
        );
        assert_eq!(SkillSourceScope::UserClientSpecific.precedence_rank(), 3);
        assert_eq!(SkillSourceScope::UserSharedConvention.precedence_rank(), 4);
        assert_eq!(SkillSourceScope::CustomConfigured.precedence_rank(), 5);
    }

    #[test]
    fn test_skill_record_precedes_by_scope_rank() {
        let higher = sample_record(
            "example_skill",
            SkillSourceScope::ProjectClientSpecific,
            0,
            "/tmp/a/SKILL.md",
        );
        let lower = sample_record(
            "example_skill",
            SkillSourceScope::UserClientSpecific,
            0,
            "/tmp/b/SKILL.md",
        );

        assert!(higher.precedes(&lower));
        assert!(!lower.precedes(&higher));
    }

    #[test]
    fn test_skill_record_precedes_by_source_order() {
        let first = sample_record(
            "example_skill",
            SkillSourceScope::CustomConfigured,
            0,
            "/tmp/a/SKILL.md",
        );
        let second = sample_record(
            "example_skill",
            SkillSourceScope::CustomConfigured,
            1,
            "/tmp/b/SKILL.md",
        );

        assert!(first.precedes(&second));
        assert!(!second.precedes(&first));
    }

    #[test]
    fn test_skill_record_precedes_by_lexicographic_path() {
        let first = sample_record(
            "example_skill",
            SkillSourceScope::CustomConfigured,
            0,
            "/tmp/a/SKILL.md",
        );
        let second = sample_record(
            "example_skill",
            SkillSourceScope::CustomConfigured,
            0,
            "/tmp/b/SKILL.md",
        );

        assert!(first.precedes(&second));
        assert!(!second.precedes(&first));
    }

    #[test]
    fn test_skill_diagnostic_shadowed_builder() {
        let diagnostic = SkillDiagnostic::shadowed(
            "example_skill",
            PathBuf::from("/tmp/loser/SKILL.md"),
            SkillSourceScope::UserClientSpecific,
            PathBuf::from("/tmp/winner/SKILL.md"),
        );

        assert_eq!(diagnostic.kind, SkillDiagnosticKind::ShadowedSkill);
        assert_eq!(diagnostic.code(), "shadowed_skill");
        assert_eq!(diagnostic.severity(), SkillDiagnosticSeverity::Warning);
        assert_eq!(
            diagnostic.overshadowed_by,
            Some(PathBuf::from("/tmp/winner/SKILL.md"))
        );
    }

    #[test]
    fn test_raw_skill_document_skill_dir_path() {
        let document = RawSkillDocument {
            skill_file_path: PathBuf::from("/tmp/example_skill/SKILL.md"),
            source_scope: SkillSourceScope::ProjectClientSpecific,
            frontmatter_present: true,
            name: Some("example_skill".to_string()),
            description: Some("Example".to_string()),
            license: None,
            compatibility: None,
            metadata: None,
            allowed_tools_raw: None,
            body_markdown: "Body".to_string(),
        };

        assert_eq!(document.skill_dir_path(), Path::new("/tmp/example_skill"));
    }

    #[test]
    fn test_skill_diagnostic_kind_default_severity() {
        assert_eq!(
            SkillDiagnosticKind::ShadowedSkill.default_severity(),
            SkillDiagnosticSeverity::Warning
        );
        assert_eq!(
            SkillDiagnosticKind::InvalidName.default_severity(),
            SkillDiagnosticSeverity::Error
        );
    }
}

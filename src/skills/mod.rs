/// Skills module for discovery, parsing, validation, and catalog management.
///
/// This module provides the Phase 1 foundation for agent skill support:
/// deterministic discovery, `SKILL.md` parsing, validation, diagnostics,
/// and a catalog of valid loaded skills.
///
/// # Examples
///
/// ```
/// use xzatoma::skills::SkillCatalog;
///
/// let catalog = SkillCatalog::default();
/// assert!(catalog.is_empty());
/// ```
pub mod catalog;
pub mod discovery;
pub mod parser;
pub mod types;
pub mod validation;

pub use catalog::SkillCatalog;
pub use discovery::{discover_skills, DiscoveryResult};
pub use parser::{parse_frontmatter_map, parse_skill_content, parse_skill_file, split_frontmatter};
pub use types::{
    RawSkillDocument, SkillDiagnostic, SkillDiagnosticKind, SkillDiagnosticSeverity, SkillMetadata,
    SkillRecord, SkillSourceScope, SkillValidationOutcome,
};
pub use validation::{
    invalid_skill_diagnostic, is_valid_skill_name, normalize_allowed_tools, validate_parsed_skill,
};

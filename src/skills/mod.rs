/// Skills module for discovery, parsing, validation, disclosure, and catalog
/// management.
///
/// This module provides the foundation for agent skill
/// support: deterministic discovery, `SKILL.md` parsing, validation,
/// diagnostics, catalog disclosure rendering, and a catalog of valid loaded
/// skills.
///
/// # Examples
///
/// ```
/// use xzatoma::skills::SkillCatalog;
///
/// let catalog = SkillCatalog::default();
/// assert!(catalog.is_empty());
/// ```
pub mod activation;
pub mod catalog;
pub mod disclosure;
pub mod discovery;
pub mod parser;
pub mod trust;
pub mod types;
pub mod validation;

pub use activation::{ActiveSkill, ActiveSkillRegistry};
pub use catalog::SkillCatalog;
pub use disclosure::{build_skill_disclosure_section, render_skill_catalog, SkillDisclosureTrust};
pub use discovery::{discover_skills, DiscoveryResult};
pub use parser::{parse_frontmatter_map, parse_skill_content, parse_skill_file, split_frontmatter};
pub use trust::{
    enumerate_skill_resources, expand_tilde_path, filter_visible_skill_records, load_trust_store,
    load_trusted_paths, resolve_skill_resource_path, resolve_trust_store_path, SkillTrustStore,
    SkillTrustStoreData,
};
pub use types::{
    RawSkillDocument, SkillDiagnostic, SkillDiagnosticKind, SkillDiagnosticSeverity, SkillMetadata,
    SkillRecord, SkillSourceScope, SkillValidationOutcome,
};
pub use validation::{
    invalid_skill_diagnostic, is_valid_skill_name, normalize_allowed_tools, validate_parsed_skill,
};

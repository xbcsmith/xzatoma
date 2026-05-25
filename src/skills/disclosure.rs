//! Skill catalog disclosure rendering and visibility filtering.
//!
//! This module implements the startup disclosure foundation for agent
//! skills. It renders a metadata-only catalog of valid visible skills for
//! prompt-time disclosure without exposing full `SKILL.md` instruction bodies.

use crate::config::SkillsConfig;
use crate::skills::catalog::SkillCatalog;
use crate::skills::types::{SkillDiagnostic, SkillRecord, SkillSourceScope};
use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};

/// Trust information used to filter visible skills for startup disclosure.
///
/// Disclosure must enforce trust gating before catalog entries are shown to the
/// model. This structure allows callers to provide the currently trusted
/// project/custom paths.
///
/// # Examples
///
/// ```
/// use std::path::PathBuf;
/// use xzatoma::skills::disclosure::SkillDisclosureTrust;
///
/// let trust = SkillDisclosureTrust::new(
///     true,
///     vec![PathBuf::from("/workspace/project")],
///     vec![PathBuf::from("/opt/xzatoma/skills")],
/// );
///
/// assert!(trust.project_trust_required);
/// assert_eq!(trust.trusted_project_paths.len(), 1);
/// assert_eq!(trust.trusted_custom_paths.len(), 1);
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SkillDisclosureTrust {
    /// Whether project-level skills require trust to be visible.
    pub project_trust_required: bool,
    /// Trusted project paths.
    pub trusted_project_paths: Vec<PathBuf>,
    /// Trusted custom skill roots.
    pub trusted_custom_paths: Vec<PathBuf>,
}

impl SkillDisclosureTrust {
    /// Creates a new disclosure trust state.
    ///
    /// # Arguments
    ///
    /// * `project_trust_required` - Whether project skills require explicit trust
    /// * `trusted_project_paths` - Trusted project paths
    /// * `trusted_custom_paths` - Trusted custom paths
    ///
    /// # Returns
    ///
    /// Returns a new `SkillDisclosureTrust`.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::PathBuf;
    /// use xzatoma::skills::disclosure::SkillDisclosureTrust;
    ///
    /// let trust = SkillDisclosureTrust::new(
    ///     true,
    ///     vec![PathBuf::from("/workspace/project")],
    ///     vec![PathBuf::from("/workspace/custom_skills")],
    /// );
    ///
    /// assert!(trust.project_trust_required);
    /// ```
    pub fn new(
        project_trust_required: bool,
        trusted_project_paths: Vec<PathBuf>,
        trusted_custom_paths: Vec<PathBuf>,
    ) -> Self {
        Self {
            project_trust_required,
            trusted_project_paths,
            trusted_custom_paths,
        }
    }
}

/// Builds the visible rendered catalog entries used for startup disclosure.
///
/// Each rendered entry contains only metadata needed at startup:
///
/// - `name`
/// - `description`
/// - location reference
///
/// Full skill bodies are never included.
///
/// # Arguments
///
/// * `catalog` - Valid loaded skill catalog
/// * `config` - Skills configuration
/// * `working_dir` - Active working directory for trust evaluation
/// * `trusted_paths` - Trusted project/custom paths
///
/// # Returns
///
/// Returns deterministic rendered catalog entries for valid visible skills.
///
/// # Examples
///
/// ```
/// use std::collections::BTreeSet;
/// use std::path::Path;
/// use xzatoma::config::SkillsConfig;
/// use xzatoma::skills::catalog::SkillCatalog;
/// use xzatoma::skills::disclosure::render_skill_catalog;
///
/// let rendered = render_skill_catalog(
///     &SkillCatalog::new(),
///     &SkillsConfig::default(),
///     Path::new("."),
///     &BTreeSet::new(),
/// );
///
/// assert!(rendered.is_empty());
/// ```
pub fn render_skill_catalog(
    catalog: &SkillCatalog,
    config: &SkillsConfig,
    working_dir: &Path,
    trusted_paths: &BTreeSet<PathBuf>,
) -> Vec<String> {
    visible_skill_records(catalog, config, working_dir, trusted_paths)
        .into_iter()
        .map(|skill| {
            format!(
                "- `{}`: {}\n  - scope: {}\n  - location: {}",
                skill.metadata.name,
                skill.metadata.description,
                skill.source_scope.as_str(),
                skill.skill_file.display()
            )
        })
        .collect()
}

/// Builds the complete startup disclosure section from a valid loaded catalog.
///
/// This function:
///
/// - filters visible skills using trust and config rules
/// - omits invalid skills by construction
/// - omits full skill bodies
/// - omits the entire block when no visible skills exist
///
/// # Arguments
///
/// * `catalog` - Valid loaded skill catalog
/// * `_invalid_diagnostics` - Invalid skill diagnostics, ignored for disclosure
/// * `config` - Skills configuration
/// * `working_dir` - Active working directory for trust checks
/// * `trusted_paths` - Trusted project/custom paths
///
/// # Returns
///
/// Returns `Some(String)` when at least one valid visible skill exists,
/// otherwise `None`.
///
/// # Examples
///
/// ```
/// use std::collections::BTreeSet;
/// use std::path::Path;
/// use xzatoma::config::SkillsConfig;
/// use xzatoma::skills::catalog::SkillCatalog;
/// use xzatoma::skills::disclosure::build_skill_disclosure_section;
///
/// let section = build_skill_disclosure_section(
///     &SkillCatalog::new(),
///     &[],
///     &SkillsConfig::default(),
///     Path::new("."),
///     &BTreeSet::new(),
/// );
///
/// assert!(section.is_none());
/// ```
pub fn build_skill_disclosure_section(
    catalog: &SkillCatalog,
    _invalid_diagnostics: &[SkillDiagnostic],
    config: &SkillsConfig,
    working_dir: &Path,
    trusted_paths: &BTreeSet<PathBuf>,
) -> Option<String> {
    let rendered_entries = render_skill_catalog(catalog, config, working_dir, trusted_paths);
    if rendered_entries.is_empty() {
        return None;
    }

    let mut lines = Vec::new();
    lines.push("## Available Skills".to_string());
    lines.push(
        "The following valid visible skills are available for activation in this session:"
            .to_string(),
    );
    lines.push(String::new());
    lines.extend(rendered_entries);

    Some(lines.join("\n"))
}

/// Returns the visible valid skill records after trust and config filtering.
///
/// Visibility rules enforced:
///
/// - project skills require trust when `skills.project_trust_required == true`
/// - user skills do not require project trust
/// - custom paths require trust unless
///   `skills.allow_custom_paths_without_trust == true`
/// - invalid skills are never present because the input catalog is valid-only
/// - returned skill count is capped by `skills.catalog_max_entries`
///
/// # Arguments
///
/// * `catalog` - Valid loaded skill catalog
/// * `config` - Skills configuration
/// * `working_dir` - Working directory for project trust evaluation
/// * `trusted_paths` - Trusted project/custom paths
///
/// # Returns
///
/// Returns visible skills in deterministic ordering.
///
/// # Examples
///
/// ```
/// use std::collections::BTreeSet;
/// use std::path::Path;
/// use xzatoma::config::SkillsConfig;
/// use xzatoma::skills::catalog::SkillCatalog;
/// use xzatoma::skills::disclosure::visible_skill_records;
///
/// let visible = visible_skill_records(
///     &SkillCatalog::new(),
///     &SkillsConfig::default(),
///     Path::new("."),
///     &BTreeSet::new(),
/// );
///
/// assert!(visible.is_empty());
/// ```
pub fn visible_skill_records(
    catalog: &SkillCatalog,
    config: &SkillsConfig,
    working_dir: &Path,
    trusted_paths: &BTreeSet<PathBuf>,
) -> Vec<SkillRecord> {
    let normalized_working_dir = normalize_path(working_dir);
    let trusted_paths = trusted_paths
        .iter()
        .map(|path| normalize_path(path))
        .collect::<HashSet<_>>();

    catalog
        .records()
        .into_iter()
        .filter(|record| is_skill_visible(record, config, &normalized_working_dir, &trusted_paths))
        .take(config.catalog_max_entries)
        .cloned()
        .collect()
}

/// Returns `true` if a valid skill record is visible for startup disclosure.
///
/// # Arguments
///
/// * `record` - Valid skill record
/// * `config` - Skills configuration
/// * `working_dir` - Normalized working directory
/// * `trusted_paths` - Trusted project/custom paths
///
/// # Returns
///
/// Returns `true` if the skill should be disclosed.
///
/// # Examples
///
/// ```
/// use std::collections::HashSet;
/// use std::path::{Path, PathBuf};
/// use xzatoma::config::SkillsConfig;
/// use xzatoma::skills::types::{SkillMetadata, SkillRecord, SkillSourceScope};
///
/// let record = SkillRecord {
///     metadata: SkillMetadata {
///         name: "user_skill".to_string(),
///         description: "Visible user skill".to_string(),
///         license: None,
///         compatibility: None,
///         metadata: std::collections::BTreeMap::new(),
///         allowed_tools_raw: None,
///         allowed_tools: Vec::new(),
///     },
///     skill_dir: PathBuf::from("/tmp/user_skill"),
///     skill_file: PathBuf::from("/tmp/user_skill/SKILL.md"),
///     source_scope: SkillSourceScope::UserClientSpecific,
///     source_order: 0,
///     body: "Body".to_string(),
/// };
///
/// let visible = xzatoma::skills::disclosure::is_skill_visible(
///     &record,
///     &SkillsConfig::default(),
///     Path::new("/workspace"),
///     &HashSet::new(),
/// );
///
/// assert!(visible);
/// ```
pub fn is_skill_visible(
    record: &SkillRecord,
    config: &SkillsConfig,
    working_dir: &Path,
    trusted_paths: &HashSet<PathBuf>,
) -> bool {
    match record.source_scope {
        SkillSourceScope::ProjectClientSpecific | SkillSourceScope::ProjectSharedConvention => {
            if !config.project_trust_required {
                return true;
            }

            is_path_trusted(&record.skill_dir, working_dir, trusted_paths)
                || is_path_trusted(&record.skill_file, working_dir, trusted_paths)
        }
        SkillSourceScope::UserClientSpecific | SkillSourceScope::UserSharedConvention => true,
        SkillSourceScope::CustomConfigured => {
            if config.allow_custom_paths_without_trust {
                return true;
            }

            is_path_trusted(&record.skill_dir, working_dir, trusted_paths)
                || is_path_trusted(&record.skill_file, working_dir, trusted_paths)
        }
    }
}

/// Returns `true` if a candidate path is covered by a trusted root.
///
/// A trusted root covers a path when the candidate path is equal to the trusted
/// root or is located beneath it.
///
/// # Arguments
///
/// * `candidate_path` - Path to check
/// * `working_dir` - Working directory used to normalize relative paths
/// * `trusted_paths` - Trusted root paths
///
/// # Returns
///
/// Returns `true` when the path is trusted.
///
/// # Examples
///
/// ```
/// use std::collections::HashSet;
/// use std::path::PathBuf;
/// use xzatoma::skills::disclosure::is_path_trusted;
///
/// let mut trusted = HashSet::new();
/// trusted.insert(PathBuf::from("/workspace/project"));
///
/// assert!(is_path_trusted(
///     &PathBuf::from("/workspace/project/.xzatoma/skills/demo"),
///     &PathBuf::from("/workspace/project"),
///     &trusted,
/// ));
/// ```
pub fn is_path_trusted(
    candidate_path: &Path,
    working_dir: &Path,
    trusted_paths: &HashSet<PathBuf>,
) -> bool {
    let normalized_candidate = if candidate_path.is_absolute() {
        normalize_path(candidate_path)
    } else {
        normalize_path(&working_dir.join(candidate_path))
    };

    trusted_paths.iter().any(|trusted_root| {
        normalized_candidate == *trusted_root || normalized_candidate.starts_with(trusted_root)
    })
}

fn normalize_path(path: &Path) -> PathBuf {
    match path.canonicalize() {
        Ok(canonical) => canonical,
        Err(_) => {
            if path.is_absolute() {
                path.to_path_buf()
            } else {
                std::env::current_dir()
                    .map(|cwd| cwd.join(path))
                    .unwrap_or_else(|_| path.to_path_buf())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SkillsConfig;
    use crate::skills::types::{SkillMetadata, SkillSourceScope};
    use std::collections::BTreeMap;

    fn sample_record(
        name: &str,
        description: &str,
        scope: SkillSourceScope,
        dir: &str,
        file: &str,
        source_order: usize,
    ) -> SkillRecord {
        SkillRecord {
            metadata: SkillMetadata {
                name: name.to_string(),
                description: description.to_string(),
                license: None,
                compatibility: None,
                metadata: BTreeMap::new(),
                allowed_tools_raw: None,
                allowed_tools: Vec::new(),
            },
            skill_dir: PathBuf::from(dir),
            skill_file: PathBuf::from(file),
            source_scope: scope,
            source_order,
            body: "Body content".to_string(),
        }
    }

    #[test]
    fn test_build_skill_disclosure_section_with_empty_catalog_returns_none() {
        let rendered = build_skill_disclosure_section(
            &SkillCatalog::new(),
            &[],
            &SkillsConfig::default(),
            Path::new("/workspace/project"),
            &BTreeSet::new(),
        );
        assert!(rendered.is_none());
    }

    #[test]
    fn test_build_skill_disclosure_section_omits_full_body_content() {
        let catalog = SkillCatalog::from_records(vec![sample_record(
            "demo_skill",
            "Demo description",
            SkillSourceScope::UserClientSpecific,
            "/tmp/demo_skill",
            "/tmp/demo_skill/SKILL.md",
            0,
        )])
        .expect("catalog should build");

        let rendered = build_skill_disclosure_section(
            &catalog,
            &[],
            &SkillsConfig::default(),
            Path::new("/workspace/project"),
            &BTreeSet::new(),
        )
        .expect("section should exist");
        assert!(rendered.contains("demo_skill"));
        assert!(rendered.contains("Demo description"));
        assert!(!rendered.contains("Body content"));
    }

    #[test]
    fn test_visible_skill_records_include_user_skills_without_trust() {
        let mut catalog = SkillCatalog::new();
        catalog.insert(sample_record(
            "user_skill",
            "Visible user skill",
            SkillSourceScope::UserClientSpecific,
            "/tmp/user_skill",
            "/tmp/user_skill/SKILL.md",
            0,
        ));

        let visible = visible_skill_records(
            &catalog,
            &SkillsConfig::default(),
            Path::new("/workspace/project"),
            &BTreeSet::new(),
        );

        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].metadata.name, "user_skill");
    }

    #[test]
    fn test_visible_skill_records_omit_untrusted_project_skills() {
        let mut catalog = SkillCatalog::new();
        catalog.insert(sample_record(
            "project_skill",
            "Project skill",
            SkillSourceScope::ProjectClientSpecific,
            "/workspace/project/.xzatoma/skills/project_skill",
            "/workspace/project/.xzatoma/skills/project_skill/SKILL.md",
            0,
        ));

        let config = SkillsConfig::default();
        let trusted_paths = BTreeSet::new();

        let visible = visible_skill_records(
            &catalog,
            &config,
            Path::new("/workspace/project"),
            &trusted_paths,
        );

        assert!(visible.is_empty());
    }

    #[test]
    fn test_visible_skill_records_include_trusted_project_skills() {
        let mut catalog = SkillCatalog::new();
        catalog.insert(sample_record(
            "project_skill",
            "Project skill",
            SkillSourceScope::ProjectClientSpecific,
            "/workspace/project/.xzatoma/skills/project_skill",
            "/workspace/project/.xzatoma/skills/project_skill/SKILL.md",
            0,
        ));

        let config = SkillsConfig::default();
        let trusted_paths = BTreeSet::from([PathBuf::from("/workspace/project")]);

        let visible = visible_skill_records(
            &catalog,
            &config,
            Path::new("/workspace/project"),
            &trusted_paths,
        );

        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].metadata.name, "project_skill");
    }

    #[test]
    fn test_visible_skill_records_omit_untrusted_custom_skills() {
        let mut catalog = SkillCatalog::new();
        catalog.insert(sample_record(
            "custom_skill",
            "Custom skill",
            SkillSourceScope::CustomConfigured,
            "/opt/xzatoma/skills/custom_skill",
            "/opt/xzatoma/skills/custom_skill/SKILL.md",
            0,
        ));

        let config = SkillsConfig::default();
        let trusted_paths = BTreeSet::new();

        let visible = visible_skill_records(
            &catalog,
            &config,
            Path::new("/workspace/project"),
            &trusted_paths,
        );

        assert!(visible.is_empty());
    }

    #[test]
    fn test_visible_skill_records_include_custom_skills_when_config_bypasses_trust() {
        let mut catalog = SkillCatalog::new();
        catalog.insert(sample_record(
            "custom_skill",
            "Custom skill",
            SkillSourceScope::CustomConfigured,
            "/opt/xzatoma/skills/custom_skill",
            "/opt/xzatoma/skills/custom_skill/SKILL.md",
            0,
        ));

        let config = SkillsConfig {
            allow_custom_paths_without_trust: true,
            ..SkillsConfig::default()
        };

        let visible = visible_skill_records(
            &catalog,
            &config,
            Path::new("/workspace/project"),
            &BTreeSet::new(),
        );

        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].metadata.name, "custom_skill");
    }

    #[test]
    fn test_render_skill_catalog_obeys_catalog_max_entries() {
        let mut catalog = SkillCatalog::new();
        catalog.insert(sample_record(
            "alpha_skill",
            "Alpha",
            SkillSourceScope::UserClientSpecific,
            "/tmp/alpha_skill",
            "/tmp/alpha_skill/SKILL.md",
            0,
        ));
        catalog.insert(sample_record(
            "beta_skill",
            "Beta",
            SkillSourceScope::UserClientSpecific,
            "/tmp/beta_skill",
            "/tmp/beta_skill/SKILL.md",
            0,
        ));

        let config = SkillsConfig {
            catalog_max_entries: 1,
            ..SkillsConfig::default()
        };

        let rendered = render_skill_catalog(
            &catalog,
            &config,
            Path::new("/workspace/project"),
            &BTreeSet::new(),
        );

        assert_eq!(rendered.len(), 1);
        assert!(rendered[0].contains("alpha_skill"));
    }

    #[test]
    fn test_render_skill_catalog_returns_empty_for_empty_visible_catalog() {
        let rendered = render_skill_catalog(
            &SkillCatalog::new(),
            &SkillsConfig::default(),
            Path::new("/workspace/project"),
            &BTreeSet::new(),
        );

        assert!(rendered.is_empty());
    }

    #[test]
    fn test_render_skill_catalog_ordering_is_deterministic() {
        let mut catalog = SkillCatalog::new();
        catalog.insert(sample_record(
            "zeta_skill",
            "Zeta",
            SkillSourceScope::UserClientSpecific,
            "/tmp/zeta_skill",
            "/tmp/zeta_skill/SKILL.md",
            0,
        ));
        catalog.insert(sample_record(
            "alpha_skill",
            "Alpha",
            SkillSourceScope::UserClientSpecific,
            "/tmp/alpha_skill",
            "/tmp/alpha_skill/SKILL.md",
            0,
        ));

        let rendered = render_skill_catalog(
            &catalog,
            &SkillsConfig::default(),
            Path::new("/workspace/project"),
            &BTreeSet::new(),
        );

        assert_eq!(rendered.len(), 2);
        assert!(rendered[0].contains("alpha_skill"));
        assert!(rendered[1].contains("zeta_skill"));
    }

    #[test]
    fn test_is_path_trusted_with_descendant_path_returns_true() {
        let mut trusted = HashSet::new();
        trusted.insert(PathBuf::from("/workspace/project"));

        assert!(is_path_trusted(
            Path::new("/workspace/project/.xzatoma/skills/demo_skill/SKILL.md"),
            Path::new("/workspace/project"),
            &trusted,
        ));
    }

    #[test]
    fn test_is_path_trusted_with_untrusted_path_returns_false() {
        let mut trusted = HashSet::new();
        trusted.insert(PathBuf::from("/workspace/project"));

        assert!(!is_path_trusted(
            Path::new("/workspace/other/.xzatoma/skills/demo_skill/SKILL.md"),
            Path::new("/workspace/project"),
            &trusted,
        ));
    }
}

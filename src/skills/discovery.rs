//! Skill discovery engine with deterministic precedence.
//!
//! This module implements the Phase 1 discovery foundation for agent skills.
//! It scans supported roots, parses `SKILL.md`, validates discovered
//! candidates, and resolves collisions into a valid-only catalog.

use crate::config::SkillsConfig;
use crate::error::{Result, XzatomaError};
use crate::skills::catalog::SkillCatalog;
use crate::skills::parser::parse_skill_file;
use crate::skills::types::{SkillDiagnostic, SkillDiagnosticKind, SkillRecord, SkillSourceScope};
use crate::skills::validation::validate_parsed_skill;
use std::collections::{BTreeMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

const SKILL_FILE_NAME: &str = "SKILL.md";

/// Result of a skills discovery pass.
///
/// This structure separates:
///
/// - valid loaded skills
/// - invalid discovered candidates
/// - valid-but-shadowed skills
///
/// # Examples
///
/// ```
/// use xzatoma::skills::discovery::DiscoveryResult;
///
/// let result = DiscoveryResult::empty();
/// assert!(result.catalog.is_empty());
/// assert!(result.valid_skills.is_empty());
/// ```
#[derive(Debug, Clone)]
pub struct DiscoveryResult {
    /// Catalog of valid winning skills after precedence resolution.
    pub catalog: SkillCatalog,
    /// Valid skills that survived precedence resolution.
    pub valid_skills: Vec<SkillRecord>,
    /// Diagnostics for invalid discovered candidates.
    pub invalid_diagnostics: Vec<SkillDiagnostic>,
    /// Diagnostics for valid discovered skills shadowed by higher-precedence
    /// winners.
    pub shadowed_diagnostics: Vec<SkillDiagnostic>,
    /// Number of directories visited while scanning.
    pub visited_directories: usize,
    /// Whether the valid-skill loading cap was reached.
    pub valid_skill_cap_reached: bool,
}

impl DiscoveryResult {
    /// Create an empty discovery result.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::skills::discovery::DiscoveryResult;
    ///
    /// let result = DiscoveryResult::empty();
    /// assert_eq!(result.visited_directories, 0);
    /// assert!(result.catalog.is_empty());
    /// ```
    pub fn empty() -> Self {
        Self {
            catalog: SkillCatalog::new(),
            valid_skills: Vec::new(),
            invalid_diagnostics: Vec::new(),
            shadowed_diagnostics: Vec::new(),
            visited_directories: 0,
            valid_skill_cap_reached: false,
        }
    }
}

#[derive(Debug, Clone)]
struct DiscoveryRoot {
    scope: SkillSourceScope,
    order_index: usize,
    path: PathBuf,
}

impl DiscoveryRoot {
    fn new(scope: SkillSourceScope, order_index: usize, path: PathBuf) -> Result<Self> {
        Ok(Self {
            scope,
            order_index,
            path: normalize_absolute_path(&path)?,
        })
    }
}

#[derive(Debug, Default)]
struct RootScanResult {
    valid_candidates: Vec<SkillRecord>,
    invalid_diagnostics: Vec<SkillDiagnostic>,
}

#[derive(Debug)]
struct CollisionResolution {
    winners: Vec<SkillRecord>,
    shadowed_diagnostics: Vec<SkillDiagnostic>,
}

/// Discover skills using the configured Phase 1 roots and limits.
///
/// Discovery order:
///
/// 1. `<working_dir>/.xzatoma/skills/`
/// 2. `<working_dir>/.agents/skills/`
/// 3. `~/.xzatoma/skills/`
/// 4. `~/.agents/skills/`
/// 5. `skills.additional_paths` in configured order
///
/// # Arguments
///
/// * `config` - Skills subsystem configuration
/// * `working_dir` - Active working directory
///
/// # Returns
///
/// Returns valid loaded skills, invalid diagnostics, and shadowed-skill
/// diagnostics.
///
/// # Errors
///
/// Returns an error if a configured path cannot be resolved.
///
/// # Examples
///
/// ```
/// use std::path::Path;
/// use xzatoma::config::SkillsConfig;
/// use xzatoma::skills::discover_skills;
///
/// let result = discover_skills(&SkillsConfig::default(), Path::new(".")).unwrap();
/// assert!(result.visited_directories <= 2000);
/// ```
pub fn discover_skills(config: &SkillsConfig, working_dir: &Path) -> Result<DiscoveryResult> {
    if !config.enabled {
        return Ok(DiscoveryResult::empty());
    }

    let roots = build_discovery_roots(config, working_dir)?;
    let mut visited_directories = 0usize;
    let mut invalid_diagnostics = Vec::new();
    let mut valid_candidates = Vec::new();
    let mut valid_skill_cap_reached = false;

    for root in &roots {
        if visited_directories >= config.max_scan_directories {
            invalid_diagnostics.push(SkillDiagnostic::new(
                SkillDiagnosticKind::ScanLimitReached,
                format!(
                    "stopped scanning after visiting {} directories",
                    config.max_scan_directories
                ),
                None,
                root.path.clone(),
                Some(root.scope),
            ));
            break;
        }

        let scan_result = scan_root(
            root,
            config,
            &mut visited_directories,
            &mut valid_skill_cap_reached,
        );

        invalid_diagnostics.extend(scan_result.invalid_diagnostics);
        valid_candidates.extend(scan_result.valid_candidates);

        if visited_directories >= config.max_scan_directories {
            invalid_diagnostics.push(SkillDiagnostic::new(
                SkillDiagnosticKind::ScanLimitReached,
                format!(
                    "stopped scanning after visiting {} directories",
                    config.max_scan_directories
                ),
                None,
                root.path.clone(),
                Some(root.scope),
            ));
            break;
        }
    }

    let resolution = resolve_collisions(valid_candidates);
    let catalog = SkillCatalog::from_records(resolution.winners.clone())?;

    Ok(DiscoveryResult {
        catalog,
        valid_skills: resolution.winners,
        invalid_diagnostics,
        shadowed_diagnostics: resolution.shadowed_diagnostics,
        visited_directories,
        valid_skill_cap_reached,
    })
}

fn build_discovery_roots(config: &SkillsConfig, working_dir: &Path) -> Result<Vec<DiscoveryRoot>> {
    let working_dir = normalize_absolute_path(working_dir)?;
    let mut roots = Vec::new();

    if config.project_enabled {
        roots.push(DiscoveryRoot::new(
            SkillSourceScope::ProjectClientSpecific,
            0,
            working_dir.join(".xzatoma").join("skills"),
        )?);
        roots.push(DiscoveryRoot::new(
            SkillSourceScope::ProjectSharedConvention,
            0,
            working_dir.join(".agents").join("skills"),
        )?);
    }

    if config.user_enabled {
        let home_dir = resolve_home_dir()?;

        roots.push(DiscoveryRoot::new(
            SkillSourceScope::UserClientSpecific,
            0,
            home_dir.join(".xzatoma").join("skills"),
        )?);
        roots.push(DiscoveryRoot::new(
            SkillSourceScope::UserSharedConvention,
            0,
            home_dir.join(".agents").join("skills"),
        )?);
    }

    for (index, configured_path) in config.additional_paths.iter().enumerate() {
        let resolved = resolve_config_path(configured_path, &working_dir)?;
        roots.push(DiscoveryRoot::new(
            SkillSourceScope::CustomConfigured,
            index,
            resolved,
        )?);
    }

    Ok(roots)
}

fn scan_root(
    root: &DiscoveryRoot,
    config: &SkillsConfig,
    visited_directories: &mut usize,
    valid_skill_cap_reached: &mut bool,
) -> RootScanResult {
    let mut result = RootScanResult::default();

    if !root.path.exists() {
        return result;
    }

    if !root.path.is_dir() {
        result.invalid_diagnostics.push(SkillDiagnostic::new(
            SkillDiagnosticKind::InvalidPath,
            format!("discovery root is not a directory: {}", root.path.display()),
            None,
            root.path.clone(),
            Some(root.scope),
        ));
        return result;
    }

    let mut queue = VecDeque::new();
    let mut seen_directories = HashSet::new();
    queue.push_back((root.path.clone(), 0usize));

    while let Some((directory, depth)) = queue.pop_front() {
        if *visited_directories >= config.max_scan_directories {
            break;
        }

        let normalized_dir = match normalize_absolute_path(&directory) {
            Ok(path) => path,
            Err(error) => {
                result.invalid_diagnostics.push(SkillDiagnostic::new(
                    SkillDiagnosticKind::InvalidPath,
                    format!(
                        "failed to normalize discovery directory '{}': {}",
                        directory.display(),
                        error
                    ),
                    None,
                    directory.clone(),
                    Some(root.scope),
                ));
                continue;
            }
        };

        if !seen_directories.insert(normalized_dir.clone()) {
            continue;
        }

        *visited_directories += 1;

        let skill_file = normalized_dir.join(SKILL_FILE_NAME);
        if skill_file.is_file() {
            match load_skill_candidate(root, &normalized_dir, &skill_file, config) {
                Ok(Some(record)) => {
                    if result.valid_candidates.len() < config.max_discovered_skills {
                        result.valid_candidates.push(record);
                    } else {
                        *valid_skill_cap_reached = true;
                    }
                }
                Ok(None) => {}
                Err(diagnostic) => result.invalid_diagnostics.push(diagnostic),
            }

            continue;
        }

        if depth >= config.max_scan_depth {
            continue;
        }

        let read_dir = match std::fs::read_dir(&normalized_dir) {
            Ok(entries) => entries,
            Err(error) => {
                result.invalid_diagnostics.push(SkillDiagnostic::new(
                    SkillDiagnosticKind::InvalidPath,
                    format!(
                        "failed to read discovery directory '{}': {}",
                        normalized_dir.display(),
                        error
                    ),
                    None,
                    normalized_dir.clone(),
                    Some(root.scope),
                ));
                continue;
            }
        };

        let mut child_directories = Vec::new();

        for entry_result in read_dir {
            let entry = match entry_result {
                Ok(entry) => entry,
                Err(error) => {
                    result.invalid_diagnostics.push(SkillDiagnostic::new(
                        SkillDiagnosticKind::InvalidPath,
                        format!(
                            "failed to read an entry under '{}': {}",
                            normalized_dir.display(),
                            error
                        ),
                        None,
                        normalized_dir.clone(),
                        Some(root.scope),
                    ));
                    continue;
                }
            };

            match entry.file_type() {
                Ok(file_type) if file_type.is_dir() => child_directories.push(entry.path()),
                Ok(_) => {}
                Err(error) => {
                    result.invalid_diagnostics.push(SkillDiagnostic::new(
                        SkillDiagnosticKind::InvalidPath,
                        format!(
                            "failed to inspect entry '{}': {}",
                            entry.path().display(),
                            error
                        ),
                        None,
                        entry.path(),
                        Some(root.scope),
                    ));
                }
            }
        }

        child_directories.sort();

        for child_directory in child_directories {
            queue.push_back((child_directory, depth + 1));
        }
    }

    result
}

fn load_skill_candidate(
    root: &DiscoveryRoot,
    skill_dir: &Path,
    skill_file: &Path,
    _config: &SkillsConfig,
) -> std::result::Result<Option<SkillRecord>, SkillDiagnostic> {
    let parsed = parse_skill_file(skill_file, root.scope).map_err(|error| {
        let kind = match error.downcast_ref::<XzatomaError>() {
            Some(XzatomaError::Config(message))
                if message.contains("frontmatter")
                    || message.contains("Skill frontmatter")
                    || message.contains("Failed to parse skill frontmatter") =>
            {
                SkillDiagnosticKind::MalformedFrontmatter
            }
            _ => SkillDiagnosticKind::InvalidPath,
        };

        SkillDiagnostic::new(
            kind,
            format!("failed to parse '{}': {}", skill_file.display(), error),
            None,
            skill_file.to_path_buf(),
            Some(root.scope),
        )
    })?;

    let validated = match validate_parsed_skill(parsed) {
        crate::skills::types::SkillValidationOutcome::Valid(document) => document,
        crate::skills::types::SkillValidationOutcome::Invalid(mut diagnostics) => {
            if diagnostics.is_empty() {
                return Err(SkillDiagnostic::new(
                    SkillDiagnosticKind::InternalError,
                    "validation failed without diagnostics",
                    None,
                    skill_file.to_path_buf(),
                    Some(root.scope),
                ));
            }

            let mut diagnostic = diagnostics.remove(0);
            if diagnostic.source_scope.is_none() {
                diagnostic.source_scope = Some(root.scope);
            }
            if diagnostic.skill_file_path.as_os_str().is_empty() {
                diagnostic.skill_file_path = skill_file.to_path_buf();
            }
            return Err(diagnostic);
        }
    };

    let metadata = crate::skills::types::SkillMetadata {
        name: validated.name.unwrap_or_default(),
        description: validated.description.unwrap_or_default(),
        license: validated.license,
        compatibility: validated.compatibility,
        metadata: validated.metadata.unwrap_or_default(),
        allowed_tools_raw: validated.allowed_tools_raw.clone(),
        allowed_tools: crate::skills::validation::normalize_allowed_tools(
            validated.allowed_tools_raw.as_deref(),
        ),
    };

    Ok(Some(SkillRecord {
        metadata,
        skill_dir: skill_dir.to_path_buf(),
        skill_file: skill_file.to_path_buf(),
        source_scope: root.scope,
        source_order: root.order_index,
        body: validated.body_markdown,
    }))
}

fn resolve_collisions(candidates: Vec<SkillRecord>) -> CollisionResolution {
    let mut grouped: BTreeMap<String, Vec<SkillRecord>> = BTreeMap::new();

    for candidate in candidates {
        grouped
            .entry(candidate.metadata.name.clone())
            .or_default()
            .push(candidate);
    }

    let mut winners = Vec::new();
    let mut shadowed_diagnostics = Vec::new();

    for (skill_name, mut group) in grouped {
        group.sort_by(compare_records);

        if let Some(winner) = group.first().cloned() {
            for shadowed in group.iter().skip(1) {
                shadowed_diagnostics.push(SkillDiagnostic::shadowed(
                    skill_name.clone(),
                    shadowed.skill_file.clone(),
                    shadowed.source_scope,
                    winner.skill_file.clone(),
                ));
            }

            winners.push(winner);
        }
    }

    winners.sort_by(compare_records);

    CollisionResolution {
        winners,
        shadowed_diagnostics,
    }
}

fn compare_records(left: &SkillRecord, right: &SkillRecord) -> std::cmp::Ordering {
    left.source_scope
        .precedence_rank()
        .cmp(&right.source_scope.precedence_rank())
        .then_with(|| left.source_order.cmp(&right.source_order))
        .then_with(|| left.skill_file.cmp(&right.skill_file))
}

fn resolve_config_path(path: &str, working_dir: &Path) -> Result<PathBuf> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(XzatomaError::Config(
            "skills.additional_paths cannot contain empty entries".to_string(),
        )
        .into());
    }

    let resolved = if let Some(stripped) = trimmed.strip_prefix("~/") {
        resolve_home_dir()?.join(stripped)
    } else {
        let candidate = PathBuf::from(trimmed);
        if candidate.is_absolute() {
            candidate
        } else {
            working_dir.join(candidate)
        }
    };

    normalize_absolute_path(&resolved)
}

fn resolve_home_dir() -> Result<PathBuf> {
    let home = std::env::var("HOME")
        .map_err(|_| XzatomaError::Config("HOME environment variable is not set".to_string()))?;
    let path = PathBuf::from(home);

    if path.is_absolute() {
        Ok(path)
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

fn normalize_absolute_path(path: &Path) -> Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };

    match absolute.canonicalize() {
        Ok(canonical) => Ok(canonical),
        Err(_) => Ok(absolute),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::{SkillMetadata, SkillSourceScope};
    use std::collections::BTreeMap;

    fn sample_record(name: &str, scope: SkillSourceScope, order: usize, path: &str) -> SkillRecord {
        SkillRecord {
            metadata: SkillMetadata {
                name: name.to_string(),
                description: "desc".to_string(),
                license: None,
                compatibility: None,
                metadata: BTreeMap::new(),
                allowed_tools_raw: None,
                allowed_tools: Vec::new(),
            },
            skill_dir: PathBuf::from(path)
                .parent()
                .unwrap_or_else(|| Path::new("/"))
                .to_path_buf(),
            skill_file: PathBuf::from(path),
            source_scope: scope,
            source_order: order,
            body: "body".to_string(),
        }
    }

    #[test]
    fn test_discovery_result_empty_returns_empty_values() {
        let result = DiscoveryResult::empty();
        assert!(result.catalog.is_empty());
        assert!(result.valid_skills.is_empty());
        assert!(result.invalid_diagnostics.is_empty());
        assert!(result.shadowed_diagnostics.is_empty());
    }

    #[test]
    fn test_compare_records_orders_by_rank_then_order_then_path() {
        let high = sample_record(
            "demo",
            SkillSourceScope::ProjectClientSpecific,
            0,
            "/tmp/a/SKILL.md",
        );
        let low = sample_record(
            "demo",
            SkillSourceScope::UserClientSpecific,
            0,
            "/tmp/b/SKILL.md",
        );

        assert_eq!(compare_records(&high, &low), std::cmp::Ordering::Less);
        assert_eq!(compare_records(&low, &high), std::cmp::Ordering::Greater);
    }

    #[test]
    fn test_resolve_collisions_keeps_highest_precedence_record() {
        let winner = sample_record(
            "demo",
            SkillSourceScope::ProjectClientSpecific,
            0,
            "/tmp/a/SKILL.md",
        );
        let loser = sample_record(
            "demo",
            SkillSourceScope::UserClientSpecific,
            0,
            "/tmp/b/SKILL.md",
        );

        let resolution = resolve_collisions(vec![loser, winner.clone()]);
        assert_eq!(resolution.winners.len(), 1);
        assert_eq!(resolution.winners[0].skill_file, winner.skill_file);
        assert_eq!(resolution.shadowed_diagnostics.len(), 1);
        assert_eq!(
            resolution.shadowed_diagnostics[0].kind,
            SkillDiagnosticKind::ShadowedSkill
        );
    }

    #[test]
    fn test_build_discovery_roots_includes_custom_paths_in_order() {
        let config = SkillsConfig {
            project_enabled: false,
            user_enabled: false,
            additional_paths: vec!["/tmp/skills_a".to_string(), "/tmp/skills_b".to_string()],
            ..SkillsConfig::default()
        };

        let roots = build_discovery_roots(&config, Path::new(".")).unwrap();
        assert_eq!(roots.len(), 2);
        assert_eq!(roots[0].scope, SkillSourceScope::CustomConfigured);
        assert_eq!(roots[0].order_index, 0);
        assert_eq!(roots[1].scope, SkillSourceScope::CustomConfigured);
        assert_eq!(roots[1].order_index, 1);
    }
}

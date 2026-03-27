//! Persistent skill trust store and resource resolution support.
//!
//! This module implements the Phase 4 trust and resource foundation for agent
//! skills.
//!
//! It provides:
//!
//! - a persistent trust store for canonicalized project/custom skill paths
//! - add/remove/show-style trust operations
//! - path-prefix trust checks for project-level skills
//! - safe resource resolution relative to a skill root only
//! - resource enumeration for `scripts/`, `references/`, and `assets/`
//! - path traversal prevention outside the skill root
//!
//! # Examples
//!
//! ```
//! use std::path::PathBuf;
//! use tempfile::tempdir;
//! use xzatoma::skills::trust::{SkillTrustStore, SkillTrustStoreData};
//!
//! let temp_dir = tempdir()?;
//! let store_path = temp_dir.path().join("skills_trust.yaml");
//! let mut store = SkillTrustStore::new(store_path.clone())?;
//!
//! let trusted = store.add_path(temp_dir.path())?;
//! assert_eq!(trusted, temp_dir.path().canonicalize()?);
//!
//! let loaded = SkillTrustStore::load_or_create(store_path)?;
//! assert!(loaded.is_trusted_path(temp_dir.path()));
//! # Ok::<(), anyhow::Error>(())
//! ```

use crate::error::{Result, XzatomaError};
use crate::skills::{SkillCatalog, SkillRecord};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

/// Persistent trust store for skill roots.
///
/// This store persists trusted project/custom paths to disk and uses
/// canonicalized path handling for stable trust checks.
///
/// # Examples
///
/// ```
/// use tempfile::tempdir;
/// use xzatoma::skills::trust::SkillTrustStore;
///
/// let temp_dir = tempdir()?;
/// let store_path = temp_dir.path().join("skills_trust.yaml");
/// let store = SkillTrustStore::new(store_path)?;
///
/// assert!(store.trusted_paths().is_empty());
/// # Ok::<(), anyhow::Error>(())
/// ```
#[derive(Debug, Clone)]
pub struct SkillTrustStore {
    path: PathBuf,
    data: SkillTrustStoreData,
}

/// Serializable trust store data.
///
/// This struct is separated from `SkillTrustStore` so it can be persisted
/// directly without runtime-specific state.
///
/// # Examples
///
/// ```
/// use std::collections::BTreeSet;
/// use std::path::PathBuf;
/// use xzatoma::skills::trust::SkillTrustStoreData;
///
/// let mut trusted_paths = BTreeSet::new();
/// trusted_paths.insert(PathBuf::from("/workspace/project"));
///
/// let data = SkillTrustStoreData { trusted_paths };
/// assert_eq!(data.trusted_paths.len(), 1);
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillTrustStoreData {
    /// Trusted canonicalized paths.
    #[serde(default)]
    pub trusted_paths: BTreeSet<PathBuf>,
}

/// Enumerated skill resources under supported subdirectories.
///
/// Phase 4 supports lazy enumeration of resources under:
///
/// - `scripts/`
/// - `references/`
/// - `assets/`
///
/// # Examples
///
/// ```
/// use xzatoma::skills::trust::SkillResources;
///
/// let resources = SkillResources::default();
/// assert!(resources.scripts.is_empty());
/// assert!(resources.references.is_empty());
/// assert!(resources.assets.is_empty());
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SkillResources {
    /// Files discovered under `scripts/`.
    pub scripts: Vec<PathBuf>,
    /// Files discovered under `references/`.
    pub references: Vec<PathBuf>,
    /// Files discovered under `assets/`.
    pub assets: Vec<PathBuf>,
}

impl SkillResources {
    /// Returns all enumerated resource paths in deterministic order.
    ///
    /// # Returns
    ///
    /// Returns all resource paths grouped into a single vector.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::PathBuf;
    /// use xzatoma::skills::trust::SkillResources;
    ///
    /// let resources = SkillResources {
    ///     scripts: vec![PathBuf::from("scripts/a.sh")],
    ///     references: vec![PathBuf::from("references/readme.md")],
    ///     assets: vec![PathBuf::from("assets/logo.png")],
    /// };
    ///
    /// let all = resources.all();
    /// assert_eq!(all.len(), 3);
    /// ```
    pub fn all(&self) -> Vec<PathBuf> {
        let mut all = Vec::new();
        all.extend(self.scripts.iter().cloned());
        all.extend(self.references.iter().cloned());
        all.extend(self.assets.iter().cloned());
        all
    }

    /// Returns `true` if no resources were found.
    ///
    /// # Returns
    ///
    /// Returns `true` when all resource groups are empty.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::skills::trust::SkillResources;
    ///
    /// assert!(SkillResources::default().is_empty());
    /// ```
    pub fn is_empty(&self) -> bool {
        self.scripts.is_empty() && self.references.is_empty() && self.assets.is_empty()
    }
}

impl SkillTrustStore {
    /// Creates a new trust store at the given path without reading existing
    /// contents.
    ///
    /// # Arguments
    ///
    /// * `path` - Trust store file path
    ///
    /// # Returns
    ///
    /// Returns a new empty trust store.
    ///
    /// # Errors
    ///
    /// Returns an error if the path is empty or invalid.
    ///
    /// # Examples
    ///
    /// ```
    /// use tempfile::tempdir;
    /// use xzatoma::skills::trust::SkillTrustStore;
    ///
    /// let temp_dir = tempdir()?;
    /// let store_path = temp_dir.path().join("skills_trust.yaml");
    /// let store = SkillTrustStore::new(store_path)?;
    ///
    /// assert!(store.trusted_paths().is_empty());
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn new(path: PathBuf) -> Result<Self> {
        if path.as_os_str().is_empty() {
            return Err(XzatomaError::Config(
                "skills trust store path cannot be empty".to_string(),
            )
            .into());
        }

        Ok(Self {
            path,
            data: SkillTrustStoreData::default(),
        })
    }

    /// Loads an existing trust store or creates an empty one if the file does not
    /// exist.
    ///
    /// # Arguments
    ///
    /// * `path` - Trust store file path
    ///
    /// # Returns
    ///
    /// Returns a loaded or newly created trust store.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or parsed.
    ///
    /// # Examples
    ///
    /// ```
    /// use tempfile::tempdir;
    /// use xzatoma::skills::trust::SkillTrustStore;
    ///
    /// let temp_dir = tempdir()?;
    /// let store_path = temp_dir.path().join("skills_trust.yaml");
    /// let store = SkillTrustStore::load_or_create(store_path)?;
    ///
    /// assert!(store.trusted_paths().is_empty());
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn load_or_create(path: PathBuf) -> Result<Self> {
        if !path.exists() {
            return Self::new(path);
        }

        let contents = fs::read_to_string(&path).map_err(|error| {
            XzatomaError::Config(format!(
                "Failed to read skills trust store '{}': {}",
                path.display(),
                error
            ))
        })?;

        let data: SkillTrustStoreData = serde_yaml::from_str(&contents).map_err(|error| {
            XzatomaError::Config(format!(
                "Failed to parse skills trust store '{}': {}",
                path.display(),
                error
            ))
        })?;

        Ok(Self { path, data })
    }

    /// Saves the trust store to disk.
    ///
    /// Parent directories are created as needed.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be written.
    ///
    /// # Examples
    ///
    /// ```
    /// use tempfile::tempdir;
    /// use xzatoma::skills::trust::SkillTrustStore;
    ///
    /// let temp_dir = tempdir()?;
    /// let store_path = temp_dir.path().join("skills_trust.yaml");
    /// let store = SkillTrustStore::new(store_path.clone())?;
    /// store.save()?;
    ///
    /// assert!(store_path.exists());
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn save(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                XzatomaError::Config(format!(
                    "Failed to create trust store directory '{}': {}",
                    parent.display(),
                    error
                ))
            })?;
        }

        let contents = serde_yaml::to_string(&self.data).map_err(|error| {
            XzatomaError::Config(format!("Failed to serialize skills trust store: {}", error))
        })?;

        fs::write(&self.path, contents).map_err(|error| {
            XzatomaError::Config(format!(
                "Failed to write skills trust store '{}': {}",
                self.path.display(),
                error
            ))
        })?;

        Ok(())
    }

    /// Returns the underlying trust store file path.
    ///
    /// # Returns
    ///
    /// Returns the configured store path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns trusted paths in deterministic order.
    ///
    /// # Returns
    ///
    /// Returns a reference to the trusted path set.
    pub fn trusted_paths(&self) -> &BTreeSet<PathBuf> {
        &self.data.trusted_paths
    }

    /// Adds a trusted path to the store and persists the updated store.
    ///
    /// The path is canonicalized before insertion.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to trust
    ///
    /// # Returns
    ///
    /// Returns the canonicalized trusted path.
    ///
    /// # Errors
    ///
    /// Returns an error if canonicalization fails or the store cannot be saved.
    ///
    /// # Examples
    ///
    /// ```
    /// use tempfile::tempdir;
    /// use xzatoma::skills::trust::SkillTrustStore;
    ///
    /// let temp_dir = tempdir()?;
    /// let store_path = temp_dir.path().join("skills_trust.yaml");
    /// let mut store = SkillTrustStore::new(store_path)?;
    ///
    /// let trusted = store.add_path(temp_dir.path())?;
    /// assert!(store.trusted_paths().contains(&trusted));
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn add_path(&mut self, path: &Path) -> Result<PathBuf> {
        let canonical = canonicalize_existing_path(path)?;
        self.data.trusted_paths.insert(canonical.clone());
        self.save()?;
        Ok(canonical)
    }

    /// Removes a trusted path from the store and persists the updated store.
    ///
    /// The input path is canonicalized before removal. If the path does not
    /// exist on disk, the method attempts to normalize it using parent
    /// canonicalization plus the final component.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to remove from the trust set
    ///
    /// # Returns
    ///
    /// Returns `true` if the path was present and removed.
    ///
    /// # Errors
    ///
    /// Returns an error if canonicalization or persistence fails.
    ///
    /// # Examples
    ///
    /// ```
    /// use tempfile::tempdir;
    /// use xzatoma::skills::trust::SkillTrustStore;
    ///
    /// let temp_dir = tempdir()?;
    /// let store_path = temp_dir.path().join("skills_trust.yaml");
    /// let mut store = SkillTrustStore::new(store_path)?;
    /// let trusted = store.add_path(temp_dir.path())?;
    ///
    /// let removed = store.remove_path(&trusted)?;
    /// assert!(removed);
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn remove_path(&mut self, path: &Path) -> Result<bool> {
        let canonical = canonicalize_maybe_missing_path(path)?;
        let removed = self.data.trusted_paths.remove(&canonical);
        self.save()?;
        Ok(removed)
    }

    /// Returns `true` if the given path is trusted by prefix match.
    ///
    /// Trust checks are canonicalized and allow child paths under a trusted
    /// root.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to check
    ///
    /// # Returns
    ///
    /// Returns `true` when the path is covered by a trusted root.
    ///
    /// # Examples
    ///
    /// ```
    /// use tempfile::tempdir;
    /// use xzatoma::skills::trust::SkillTrustStore;
    ///
    /// let temp_dir = tempdir()?;
    /// let nested = temp_dir.path().join("nested").join("child");
    /// std::fs::create_dir_all(&nested)?;
    ///
    /// let store_path = temp_dir.path().join("skills_trust.yaml");
    /// let mut store = SkillTrustStore::new(store_path)?;
    /// let trusted = store.add_path(temp_dir.path())?;
    ///
    /// assert!(store.is_trusted_path(&trusted));
    /// assert!(store.is_trusted_path(&nested));
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn is_trusted_path(&self, path: &Path) -> bool {
        let candidate = match canonicalize_maybe_missing_path(path) {
            Ok(path) => path,
            Err(_) => return false,
        };

        self.data
            .trusted_paths
            .iter()
            .any(|trusted| candidate == *trusted || candidate.starts_with(trusted))
    }

    /// Replaces the trust store contents with the provided data and persists the
    /// result.
    ///
    /// # Arguments
    ///
    /// * `data` - New trust store data
    ///
    /// # Errors
    ///
    /// Returns an error if the store cannot be saved.
    pub fn replace_data(&mut self, data: SkillTrustStoreData) -> Result<()> {
        self.data = data;
        self.save()
    }

    /// Returns a copy of the current serializable trust store data.
    ///
    /// # Returns
    ///
    /// Returns the underlying trust store data.
    pub fn data(&self) -> SkillTrustStoreData {
        self.data.clone()
    }
}

/// Returns the default trust store path for skills.
///
/// The default path is:
///
/// - `~/.xzatoma/skills_trust.yaml`
///
/// # Returns
///
/// Returns the default trust store path.
///
/// # Errors
///
/// Returns an error if the home directory cannot be determined.
///
/// # Examples
///
/// ```
/// use xzatoma::skills::trust::default_trust_store_path;
///
/// let path = default_trust_store_path()?;
/// assert!(path.to_string_lossy().contains(".xzatoma"));
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn default_trust_store_path() -> Result<PathBuf> {
    let home = std::env::var("HOME")
        .map_err(|_| XzatomaError::Config("HOME environment variable is not set".to_string()))?;
    Ok(PathBuf::from(home)
        .join(".xzatoma")
        .join("skills_trust.yaml"))
}

/// Resolves the configured trust store path.
///
/// If `configured_path` is provided, it supports `~/` expansion.
/// Otherwise the default trust store path is returned.
///
/// # Arguments
///
/// * `configured_path` - Optional configured trust store path
///
/// # Returns
///
/// Returns the resolved trust store path.
///
/// # Errors
///
/// Returns an error if the default home-based path cannot be determined.
///
/// # Examples
///
/// ```
/// use xzatoma::skills::trust::resolve_trust_store_path;
///
/// let path = resolve_trust_store_path(Some("~/custom_skills_trust.yaml"))?;
/// assert!(path.to_string_lossy().contains("custom_skills_trust.yaml"));
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn resolve_trust_store_path(configured_path: Option<&str>) -> Result<PathBuf> {
    match configured_path {
        Some(path) => expand_tilde_path(path),
        None => default_trust_store_path(),
    }
}

/// Loads the persistent trust store for skills.
///
/// This helper resolves the effective trust store path from configuration and
/// loads the store from disk, creating an empty in-memory store when the file
/// does not yet exist.
///
/// # Arguments
///
/// * `skills_config` - Skills configuration
/// * `_working_dir` - Current working directory
///
/// # Returns
///
/// Returns the loaded trust store.
///
/// # Errors
///
/// Returns an error if the store path cannot be resolved or the store cannot be
/// loaded.
///
/// # Examples
///
/// ```
/// use tempfile::tempdir;
/// use xzatoma::config::SkillsConfig;
/// use xzatoma::skills::trust::load_trust_store;
///
/// let temp_dir = tempdir()?;
/// let config = SkillsConfig {
///     trust_store_path: Some(
///         temp_dir
///             .path()
///             .join("skills_trust.yaml")
///             .to_string_lossy()
///             .to_string(),
///     ),
///     ..SkillsConfig::default()
/// };
///
/// let store = load_trust_store(&config, temp_dir.path())?;
/// assert!(store.trusted_paths().is_empty());
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn load_trust_store(
    skills_config: &crate::config::SkillsConfig,
    _working_dir: &Path,
) -> Result<SkillTrustStore> {
    let store_path = resolve_trust_store_path(skills_config.trust_store_path.as_deref())?;
    SkillTrustStore::load_or_create(store_path)
}

/// Loads trusted paths from the persistent trust store.
///
/// # Arguments
///
/// * `skills_config` - Skills configuration
/// * `working_dir` - Current working directory
///
/// # Returns
///
/// Returns canonicalized trusted paths in deterministic order.
///
/// # Errors
///
/// Returns an error if the trust store cannot be loaded.
///
/// # Examples
///
/// ```
/// use tempfile::tempdir;
/// use xzatoma::config::SkillsConfig;
/// use xzatoma::skills::trust::load_trusted_paths;
///
/// let temp_dir = tempdir()?;
/// let config = SkillsConfig {
///     trust_store_path: Some(
///         temp_dir
///             .path()
///             .join("skills_trust.yaml")
///             .to_string_lossy()
///             .to_string(),
///     ),
///     ..SkillsConfig::default()
/// };
///
/// let trusted = load_trusted_paths(&config, temp_dir.path())?;
/// assert!(trusted.is_empty());
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn load_trusted_paths(
    skills_config: &crate::config::SkillsConfig,
    working_dir: &Path,
) -> Result<BTreeSet<PathBuf>> {
    let store = load_trust_store(skills_config, working_dir)?;
    Ok(store.trusted_paths().clone())
}

/// Filters a valid loaded skill catalog into a visible valid skill list using
/// persistent trust state.
///
/// Visibility rules:
///
/// - project skills require trust when `project_trust_required == true`
/// - user skills are trusted by default
/// - custom paths require trust unless
///   `allow_custom_paths_without_trust == true`
/// - invalid skills are already excluded because the input catalog is valid-only
///
/// # Arguments
///
/// * `catalog` - Valid loaded skill catalog
/// * `skills_config` - Skills configuration
/// * `working_dir` - Current working directory
/// * `trusted_paths` - Trusted canonicalized paths
///
/// # Returns
///
/// Returns visible valid skills in deterministic order.
///
/// # Examples
///
/// ```
/// use std::collections::BTreeSet;
/// use std::path::Path;
/// use xzatoma::config::SkillsConfig;
/// use xzatoma::skills::catalog::SkillCatalog;
/// use xzatoma::skills::trust::filter_visible_skill_records;
///
/// let visible = filter_visible_skill_records(
///     &SkillCatalog::new(),
///     &SkillsConfig::default(),
///     Path::new("."),
///     &BTreeSet::new(),
/// );
///
/// assert!(visible.is_empty());
/// ```
pub fn filter_visible_skill_records(
    catalog: &SkillCatalog,
    skills_config: &crate::config::SkillsConfig,
    working_dir: &Path,
    trusted_paths: &BTreeSet<PathBuf>,
) -> Vec<SkillRecord> {
    let normalized_working_dir =
        canonicalize_maybe_missing_path(working_dir).unwrap_or_else(|_| working_dir.to_path_buf());

    catalog
        .records()
        .into_iter()
        .filter(|record| {
            is_skill_record_visible(
                record,
                skills_config,
                &normalized_working_dir,
                trusted_paths,
            )
        })
        .take(skills_config.catalog_max_entries)
        .cloned()
        .collect()
}

fn is_skill_record_visible(
    record: &SkillRecord,
    skills_config: &crate::config::SkillsConfig,
    working_dir: &Path,
    trusted_paths: &BTreeSet<PathBuf>,
) -> bool {
    match record.source_scope {
        crate::skills::SkillSourceScope::ProjectClientSpecific
        | crate::skills::SkillSourceScope::ProjectSharedConvention => {
            if !skills_config.project_trust_required {
                return true;
            }

            trusted_paths.iter().any(|trusted| {
                working_dir == trusted
                    || record.skill_dir.starts_with(trusted)
                    || record.skill_file.starts_with(trusted)
            })
        }
        crate::skills::SkillSourceScope::UserClientSpecific
        | crate::skills::SkillSourceScope::UserSharedConvention => true,
        crate::skills::SkillSourceScope::CustomConfigured => {
            if skills_config.allow_custom_paths_without_trust {
                return true;
            }

            trusted_paths.iter().any(|trusted| {
                record.skill_dir.starts_with(trusted) || record.skill_file.starts_with(trusted)
            })
        }
    }
}

/// Expands a `~/` path using the current home directory.
///
/// Non-tilde paths are returned unchanged.
///
/// # Arguments
///
/// * `path` - Path string to expand
///
/// # Returns
///
/// Returns the expanded path.
///
/// # Errors
///
/// Returns an error if `~/` expansion is requested but `HOME` is unavailable.
pub fn expand_tilde_path(path: &str) -> Result<PathBuf> {
    if let Some(stripped) = path.strip_prefix("~/") {
        let home = std::env::var("HOME").map_err(|_| {
            XzatomaError::Config("HOME environment variable is not set".to_string())
        })?;
        Ok(PathBuf::from(home).join(stripped))
    } else {
        Ok(PathBuf::from(path))
    }
}

/// Resolves a relative resource path from a skill root safely.
///
/// This function enforces that the resolved path remains within the provided
/// skill root and rejects traversal attempts outside the root.
///
/// # Arguments
///
/// * `skill_root` - Canonical skill root directory
/// * `relative_path` - Relative resource path to resolve
///
/// # Returns
///
/// Returns the resolved canonical-or-normalized path inside the skill root.
///
/// # Errors
///
/// Returns an error if:
///
/// - the path is absolute
/// - the path attempts traversal outside the skill root
/// - canonicalization fails in a way that escapes the skill root
///
/// # Examples
///
/// ```
/// use tempfile::tempdir;
/// use xzatoma::skills::trust::resolve_skill_resource_path;
///
/// let temp_dir = tempdir()?;
/// let skill_root = temp_dir.path().join("skill");
/// std::fs::create_dir_all(skill_root.join("references"))?;
/// std::fs::write(skill_root.join("references").join("guide.md"), "hello")?;
///
/// let resolved = resolve_skill_resource_path(&skill_root, "references/guide.md")?;
/// assert!(resolved.ends_with("guide.md"));
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn resolve_skill_resource_path(skill_root: &Path, relative_path: &str) -> Result<PathBuf> {
    let canonical_root = canonicalize_existing_path(skill_root)?;

    let relative = Path::new(relative_path);
    if relative.is_absolute() {
        return Err(XzatomaError::Tool(
            "Skill resource path must be relative to the skill root".to_string(),
        )
        .into());
    }

    if path_contains_parent_traversal(relative) {
        return Err(XzatomaError::Tool(
            "Skill resource path traversal outside skill root is not allowed".to_string(),
        )
        .into());
    }

    let joined = canonical_root.join(relative);
    let resolved = canonicalize_maybe_missing_path(&joined)?;
    if !resolved.starts_with(&canonical_root) {
        return Err(XzatomaError::Tool(
            "Resolved skill resource escaped the skill root".to_string(),
        )
        .into());
    }

    Ok(resolved)
}

/// Enumerates supported skill resources under the skill root.
///
/// Supported subdirectories:
///
/// - `scripts/`
/// - `references/`
/// - `assets/`
///
/// Enumeration is recursive and constrained to remain within the skill root.
///
/// # Arguments
///
/// * `skill_root` - Skill root directory
///
/// # Returns
///
/// Returns grouped enumerated resources in deterministic order.
///
/// # Errors
///
/// Returns an error if the skill root is invalid or a discovered resource path
/// escapes the root.
///
/// # Examples
///
/// ```
/// use tempfile::tempdir;
/// use xzatoma::skills::trust::enumerate_skill_resources;
///
/// let temp_dir = tempdir()?;
/// let skill_root = temp_dir.path().join("skill");
/// std::fs::create_dir_all(skill_root.join("scripts"))?;
/// std::fs::create_dir_all(skill_root.join("references"))?;
/// std::fs::write(skill_root.join("scripts").join("build.sh"), "echo hi")?;
/// std::fs::write(skill_root.join("references").join("guide.md"), "guide")?;
///
/// let resources = enumerate_skill_resources(&skill_root)?;
/// assert_eq!(resources.scripts.len(), 1);
/// assert_eq!(resources.references.len(), 1);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn enumerate_skill_resources(skill_root: &Path) -> Result<SkillResources> {
    let canonical_root = canonicalize_existing_path(skill_root)?;
    let resources = SkillResources {
        scripts: enumerate_resource_group(&canonical_root, "scripts")?,
        references: enumerate_resource_group(&canonical_root, "references")?,
        assets: enumerate_resource_group(&canonical_root, "assets")?,
    };

    Ok(resources)
}

fn enumerate_resource_group(skill_root: &Path, directory_name: &str) -> Result<Vec<PathBuf>> {
    let directory = skill_root.join(directory_name);
    if !directory.exists() {
        return Ok(Vec::new());
    }

    if !directory.is_dir() {
        return Err(XzatomaError::Tool(format!(
            "Skill resource directory '{}' exists but is not a directory",
            directory.display()
        ))
        .into());
    }

    let mut results = Vec::new();
    walk_resource_directory(skill_root, &directory, &mut results)?;
    results.sort();
    Ok(results)
}

fn walk_resource_directory(
    skill_root: &Path,
    directory: &Path,
    results: &mut Vec<PathBuf>,
) -> Result<()> {
    for entry in fs::read_dir(directory).map_err(|error| {
        XzatomaError::Tool(format!(
            "Failed to read skill resource directory '{}': {}",
            directory.display(),
            error
        ))
    })? {
        let entry = entry.map_err(|error| {
            XzatomaError::Tool(format!(
                "Failed to read skill resource entry under '{}': {}",
                directory.display(),
                error
            ))
        })?;

        let path = entry.path();
        let file_type = entry.file_type().map_err(|error| {
            XzatomaError::Tool(format!(
                "Failed to inspect skill resource entry '{}': {}",
                path.display(),
                error
            ))
        })?;

        let canonical_or_normalized = canonicalize_maybe_missing_path(&path)?;
        if !canonical_or_normalized.starts_with(skill_root) {
            return Err(XzatomaError::Tool(
                "Skill resource enumeration escaped the skill root".to_string(),
            )
            .into());
        }

        if file_type.is_dir() {
            walk_resource_directory(skill_root, &path, results)?;
        } else if file_type.is_file() {
            results.push(canonical_or_normalized);
        }
    }

    Ok(())
}

fn path_contains_parent_traversal(path: &Path) -> bool {
    path.components()
        .any(|component| matches!(component, Component::ParentDir))
}

fn canonicalize_existing_path(path: &Path) -> Result<PathBuf> {
    path.canonicalize().map_err(|error| {
        XzatomaError::Config(format!(
            "Failed to canonicalize path '{}': {}",
            path.display(),
            error
        ))
        .into()
    })
}

fn canonicalize_maybe_missing_path(path: &Path) -> Result<PathBuf> {
    if path.exists() {
        return canonicalize_existing_path(path);
    }

    let parent = path.parent().ok_or_else(|| {
        XzatomaError::Config(format!(
            "Path '{}' has no parent for normalization",
            path.display()
        ))
    })?;

    let canonical_parent = canonicalize_existing_path(parent)?;
    let file_name = path.file_name().ok_or_else(|| {
        XzatomaError::Config(format!(
            "Path '{}' has no final component for normalization",
            path.display()
        ))
    })?;

    Ok(canonical_parent.join(file_name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_trust_store_new_starts_empty() {
        let temp_dir = tempdir().expect("temp dir should exist");
        let store_path = temp_dir.path().join("skills_trust.yaml");
        let store = SkillTrustStore::new(store_path).expect("store should be created");

        assert!(store.trusted_paths().is_empty());
    }

    #[test]
    fn test_trust_store_add_show_remove() {
        let temp_dir = tempdir().expect("temp dir should exist");
        let project_dir = temp_dir.path().join("project");
        fs::create_dir_all(&project_dir).expect("project dir should exist");

        let store_path = temp_dir.path().join("skills_trust.yaml");
        let mut store = SkillTrustStore::new(store_path).expect("store should be created");

        let canonical = store
            .add_path(&project_dir)
            .expect("trust add should succeed");
        assert!(store.trusted_paths().contains(&canonical));
        assert!(store.is_trusted_path(&project_dir));

        let removed = store
            .remove_path(&project_dir)
            .expect("trust remove should succeed");
        assert!(removed);
        assert!(!store.is_trusted_path(&project_dir));
    }

    #[test]
    fn test_trust_store_persists_to_disk() {
        let temp_dir = tempdir().expect("temp dir should exist");
        let project_dir = temp_dir.path().join("project");
        fs::create_dir_all(&project_dir).expect("project dir should exist");

        let store_path = temp_dir.path().join("skills_trust.yaml");
        let mut store = SkillTrustStore::new(store_path.clone()).expect("store should be created");
        let canonical = store
            .add_path(&project_dir)
            .expect("trust add should succeed");

        let loaded =
            SkillTrustStore::load_or_create(store_path).expect("store should load successfully");
        assert!(loaded.trusted_paths().contains(&canonical));
        assert!(loaded.is_trusted_path(&project_dir));
    }

    #[test]
    fn test_trust_store_prefix_check_includes_descendants() {
        let temp_dir = tempdir().expect("temp dir should exist");
        let project_dir = temp_dir.path().join("project");
        let nested_dir = project_dir.join(".xzatoma").join("skills").join("demo");
        fs::create_dir_all(&nested_dir).expect("nested dir should exist");

        let store_path = temp_dir.path().join("skills_trust.yaml");
        let mut store = SkillTrustStore::new(store_path).expect("store should be created");
        store
            .add_path(&project_dir)
            .expect("trust add should succeed");

        assert!(store.is_trusted_path(&nested_dir));
    }

    #[test]
    fn test_default_trust_store_path_contains_xzatoma_directory() {
        let original_home = std::env::var("HOME").ok();
        let temp_dir = tempdir().expect("temp dir should exist");

        unsafe {
            std::env::set_var("HOME", temp_dir.path());
        }

        let result = default_trust_store_path();

        match original_home {
            Some(value) => unsafe {
                std::env::set_var("HOME", value);
            },
            None => unsafe {
                std::env::remove_var("HOME");
            },
        }

        let path = result.expect("default trust store path should resolve");
        assert!(path.to_string_lossy().contains(".xzatoma"));
        assert!(path.to_string_lossy().contains("skills_trust.yaml"));
    }

    #[test]
    fn test_resolve_skill_resource_path_rejects_absolute_path() {
        let temp_dir = tempdir().expect("temp dir should exist");
        let skill_root = temp_dir.path().join("skill");
        fs::create_dir_all(&skill_root).expect("skill root should exist");

        let result = resolve_skill_resource_path(&skill_root, "/etc/passwd");
        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_skill_resource_path_rejects_parent_traversal() {
        let temp_dir = tempdir().expect("temp dir should exist");
        let skill_root = temp_dir.path().join("skill");
        fs::create_dir_all(skill_root.join("references")).expect("references should exist");

        let result = resolve_skill_resource_path(&skill_root, "../outside.txt");
        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_skill_resource_path_stays_within_skill_root() {
        let temp_dir = tempdir().expect("temp dir should exist");
        let skill_root = temp_dir.path().join("skill");
        let references_dir = skill_root.join("references");
        fs::create_dir_all(&references_dir).expect("references dir should exist");
        let guide = references_dir.join("guide.md");
        fs::write(&guide, "guide").expect("guide should be written");

        let resolved = resolve_skill_resource_path(&skill_root, "references/guide.md")
            .expect("resource path should resolve");
        assert!(resolved.starts_with(skill_root.canonicalize().expect("canonical root")));
        assert!(resolved.ends_with("guide.md"));
    }

    #[test]
    fn test_enumerate_skill_resources_only_returns_supported_directories() {
        let temp_dir = tempdir().expect("temp dir should exist");
        let skill_root = temp_dir.path().join("skill");
        fs::create_dir_all(skill_root.join("scripts")).expect("scripts dir should exist");
        fs::create_dir_all(skill_root.join("references")).expect("references dir should exist");
        fs::create_dir_all(skill_root.join("assets")).expect("assets dir should exist");
        fs::create_dir_all(skill_root.join("other")).expect("other dir should exist");

        fs::write(skill_root.join("scripts").join("build.sh"), "echo hi")
            .expect("script should be written");
        fs::write(skill_root.join("references").join("guide.md"), "guide")
            .expect("reference should be written");
        fs::write(skill_root.join("assets").join("logo.png"), "png")
            .expect("asset should be written");
        fs::write(skill_root.join("other").join("ignored.txt"), "ignored")
            .expect("ignored file should be written");

        let resources =
            enumerate_skill_resources(&skill_root).expect("resource enumeration should succeed");

        assert_eq!(resources.scripts.len(), 1);
        assert_eq!(resources.references.len(), 1);
        assert_eq!(resources.assets.len(), 1);

        let all = resources.all();
        assert_eq!(all.len(), 3);
        assert!(!all.iter().any(|path| path.ends_with("ignored.txt")));
    }

    #[test]
    fn test_enumerate_skill_resources_stays_within_skill_root() {
        let temp_dir = tempdir().expect("temp dir should exist");
        let skill_root = temp_dir.path().join("skill");
        let scripts_dir = skill_root.join("scripts");
        fs::create_dir_all(&scripts_dir).expect("scripts dir should exist");
        fs::write(scripts_dir.join("build.sh"), "echo hi").expect("script should be written");

        let resources =
            enumerate_skill_resources(&skill_root).expect("resource enumeration should succeed");
        let canonical_root = skill_root
            .canonicalize()
            .expect("canonical root should exist");

        for path in resources.all() {
            assert!(path.starts_with(&canonical_root));
        }
    }

    #[test]
    fn test_load_trusted_paths_returns_paths_from_persistent_store() {
        let temp_dir = tempdir().expect("temp dir should exist");
        let trusted_root = temp_dir.path().join("project");
        fs::create_dir_all(&trusted_root).expect("trusted root should exist");

        let store_path = temp_dir.path().join("skills_trust.yaml");
        let mut store = SkillTrustStore::new(store_path.clone()).expect("store should be created");
        let canonical = store
            .add_path(&trusted_root)
            .expect("trusted root should be added");

        let skills_config = crate::config::SkillsConfig {
            trust_store_path: Some(store_path.to_string_lossy().to_string()),
            ..crate::config::SkillsConfig::default()
        };

        let trusted =
            load_trusted_paths(&skills_config, temp_dir.path()).expect("trusted paths should load");
        assert!(trusted.contains(&canonical));
    }

    #[test]
    fn test_filter_visible_skill_records_omits_untrusted_project_skill() {
        let catalog = {
            let mut catalog = SkillCatalog::new();
            catalog.insert(crate::skills::SkillRecord {
                metadata: crate::skills::SkillMetadata {
                    name: "project_skill".to_string(),
                    description: "Project description".to_string(),
                    license: None,
                    compatibility: None,
                    metadata: std::collections::BTreeMap::new(),
                    allowed_tools_raw: Some("read_file".to_string()),
                    allowed_tools: vec!["read_file".to_string()],
                },
                skill_dir: PathBuf::from("/workspace/project/.xzatoma/skills/project_skill"),
                skill_file: PathBuf::from(
                    "/workspace/project/.xzatoma/skills/project_skill/SKILL.md",
                ),
                source_scope: crate::skills::SkillSourceScope::ProjectClientSpecific,
                source_order: 0,
                body: "Project body".to_string(),
            });
            catalog
        };

        let config = crate::config::SkillsConfig {
            project_trust_required: true,
            ..crate::config::SkillsConfig::default()
        };

        let visible = filter_visible_skill_records(
            &catalog,
            &config,
            Path::new("/workspace/project"),
            &BTreeSet::new(),
        );
        assert!(visible.is_empty());
    }

    #[test]
    fn test_filter_visible_skill_records_includes_trusted_project_skill() {
        let catalog = {
            let mut catalog = SkillCatalog::new();
            catalog.insert(crate::skills::SkillRecord {
                metadata: crate::skills::SkillMetadata {
                    name: "project_skill".to_string(),
                    description: "Project description".to_string(),
                    license: None,
                    compatibility: None,
                    metadata: std::collections::BTreeMap::new(),
                    allowed_tools_raw: Some("read_file".to_string()),
                    allowed_tools: vec!["read_file".to_string()],
                },
                skill_dir: PathBuf::from("/workspace/project/.xzatoma/skills/project_skill"),
                skill_file: PathBuf::from(
                    "/workspace/project/.xzatoma/skills/project_skill/SKILL.md",
                ),
                source_scope: crate::skills::SkillSourceScope::ProjectClientSpecific,
                source_order: 0,
                body: "Project body".to_string(),
            });
            catalog
        };

        let mut trusted = BTreeSet::new();
        trusted.insert(PathBuf::from("/workspace/project"));

        let config = crate::config::SkillsConfig {
            project_trust_required: true,
            ..crate::config::SkillsConfig::default()
        };

        let visible = filter_visible_skill_records(
            &catalog,
            &config,
            Path::new("/workspace/project"),
            &trusted,
        );
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].metadata.name, "project_skill");
    }

    #[test]
    fn test_filter_visible_skill_records_custom_path_trust_behavior() {
        let catalog = {
            let mut catalog = SkillCatalog::new();
            catalog.insert(crate::skills::SkillRecord {
                metadata: crate::skills::SkillMetadata {
                    name: "custom_skill".to_string(),
                    description: "Custom description".to_string(),
                    license: None,
                    compatibility: None,
                    metadata: std::collections::BTreeMap::new(),
                    allowed_tools_raw: Some("read_file".to_string()),
                    allowed_tools: vec!["read_file".to_string()],
                },
                skill_dir: PathBuf::from("/opt/xzatoma/custom_skills/custom_skill"),
                skill_file: PathBuf::from("/opt/xzatoma/custom_skills/custom_skill/SKILL.md"),
                source_scope: crate::skills::SkillSourceScope::CustomConfigured,
                source_order: 0,
                body: "Custom body".to_string(),
            });
            catalog
        };

        let default_config = crate::config::SkillsConfig::default();
        let hidden = filter_visible_skill_records(
            &catalog,
            &default_config,
            Path::new("/workspace/project"),
            &BTreeSet::new(),
        );
        assert!(hidden.is_empty());

        let bypass_config = crate::config::SkillsConfig {
            allow_custom_paths_without_trust: true,
            ..crate::config::SkillsConfig::default()
        };
        let visible = filter_visible_skill_records(
            &catalog,
            &bypass_config,
            Path::new("/workspace/project"),
            &BTreeSet::new(),
        );
        assert_eq!(visible.len(), 1);

        let mut trusted = BTreeSet::new();
        trusted.insert(PathBuf::from("/opt/xzatoma/custom_skills"));
        let trusted_visible = filter_visible_skill_records(
            &catalog,
            &default_config,
            Path::new("/workspace/project"),
            &trusted,
        );
        assert_eq!(trusted_visible.len(), 1);
    }
}

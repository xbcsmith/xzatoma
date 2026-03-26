//! Active skill registry for Phase 3 skill activation.
//!
//! This module implements the runtime registry used to track active skills
//! during a single process/session. Activated skills are kept separate from
//! `Conversation.messages` so prompt assembly can inject them on demand without
//! polluting persisted conversation history.
//!
//! The registry is intentionally simple:
//!
//! - skills are tracked by canonical skill name
//! - activation is deduplicated by skill name
//! - activation records contain prompt-injection-ready content
//! - activation operates only on valid loaded catalog entries
//!
//! # Examples
//!
//! ```
//! use std::collections::BTreeMap;
//! use std::path::PathBuf;
//! use xzatoma::skills::activation::{ActiveSkillRegistry, ActiveSkill};
//! use xzatoma::skills::{SkillCatalog, SkillMetadata, SkillRecord, SkillSourceScope};
//!
//! let mut catalog = SkillCatalog::new();
//! catalog.insert(SkillRecord {
//!     metadata: SkillMetadata {
//!         name: "example_skill".to_string(),
//!         description: "Example description".to_string(),
//!         license: None,
//!         compatibility: None,
//!         metadata: BTreeMap::new(),
//!         allowed_tools_raw: Some("read_file, grep".to_string()),
//!         allowed_tools: vec!["read_file".to_string(), "grep".to_string()],
//!     },
//!     skill_dir: PathBuf::from("/tmp/example_skill"),
//!     skill_file: PathBuf::from("/tmp/example_skill/SKILL.md"),
//!     source_scope: SkillSourceScope::UserClientSpecific,
//!     source_order: 0,
//!     body: "Use this skill body".to_string(),
//! });
//!
//! let mut registry = ActiveSkillRegistry::new();
//! let activation = registry.activate(&catalog, "example_skill")?;
//!
//! let active_skill = activation.active_skill().expect("active skill payload");
//! assert_eq!(active_skill.skill_name, "example_skill");
//! assert_eq!(registry.len(), 1);
//! assert!(registry.is_active("example_skill"));
//! # Ok::<(), anyhow::Error>(())
//! ```
use crate::error::{Result, XzatomaError};
use crate::skills::{SkillCatalog, SkillRecord};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Activated skill record stored in the runtime registry.
///
/// This structure contains the metadata and body content needed for
/// prompt-layer injection after activation.
///
/// # Examples
///
/// ```
/// use std::path::PathBuf;
/// use xzatoma::skills::activation::ActiveSkill;
///
/// let skill = ActiveSkill {
///     skill_name: "example_skill".to_string(),
///     skill_directory: PathBuf::from("/tmp/example_skill"),
///     skill_file: PathBuf::from("/tmp/example_skill/SKILL.md"),
///     description: "Example description".to_string(),
///     allowed_tools: vec!["read_file".to_string(), "grep".to_string()],
///     body_content: "Use this skill body".to_string(),
///     resources: Vec::new(),
/// };
///
/// assert_eq!(skill.skill_name, "example_skill");
/// assert_eq!(skill.allowed_tools.len(), 2);
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveSkill {
    /// Canonical skill name.
    pub skill_name: String,
    /// Absolute path to the skill directory.
    pub skill_directory: PathBuf,
    /// Absolute path to the `SKILL.md` file.
    pub skill_file: PathBuf,
    /// Human-readable skill description.
    pub description: String,
    /// Normalized advisory allowed-tools list.
    pub allowed_tools: Vec<String>,
    /// Full activated body content for prompt injection.
    pub body_content: String,
    /// Optional enumerated resource list.
    ///
    /// Phase 3 does not yet resolve separate resources, so this defaults to an
    /// empty list and can be populated in later phases.
    pub resources: Vec<PathBuf>,
}

impl ActiveSkill {
    /// Builds a prompt-injection-ready block for this activated skill.
    ///
    /// # Returns
    ///
    /// Returns a formatted string suitable for system-prompt injection.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::PathBuf;
    /// use xzatoma::skills::activation::ActiveSkill;
    ///
    /// let skill = ActiveSkill {
    ///     skill_name: "example_skill".to_string(),
    ///     skill_directory: PathBuf::from("/tmp/example_skill"),
    ///     skill_file: PathBuf::from("/tmp/example_skill/SKILL.md"),
    ///     description: "Example description".to_string(),
    ///     allowed_tools: vec!["read_file".to_string()],
    ///     body_content: "Use this skill body".to_string(),
    ///     resources: Vec::new(),
    /// };
    ///
    /// let rendered = skill.render_for_prompt_injection();
    /// assert!(rendered.contains("example_skill"));
    /// assert!(rendered.contains("Use this skill body"));
    /// ```
    pub fn render_for_prompt_injection(&self) -> String {
        let allowed_tools = if self.allowed_tools.is_empty() {
            "none".to_string()
        } else {
            self.allowed_tools.join(", ")
        };

        let resources = if self.resources.is_empty() {
            "none".to_string()
        } else {
            self.resources
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        };

        format!(
            "### Active Skill: {name}\n\
             - Description: {description}\n\
             - Directory: {directory}\n\
             - Allowed Tools: {allowed_tools}\n\
             - Resources: {resources}\n\n\
             {body}",
            name = self.skill_name,
            description = self.description,
            directory = self.skill_directory.display(),
            allowed_tools = allowed_tools,
            resources = resources,
            body = self.body_content,
        )
    }
}

/// Result of a skill activation attempt.
///
/// This indicates whether a skill was newly activated or was already active and
/// simply reused from the registry.
///
/// # Examples
///
/// ```
/// use std::path::PathBuf;
/// use xzatoma::skills::activation::{ActiveSkill, ActivationStatus};
///
/// let skill = ActiveSkill {
///     skill_name: "example_skill".to_string(),
///     skill_directory: PathBuf::from("/tmp/example_skill"),
///     skill_file: PathBuf::from("/tmp/example_skill/SKILL.md"),
///     description: "Example description".to_string(),
///     allowed_tools: vec!["read_file".to_string()],
///     body_content: "Use this skill body".to_string(),
///     resources: Vec::new(),
/// };
///
/// let status = ActivationStatus::Activated(skill.clone());
/// assert!(status.active_skill().is_some());
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActivationStatus {
    /// The skill was newly activated and inserted into the registry.
    Activated(ActiveSkill),
    /// The skill was already active and no duplicate entry was created.
    AlreadyActive(ActiveSkill),
}

impl ActivationStatus {
    /// Returns a reference to the active skill payload.
    ///
    /// # Returns
    ///
    /// Returns the active skill for either activation outcome.
    pub fn active_skill(&self) -> Option<&ActiveSkill> {
        match self {
            Self::Activated(skill) | Self::AlreadyActive(skill) => Some(skill),
        }
    }

    /// Returns `true` if the activation created a new registry entry.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::PathBuf;
    /// use xzatoma::skills::activation::{ActiveSkill, ActivationStatus};
    ///
    /// let skill = ActiveSkill {
    ///     skill_name: "example_skill".to_string(),
    ///     skill_directory: PathBuf::from("/tmp/example_skill"),
    ///     skill_file: PathBuf::from("/tmp/example_skill/SKILL.md"),
    ///     description: "Example description".to_string(),
    ///     allowed_tools: vec!["read_file".to_string()],
    ///     body_content: "Use this skill body".to_string(),
    ///     resources: Vec::new(),
    /// };
    ///
    /// assert!(ActivationStatus::Activated(skill.clone()).is_new_activation());
    /// assert!(!ActivationStatus::AlreadyActive(skill).is_new_activation());
    /// ```
    pub fn is_new_activation(&self) -> bool {
        matches!(self, Self::Activated(_))
    }
}

/// Runtime registry of active skills for the current session.
///
/// Active skills are stored by canonical skill name and are deduplicated on
/// activation.
///
/// # Examples
///
/// ```
/// use xzatoma::skills::activation::ActiveSkillRegistry;
///
/// let registry = ActiveSkillRegistry::new();
/// assert!(registry.is_empty());
/// assert_eq!(registry.len(), 0);
/// ```
#[derive(Debug, Clone, Default)]
pub struct ActiveSkillRegistry {
    active_skills: BTreeMap<String, ActiveSkill>,
}

impl ActiveSkillRegistry {
    /// Creates an empty active-skill registry.
    ///
    /// # Returns
    ///
    /// Returns a new empty `ActiveSkillRegistry`.
    pub fn new() -> Self {
        Self {
            active_skills: BTreeMap::new(),
        }
    }

    /// Activates a valid skill from the loaded catalog.
    ///
    /// Activation is deduplicated by skill name. If the skill is already active,
    /// the existing activation is returned instead of inserting a duplicate.
    ///
    /// # Arguments
    ///
    /// * `catalog` - Valid loaded skill catalog
    /// * `skill_name` - Canonical skill name to activate
    ///
    /// # Returns
    ///
    /// Returns an `ActivationStatus` indicating whether a new entry was
    /// inserted or an existing one was reused.
    ///
    /// # Errors
    ///
    /// Returns an error if the skill is missing from the valid loaded catalog.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::collections::BTreeMap;
    /// use std::path::PathBuf;
    /// use xzatoma::skills::activation::ActiveSkillRegistry;
    /// use xzatoma::skills::{SkillCatalog, SkillMetadata, SkillRecord, SkillSourceScope};
    ///
    /// let mut catalog = SkillCatalog::new();
    /// catalog.insert(SkillRecord {
    ///     metadata: SkillMetadata {
    ///         name: "example_skill".to_string(),
    ///         description: "Example description".to_string(),
    ///         license: None,
    ///         compatibility: None,
    ///         metadata: BTreeMap::new(),
    ///         allowed_tools_raw: None,
    ///         allowed_tools: vec!["read_file".to_string()],
    ///     },
    ///     skill_dir: PathBuf::from("/tmp/example_skill"),
    ///     skill_file: PathBuf::from("/tmp/example_skill/SKILL.md"),
    ///     source_scope: SkillSourceScope::UserClientSpecific,
    ///     source_order: 0,
    ///     body: "Use this skill body".to_string(),
    /// });
    ///
    /// let mut registry = ActiveSkillRegistry::new();
    /// let status = registry.activate(&catalog, "example_skill")?;
    ///
    /// assert!(status.active_skill().is_some());
    /// assert_eq!(registry.len(), 1);
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn activate(
        &mut self,
        catalog: &SkillCatalog,
        skill_name: &str,
    ) -> Result<ActivationStatus> {
        if let Some(existing) = self.active_skills.get(skill_name) {
            return Ok(ActivationStatus::AlreadyActive(existing.clone()));
        }

        let record = catalog.get(skill_name).ok_or_else(|| {
            XzatomaError::Tool(format!(
                "Cannot activate missing or filtered skill '{}'",
                skill_name
            ))
        })?;

        let active_skill = ActiveSkill::from_skill_record(record);
        self.active_skills
            .insert(skill_name.to_string(), active_skill.clone());

        Ok(ActivationStatus::Activated(active_skill))
    }

    /// Inserts a prebuilt active skill into the registry.
    ///
    /// This is primarily useful for tests and controlled restoration paths.
    ///
    /// # Arguments
    ///
    /// * `skill` - Active skill to insert
    ///
    /// # Returns
    ///
    /// Returns the previously stored active skill if one existed under the same
    /// canonical name.
    pub fn insert(&mut self, skill: ActiveSkill) -> Option<ActiveSkill> {
        self.active_skills.insert(skill.skill_name.clone(), skill)
    }

    /// Returns a currently active skill by name.
    ///
    /// # Arguments
    ///
    /// * `skill_name` - Canonical skill name
    ///
    /// # Returns
    ///
    /// Returns the active skill if present.
    pub fn get(&self, skill_name: &str) -> Option<&ActiveSkill> {
        self.active_skills.get(skill_name)
    }

    /// Returns `true` if the skill is currently active.
    ///
    /// # Arguments
    ///
    /// * `skill_name` - Canonical skill name
    ///
    /// # Returns
    ///
    /// Returns `true` when the registry contains the skill.
    pub fn is_active(&self, skill_name: &str) -> bool {
        self.active_skills.contains_key(skill_name)
    }

    /// Returns the number of active skills.
    ///
    /// # Returns
    ///
    /// Returns the number of active skills currently stored.
    pub fn len(&self) -> usize {
        self.active_skills.len()
    }

    /// Returns `true` if the registry is empty.
    ///
    /// # Returns
    ///
    /// Returns `true` when no skills are active.
    pub fn is_empty(&self) -> bool {
        self.active_skills.is_empty()
    }

    /// Returns active skills in deterministic name order.
    ///
    /// # Returns
    ///
    /// Returns active skill references sorted by canonical skill name.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &ActiveSkill)> {
        self.active_skills
            .iter()
            .map(|(name, skill)| (name.as_str(), skill))
    }

    /// Removes an active skill by name.
    ///
    /// # Arguments
    ///
    /// * `skill_name` - Canonical skill name
    ///
    /// # Returns
    ///
    /// Returns the removed skill if present.
    pub fn remove(&mut self, skill_name: &str) -> Option<ActiveSkill> {
        self.active_skills.remove(skill_name)
    }

    /// Clears all active skills from the registry.
    pub fn clear(&mut self) {
        self.active_skills.clear();
    }

    /// Builds the combined prompt injection block for all active skills.
    ///
    /// Skills are rendered in deterministic canonical-name order.
    ///
    /// # Returns
    ///
    /// Returns `None` when no skills are active, otherwise a formatted block
    /// ready to be injected into prompt assembly.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::PathBuf;
    /// use xzatoma::skills::activation::{ActiveSkill, ActiveSkillRegistry};
    ///
    /// let mut registry = ActiveSkillRegistry::new();
    /// registry.insert(ActiveSkill {
    ///     skill_name: "example_skill".to_string(),
    ///     skill_directory: PathBuf::from("/tmp/example_skill"),
    ///     skill_file: PathBuf::from("/tmp/example_skill/SKILL.md"),
    ///     description: "Example description".to_string(),
    ///     allowed_tools: vec!["read_file".to_string()],
    ///     body_content: "Use this skill body".to_string(),
    ///     resources: Vec::new(),
    /// });
    ///
    /// let prompt = registry.render_for_prompt_injection().unwrap();
    /// assert!(prompt.contains("Active Skills"));
    /// assert!(prompt.contains("example_skill"));
    /// ```
    pub fn render_for_prompt_injection(&self) -> Option<String> {
        if self.active_skills.is_empty() {
            return None;
        }

        let mut sections = Vec::new();
        sections.push("## Active Skills".to_string());
        sections.push(
            "The following skills have been explicitly activated for this session. \
             Follow their guidance when relevant."
                .to_string(),
        );
        sections.push(String::new());

        for (_, skill) in self.iter() {
            sections.push(skill.render_for_prompt_injection());
            sections.push(String::new());
        }

        Some(sections.join("\n").trim().to_string())
    }

    /// Returns the active skill names in deterministic order.
    ///
    /// # Returns
    ///
    /// Returns canonical skill names sorted lexicographically.
    pub fn names(&self) -> Vec<&str> {
        self.active_skills.keys().map(String::as_str).collect()
    }
}

impl ActiveSkill {
    /// Creates an active skill from a valid skill record.
    ///
    /// # Arguments
    ///
    /// * `record` - Valid loaded skill record
    ///
    /// # Returns
    ///
    /// Returns a prompt-injection-ready active skill.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::collections::BTreeMap;
    /// use std::path::PathBuf;
    /// use xzatoma::skills::activation::ActiveSkill;
    /// use xzatoma::skills::{SkillMetadata, SkillRecord, SkillSourceScope};
    ///
    /// let record = SkillRecord {
    ///     metadata: SkillMetadata {
    ///         name: "example_skill".to_string(),
    ///         description: "Example description".to_string(),
    ///         license: None,
    ///         compatibility: None,
    ///         metadata: BTreeMap::new(),
    ///         allowed_tools_raw: None,
    ///         allowed_tools: vec!["read_file".to_string()],
    ///     },
    ///     skill_dir: PathBuf::from("/tmp/example_skill"),
    ///     skill_file: PathBuf::from("/tmp/example_skill/SKILL.md"),
    ///     source_scope: SkillSourceScope::UserClientSpecific,
    ///     source_order: 0,
    ///     body: "Use this skill body".to_string(),
    /// };
    ///
    /// let active = ActiveSkill::from_skill_record(&record);
    /// assert_eq!(active.skill_name, "example_skill");
    /// assert_eq!(active.body_content, "Use this skill body");
    /// ```
    pub fn from_skill_record(record: &SkillRecord) -> Self {
        Self {
            skill_name: record.metadata.name.clone(),
            skill_directory: record.skill_dir.clone(),
            skill_file: record.skill_file.clone(),
            description: record.metadata.description.clone(),
            allowed_tools: record.metadata.allowed_tools.clone(),
            body_content: record.body.clone(),
            resources: Vec::new(),
        }
    }

    /// Returns the skill directory as a `Path`.
    ///
    /// # Returns
    ///
    /// Returns the active skill directory path.
    pub fn directory_path(&self) -> &Path {
        &self.skill_directory
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::{SkillMetadata, SkillSourceScope};
    use std::collections::BTreeMap;

    fn sample_record(name: &str, body: &str) -> SkillRecord {
        SkillRecord {
            metadata: SkillMetadata {
                name: name.to_string(),
                description: format!("Description for {}", name),
                license: None,
                compatibility: None,
                metadata: BTreeMap::new(),
                allowed_tools_raw: Some("read_file, grep".to_string()),
                allowed_tools: vec!["read_file".to_string(), "grep".to_string()],
            },
            skill_dir: PathBuf::from(format!("/tmp/{}", name)),
            skill_file: PathBuf::from(format!("/tmp/{}/SKILL.md", name)),
            source_scope: SkillSourceScope::UserClientSpecific,
            source_order: 0,
            body: body.to_string(),
        }
    }

    fn sample_catalog() -> SkillCatalog {
        let mut catalog = SkillCatalog::new();
        catalog.insert(sample_record("alpha_skill", "Alpha body"));
        catalog.insert(sample_record("beta_skill", "Beta body"));
        catalog
    }

    #[test]
    fn test_active_skill_from_skill_record() {
        let record = sample_record("example_skill", "Body content");
        let active = ActiveSkill::from_skill_record(&record);

        assert_eq!(active.skill_name, "example_skill");
        assert_eq!(active.description, "Description for example_skill");
        assert_eq!(active.allowed_tools, vec!["read_file", "grep"]);
        assert_eq!(active.body_content, "Body content");
        assert!(active.resources.is_empty());
    }

    #[test]
    fn test_active_skill_render_for_prompt_injection() {
        let active = ActiveSkill {
            skill_name: "example_skill".to_string(),
            skill_directory: PathBuf::from("/tmp/example_skill"),
            skill_file: PathBuf::from("/tmp/example_skill/SKILL.md"),
            description: "Example description".to_string(),
            allowed_tools: vec!["read_file".to_string()],
            body_content: "Use this skill body".to_string(),
            resources: Vec::new(),
        };

        let rendered = active.render_for_prompt_injection();
        assert!(rendered.contains("example_skill"));
        assert!(rendered.contains("Example description"));
        assert!(rendered.contains("read_file"));
        assert!(rendered.contains("Use this skill body"));
    }

    #[test]
    fn test_activation_status_is_new_activation() {
        let active = ActiveSkill {
            skill_name: "example_skill".to_string(),
            skill_directory: PathBuf::from("/tmp/example_skill"),
            skill_file: PathBuf::from("/tmp/example_skill/SKILL.md"),
            description: "Example description".to_string(),
            allowed_tools: vec!["read_file".to_string()],
            body_content: "Use this skill body".to_string(),
            resources: Vec::new(),
        };

        assert!(ActivationStatus::Activated(active.clone()).is_new_activation());
        assert!(!ActivationStatus::AlreadyActive(active).is_new_activation());
    }

    #[test]
    fn test_active_skill_registry_new_starts_empty() {
        let registry = ActiveSkillRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn test_active_skill_registry_activate_inserts_new_skill() {
        let catalog = sample_catalog();
        let mut registry = ActiveSkillRegistry::new();

        let status = registry
            .activate(&catalog, "alpha_skill")
            .expect("activation should succeed");

        assert!(matches!(status, ActivationStatus::Activated(_)));
        assert_eq!(registry.len(), 1);
        assert!(registry.is_active("alpha_skill"));
    }

    #[test]
    fn test_active_skill_registry_activate_deduplicates_existing_skill() {
        let catalog = sample_catalog();
        let mut registry = ActiveSkillRegistry::new();

        let first = registry
            .activate(&catalog, "alpha_skill")
            .expect("first activation should succeed");
        let second = registry
            .activate(&catalog, "alpha_skill")
            .expect("second activation should succeed");

        assert!(matches!(first, ActivationStatus::Activated(_)));
        assert!(matches!(second, ActivationStatus::AlreadyActive(_)));
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn test_active_skill_registry_activate_missing_skill_fails() {
        let catalog = sample_catalog();
        let mut registry = ActiveSkillRegistry::new();

        let result = registry.activate(&catalog, "missing_skill");
        assert!(result.is_err());
        assert!(registry.is_empty());
    }

    #[test]
    fn test_active_skill_registry_get_returns_active_skill() {
        let catalog = sample_catalog();
        let mut registry = ActiveSkillRegistry::new();
        registry
            .activate(&catalog, "alpha_skill")
            .expect("activation should succeed");

        let active = registry.get("alpha_skill");
        assert!(active.is_some());
        assert_eq!(
            active.map(|skill| skill.description.as_str()),
            Some("Description for alpha_skill")
        );
    }

    #[test]
    fn test_active_skill_registry_names_are_deterministic() {
        let catalog = sample_catalog();
        let mut registry = ActiveSkillRegistry::new();
        registry
            .activate(&catalog, "beta_skill")
            .expect("activation should succeed");
        registry
            .activate(&catalog, "alpha_skill")
            .expect("activation should succeed");

        assert_eq!(registry.names(), vec!["alpha_skill", "beta_skill"]);
    }

    #[test]
    fn test_active_skill_registry_render_for_prompt_injection_empty_returns_none() {
        let registry = ActiveSkillRegistry::new();
        assert!(registry.render_for_prompt_injection().is_none());
    }

    #[test]
    fn test_active_skill_registry_render_for_prompt_injection_contains_active_skills() {
        let catalog = sample_catalog();
        let mut registry = ActiveSkillRegistry::new();
        registry
            .activate(&catalog, "alpha_skill")
            .expect("activation should succeed");
        registry
            .activate(&catalog, "beta_skill")
            .expect("activation should succeed");

        let rendered = registry
            .render_for_prompt_injection()
            .expect("rendered prompt should exist");

        assert!(rendered.contains("## Active Skills"));
        assert!(rendered.contains("alpha_skill"));
        assert!(rendered.contains("Alpha body"));
        assert!(rendered.contains("beta_skill"));
        assert!(rendered.contains("Beta body"));
    }

    #[test]
    fn test_active_skill_registry_remove_deletes_skill() {
        let catalog = sample_catalog();
        let mut registry = ActiveSkillRegistry::new();
        registry
            .activate(&catalog, "alpha_skill")
            .expect("activation should succeed");

        let removed = registry.remove("alpha_skill");
        assert!(removed.is_some());
        assert!(!registry.is_active("alpha_skill"));
        assert!(registry.is_empty());
    }

    #[test]
    fn test_active_skill_registry_clear_removes_all_skills() {
        let catalog = sample_catalog();
        let mut registry = ActiveSkillRegistry::new();
        registry
            .activate(&catalog, "alpha_skill")
            .expect("activation should succeed");
        registry
            .activate(&catalog, "beta_skill")
            .expect("activation should succeed");

        registry.clear();
        assert!(registry.is_empty());
        assert!(registry.render_for_prompt_injection().is_none());
    }

    #[test]
    fn test_active_skill_directory_path_returns_directory() {
        let active = ActiveSkill {
            skill_name: "example_skill".to_string(),
            skill_directory: PathBuf::from("/tmp/example_skill"),
            skill_file: PathBuf::from("/tmp/example_skill/SKILL.md"),
            description: "Example description".to_string(),
            allowed_tools: vec!["read_file".to_string()],
            body_content: "Use this skill body".to_string(),
            resources: Vec::new(),
        };

        assert_eq!(active.directory_path(), Path::new("/tmp/example_skill"));
    }
}

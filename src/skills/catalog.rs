//! Skill catalog for loaded and validated skills.
//!
//! This module provides a deterministic wrapper around valid discovered skills.
//! Only valid, non-shadowed skills should be inserted into the catalog.

use crate::error::{Result, XzatomaError};
use crate::skills::types::SkillRecord;
use std::collections::BTreeMap;

/// Catalog of valid discovered skills.
///
/// This structure stores only the winning valid skill for each unique skill
/// name after deterministic precedence and collision resolution have been
/// applied.
///
/// Invalid skills are not inserted into the catalog. They should instead be
/// surfaced through diagnostics returned by discovery and validation flows.
///
/// # Examples
///
/// ```
/// use std::collections::BTreeMap;
/// use std::path::PathBuf;
/// use xzatoma::skills::catalog::SkillCatalog;
/// use xzatoma::skills::types::{SkillMetadata, SkillRecord, SkillSourceScope};
///
/// let mut catalog = SkillCatalog::new();
///
/// let record = SkillRecord {
///     metadata: SkillMetadata {
///         name: "example_skill".to_string(),
///         description: "Example description".to_string(),
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
/// catalog.insert(record);
///
/// assert_eq!(catalog.len(), 1);
/// assert!(catalog.get("example_skill").is_some());
/// ```
#[derive(Debug, Clone, Default)]
pub struct SkillCatalog {
    skills: BTreeMap<String, SkillRecord>,
}

impl SkillCatalog {
    /// Creates an empty skill catalog.
    ///
    /// # Returns
    ///
    /// Returns a new empty `SkillCatalog`.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::skills::catalog::SkillCatalog;
    ///
    /// let catalog = SkillCatalog::new();
    /// assert!(catalog.is_empty());
    /// ```
    pub fn new() -> Self {
        Self {
            skills: BTreeMap::new(),
        }
    }

    /// Builds a catalog from pre-validated skill records.
    ///
    /// This constructor rejects duplicate skill names because the catalog is
    /// expected to contain only precedence-resolved winners.
    ///
    /// # Arguments
    ///
    /// * `records` - Valid skill records to store in the catalog
    ///
    /// # Returns
    ///
    /// Returns a populated `SkillCatalog`.
    ///
    /// # Errors
    ///
    /// Returns an error if duplicate skill names are provided.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::collections::BTreeMap;
    /// use std::path::PathBuf;
    /// use xzatoma::skills::catalog::SkillCatalog;
    /// use xzatoma::skills::types::{SkillMetadata, SkillRecord, SkillSourceScope};
    ///
    /// let records = vec![SkillRecord {
    ///     metadata: SkillMetadata {
    ///         name: "example_skill".to_string(),
    ///         description: "Example description".to_string(),
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
    /// }];
    ///
    /// let catalog = SkillCatalog::from_records(records).unwrap();
    /// assert_eq!(catalog.len(), 1);
    /// ```
    pub fn from_records(records: Vec<SkillRecord>) -> Result<Self> {
        let mut catalog = Self::new();

        for record in records {
            catalog.try_insert(record)?;
        }

        Ok(catalog)
    }

    /// Inserts a valid skill record into the catalog, replacing any existing
    /// entry with the same name.
    ///
    /// This method is useful when the caller intentionally wants replacement
    /// semantics.
    ///
    /// # Arguments
    ///
    /// * `record` - Valid skill record to insert
    ///
    /// # Returns
    ///
    /// Returns the previous record if one existed for the same name.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::collections::BTreeMap;
    /// use std::path::PathBuf;
    /// use xzatoma::skills::catalog::SkillCatalog;
    /// use xzatoma::skills::types::{SkillMetadata, SkillRecord, SkillSourceScope};
    ///
    /// let mut catalog = SkillCatalog::new();
    ///
    /// let record = SkillRecord {
    ///     metadata: SkillMetadata {
    ///         name: "example_skill".to_string(),
    ///         description: "Example description".to_string(),
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
    /// let previous = catalog.insert(record);
    /// assert!(previous.is_none());
    /// ```
    pub fn insert(&mut self, record: SkillRecord) -> Option<SkillRecord> {
        self.skills.insert(record.metadata.name.clone(), record)
    }

    /// Inserts a valid skill record into the catalog and rejects duplicates.
    ///
    /// # Arguments
    ///
    /// * `record` - Valid skill record to insert
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` when the record is inserted.
    ///
    /// # Errors
    ///
    /// Returns an error if a record with the same skill name already exists.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::collections::BTreeMap;
    /// use std::path::PathBuf;
    /// use xzatoma::skills::catalog::SkillCatalog;
    /// use xzatoma::skills::types::{SkillMetadata, SkillRecord, SkillSourceScope};
    ///
    /// let mut catalog = SkillCatalog::new();
    ///
    /// let record = SkillRecord {
    ///     metadata: SkillMetadata {
    ///         name: "example_skill".to_string(),
    ///         description: "Example description".to_string(),
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
    /// assert!(catalog.try_insert(record).is_ok());
    /// ```
    pub fn try_insert(&mut self, record: SkillRecord) -> Result<()> {
        let name = record.metadata.name.clone();

        if self.skills.contains_key(&name) {
            return Err(XzatomaError::Config(format!(
                "duplicate skill '{}' cannot be inserted into catalog",
                name
            ))
            .into());
        }

        self.skills.insert(name, record);
        Ok(())
    }

    /// Returns a skill record by name.
    ///
    /// # Arguments
    ///
    /// * `name` - Skill name to look up
    ///
    /// # Returns
    ///
    /// Returns the matching `SkillRecord` when present.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::skills::catalog::SkillCatalog;
    ///
    /// let catalog = SkillCatalog::new();
    /// assert!(catalog.get("missing_skill").is_none());
    /// ```
    pub fn get(&self, name: &str) -> Option<&SkillRecord> {
        self.skills.get(name)
    }

    /// Returns `true` if the catalog contains a skill with the given name.
    ///
    /// # Arguments
    ///
    /// * `name` - Skill name to check
    ///
    /// # Returns
    ///
    /// Returns `true` when the skill exists in the catalog.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::skills::catalog::SkillCatalog;
    ///
    /// let catalog = SkillCatalog::new();
    /// assert!(!catalog.contains("missing_skill"));
    /// ```
    pub fn contains(&self, name: &str) -> bool {
        self.skills.contains_key(name)
    }

    /// Returns the number of valid skills in the catalog.
    ///
    /// # Returns
    ///
    /// Returns the count of loaded valid skills.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::skills::catalog::SkillCatalog;
    ///
    /// let catalog = SkillCatalog::new();
    /// assert_eq!(catalog.len(), 0);
    /// ```
    pub fn len(&self) -> usize {
        self.skills.len()
    }

    /// Returns `true` when the catalog has no skills.
    ///
    /// # Returns
    ///
    /// Returns `true` if no valid skills are stored.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::skills::catalog::SkillCatalog;
    ///
    /// let catalog = SkillCatalog::new();
    /// assert!(catalog.is_empty());
    /// ```
    pub fn is_empty(&self) -> bool {
        self.skills.is_empty()
    }

    /// Returns an iterator over skill names and records in deterministic order.
    ///
    /// The ordering is lexicographic by skill name.
    ///
    /// # Returns
    ///
    /// Returns an iterator over all catalog entries.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &SkillRecord)> {
        self.skills
            .iter()
            .map(|(name, record)| (name.as_str(), record))
    }

    /// Returns all skill records in deterministic order.
    ///
    /// # Returns
    ///
    /// Returns a vector of references to skill records sorted by skill name.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::skills::catalog::SkillCatalog;
    ///
    /// let catalog = SkillCatalog::new();
    /// let records = catalog.records();
    /// assert!(records.is_empty());
    /// ```
    pub fn records(&self) -> Vec<&SkillRecord> {
        self.skills.values().collect()
    }

    /// Returns all skill names in deterministic order.
    ///
    /// # Returns
    ///
    /// Returns a vector of skill names sorted lexicographically.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::skills::catalog::SkillCatalog;
    ///
    /// let catalog = SkillCatalog::new();
    /// let names = catalog.names();
    /// assert!(names.is_empty());
    /// ```
    pub fn names(&self) -> Vec<&str> {
        self.skills.keys().map(String::as_str).collect()
    }

    /// Consumes the catalog and returns the underlying records map.
    ///
    /// # Returns
    ///
    /// Returns the underlying deterministic map keyed by skill name.
    pub fn into_inner(self) -> BTreeMap<String, SkillRecord> {
        self.skills
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::types::{SkillMetadata, SkillSourceScope};
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    fn sample_record(name: &str) -> SkillRecord {
        SkillRecord {
            metadata: SkillMetadata {
                name: name.to_string(),
                description: format!("Description for {}", name),
                license: None,
                compatibility: None,
                metadata: BTreeMap::new(),
                allowed_tools_raw: None,
                allowed_tools: Vec::new(),
            },
            skill_dir: PathBuf::from(format!("/tmp/{}", name)),
            skill_file: PathBuf::from(format!("/tmp/{}/SKILL.md", name)),
            source_scope: SkillSourceScope::ProjectClientSpecific,
            source_order: 0,
            body: "Body".to_string(),
        }
    }

    #[test]
    fn test_skill_catalog_new_starts_empty() {
        let catalog = SkillCatalog::new();
        assert!(catalog.is_empty());
        assert_eq!(catalog.len(), 0);
    }

    #[test]
    fn test_skill_catalog_insert_and_get() {
        let mut catalog = SkillCatalog::new();
        catalog.insert(sample_record("example_skill"));

        let loaded = catalog.get("example_skill");
        assert!(loaded.is_some());
        assert_eq!(
            loaded.map(|skill| skill.metadata.description.as_str()),
            Some("Description for example_skill")
        );
    }

    #[test]
    fn test_skill_catalog_contains_returns_true_for_inserted_skill() {
        let mut catalog = SkillCatalog::new();
        catalog.insert(sample_record("example_skill"));

        assert!(catalog.contains("example_skill"));
        assert!(!catalog.contains("missing_skill"));
    }

    #[test]
    fn test_skill_catalog_insert_replaces_duplicate_name() {
        let mut catalog = SkillCatalog::new();
        let mut replacement = sample_record("example_skill");
        replacement.metadata.description = "Replacement description".to_string();

        let previous = catalog.insert(sample_record("example_skill"));
        assert!(previous.is_none());

        let previous = catalog.insert(replacement);
        assert!(previous.is_some());
        assert_eq!(catalog.len(), 1);
        assert_eq!(
            catalog
                .get("example_skill")
                .map(|skill| skill.metadata.description.as_str()),
            Some("Replacement description")
        );
    }

    #[test]
    fn test_skill_catalog_try_insert_rejects_duplicate_name() {
        let mut catalog = SkillCatalog::new();
        assert!(catalog.try_insert(sample_record("example_skill")).is_ok());

        let duplicate = catalog.try_insert(sample_record("example_skill"));
        assert!(duplicate.is_err());
    }

    #[test]
    fn test_skill_catalog_names_are_deterministic() {
        let mut catalog = SkillCatalog::new();
        catalog.insert(sample_record("zeta_skill"));
        catalog.insert(sample_record("alpha_skill"));

        assert_eq!(catalog.names(), vec!["alpha_skill", "zeta_skill"]);
    }

    #[test]
    fn test_skill_catalog_from_records_populates_catalog() {
        let catalog = SkillCatalog::from_records(vec![
            sample_record("alpha_skill"),
            sample_record("beta_skill"),
        ])
        .unwrap();

        assert_eq!(catalog.len(), 2);
        assert!(catalog.contains("alpha_skill"));
        assert!(catalog.contains("beta_skill"));
    }

    #[test]
    fn test_skill_catalog_from_records_rejects_duplicates() {
        let result = SkillCatalog::from_records(vec![
            sample_record("alpha_skill"),
            sample_record("alpha_skill"),
        ]);

        assert!(result.is_err());
    }

    #[test]
    fn test_skill_catalog_records_are_sorted_by_name() {
        let mut catalog = SkillCatalog::new();
        catalog.insert(sample_record("gamma_skill"));
        catalog.insert(sample_record("alpha_skill"));

        let names: Vec<&str> = catalog
            .records()
            .iter()
            .map(|record| record.metadata.name.as_str())
            .collect();
        assert_eq!(names, vec!["alpha_skill", "gamma_skill"]);
    }

    #[test]
    fn test_skill_catalog_into_inner_returns_map() {
        let mut catalog = SkillCatalog::new();
        catalog.insert(sample_record("example_skill"));

        let inner = catalog.into_inner();
        assert_eq!(inner.len(), 1);
        assert!(inner.contains_key("example_skill"));
    }
}

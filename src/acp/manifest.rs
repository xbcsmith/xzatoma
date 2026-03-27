/// ACP manifest protocol models.
///
/// This module defines transport-independent ACP manifest types used to describe
/// an ACP-capable XZatoma agent. The structures are protocol-facing, serializable
/// with `serde`, and intentionally decoupled from HTTP and persistence concerns.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::manifest::{AcpAgentCapability, AcpAgentManifest};
///
/// let manifest = AcpAgentManifest::new(
///     "xzatoma".to_string(),
///     "0.2.0".to_string(),
///     "XZatoma ACP Agent".to_string(),
/// );
///
/// assert_eq!(manifest.name, "xzatoma");
/// assert!(manifest
///     .capabilities
///     .contains(&AcpAgentCapability::RunsCreate));
/// ```
use crate::error::{Result, XzatomaError};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Validates an ACP agent name.
///
/// ACP agent names must:
///
/// - be non-empty
/// - be at most 128 characters
/// - start with an ASCII lowercase letter
/// - contain only ASCII lowercase letters, digits, underscores, hyphens, or dots
///
/// # Arguments
///
/// * `value` - Candidate ACP agent name
///
/// # Returns
///
/// Returns `Ok(())` when the name is valid.
///
/// # Errors
///
/// Returns an error if the name violates ACP naming constraints.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::manifest::validate_agent_name;
///
/// assert!(validate_agent_name("xzatoma").is_ok());
/// assert!(validate_agent_name("Invalid Agent").is_err());
/// ```
pub fn validate_agent_name(value: &str) -> Result<()> {
    if value.is_empty() {
        return Err(XzatomaError::AcpValidation(
            "invalid ACP agent name: value cannot be empty".to_string(),
        )
        .into());
    }

    if value.len() > 128 {
        return Err(XzatomaError::AcpValidation(
            "invalid ACP agent name: value cannot exceed 128 characters".to_string(),
        )
        .into());
    }

    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return Err(XzatomaError::AcpValidation(
            "invalid ACP agent name: value cannot be empty".to_string(),
        )
        .into());
    };

    if !first.is_ascii_lowercase() {
        return Err(XzatomaError::AcpValidation(
            "invalid ACP agent name: value must start with an ASCII lowercase letter".to_string(),
        )
        .into());
    }

    if !value.chars().all(|character| {
        character.is_ascii_lowercase()
            || character.is_ascii_digit()
            || matches!(character, '_' | '-' | '.')
    }) {
        return Err(XzatomaError::AcpValidation(
            "invalid ACP agent name: value must contain only ASCII lowercase letters, digits, underscores, hyphens, or dots".to_string(),
        )
        .into());
    }

    Ok(())
}

/// Validates an ACP manifest version string.
///
/// The version is intentionally validated conservatively in Phase 1:
///
/// - it must be non-empty
/// - it must not exceed 64 characters
/// - it must not contain leading or trailing whitespace
///
/// # Arguments
///
/// * `value` - Candidate version string
///
/// # Returns
///
/// Returns `Ok(())` when the version is valid.
///
/// # Errors
///
/// Returns an error if the version is invalid.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::manifest::validate_manifest_version;
///
/// assert!(validate_manifest_version("0.2.0").is_ok());
/// assert!(validate_manifest_version(" ").is_err());
/// ```
pub fn validate_manifest_version(value: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(XzatomaError::AcpValidation(
            "invalid ACP manifest version: value cannot be empty".to_string(),
        )
        .into());
    }

    if value.len() > 64 {
        return Err(XzatomaError::AcpValidation(
            "invalid ACP manifest version: value cannot exceed 64 characters".to_string(),
        )
        .into());
    }

    if value.trim() != value {
        return Err(XzatomaError::AcpValidation(
            "invalid ACP manifest version: value cannot contain leading or trailing whitespace"
                .to_string(),
        )
        .into());
    }

    Ok(())
}

/// Validates an ACP manifest display name.
///
/// # Arguments
///
/// * `value` - Candidate display name
///
/// # Returns
///
/// Returns `Ok(())` when the display name is valid.
///
/// # Errors
///
/// Returns an error if the display name is empty or excessively long.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::manifest::validate_manifest_display_name;
///
/// assert!(validate_manifest_display_name("XZatoma ACP Agent").is_ok());
/// assert!(validate_manifest_display_name("").is_err());
/// ```
pub fn validate_manifest_display_name(value: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(XzatomaError::AcpValidation(
            "invalid ACP manifest display name: value cannot be empty".to_string(),
        )
        .into());
    }

    if value.len() > 256 {
        return Err(XzatomaError::AcpValidation(
            "invalid ACP manifest display name: value cannot exceed 256 characters".to_string(),
        )
        .into());
    }

    Ok(())
}

/// ACP agent capability flags.
///
/// These capabilities describe which ACP surfaces the agent intends to support.
/// Phase 1 keeps them protocol-facing and transport-independent.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::manifest::AcpAgentCapability;
///
/// let capability = AcpAgentCapability::RunsCreate;
/// assert_eq!(capability.as_str(), "runs.create");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AcpAgentCapability {
    /// Supports manifest discovery.
    ManifestRead,
    /// Supports run creation.
    RunsCreate,
    /// Supports run retrieval.
    RunsGet,
    /// Supports run event history.
    RunsEvents,
    /// Supports run cancellation.
    RunsCancel,
    /// Supports session creation or retrieval.
    SessionsGet,
    /// Supports session resume semantics.
    SessionsResume,
}

impl AcpAgentCapability {
    /// Returns the stable ACP capability string.
    ///
    /// # Returns
    ///
    /// Returns the protocol-facing capability identifier.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::acp::manifest::AcpAgentCapability;
    ///
    /// assert_eq!(AcpAgentCapability::ManifestRead.as_str(), "manifest.read");
    /// ```
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ManifestRead => "manifest.read",
            Self::RunsCreate => "runs.create",
            Self::RunsGet => "runs.get",
            Self::RunsEvents => "runs.events",
            Self::RunsCancel => "runs.cancel",
            Self::SessionsGet => "sessions.get",
            Self::SessionsResume => "sessions.resume",
        }
    }
}

/// ACP manifest link entry.
///
/// This structure allows the manifest to advertise relevant external
/// documentation or endpoint references without binding to a specific transport.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::manifest::AcpManifestLink;
///
/// let link = AcpManifestLink::new(
///     "documentation".to_string(),
///     "https://example.com/docs".to_string(),
/// )
/// .unwrap();
///
/// assert_eq!(link.rel, "documentation");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcpManifestLink {
    /// Relationship type for the link.
    pub rel: String,
    /// Target URL for the relationship.
    pub href: String,
    /// Optional human-readable title.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

impl AcpManifestLink {
    /// Creates a new validated manifest link.
    ///
    /// # Arguments
    ///
    /// * `rel` - Link relationship identifier
    /// * `href` - Absolute URL target
    ///
    /// # Returns
    ///
    /// Returns a validated `AcpManifestLink`.
    ///
    /// # Errors
    ///
    /// Returns an error if `rel` or `href` is invalid.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::acp::manifest::AcpManifestLink;
    ///
    /// let link = AcpManifestLink::new(
    ///     "documentation".to_string(),
    ///     "https://example.com/docs".to_string(),
    /// )
    /// .unwrap();
    ///
    /// assert_eq!(link.href, "https://example.com/docs");
    /// ```
    pub fn new(rel: String, href: String) -> Result<Self> {
        validate_non_empty_field(&rel, "manifest.links.rel")?;
        validate_absolute_url(&href, "manifest.links.href")?;

        Ok(Self {
            rel,
            href,
            title: None,
        })
    }

    /// Adds an optional title to the manifest link.
    ///
    /// # Arguments
    ///
    /// * `title` - Human-readable link title
    ///
    /// # Returns
    ///
    /// Returns the updated link.
    ///
    /// # Errors
    ///
    /// Returns an error if the title is empty.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::acp::manifest::AcpManifestLink;
    ///
    /// let link = AcpManifestLink::new(
    ///     "documentation".to_string(),
    ///     "https://example.com/docs".to_string(),
    /// )
    /// .unwrap()
    /// .with_title("ACP Documentation".to_string())
    /// .unwrap();
    ///
    /// assert_eq!(link.title.as_deref(), Some("ACP Documentation"));
    /// ```
    pub fn with_title(mut self, title: String) -> Result<Self> {
        validate_non_empty_field(&title, "manifest.links.title")?;
        self.title = Some(title);
        Ok(self)
    }

    /// Validates the manifest link.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` when the link is valid.
    ///
    /// # Errors
    ///
    /// Returns an error if the link is invalid.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::acp::manifest::AcpManifestLink;
    ///
    /// let link = AcpManifestLink::new(
    ///     "documentation".to_string(),
    ///     "https://example.com/docs".to_string(),
    /// )
    /// .unwrap();
    ///
    /// assert!(link.validate().is_ok());
    /// ```
    pub fn validate(&self) -> Result<()> {
        validate_non_empty_field(&self.rel, "manifest.links.rel")?;
        validate_absolute_url(&self.href, "manifest.links.href")?;

        if let Some(title) = &self.title {
            validate_non_empty_field(title, "manifest.links.title")?;
        }

        Ok(())
    }
}

/// ACP agent manifest.
///
/// This is the Phase 1 protocol-facing description of an ACP-capable XZatoma
/// agent. It is intentionally transport-independent and serializable so later
/// phases can expose it over discovery surfaces.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::manifest::{AcpAgentCapability, AcpAgentManifest};
///
/// let manifest = AcpAgentManifest::new(
///     "xzatoma".to_string(),
///     "0.2.0".to_string(),
///     "XZatoma ACP Agent".to_string(),
/// );
///
/// assert_eq!(manifest.name, "xzatoma");
/// assert!(manifest
///     .capabilities
///     .contains(&AcpAgentCapability::RunsCreate));
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcpAgentManifest {
    /// Stable ACP agent name.
    pub name: String,
    /// Agent implementation version.
    pub version: String,
    /// Human-readable display name.
    pub display_name: String,
    /// Optional descriptive summary.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Declared ACP capabilities.
    pub capabilities: Vec<AcpAgentCapability>,
    /// Optional protocol-facing metadata.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, String>,
    /// Optional external links.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<AcpManifestLink>,
}

impl AcpAgentManifest {
    /// Creates a new ACP agent manifest with sensible Phase 1 defaults.
    ///
    /// The default capability set includes manifest and run-readiness
    /// capabilities that later phases can expose incrementally.
    ///
    /// # Arguments
    ///
    /// * `name` - Stable ACP agent name
    /// * `version` - Agent implementation version
    /// * `display_name` - Human-readable display name
    ///
    /// # Returns
    ///
    /// Returns a manifest initialized with default capabilities.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::acp::manifest::AcpAgentManifest;
    ///
    /// let manifest = AcpAgentManifest::new(
    ///     "xzatoma".to_string(),
    ///     "0.2.0".to_string(),
    ///     "XZatoma ACP Agent".to_string(),
    /// );
    ///
    /// assert_eq!(manifest.name, "xzatoma");
    /// ```
    pub fn new(name: String, version: String, display_name: String) -> Self {
        Self {
            name,
            version,
            display_name,
            description: None,
            capabilities: vec![
                AcpAgentCapability::ManifestRead,
                AcpAgentCapability::RunsCreate,
                AcpAgentCapability::RunsGet,
                AcpAgentCapability::RunsEvents,
                AcpAgentCapability::RunsCancel,
                AcpAgentCapability::SessionsGet,
                AcpAgentCapability::SessionsResume,
            ],
            metadata: BTreeMap::new(),
            links: Vec::new(),
        }
    }

    /// Adds an optional description to the manifest.
    ///
    /// # Arguments
    ///
    /// * `description` - Human-readable manifest description
    ///
    /// # Returns
    ///
    /// Returns the updated manifest.
    ///
    /// # Errors
    ///
    /// Returns an error if the description is empty.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::acp::manifest::AcpAgentManifest;
    ///
    /// let manifest = AcpAgentManifest::new(
    ///     "xzatoma".to_string(),
    ///     "0.2.0".to_string(),
    ///     "XZatoma ACP Agent".to_string(),
    /// )
    /// .with_description("ACP-ready XZatoma agent".to_string())
    /// .unwrap();
    ///
    /// assert_eq!(
    ///     manifest.description.as_deref(),
    ///     Some("ACP-ready XZatoma agent")
    /// );
    /// ```
    pub fn with_description(mut self, description: String) -> Result<Self> {
        validate_non_empty_field(&description, "manifest.description")?;
        self.description = Some(description);
        Ok(self)
    }

    /// Replaces the declared capability set.
    ///
    /// # Arguments
    ///
    /// * `capabilities` - New capability list
    ///
    /// # Returns
    ///
    /// Returns the updated manifest.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::acp::manifest::{AcpAgentCapability, AcpAgentManifest};
    ///
    /// let manifest = AcpAgentManifest::new(
    ///     "xzatoma".to_string(),
    ///     "0.2.0".to_string(),
    ///     "XZatoma ACP Agent".to_string(),
    /// )
    /// .with_capabilities(vec![AcpAgentCapability::ManifestRead]);
    ///
    /// assert_eq!(manifest.capabilities.len(), 1);
    /// ```
    pub fn with_capabilities(mut self, capabilities: Vec<AcpAgentCapability>) -> Self {
        self.capabilities = capabilities;
        self
    }

    /// Adds or updates a metadata key-value pair.
    ///
    /// # Arguments
    ///
    /// * `key` - Metadata key
    /// * `value` - Metadata value
    ///
    /// # Returns
    ///
    /// Returns the updated manifest.
    ///
    /// # Errors
    ///
    /// Returns an error if the key or value is empty.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::acp::manifest::AcpAgentManifest;
    ///
    /// let manifest = AcpAgentManifest::new(
    ///     "xzatoma".to_string(),
    ///     "0.2.0".to_string(),
    ///     "XZatoma ACP Agent".to_string(),
    /// )
    /// .with_metadata("provider".to_string(), "copilot".to_string())
    /// .unwrap();
    ///
    /// assert_eq!(manifest.metadata.get("provider").map(String::as_str), Some("copilot"));
    /// ```
    pub fn with_metadata(mut self, key: String, value: String) -> Result<Self> {
        validate_non_empty_field(&key, "manifest.metadata.key")?;
        validate_non_empty_field(&value, "manifest.metadata.value")?;
        self.metadata.insert(key, value);
        Ok(self)
    }

    /// Appends a validated external link to the manifest.
    ///
    /// # Arguments
    ///
    /// * `link` - Validated manifest link
    ///
    /// # Returns
    ///
    /// Returns the updated manifest.
    ///
    /// # Errors
    ///
    /// Returns an error if the link is invalid.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::acp::manifest::{AcpAgentManifest, AcpManifestLink};
    ///
    /// let link = AcpManifestLink::new(
    ///     "documentation".to_string(),
    ///     "https://example.com/docs".to_string(),
    /// )
    /// .unwrap();
    ///
    /// let manifest = AcpAgentManifest::new(
    ///     "xzatoma".to_string(),
    ///     "0.2.0".to_string(),
    ///     "XZatoma ACP Agent".to_string(),
    /// )
    /// .with_link(link)
    /// .unwrap();
    ///
    /// assert_eq!(manifest.links.len(), 1);
    /// ```
    pub fn with_link(mut self, link: AcpManifestLink) -> Result<Self> {
        link.validate()?;
        self.links.push(link);
        Ok(self)
    }

    /// Validates the ACP manifest.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` when the manifest is valid.
    ///
    /// # Errors
    ///
    /// Returns an error if any manifest field or nested structure is invalid.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::acp::manifest::AcpAgentManifest;
    ///
    /// let manifest = AcpAgentManifest::new(
    ///     "xzatoma".to_string(),
    ///     "0.2.0".to_string(),
    ///     "XZatoma ACP Agent".to_string(),
    /// );
    ///
    /// assert!(manifest.validate().is_ok());
    /// ```
    pub fn validate(&self) -> Result<()> {
        validate_agent_name(&self.name)?;
        validate_manifest_version(&self.version)?;
        validate_manifest_display_name(&self.display_name)?;

        if let Some(description) = &self.description {
            validate_non_empty_field(description, "manifest.description")?;
        }

        if self.capabilities.is_empty() {
            return Err(XzatomaError::AcpValidation(
                "invalid ACP manifest: capabilities cannot be empty".to_string(),
            )
            .into());
        }

        for (key, value) in &self.metadata {
            validate_non_empty_field(key, "manifest.metadata.key")?;
            validate_non_empty_field(value, "manifest.metadata.value")?;
        }

        for link in &self.links {
            link.validate()?;
        }

        Ok(())
    }
}

fn validate_non_empty_field(value: &str, field: &str) -> Result<()> {
    if value.trim().is_empty() {
        Err(XzatomaError::AcpValidation(format!(
            "invalid ACP value for '{}': value cannot be empty",
            field
        ))
        .into())
    } else {
        Ok(())
    }
}

fn validate_absolute_url(value: &str, field: &str) -> Result<()> {
    validate_non_empty_field(value, field)?;

    let parsed = url::Url::parse(value).map_err(|error| {
        XzatomaError::AcpValidation(format!("invalid ACP URL for '{}': {}", field, error))
    })?;

    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return Err(XzatomaError::AcpValidation(format!(
            "invalid ACP URL for '{}': scheme must be http or https",
            field
        ))
        .into());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_agent_name_accepts_valid_name() {
        assert!(validate_agent_name("xzatoma").is_ok());
        assert!(validate_agent_name("xzatoma_acp").is_ok());
        assert!(validate_agent_name("xzatoma-acp").is_ok());
        assert!(validate_agent_name("xzatoma.acp").is_ok());
    }

    #[test]
    fn test_validate_agent_name_rejects_invalid_name() {
        assert!(validate_agent_name("").is_err());
        assert!(validate_agent_name("Xzatoma").is_err());
        assert!(validate_agent_name("xzatoma agent").is_err());
        assert!(validate_agent_name("xzatoma/agent").is_err());
    }

    #[test]
    fn test_validate_manifest_version_accepts_valid_version() {
        assert!(validate_manifest_version("0.2.0").is_ok());
        assert!(validate_manifest_version("2026.01").is_ok());
    }

    #[test]
    fn test_validate_manifest_version_rejects_invalid_version() {
        assert!(validate_manifest_version("").is_err());
        assert!(validate_manifest_version(" ").is_err());
        assert!(validate_manifest_version(" 0.2.0 ").is_err());
    }

    #[test]
    fn test_validate_manifest_display_name_accepts_valid_name() {
        assert!(validate_manifest_display_name("XZatoma ACP Agent").is_ok());
    }

    #[test]
    fn test_validate_manifest_display_name_rejects_invalid_name() {
        assert!(validate_manifest_display_name("").is_err());
        assert!(validate_manifest_display_name("   ").is_err());
    }

    #[test]
    fn test_manifest_link_new_accepts_valid_values() {
        let link = AcpManifestLink::new(
            "documentation".to_string(),
            "https://example.com/docs".to_string(),
        )
        .unwrap();

        assert_eq!(link.rel, "documentation");
        assert_eq!(link.href, "https://example.com/docs");
    }

    #[test]
    fn test_manifest_link_new_rejects_invalid_url() {
        let result = AcpManifestLink::new(
            "documentation".to_string(),
            "ftp://example.com/docs".to_string(),
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_manifest_validate_accepts_valid_manifest() {
        let manifest = AcpAgentManifest::new(
            "xzatoma".to_string(),
            "0.2.0".to_string(),
            "XZatoma ACP Agent".to_string(),
        );

        assert!(manifest.validate().is_ok());
    }

    #[test]
    fn test_manifest_validate_rejects_invalid_name() {
        let manifest = AcpAgentManifest::new(
            "Invalid Agent".to_string(),
            "0.2.0".to_string(),
            "XZatoma ACP Agent".to_string(),
        );

        assert!(manifest.validate().is_err());
    }

    #[test]
    fn test_manifest_validate_rejects_empty_capabilities() {
        let manifest = AcpAgentManifest::new(
            "xzatoma".to_string(),
            "0.2.0".to_string(),
            "XZatoma ACP Agent".to_string(),
        )
        .with_capabilities(Vec::new());

        assert!(manifest.validate().is_err());
    }

    #[test]
    fn test_manifest_with_description_sets_description() {
        let manifest = AcpAgentManifest::new(
            "xzatoma".to_string(),
            "0.2.0".to_string(),
            "XZatoma ACP Agent".to_string(),
        )
        .with_description("ACP-ready autonomous agent".to_string())
        .unwrap();

        assert_eq!(
            manifest.description.as_deref(),
            Some("ACP-ready autonomous agent")
        );
    }

    #[test]
    fn test_manifest_with_metadata_adds_entry() {
        let manifest = AcpAgentManifest::new(
            "xzatoma".to_string(),
            "0.2.0".to_string(),
            "XZatoma ACP Agent".to_string(),
        )
        .with_metadata("provider".to_string(), "copilot".to_string())
        .unwrap();

        assert_eq!(
            manifest.metadata.get("provider").map(String::as_str),
            Some("copilot")
        );
    }

    #[test]
    fn test_manifest_with_link_appends_link() {
        let link = AcpManifestLink::new(
            "documentation".to_string(),
            "https://example.com/docs".to_string(),
        )
        .unwrap();

        let manifest = AcpAgentManifest::new(
            "xzatoma".to_string(),
            "0.2.0".to_string(),
            "XZatoma ACP Agent".to_string(),
        )
        .with_link(link)
        .unwrap();

        assert_eq!(manifest.links.len(), 1);
    }

    #[test]
    fn test_manifest_serialization_uses_camel_case() {
        let manifest = AcpAgentManifest::new(
            "xzatoma".to_string(),
            "0.2.0".to_string(),
            "XZatoma ACP Agent".to_string(),
        );

        let value = serde_json::to_value(&manifest).unwrap();
        assert!(value.get("displayName").is_some());
        assert!(value.get("display_name").is_none());
    }

    #[test]
    fn test_capability_as_str_returns_protocol_value() {
        assert_eq!(AcpAgentCapability::ManifestRead.as_str(), "manifest.read");
        assert_eq!(AcpAgentCapability::RunsCreate.as_str(), "runs.create");
        assert_eq!(
            AcpAgentCapability::SessionsResume.as_str(),
            "sessions.resume"
        );
    }
}

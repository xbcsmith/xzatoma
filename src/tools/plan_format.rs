//! Plan format detection and validation
//!
//! This module provides utilities for detecting and validating different plan formats:
//! - YAML: Structured key-value format
//! - Markdown: Readable format with headers and sections
//! - Markdown with YAML frontmatter: Combines both formats
//!
//! Validation ensures plans are well-formed and contain required structure.

use crate::error::{Result, XzatomaError};
use serde::{Deserialize, Serialize};

/// Supported plan formats
///
/// Represents the different formats that plan outputs can use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanFormat {
    /// YAML format - structured key-value data
    Yaml,
    /// Markdown format - readable text with headers
    Markdown,
    /// Markdown with YAML frontmatter - combined format
    MarkdownWithFrontmatter,
}

impl std::fmt::Display for PlanFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Yaml => write!(f, "YAML"),
            Self::Markdown => write!(f, "Markdown"),
            Self::MarkdownWithFrontmatter => write!(f, "Markdown with YAML frontmatter"),
        }
    }
}

/// Validated plan structure
///
/// Represents a plan that has been validated and can be safely processed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatedPlan {
    /// The detected format of the plan
    pub format: String,
    /// The title or heading of the plan
    pub title: String,
    /// The raw content of the plan
    pub content: String,
    /// Whether the plan is well-formed
    pub is_valid: bool,
    /// Validation errors, if any
    pub errors: Vec<String>,
}

impl ValidatedPlan {
    /// Creates a new validated plan
    ///
    /// # Arguments
    ///
    /// * `format` - The plan format
    /// * `title` - The plan title
    /// * `content` - The plan content
    /// * `is_valid` - Whether the plan is valid
    /// * `errors` - Any validation errors
    ///
    /// # Returns
    ///
    /// A new ValidatedPlan instance
    pub fn new(
        format: String,
        title: String,
        content: String,
        is_valid: bool,
        errors: Vec<String>,
    ) -> Self {
        Self {
            format,
            title,
            content,
            is_valid,
            errors,
        }
    }

    /// Check if the plan is valid
    ///
    /// # Returns
    ///
    /// Returns true if the plan has no validation errors
    pub fn is_valid_plan(&self) -> bool {
        self.is_valid && self.errors.is_empty()
    }
}

/// Detects the format of a plan from its content
///
/// Analyzes the beginning of the content to determine which format is being used.
/// YAML frontmatter is detected by the presence of "---" at the start.
/// Markdown is detected by the presence of headers like "# " or "## ".
/// Otherwise, YAML is assumed.
///
/// # Arguments
///
/// * `content` - The plan content to analyze
///
/// # Returns
///
/// The detected PlanFormat
///
/// # Examples
///
/// ```
/// use xzatoma::tools::plan_format::{detect_plan_format, PlanFormat};
///
/// // Detect YAML frontmatter
/// let yaml_fm = "---\ntitle: Plan\n---\nContent here";
/// assert_eq!(detect_plan_format(yaml_fm), PlanFormat::MarkdownWithFrontmatter);
///
/// // Detect Markdown
/// let markdown = "# Plan Title\n## Section";
/// assert_eq!(detect_plan_format(markdown), PlanFormat::Markdown);
///
/// // Detect YAML
/// let yaml = "title: Plan\nsteps:\n  - name: Step 1";
/// assert_eq!(detect_plan_format(yaml), PlanFormat::Yaml);
/// ```
#[allow(dead_code)]
pub fn detect_plan_format(content: &str) -> PlanFormat {
    let trimmed = content.trim_start();

    // Check for YAML frontmatter (starts with ---)
    if trimmed.starts_with("---") {
        return PlanFormat::MarkdownWithFrontmatter;
    }

    // Check for Markdown (headers starting with #)
    if trimmed.starts_with("# ") || trimmed.starts_with("## ") || trimmed.starts_with("### ") {
        return PlanFormat::Markdown;
    }

    // Check for Markdown headers anywhere in the beginning
    if trimmed[..std::cmp::min(200, trimmed.len())].contains("\n# ")
        || trimmed[..std::cmp::min(200, trimmed.len())].contains("\n## ")
    {
        return PlanFormat::Markdown;
    }

    // Default to YAML
    PlanFormat::Yaml
}

/// Validates a plan in YAML format
///
/// Checks that the YAML is well-formed and contains basic required structure.
///
/// # Arguments
///
/// * `content` - The YAML content to validate
///
/// # Returns
///
/// A Result containing the validated plan or an error
fn validate_yaml_plan(content: &str) -> Result<ValidatedPlan> {
    // Try to parse as YAML
    let parse_result: serde_yaml::Result<serde_yaml::Value> = serde_yaml::from_str(content);

    match parse_result {
        Ok(value) => {
            // Extract title
            let title = if let Some(title_val) = value.get("title") {
                title_val.as_str().unwrap_or("Untitled Plan").to_string()
            } else {
                "Untitled Plan".to_string()
            };

            Ok(ValidatedPlan::new(
                "YAML".to_string(),
                title,
                content.to_string(),
                true,
                vec![],
            ))
        }
        Err(e) => {
            let error_msg = format!("Invalid YAML: {}", e);
            Ok(ValidatedPlan::new(
                "YAML".to_string(),
                "Invalid".to_string(),
                content.to_string(),
                false,
                vec![error_msg],
            ))
        }
    }
}

/// Validates a plan in Markdown format
///
/// Checks that the Markdown has a reasonable structure with at least one header.
///
/// # Arguments
///
/// * `content` - The Markdown content to validate
///
/// # Returns
///
/// A Result containing the validated plan or an error
fn validate_markdown_plan(content: &str) -> Result<ValidatedPlan> {
    let mut errors = vec![];

    // Extract title from first header
    let title = content
        .lines()
        .find(|line| line.starts_with('#'))
        .map(|line| line.trim_start_matches('#').trim().to_string())
        .unwrap_or_else(|| "Untitled Plan".to_string());

    // Check for at least some structure
    let has_headers = content.contains("# ") || content.contains("## ") || content.contains("### ");
    let is_valid = has_headers && !content.is_empty();

    if !has_headers {
        errors.push("Markdown plan should contain at least one header (# )".to_string());
    }

    if content.is_empty() {
        errors.push("Plan content cannot be empty".to_string());
    }

    Ok(ValidatedPlan::new(
        "Markdown".to_string(),
        title,
        content.to_string(),
        is_valid,
        errors,
    ))
}

/// Validates a plan in Markdown with YAML frontmatter format
///
/// Checks that the frontmatter is valid YAML and the Markdown is well-formed.
///
/// # Arguments
///
/// * `content` - The content with YAML frontmatter to validate
///
/// # Returns
///
/// A Result containing the validated plan or an error
fn validate_frontmatter_plan(content: &str) -> Result<ValidatedPlan> {
    let mut errors = vec![];

    // Split on the first --- that appears after the start
    let parts: Vec<&str> = content.splitn(3, "---").collect();

    if parts.len() < 3 {
        return Ok(ValidatedPlan::new(
            "Markdown with YAML frontmatter".to_string(),
            "Invalid".to_string(),
            content.to_string(),
            false,
            vec!["Invalid frontmatter: must have opening and closing ---".to_string()],
        ));
    }

    let frontmatter = parts[1];
    let markdown_content = parts[2];

    // Validate YAML frontmatter
    let yaml_result: serde_yaml::Result<serde_yaml::Value> = serde_yaml::from_str(frontmatter);

    if yaml_result.is_err() {
        errors.push("Invalid YAML in frontmatter".to_string());
    }

    // Extract title from frontmatter or markdown
    let title = if let Ok(yaml) = yaml_result {
        yaml.get("title")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| {
                markdown_content
                    .lines()
                    .find(|line| line.starts_with('#'))
                    .map(|line| line.trim_start_matches('#').trim().to_string())
                    .unwrap_or_else(|| "Untitled Plan".to_string())
            })
    } else {
        "Untitled Plan".to_string()
    };

    // Check markdown content
    if markdown_content.trim().is_empty() {
        errors.push("Markdown content cannot be empty".to_string());
    }

    let is_valid = errors.is_empty();

    Ok(ValidatedPlan::new(
        "Markdown with YAML frontmatter".to_string(),
        title,
        content.to_string(),
        is_valid,
        errors,
    ))
}

/// Validates a plan, detecting its format and checking validity
///
/// Automatically detects the format and applies the appropriate validation.
///
/// # Arguments
///
/// * `content` - The plan content to validate
///
/// # Returns
///
/// A Result containing the validated plan or an error
///
/// # Examples
///
/// ```
/// use xzatoma::tools::plan_format::{validate_plan, ValidatedPlan};
///
/// let plan = "title: Test Plan\nsteps:\n  - name: Step 1";
/// let result = validate_plan(plan);
/// assert!(result.is_ok());
/// let validated = result.unwrap();
/// assert!(validated.is_valid_plan());
/// ```
#[allow(dead_code)]
pub fn validate_plan(content: &str) -> Result<ValidatedPlan> {
    if content.is_empty() {
        return Ok(ValidatedPlan::new(
            "Unknown".to_string(),
            "Empty".to_string(),
            String::new(),
            false,
            vec!["Plan content cannot be empty".to_string()],
        ));
    }

    let format = detect_plan_format(content);

    match format {
        PlanFormat::Yaml => validate_yaml_plan(content),
        PlanFormat::Markdown => validate_markdown_plan(content),
        PlanFormat::MarkdownWithFrontmatter => validate_frontmatter_plan(content),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_format_yaml() {
        let yaml = "title: Plan\nsteps:\n  - name: Step 1";
        assert_eq!(detect_plan_format(yaml), PlanFormat::Yaml);
    }

    #[test]
    fn test_detect_format_yaml_with_colons() {
        let yaml = "version: 1.0\nname: test plan";
        assert_eq!(detect_plan_format(yaml), PlanFormat::Yaml);
    }

    #[test]
    fn test_detect_format_markdown() {
        let markdown = "# Plan Title\n## Section\nContent here";
        assert_eq!(detect_plan_format(markdown), PlanFormat::Markdown);
    }

    #[test]
    fn test_detect_format_markdown_level_2() {
        let markdown = "## Section\nContent here";
        assert_eq!(detect_plan_format(markdown), PlanFormat::Markdown);
    }

    #[test]
    fn test_detect_format_markdown_level_3() {
        let markdown = "### Subsection\nContent here";
        assert_eq!(detect_plan_format(markdown), PlanFormat::Markdown);
    }

    #[test]
    fn test_detect_format_frontmatter() {
        let frontmatter = "---\ntitle: Plan\n---\n# Content\nText here";
        assert_eq!(
            detect_plan_format(frontmatter),
            PlanFormat::MarkdownWithFrontmatter
        );
    }

    #[test]
    fn test_detect_format_frontmatter_with_space() {
        let frontmatter = "---\ntitle: Plan\nsteps: []\n---\nContent here";
        assert_eq!(
            detect_plan_format(frontmatter),
            PlanFormat::MarkdownWithFrontmatter
        );
    }

    #[test]
    fn test_validate_yaml_plan_valid() {
        let yaml = "title: Test Plan\nsteps:\n  - name: Step 1";
        let result = validate_yaml_plan(yaml).unwrap();
        assert!(result.is_valid);
        assert_eq!(result.title, "Test Plan");
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_validate_yaml_plan_no_title() {
        let yaml = "steps:\n  - name: Step 1";
        let result = validate_yaml_plan(yaml).unwrap();
        assert!(result.is_valid);
        assert_eq!(result.title, "Untitled Plan");
    }

    #[test]
    fn test_validate_yaml_plan_invalid() {
        let invalid_yaml = "title: Test\n\t- invalid indentation\n  bad: : yaml";
        let result = validate_yaml_plan(invalid_yaml).unwrap();
        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn test_validate_markdown_plan_valid() {
        let markdown = "# Test Plan\n## Steps\n1. Step one\n2. Step two";
        let result = validate_markdown_plan(markdown).unwrap();
        assert!(result.is_valid);
        assert_eq!(result.title, "Test Plan");
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_validate_markdown_plan_no_headers() {
        let markdown = "Just some text without headers";
        let result = validate_markdown_plan(markdown).unwrap();
        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn test_validate_markdown_plan_empty() {
        let markdown = "";
        let result = validate_markdown_plan(markdown).unwrap();
        assert!(!result.is_valid);
    }

    #[test]
    fn test_validate_frontmatter_plan_valid() {
        let content = "---\ntitle: Test Plan\n---\n# Content\nTest content";
        let result = validate_frontmatter_plan(content).unwrap();
        assert!(result.is_valid);
        assert_eq!(result.title, "Test Plan");
    }

    #[test]
    fn test_validate_frontmatter_plan_title_from_markdown() {
        let content = "---\ndescription: A test\n---\n# Content Title\nTest";
        let result = validate_frontmatter_plan(content).unwrap();
        assert_eq!(result.title, "Content Title");
    }

    #[test]
    fn test_validate_frontmatter_plan_invalid_yaml() {
        let content = "---\ninvalid:\n  - yaml\nhere\n---\n# Content";
        let result = validate_frontmatter_plan(content).unwrap();
        assert!(!result.is_valid);
    }

    #[test]
    fn test_validate_frontmatter_plan_no_closing() {
        let content = "---\ntitle: Test\n# Content";
        let result = validate_frontmatter_plan(content).unwrap();
        assert!(!result.is_valid);
    }

    #[test]
    fn test_validate_plan_yaml() {
        let yaml = "title: Test\nsteps: []";
        let result = validate_plan(yaml).unwrap();
        assert!(result.is_valid);
        assert_eq!(result.format, "YAML");
    }

    #[test]
    fn test_validate_plan_markdown() {
        let markdown = "# Test\n## Content";
        let result = validate_plan(markdown).unwrap();
        assert!(result.is_valid);
        assert_eq!(result.format, "Markdown");
    }

    #[test]
    fn test_validate_plan_frontmatter() {
        let content = "---\ntitle: Test\n---\n# Content";
        let result = validate_plan(content).unwrap();
        assert!(result.is_valid);
    }

    #[test]
    fn test_validate_plan_empty() {
        let result = validate_plan("").unwrap();
        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn test_validated_plan_is_valid_plan_method() {
        let plan = ValidatedPlan::new(
            "YAML".to_string(),
            "Test".to_string(),
            "content".to_string(),
            true,
            vec![],
        );
        assert!(plan.is_valid_plan());
    }

    #[test]
    fn test_validated_plan_with_errors() {
        let plan = ValidatedPlan::new(
            "YAML".to_string(),
            "Test".to_string(),
            "content".to_string(),
            false,
            vec!["error".to_string()],
        );
        assert!(!plan.is_valid_plan());
    }

    #[test]
    fn test_plan_format_display() {
        assert_eq!(PlanFormat::Yaml.to_string(), "YAML");
        assert_eq!(PlanFormat::Markdown.to_string(), "Markdown");
        assert_eq!(
            PlanFormat::MarkdownWithFrontmatter.to_string(),
            "Markdown with YAML frontmatter"
        );
    }
}

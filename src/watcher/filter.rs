//! Event filtering for CloudEvents messages
//!
//! This module provides filtering logic to determine which CloudEvents
//! should be processed based on configured criteria.

use crate::config::EventFilterConfig;
use crate::xzepr::CloudEventMessage;
use anyhow::Result;
use regex::Regex;

/// Event filter for determining which CloudEvents to process.
///
/// Filters events based on configured criteria including event types,
/// source patterns, platform ID, package name, API version, and success status.
#[derive(Clone)]
pub struct EventFilter {
    config: EventFilterConfig,
    source_regex: Option<std::sync::Arc<Regex>>,
}

impl EventFilter {
    /// Create a new event filter from configuration.
    ///
    /// # Arguments
    ///
    /// * `config` - Event filter configuration
    ///
    /// # Returns
    ///
    /// Returns the event filter or error if regex compilation fails
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::config::EventFilterConfig;
    /// use xzatoma::watcher::EventFilter;
    ///
    /// let config = EventFilterConfig {
    ///     event_types: vec!["deployment.success".to_string()],
    ///     source_pattern: None,
    ///     platform_id: None,
    ///     package: None,
    ///     api_version: None,
    ///     success_only: true,
    /// };
    ///
    /// let filter = EventFilter::new(config);
    /// assert!(filter.is_ok());
    /// ```
    pub fn new(config: EventFilterConfig) -> Result<Self> {
        let source_regex = if let Some(pattern) = &config.source_pattern {
            Some(std::sync::Arc::new(Regex::new(pattern)?))
        } else {
            None
        };

        Ok(Self {
            config,
            source_regex,
        })
    }

    /// Check if an event should be processed based on filter criteria.
    ///
    /// # Arguments
    ///
    /// * `event` - CloudEvent message to evaluate
    ///
    /// # Returns
    ///
    /// Returns true if the event matches all configured filters
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::config::EventFilterConfig;
    /// use xzatoma::watcher::EventFilter;
    /// use xzatoma::xzepr::CloudEventMessage;
    ///
    /// let config = EventFilterConfig {
    ///     event_types: vec!["deployment.success".to_string()],
    ///     source_pattern: None,
    ///     platform_id: None,
    ///     package: None,
    ///     api_version: None,
    ///     success_only: true,
    /// };
    ///
    /// let filter = EventFilter::new(config).unwrap();
    /// // Can check with a real CloudEventMessage instance
    /// ```
    pub fn should_process(&self, event: &CloudEventMessage) -> bool {
        // Filter by success flag
        if self.config.success_only && !event.success {
            return false;
        }

        // Filter by event type
        if !self.config.event_types.is_empty()
            && !self.config.event_types.contains(&event.event_type)
        {
            return false;
        }

        // Filter by source pattern
        if let Some(regex) = &self.source_regex {
            if !regex.is_match(&event.source) {
                return false;
            }
        }

        // Filter by platform_id
        if let Some(platform) = &self.config.platform_id {
            if &event.platform_id != platform {
                return false;
            }
        }

        // Filter by package
        if let Some(package) = &self.config.package {
            if &event.package != package {
                return false;
            }
        }

        // Filter by api_version
        if let Some(version) = &self.config.api_version {
            if &event.api_version != version {
                return false;
            }
        }

        true
    }

    /// Get filter summary for logging.
    ///
    /// # Returns
    ///
    /// Returns a human-readable summary of active filters
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::config::EventFilterConfig;
    /// use xzatoma::watcher::EventFilter;
    ///
    /// let config = EventFilterConfig {
    ///     event_types: vec!["deployment.success".to_string()],
    ///     source_pattern: None,
    ///     platform_id: None,
    ///     package: None,
    ///     api_version: None,
    ///     success_only: true,
    /// };
    ///
    /// let filter = EventFilter::new(config).unwrap();
    /// let summary = filter.summary();
    /// assert!(summary.contains("types=deployment.success"));
    /// assert!(summary.contains("success=true"));
    /// ```
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();

        if !self.config.event_types.is_empty() {
            parts.push(format!("types={}", self.config.event_types.join(",")));
        }

        if let Some(pattern) = &self.config.source_pattern {
            parts.push(format!("source~{}", pattern));
        }

        if let Some(platform) = &self.config.platform_id {
            parts.push(format!("platform={}", platform));
        }

        if let Some(package) = &self.config.package {
            parts.push(format!("package={}", package));
        }

        if let Some(version) = &self.config.api_version {
            parts.push(format!("api_version={}", version));
        }

        if self.config.success_only {
            parts.push("success=true".to_string());
        }

        if parts.is_empty() {
            "no filters (all events)".to_string()
        } else {
            parts.join(", ")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_event(
        success: bool,
        event_type: &str,
        source: &str,
        platform_id: &str,
        package: &str,
        api_version: &str,
    ) -> CloudEventMessage {
        use crate::xzepr::consumer::message::CloudEventData;

        CloudEventMessage {
            success,
            id: "test-id".to_string(),
            specversion: "1.0.1".to_string(),
            event_type: event_type.to_string(),
            source: source.to_string(),
            api_version: api_version.to_string(),
            name: "test".to_string(),
            version: "1.0.0".to_string(),
            release: "1.0.0".to_string(),
            platform_id: platform_id.to_string(),
            package: package.to_string(),
            data: CloudEventData::default(),
        }
    }

    #[test]
    fn test_filter_by_event_type() {
        let config = EventFilterConfig {
            event_types: vec!["deployment.success".to_string()],
            source_pattern: None,
            platform_id: None,
            package: None,
            api_version: None,
            success_only: false,
        };

        let filter = EventFilter::new(config).unwrap();
        let matching = create_test_event(true, "deployment.success", "source", "k8s", "app", "v1");
        let non_matching =
            create_test_event(true, "deployment.failed", "source", "k8s", "app", "v1");

        assert!(filter.should_process(&matching));
        assert!(!filter.should_process(&non_matching));
    }

    #[test]
    fn test_filter_by_multiple_event_types() {
        let config = EventFilterConfig {
            event_types: vec![
                "deployment.success".to_string(),
                "deployment.started".to_string(),
            ],
            source_pattern: None,
            platform_id: None,
            package: None,
            api_version: None,
            success_only: false,
        };

        let filter = EventFilter::new(config).unwrap();
        let success = create_test_event(true, "deployment.success", "source", "k8s", "app", "v1");
        let started = create_test_event(true, "deployment.started", "source", "k8s", "app", "v1");
        let failed = create_test_event(true, "deployment.failed", "source", "k8s", "app", "v1");

        assert!(filter.should_process(&success));
        assert!(filter.should_process(&started));
        assert!(!filter.should_process(&failed));
    }

    #[test]
    fn test_filter_by_success_only() {
        let config = EventFilterConfig {
            event_types: vec![],
            source_pattern: None,
            platform_id: None,
            package: None,
            api_version: None,
            success_only: true,
        };

        let filter = EventFilter::new(config).unwrap();
        let success = create_test_event(true, "test", "source", "k8s", "app", "v1");
        let failure = create_test_event(false, "test", "source", "k8s", "app", "v1");

        assert!(filter.should_process(&success));
        assert!(!filter.should_process(&failure));
    }

    #[test]
    fn test_filter_by_source_pattern() {
        let config = EventFilterConfig {
            event_types: vec![],
            source_pattern: Some("xzepr\\.receiver\\..*".to_string()),
            platform_id: None,
            package: None,
            api_version: None,
            success_only: false,
        };

        let filter = EventFilter::new(config).unwrap();
        let matching = create_test_event(
            true,
            "test",
            "xzepr.receiver.01JXXXXXXX",
            "k8s",
            "app",
            "v1",
        );
        let non_matching = create_test_event(true, "test", "other.source", "k8s", "app", "v1");

        assert!(filter.should_process(&matching));
        assert!(!filter.should_process(&non_matching));
    }

    #[test]
    fn test_filter_by_platform_id() {
        let config = EventFilterConfig {
            event_types: vec![],
            source_pattern: None,
            platform_id: Some("kubernetes".to_string()),
            package: None,
            api_version: None,
            success_only: false,
        };

        let filter = EventFilter::new(config).unwrap();
        let matching = create_test_event(true, "test", "source", "kubernetes", "app", "v1");
        let non_matching = create_test_event(true, "test", "source", "docker", "app", "v1");

        assert!(filter.should_process(&matching));
        assert!(!filter.should_process(&non_matching));
    }

    #[test]
    fn test_filter_by_package() {
        let config = EventFilterConfig {
            event_types: vec![],
            source_pattern: None,
            platform_id: None,
            package: Some("myapp".to_string()),
            api_version: None,
            success_only: false,
        };

        let filter = EventFilter::new(config).unwrap();
        let matching = create_test_event(true, "test", "source", "k8s", "myapp", "v1");
        let non_matching = create_test_event(true, "test", "source", "k8s", "otherapp", "v1");

        assert!(filter.should_process(&matching));
        assert!(!filter.should_process(&non_matching));
    }

    #[test]
    fn test_filter_by_api_version() {
        let config = EventFilterConfig {
            event_types: vec![],
            source_pattern: None,
            platform_id: None,
            package: None,
            api_version: Some("v1".to_string()),
            success_only: false,
        };

        let filter = EventFilter::new(config).unwrap();
        let matching = create_test_event(true, "test", "source", "k8s", "app", "v1");
        let non_matching = create_test_event(true, "test", "source", "k8s", "app", "v2");

        assert!(filter.should_process(&matching));
        assert!(!filter.should_process(&non_matching));
    }

    #[test]
    fn test_filter_by_multiple_criteria() {
        let config = EventFilterConfig {
            event_types: vec!["deployment.success".to_string()],
            source_pattern: Some("xzepr\\..*".to_string()),
            platform_id: Some("kubernetes".to_string()),
            package: Some("myapp".to_string()),
            api_version: Some("v1".to_string()),
            success_only: true,
        };

        let filter = EventFilter::new(config).unwrap();

        let matching = create_test_event(
            true,
            "deployment.success",
            "xzepr.receiver.01JXXXXXXX",
            "kubernetes",
            "myapp",
            "v1",
        );
        assert!(filter.should_process(&matching));

        // Missing one criterion: wrong event type
        let wrong_type = create_test_event(
            true,
            "deployment.failed",
            "xzepr.receiver.01JXXXXXXX",
            "kubernetes",
            "myapp",
            "v1",
        );
        assert!(!filter.should_process(&wrong_type));

        // Missing one criterion: wrong platform
        let wrong_platform = create_test_event(
            true,
            "deployment.success",
            "xzepr.receiver.01JXXXXXXX",
            "docker",
            "myapp",
            "v1",
        );
        assert!(!filter.should_process(&wrong_platform));

        // Missing one criterion: success=false
        let failed_event = create_test_event(
            false,
            "deployment.success",
            "xzepr.receiver.01JXXXXXXX",
            "kubernetes",
            "myapp",
            "v1",
        );
        assert!(!filter.should_process(&failed_event));
    }

    #[test]
    fn test_no_filters_accepts_all() {
        let config = EventFilterConfig {
            event_types: vec![],
            source_pattern: None,
            platform_id: None,
            package: None,
            api_version: None,
            success_only: false,
        };

        let filter = EventFilter::new(config).unwrap();
        let event = create_test_event(false, "anything", "anywhere", "anywhereid", "anyapp", "v1");

        assert!(filter.should_process(&event));
    }

    #[test]
    fn test_filter_summary_empty() {
        let config = EventFilterConfig::default();
        let filter = EventFilter::new(config).unwrap();
        assert_eq!(filter.summary(), "no filters (all events)");
    }

    #[test]
    fn test_filter_summary_with_event_types() {
        let config = EventFilterConfig {
            event_types: vec![
                "deployment.success".to_string(),
                "deployment.started".to_string(),
            ],
            source_pattern: None,
            platform_id: None,
            package: None,
            api_version: None,
            success_only: false,
        };

        let filter = EventFilter::new(config).unwrap();
        let summary = filter.summary();
        assert!(summary.contains("types=deployment.success,deployment.started"));
    }

    #[test]
    fn test_filter_summary_with_all_criteria() {
        let config = EventFilterConfig {
            event_types: vec!["test.event".to_string()],
            source_pattern: Some("xzepr\\..*".to_string()),
            platform_id: Some("kubernetes".to_string()),
            package: Some("myapp".to_string()),
            api_version: Some("v1".to_string()),
            success_only: true,
        };

        let filter = EventFilter::new(config).unwrap();
        let summary = filter.summary();

        assert!(summary.contains("types=test.event"));
        assert!(summary.contains("source~xzepr"));
        assert!(summary.contains("platform=kubernetes"));
        assert!(summary.contains("package=myapp"));
        assert!(summary.contains("api_version=v1"));
        assert!(summary.contains("success=true"));
    }

    #[test]
    fn test_invalid_regex_pattern() {
        let config = EventFilterConfig {
            event_types: vec![],
            source_pattern: Some("[invalid(".to_string()),
            platform_id: None,
            package: None,
            api_version: None,
            success_only: false,
        };

        let result = EventFilter::new(config);
        assert!(result.is_err());
    }
}

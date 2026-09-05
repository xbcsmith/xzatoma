//! Canonical Kafka security configuration types shared across watcher backends.
//!
//! This module holds the single source of truth for the Kafka security
//! primitives used by both watcher backends and their supporting components
//! (the XZepr consumer, the generic result producer, and the topic admin
//! helper):
//!
//! - [`SecurityProtocol`]: connection security protocol
//! - [`SaslMechanism`]: SASL authentication mechanism
//! - [`SaslConfig`]: resolved SASL credentials and mechanism
//! - [`SslConfig`]: resolved SSL/TLS certificate locations
//!
//! It also provides the shared parsing helpers [`parse_security_protocol`] and
//! [`parse_sasl_mechanism`], plus [`apply_security_config`] which resolves a
//! [`KafkaSecurityConfig`] into a [`ResolvedSecurity`] value that both the
//! topic admin helper and the producer/consumer paths can reuse.
//!
//! These types were previously duplicated across several watcher modules. They
//! are consolidated here to keep a single, well-documented definition.

use crate::config::KafkaSecurityConfig;
use crate::error::{Result, XzatomaError};
use tracing::warn;

/// Default maximum accepted size, in bytes, of an inbound Kafka message
/// payload before parsing.
///
/// Used as the fallback bound for [`crate::watcher::generic::event_handler::GenericEventHandler`]
/// and [`crate::watcher::xzepr::consumer::KafkaConsumerConfig`] when no
/// explicit override is configured. Production callers normally override this
/// with the value from `watcher.execution.max_payload_bytes`
/// (`XZATOMA_WATCHER_MAX_PAYLOAD_BYTES`).
pub const DEFAULT_MAX_PAYLOAD_BYTES: usize = 1024 * 1024;

/// Validates that a raw Kafka payload does not exceed `max_bytes`.
///
/// Both watcher backends (generic and XZepr) call this before handing an
/// untrusted payload to their respective parsers, so an oversized message is
/// rejected with a handled, source-preserving error rather than reaching the
/// parser at all.
///
/// # Arguments
///
/// * `payload` - The raw UTF-8 Kafka message payload.
/// * `max_bytes` - The maximum accepted payload size, in bytes.
///
/// # Errors
///
/// Returns [`XzatomaError::Watcher`] when `payload.len() > max_bytes`.
///
/// # Examples
///
/// ```
/// use xzatoma::watcher::kafka_security::validate_payload_size;
///
/// assert!(validate_payload_size("small", 1024).is_ok());
/// assert!(validate_payload_size(&"x".repeat(2048), 1024).is_err());
/// ```
pub fn validate_payload_size(payload: &str, max_bytes: usize) -> Result<()> {
    let actual = payload.len();
    if actual > max_bytes {
        return Err(XzatomaError::Watcher(format!(
            "Kafka payload of {actual} bytes exceeds the configured maximum of {max_bytes} bytes"
        )));
    }
    Ok(())
}

/// Security protocol for a Kafka connection.
///
/// Determines how the client connects to Kafka brokers.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum SecurityProtocol {
    /// No encryption or authentication.
    #[default]
    Plaintext,
    /// TLS encryption without SASL.
    Ssl,
    /// SASL authentication without TLS.
    SaslPlaintext,
    /// SASL authentication with TLS encryption.
    SaslSsl,
}

impl SecurityProtocol {
    /// Returns the Kafka configuration string for this protocol.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::watcher::kafka_security::SecurityProtocol;
    ///
    /// assert_eq!(SecurityProtocol::SaslSsl.as_str(), "SASL_SSL");
    /// ```
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Plaintext => "PLAINTEXT",
            Self::Ssl => "SSL",
            Self::SaslPlaintext => "SASL_PLAINTEXT",
            Self::SaslSsl => "SASL_SSL",
        }
    }
}

/// SASL authentication mechanism.
///
/// Supported mechanisms for SASL authentication with Kafka.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum SaslMechanism {
    /// PLAIN mechanism (username/password in clear text).
    Plain,
    /// SCRAM-SHA-256 mechanism (recommended).
    #[default]
    ScramSha256,
    /// SCRAM-SHA-512 mechanism.
    ScramSha512,
}

impl SaslMechanism {
    /// Returns the Kafka configuration string for this mechanism.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::watcher::kafka_security::SaslMechanism;
    ///
    /// assert_eq!(SaslMechanism::ScramSha256.as_str(), "SCRAM-SHA-256");
    /// ```
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Plain => "PLAIN",
            Self::ScramSha256 => "SCRAM-SHA-256",
            Self::ScramSha512 => "SCRAM-SHA-512",
        }
    }
}

/// SASL authentication configuration.
///
/// Contains credentials and mechanism for SASL authentication.
#[derive(Debug, Clone)]
pub struct SaslConfig {
    /// Authentication mechanism to use.
    pub mechanism: SaslMechanism,
    /// SASL username.
    pub username: String,
    /// SASL password.
    pub password: String,
}

/// SSL/TLS configuration.
///
/// Contains paths to certificates for TLS connections.
#[derive(Debug, Clone)]
pub struct SslConfig {
    /// Path to CA certificate file.
    pub ca_location: Option<String>,
    /// Path to client certificate file (for mTLS).
    pub certificate_location: Option<String>,
    /// Path to client key file (for mTLS).
    pub key_location: Option<String>,
}

/// Resolved Kafka security settings.
///
/// Produced by [`apply_security_config`] from a [`KafkaSecurityConfig`]. It
/// bundles the resolved protocol together with the optional SASL and SSL
/// configuration so both the topic admin helper and the producer/consumer
/// paths can reuse the same resolution logic.
#[derive(Debug, Clone)]
pub struct ResolvedSecurity {
    /// Resolved security protocol.
    pub security_protocol: SecurityProtocol,
    /// Resolved SASL configuration, when a mechanism was requested.
    pub sasl_config: Option<SaslConfig>,
    /// Resolved SSL configuration, when the protocol requires TLS.
    pub ssl_config: Option<SslConfig>,
}

/// Parse a security protocol string into the corresponding enum variant.
///
/// Matching is case-insensitive.
///
/// # Errors
///
/// Returns [`XzatomaError::Config`] if the string does not name a known
/// protocol.
///
/// # Examples
///
/// ```
/// use xzatoma::watcher::kafka_security::{parse_security_protocol, SecurityProtocol};
///
/// assert_eq!(
///     parse_security_protocol("sasl_ssl").unwrap(),
///     SecurityProtocol::SaslSsl
/// );
/// assert!(parse_security_protocol("nope").is_err());
/// ```
pub fn parse_security_protocol(protocol: &str) -> Result<SecurityProtocol> {
    match protocol.to_uppercase().as_str() {
        "PLAINTEXT" => Ok(SecurityProtocol::Plaintext),
        "SSL" => Ok(SecurityProtocol::Ssl),
        "SASL_PLAINTEXT" => Ok(SecurityProtocol::SaslPlaintext),
        "SASL_SSL" => Ok(SecurityProtocol::SaslSsl),
        _ => Err(XzatomaError::Config(format!(
            "Invalid security protocol: {protocol}"
        ))),
    }
}

/// Parse a SASL mechanism string into the corresponding enum variant.
///
/// Matching is case-insensitive.
///
/// # Errors
///
/// Returns [`XzatomaError::Config`] if the string does not name a known
/// mechanism.
///
/// # Examples
///
/// ```
/// use xzatoma::watcher::kafka_security::{parse_sasl_mechanism, SaslMechanism};
///
/// assert_eq!(
///     parse_sasl_mechanism("scram-sha-512").unwrap(),
///     SaslMechanism::ScramSha512
/// );
/// assert!(parse_sasl_mechanism("nope").is_err());
/// ```
pub fn parse_sasl_mechanism(mechanism: &str) -> Result<SaslMechanism> {
    match mechanism.to_uppercase().as_str() {
        "PLAIN" => Ok(SaslMechanism::Plain),
        "SCRAM-SHA-256" => Ok(SaslMechanism::ScramSha256),
        "SCRAM-SHA-512" => Ok(SaslMechanism::ScramSha512),
        _ => Err(XzatomaError::Config(format!(
            "Invalid SASL mechanism: {mechanism}"
        ))),
    }
}

/// Returns whether a security protocol transmits data without encryption.
///
/// `PLAINTEXT` and `SASL_PLAINTEXT` send credentials and message payloads over
/// the wire unencrypted, so they are considered insecure for production use.
/// This is the pure predicate backing [`warn_if_insecure`].
///
/// # Examples
///
/// ```
/// use xzatoma::watcher::kafka_security::{is_insecure_protocol, SecurityProtocol};
///
/// assert!(is_insecure_protocol(&SecurityProtocol::Plaintext));
/// assert!(is_insecure_protocol(&SecurityProtocol::SaslPlaintext));
/// assert!(!is_insecure_protocol(&SecurityProtocol::Ssl));
/// assert!(!is_insecure_protocol(&SecurityProtocol::SaslSsl));
/// ```
pub fn is_insecure_protocol(protocol: &SecurityProtocol) -> bool {
    matches!(
        protocol,
        SecurityProtocol::Plaintext | SecurityProtocol::SaslPlaintext
    )
}

/// Emit a warning when the resolved protocol transmits data unencrypted.
///
/// This is the single shared apply-time warning used by every production path
/// that applies a Kafka security protocol (the generic producer via
/// [`apply_security_config`] and the XZepr consumer). It should be called once,
/// at the point where the protocol is applied, rather than during pure parsing
/// which may run repeatedly at config-load time.
///
/// Protocols that are already encrypted (`SSL`, `SASL_SSL`) produce no output.
pub fn warn_if_insecure(protocol: &SecurityProtocol) {
    match protocol {
        SecurityProtocol::Plaintext => {
            warn!(
                "Kafka SecurityProtocol is PLAINTEXT: all traffic will be UNENCRYPTED and \
                 credentials and message payloads may be exposed on the network. Use \
                 SASL_SSL or SSL for production deployments."
            );
        }
        SecurityProtocol::SaslPlaintext => {
            warn!(
                "Kafka SecurityProtocol is SASL_PLAINTEXT: SASL credentials and message \
                 payloads travel UNENCRYPTED and may be exposed on the network. Use \
                 SASL_SSL or SSL for production deployments."
            );
        }
        SecurityProtocol::Ssl | SecurityProtocol::SaslSsl => {}
    }
}

/// Resolve a [`KafkaSecurityConfig`] into concrete security settings.
///
/// This is the shared resolution logic reused by the topic admin helper and
/// the generic result producer. It:
///
/// 1. Parses `security.protocol` via [`parse_security_protocol`].
/// 2. Initializes an empty [`SslConfig`] when the protocol requires TLS
///    (`SSL` or `SASL_SSL`).
/// 3. Builds a [`SaslConfig`] when `security.sasl_mechanism` is set, resolving
///    the username from config and the password from config or the
///    `KAFKA_SASL_PASSWORD` environment variable.
///
/// # Arguments
///
/// * `security` - The Kafka security configuration to resolve.
///
/// # Errors
///
/// Returns [`XzatomaError::Config`] if the protocol or mechanism is invalid,
/// or if a SASL mechanism is set without a resolvable username or password.
///
/// # Examples
///
/// ```
/// use xzatoma::config::KafkaSecurityConfig;
/// use xzatoma::watcher::kafka_security::{apply_security_config, SecurityProtocol};
///
/// let security = KafkaSecurityConfig {
///     protocol: "SASL_SSL".to_string(),
///     sasl_mechanism: Some("SCRAM-SHA-256".to_string()),
///     sasl_username: Some("user".to_string()),
///     sasl_password: Some("pass".to_string()),
///     ssl_ca_location: None,
/// };
///
/// let resolved = apply_security_config(&security).unwrap();
/// assert_eq!(resolved.security_protocol, SecurityProtocol::SaslSsl);
/// assert!(resolved.sasl_config.is_some());
/// assert!(resolved.ssl_config.is_some());
/// ```
pub fn apply_security_config(security: &KafkaSecurityConfig) -> Result<ResolvedSecurity> {
    let security_protocol = parse_security_protocol(&security.protocol)?;

    // Warn once, at the apply step, if the resolved protocol is unencrypted.
    warn_if_insecure(&security_protocol);

    let ssl_config = if matches!(
        security_protocol,
        SecurityProtocol::Ssl | SecurityProtocol::SaslSsl
    ) {
        Some(SslConfig {
            ca_location: security.ssl_ca_location.clone(),
            certificate_location: None,
            key_location: None,
        })
    } else {
        None
    };

    let sasl_config = if let Some(mechanism) = &security.sasl_mechanism {
        let username = security.sasl_username.clone().ok_or_else(|| {
            XzatomaError::Config("SASL username is required when mechanism is set".to_string())
        })?;

        let password = security
            .sasl_password
            .clone()
            .or_else(|| std::env::var("KAFKA_SASL_PASSWORD").ok())
            .ok_or_else(|| {
                XzatomaError::Config(
                    "SASL password required (set via config or KAFKA_SASL_PASSWORD env var)"
                        .to_string(),
                )
            })?;

        Some(SaslConfig {
            mechanism: parse_sasl_mechanism(mechanism)?,
            username,
            password,
        })
    } else {
        None
    };

    Ok(ResolvedSecurity {
        security_protocol,
        sasl_config,
        ssl_config,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_security_protocol_as_str() {
        assert_eq!(SecurityProtocol::Plaintext.as_str(), "PLAINTEXT");
        assert_eq!(SecurityProtocol::Ssl.as_str(), "SSL");
        assert_eq!(SecurityProtocol::SaslPlaintext.as_str(), "SASL_PLAINTEXT");
        assert_eq!(SecurityProtocol::SaslSsl.as_str(), "SASL_SSL");
    }

    #[test]
    fn test_sasl_mechanism_as_str() {
        assert_eq!(SaslMechanism::Plain.as_str(), "PLAIN");
        assert_eq!(SaslMechanism::ScramSha256.as_str(), "SCRAM-SHA-256");
        assert_eq!(SaslMechanism::ScramSha512.as_str(), "SCRAM-SHA-512");
    }

    #[test]
    fn test_security_protocol_default_is_plaintext() {
        assert_eq!(SecurityProtocol::default(), SecurityProtocol::Plaintext);
    }

    #[test]
    fn test_sasl_mechanism_default_is_scram_sha256() {
        assert_eq!(SaslMechanism::default(), SaslMechanism::ScramSha256);
    }

    #[test]
    fn test_parse_security_protocol_valid_values() {
        assert_eq!(
            parse_security_protocol("PLAINTEXT").unwrap(),
            SecurityProtocol::Plaintext
        );
        assert_eq!(
            parse_security_protocol("SSL").unwrap(),
            SecurityProtocol::Ssl
        );
        assert_eq!(
            parse_security_protocol("SASL_PLAINTEXT").unwrap(),
            SecurityProtocol::SaslPlaintext
        );
        assert_eq!(
            parse_security_protocol("SASL_SSL").unwrap(),
            SecurityProtocol::SaslSsl
        );
    }

    #[test]
    fn test_parse_security_protocol_is_case_insensitive() {
        assert_eq!(
            parse_security_protocol("sasl_ssl").unwrap(),
            SecurityProtocol::SaslSsl
        );
        assert_eq!(
            parse_security_protocol("Plaintext").unwrap(),
            SecurityProtocol::Plaintext
        );
    }

    #[test]
    fn test_parse_security_protocol_invalid_errors() {
        assert!(parse_security_protocol("INVALID").is_err());
        assert!(parse_security_protocol("").is_err());
    }

    #[test]
    fn test_parse_sasl_mechanism_valid_values() {
        assert_eq!(parse_sasl_mechanism("PLAIN").unwrap(), SaslMechanism::Plain);
        assert_eq!(
            parse_sasl_mechanism("SCRAM-SHA-256").unwrap(),
            SaslMechanism::ScramSha256
        );
        assert_eq!(
            parse_sasl_mechanism("SCRAM-SHA-512").unwrap(),
            SaslMechanism::ScramSha512
        );
    }

    #[test]
    fn test_parse_sasl_mechanism_is_case_insensitive() {
        assert_eq!(
            parse_sasl_mechanism("scram-sha-256").unwrap(),
            SaslMechanism::ScramSha256
        );
        assert_eq!(parse_sasl_mechanism("plain").unwrap(), SaslMechanism::Plain);
    }

    #[test]
    fn test_parse_sasl_mechanism_invalid_errors() {
        assert!(parse_sasl_mechanism("INVALID").is_err());
        assert!(parse_sasl_mechanism("").is_err());
    }

    #[test]
    fn test_apply_security_config_plaintext_has_no_sasl_or_ssl() {
        let security = KafkaSecurityConfig {
            protocol: "PLAINTEXT".to_string(),
            sasl_mechanism: None,
            sasl_username: None,
            sasl_password: None,
            ssl_ca_location: None,
        };

        let resolved = apply_security_config(&security).unwrap();
        assert_eq!(resolved.security_protocol, SecurityProtocol::Plaintext);
        assert!(resolved.sasl_config.is_none());
        assert!(resolved.ssl_config.is_none());
    }

    #[test]
    fn test_apply_security_config_ssl_sets_ssl_config() {
        let security = KafkaSecurityConfig {
            protocol: "SSL".to_string(),
            sasl_mechanism: None,
            sasl_username: None,
            sasl_password: None,
            ssl_ca_location: None,
        };

        let resolved = apply_security_config(&security).unwrap();
        assert_eq!(resolved.security_protocol, SecurityProtocol::Ssl);
        assert!(resolved.ssl_config.is_some());
    }

    #[test]
    fn test_apply_security_config_sasl_ssl_resolves_credentials() {
        let security = KafkaSecurityConfig {
            protocol: "SASL_SSL".to_string(),
            sasl_mechanism: Some("SCRAM-SHA-256".to_string()),
            sasl_username: Some("user1".to_string()),
            sasl_password: Some("pass1".to_string()),
            ssl_ca_location: None,
        };

        let resolved = apply_security_config(&security).unwrap();
        assert_eq!(resolved.security_protocol, SecurityProtocol::SaslSsl);
        let sasl = resolved.sasl_config.unwrap();
        assert_eq!(sasl.mechanism, SaslMechanism::ScramSha256);
        assert_eq!(sasl.username, "user1");
        assert_eq!(sasl.password, "pass1");
        assert!(resolved.ssl_config.is_some());
    }

    #[test]
    fn test_apply_security_config_invalid_protocol_errors() {
        let security = KafkaSecurityConfig {
            protocol: "INVALID".to_string(),
            sasl_mechanism: None,
            sasl_username: None,
            sasl_password: None,
            ssl_ca_location: None,
        };

        assert!(apply_security_config(&security).is_err());
    }

    #[test]
    fn test_apply_security_config_missing_username_errors() {
        let security = KafkaSecurityConfig {
            protocol: "SASL_SSL".to_string(),
            sasl_mechanism: Some("PLAIN".to_string()),
            sasl_username: None,
            sasl_password: Some("secret".to_string()),
            ssl_ca_location: None,
        };

        assert!(apply_security_config(&security).is_err());
    }

    #[test]
    fn test_apply_security_config_ssl_propagates_ca_location() {
        let security = KafkaSecurityConfig {
            protocol: "SSL".to_string(),
            sasl_mechanism: None,
            sasl_username: None,
            sasl_password: None,
            ssl_ca_location: Some("/etc/ssl/certs/ca.pem".to_string()),
        };

        let resolved = apply_security_config(&security).unwrap();
        let ssl = resolved.ssl_config.unwrap();
        assert_eq!(ssl.ca_location.as_deref(), Some("/etc/ssl/certs/ca.pem"));
    }

    #[test]
    fn test_apply_security_config_sasl_ssl_propagates_ca_location() {
        let security = KafkaSecurityConfig {
            protocol: "SASL_SSL".to_string(),
            sasl_mechanism: Some("SCRAM-SHA-256".to_string()),
            sasl_username: Some("user".to_string()),
            sasl_password: Some("pass".to_string()),
            ssl_ca_location: Some("/certs/ca.pem".to_string()),
        };

        let resolved = apply_security_config(&security).unwrap();
        let ssl = resolved.ssl_config.unwrap();
        assert_eq!(ssl.ca_location.as_deref(), Some("/certs/ca.pem"));
    }

    #[test]
    fn test_apply_security_config_ssl_none_ca_location_gives_none() {
        let security = KafkaSecurityConfig {
            protocol: "SSL".to_string(),
            sasl_mechanism: None,
            sasl_username: None,
            sasl_password: None,
            ssl_ca_location: None,
        };

        let resolved = apply_security_config(&security).unwrap();
        let ssl = resolved.ssl_config.unwrap();
        assert!(ssl.ca_location.is_none());
    }

    #[test]
    fn test_apply_security_config_plaintext_ignores_ca_location() {
        let security = KafkaSecurityConfig {
            protocol: "PLAINTEXT".to_string(),
            sasl_mechanism: None,
            sasl_username: None,
            sasl_password: None,
            ssl_ca_location: Some("/etc/ssl/certs/ca.pem".to_string()),
        };

        let resolved = apply_security_config(&security).unwrap();
        assert!(resolved.ssl_config.is_none());
    }

    #[test]
    fn test_apply_security_config_sasl_plaintext_ignores_ca_location() {
        let security = KafkaSecurityConfig {
            protocol: "SASL_PLAINTEXT".to_string(),
            sasl_mechanism: Some("SCRAM-SHA-256".to_string()),
            sasl_username: Some("user".to_string()),
            sasl_password: Some("pass".to_string()),
            ssl_ca_location: Some("/etc/ssl/certs/ca.pem".to_string()),
        };

        let resolved = apply_security_config(&security).unwrap();
        assert!(resolved.ssl_config.is_none());
    }

    #[test]
    fn test_is_insecure_protocol_flags_unencrypted_protocols() {
        assert!(is_insecure_protocol(&SecurityProtocol::Plaintext));
        assert!(is_insecure_protocol(&SecurityProtocol::SaslPlaintext));
        assert!(!is_insecure_protocol(&SecurityProtocol::Ssl));
        assert!(!is_insecure_protocol(&SecurityProtocol::SaslSsl));
    }

    #[test]
    fn test_validate_payload_size_at_limit_is_ok() {
        let payload = "x".repeat(1024);
        assert!(validate_payload_size(&payload, 1024).is_ok());
    }

    #[test]
    fn test_validate_payload_size_under_limit_is_ok() {
        assert!(validate_payload_size("small payload", 1024).is_ok());
    }

    #[test]
    fn test_validate_payload_size_over_limit_is_err() {
        let payload = "x".repeat(1025);
        let error = validate_payload_size(&payload, 1024).unwrap_err();
        assert!(matches!(error, XzatomaError::Watcher(_)));
        assert!(error.to_string().contains("1025"));
        assert!(error.to_string().contains("1024"));
    }

    #[test]
    fn test_default_max_payload_bytes_is_one_mebibyte() {
        assert_eq!(DEFAULT_MAX_PAYLOAD_BYTES, 1024 * 1024);
    }
}

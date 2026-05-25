//! Shared security helpers for network and credential handling.
//!
//! This module contains small, dependency-light helpers used by providers,
//! tools, ACP, and MCP code paths to keep URL validation and secret redaction
//! consistent without coupling those modules to each other.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use url::Url;

use crate::error::{Result, XzatomaError};

/// Normalizes an HTTP base URL after validating that it is structurally safe.
///
/// The returned string has trailing slash characters removed so callers can
/// append endpoint paths deterministically.
pub(crate) fn normalize_http_base_url(input: &str, field_name: &str) -> Result<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(XzatomaError::Config(format!(
            "{} cannot be empty",
            field_name
        )));
    }

    let url = Url::parse(trimmed).map_err(|error| {
        XzatomaError::Config(format!("{} must be a valid URL: {}", field_name, error))
    })?;
    validate_http_url_components(&url, field_name)?;

    Ok(trimmed.trim_end_matches('/').to_string())
}

/// Validates a provider base URL used for prompt or credential-bearing traffic.
///
/// The policy allows local development over plain HTTP only for loopback hosts.
/// Remote hosts must use HTTPS and must not be literal private or special-use
/// IP addresses. This keeps normal configuration from sending prompts, code, or
/// API keys to untrusted plaintext endpoints.
pub(crate) fn validate_provider_base_url(input: &str, field_name: &str) -> Result<String> {
    let normalized = normalize_http_base_url(input, field_name)?;
    let url = Url::parse(&normalized).map_err(|error| {
        XzatomaError::Config(format!("{} must be a valid URL: {}", field_name, error))
    })?;
    let host = url
        .host_str()
        .ok_or_else(|| XzatomaError::Config(format!("{} must include a host", field_name)))?;

    match url.scheme() {
        "http" if is_loopback_host(host) => Ok(normalized),
        "http" => Err(XzatomaError::Config(format!(
            "{} may use http only for localhost or loopback addresses; use https or an explicit local endpoint",
            field_name
        ))),
        "https" => {
            if host.eq_ignore_ascii_case("localhost") {
                return Ok(normalized);
            }
            if let Ok(ip) = host.parse::<IpAddr>() {
                validate_config_public_ip(ip, field_name)?;
            }
            Ok(normalized)
        }
        other => Err(XzatomaError::Config(format!(
            "{} scheme must be 'http' or 'https', got '{}'",
            field_name, other
        ))),
    }
}

/// Validates that an HTTP base URL points at a loopback host.
///
/// This is used for test-only provider endpoint overrides that may receive
/// credentials during local mock-server tests.
pub(crate) fn validate_loopback_http_base_url(input: &str, field_name: &str) -> Result<()> {
    let normalized = normalize_http_base_url(input, field_name)?;
    let url = Url::parse(&normalized).map_err(|error| {
        XzatomaError::Config(format!("{} must be a valid URL: {}", field_name, error))
    })?;
    let host = url
        .host_str()
        .ok_or_else(|| XzatomaError::Config(format!("{} must include a host", field_name)))?;

    if is_loopback_host(host) {
        Ok(())
    } else {
        Err(XzatomaError::Config(format!(
            "{} may only target localhost or loopback addresses",
            field_name
        )))
    }
}

/// Validates an OAuth URL before it is used for discovery or token exchange.
///
/// The synchronous checks reject non-HTTPS URLs, embedded credentials,
/// fragments, missing hosts, and literal private or special-purpose IPs.
pub(crate) fn validate_public_https_url_sync(url: &Url, field_name: &str) -> Result<()> {
    if url.scheme() != "https" {
        return Err(XzatomaError::McpAuth(format!(
            "{} must use https, got '{}'",
            field_name,
            url.scheme()
        )));
    }

    if !url.username().is_empty() || url.password().is_some() {
        return Err(XzatomaError::McpAuth(format!(
            "{} must not include embedded credentials",
            field_name
        )));
    }

    if url.host_str().is_none() {
        return Err(XzatomaError::McpAuth(format!(
            "{} must include a host",
            field_name
        )));
    }

    if url.fragment().is_some() {
        return Err(XzatomaError::McpAuth(format!(
            "{} must not include a fragment",
            field_name
        )));
    }

    if let Some(host) = url.host_str() {
        if host.eq_ignore_ascii_case("localhost") {
            return Err(XzatomaError::McpAuth(format!(
                "{} must not target localhost",
                field_name
            )));
        }

        if let Ok(ip) = host.parse::<IpAddr>() {
            validate_public_ip(ip, field_name)?;
        }
    }

    Ok(())
}

/// Validates an OAuth URL and its DNS resolution before issuing a request.
///
/// Every resolved address must be public. DNS failures are treated as validation
/// failures so discovery cannot silently fall back to an unverified target.
pub(crate) async fn validate_public_https_url(url: &Url, field_name: &str) -> Result<()> {
    validate_public_https_url_sync(url, field_name)?;

    let host = url
        .host_str()
        .ok_or_else(|| XzatomaError::McpAuth(format!("{} must include a host", field_name)))?;

    if host.parse::<IpAddr>().is_ok() {
        return Ok(());
    }

    let port = url.port_or_known_default().unwrap_or(443);
    let resolved = tokio::net::lookup_host((host, port))
        .await
        .map_err(|error| {
            XzatomaError::McpAuth(format!(
                "failed to resolve {} host '{}': {}",
                field_name, host, error
            ))
        })?;

    let mut saw_address = false;
    for socket in resolved {
        saw_address = true;
        validate_public_ip(socket.ip(), field_name)?;
    }

    if saw_address {
        Ok(())
    } else {
        Err(XzatomaError::McpAuth(format!(
            "{} host '{}' resolved to no addresses",
            field_name, host
        )))
    }
}

/// Validates that two URLs have the same scheme, host, and effective port.
pub(crate) fn validate_same_origin(expected: &Url, actual: &Url, field_name: &str) -> Result<()> {
    let expected_port = expected.port_or_known_default();
    let actual_port = actual.port_or_known_default();
    if expected.scheme() == actual.scheme()
        && expected.host_str() == actual.host_str()
        && expected_port == actual_port
    {
        Ok(())
    } else {
        Err(XzatomaError::McpAuth(format!(
            "{} must have the same origin as issuer {}",
            field_name, expected
        )))
    }
}

/// Redacts common credential patterns from diagnostic text.
pub(crate) fn redact_sensitive_text(input: &str) -> String {
    let mut redacted = input.to_string();

    for marker in [
        "Authorization:",
        "authorization:",
        "Bearer ",
        "token ",
        "api_key",
    ] {
        if redacted.contains(marker) {
            redacted = redact_after_marker(&redacted, marker);
        }
    }

    redacted
}

fn validate_http_url_components(url: &Url, field_name: &str) -> Result<()> {
    match url.scheme() {
        "http" | "https" => {}
        other => {
            return Err(XzatomaError::Config(format!(
                "{} scheme must be 'http' or 'https', got '{}'",
                field_name, other
            )));
        }
    }

    if url.host_str().is_none() {
        return Err(XzatomaError::Config(format!(
            "{} must include a host",
            field_name
        )));
    }

    if !url.username().is_empty() || url.password().is_some() {
        return Err(XzatomaError::Config(format!(
            "{} must not include embedded credentials",
            field_name
        )));
    }

    if url.query().is_some() || url.fragment().is_some() {
        return Err(XzatomaError::Config(format!(
            "{} must not include a query string or fragment",
            field_name
        )));
    }

    Ok(())
}

fn is_loopback_host(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }

    host.parse::<IpAddr>()
        .map(|ip| match ip {
            IpAddr::V4(v4) => v4.is_loopback(),
            IpAddr::V6(v6) => v6.is_loopback(),
        })
        .unwrap_or(false)
}

fn validate_config_public_ip(ip: IpAddr, field_name: &str) -> Result<()> {
    if is_blocked_ip(ip) {
        Err(XzatomaError::Config(format!(
            "{} resolved to blocked address {}",
            field_name, ip
        )))
    } else {
        Ok(())
    }
}

fn validate_public_ip(ip: IpAddr, field_name: &str) -> Result<()> {
    if is_blocked_ip(ip) {
        Err(XzatomaError::McpAuth(format!(
            "{} resolved to blocked address {}",
            field_name, ip
        )))
    } else {
        Ok(())
    }
}

fn is_blocked_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_blocked_ipv4(v4),
        IpAddr::V6(v6) => is_blocked_ipv6(v6),
    }
}

fn is_blocked_ipv4(ip: Ipv4Addr) -> bool {
    let octets = ip.octets();
    ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_unspecified()
        || ip.is_multicast()
        || ip == Ipv4Addr::BROADCAST
        || octets[0] == 100 && (octets[1] & 0b1100_0000) == 64
        || octets[0] == 169 && octets[1] == 254
}

fn is_blocked_ipv6(ip: Ipv6Addr) -> bool {
    let first = ip.segments()[0];
    ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        || (first & 0xfe00) == 0xfc00
        || (first & 0xffc0) == 0xfe80
}

fn redact_after_marker(input: &str, marker: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut remaining = input;

    while let Some(index) = remaining.find(marker) {
        let (prefix, suffix) = remaining.split_at(index + marker.len());
        output.push_str(prefix);
        let secret_len = suffix
            .chars()
            .take_while(|ch| !ch.is_whitespace() && *ch != ',' && *ch != '"')
            .map(char::len_utf8)
            .sum::<usize>();
        if secret_len > 0 {
            output.push_str("[REDACTED]");
            remaining = &suffix[secret_len..];
        } else {
            remaining = suffix;
        }
    }

    output.push_str(remaining);
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_http_base_url_trims_trailing_slash() {
        let normalized = normalize_http_base_url("https://api.example.com/v1/", "base_url")
            .expect("valid URL should normalize");
        assert_eq!(normalized, "https://api.example.com/v1");
    }

    #[test]
    fn test_normalize_http_base_url_rejects_credentials() {
        let result = normalize_http_base_url("https://user:pass@example.com", "base_url");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_loopback_http_base_url_accepts_loopback() {
        let result = validate_loopback_http_base_url("http://127.0.0.1:8080", "api_base");
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_loopback_http_base_url_rejects_external_host() {
        let result = validate_loopback_http_base_url("https://api.example.com", "api_base");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_provider_base_url_accepts_https_public_host() {
        let result = validate_provider_base_url("https://api.example.com/v1/", "base_url")
            .expect("https public provider URL should be accepted");
        assert_eq!(result, "https://api.example.com/v1");
    }

    #[test]
    fn test_validate_provider_base_url_accepts_http_loopback() {
        let result = validate_provider_base_url("http://127.0.0.1:11434/", "base_url")
            .expect("http loopback provider URL should be accepted");
        assert_eq!(result, "http://127.0.0.1:11434");
    }

    #[test]
    fn test_validate_provider_base_url_rejects_http_remote_host() {
        let result = validate_provider_base_url("http://api.example.com/v1", "base_url");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_provider_base_url_rejects_private_ip() {
        let result = validate_provider_base_url("https://192.168.1.10/v1", "base_url");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_public_https_url_sync_rejects_localhost() {
        let url = Url::parse("https://localhost/oauth").expect("test URL should parse");
        let result = validate_public_https_url_sync(&url, "metadata URL");
        assert!(result.is_err());
    }

    #[test]
    fn test_redact_sensitive_text_removes_bearer_value() {
        let redacted = redact_sensitive_text("Authorization: Bearer abc123");
        assert!(redacted.contains("[REDACTED]"));
        assert!(!redacted.contains("abc123"));
    }
}

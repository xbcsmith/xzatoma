//! Fetch tool for retrieving web content via HTTP
//!
//! This module provides secure HTTP content fetching with:
//! - SSRF prevention (blocking private IP ranges and dangerous schemes)
//! - Content type validation and conversion to Markdown
//! - Size limits and timeouts
//! - Rate limiting
//! - Caching support

use crate::error::{Result, XzatomaError};
use futures::StreamExt;
use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
use std::str::FromStr;
use std::sync::LazyLock;
use std::time::Duration;
use url::Url;

// Precompiled regexes used by `html_to_markdown`. Compiling these once at
// startup avoids re-parsing the patterns (and the associated fallible
// `Regex::new(...).unwrap()`) on every HTML-to-Markdown conversion.
//
// SAFETY: Every pattern below is a constant string literal that is known-valid
// at author time; `Regex::new` on these cannot fail at runtime, so `.expect`
// in each initializer is unreachable in practice.

// Matches `<script>...</script>` blocks (case-insensitive) so their contents
// can be stripped before conversion.
#[allow(clippy::expect_used)]
static SCRIPT_TAG_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"(?i)<script[^>]*>.*?</script>").expect("valid regex"));

// Matches `<style>...</style>` blocks (case-insensitive) so their contents can
// be stripped before conversion.
#[allow(clippy::expect_used)]
static STYLE_TAG_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"(?i)<style[^>]*>.*?</style>").expect("valid regex"));

// Matches `<p>...</p>` paragraph tags for conversion to blank-line separated
// text.
#[allow(clippy::expect_used)]
static PARAGRAPH_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"(?i)<p[^>]*>(.*?)</p>").expect("valid regex"));

// Matches `<a href="...">text</a>` anchors for conversion to Markdown links.
#[allow(clippy::expect_used)]
static ANCHOR_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r#"(?i)<a[^>]*href\s*=\s*['"]([^'"]*)['"'][^>]*>(.*?)</a>"#)
        .expect("valid regex")
});

// Matches `<b>`/`<strong>` tags for conversion to Markdown bold.
#[allow(clippy::expect_used)]
static BOLD_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"(?i)<(?:b|strong)[^>]*>(.*?)</(?:b|strong)>").expect("valid regex")
});

// Matches `<i>`/`<em>` tags for conversion to Markdown italic.
#[allow(clippy::expect_used)]
static ITALIC_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"(?i)<(?:i|em)[^>]*>(.*?)</(?:i|em)>").expect("valid regex")
});

// Matches `<br>` line breaks (and trailing spaces) for conversion to newlines.
#[allow(clippy::expect_used)]
static LINE_BREAK_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"(?i)<br\s*/?> *").expect("valid regex"));

// Matches any remaining HTML tag so it can be removed from the output.
#[allow(clippy::expect_used)]
static HTML_TAG_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"<[^>]+>").expect("valid regex"));

// Matches runs of three or more blank lines for whitespace normalization.
#[allow(clippy::expect_used)]
static WHITESPACE_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"\n\s*\n\s*\n+").expect("valid regex"));

/// Information about fetched web content
///
/// Contains the fetched content, metadata, and fetch information.
#[derive(Debug, Clone)]
pub struct FetchedContent {
    /// The fetched content converted to Markdown
    pub content: String,
    /// Original URL that was fetched
    pub url: String,
    /// Content type of the fetched resource
    pub content_type: String,
    /// Size of the fetched content in bytes
    pub size_bytes: usize,
    /// Whether the content was truncated due to size limit
    pub truncated: bool,
    /// HTTP status code
    pub status_code: u16,
}

impl FetchedContent {
    /// Create a new FetchedContent instance
    ///
    /// # Arguments
    ///
    /// * `content` - The fetched content
    /// * `url` - The URL that was fetched
    /// * `content_type` - The content type of the resource
    /// * `status_code` - The HTTP status code
    ///
    /// # Returns
    ///
    /// Returns a new FetchedContent instance
    pub fn new(content: String, url: String, content_type: String, status_code: u16) -> Self {
        let size_bytes = content.len();
        Self {
            content,
            url,
            content_type,
            size_bytes,
            truncated: false,
            status_code,
        }
    }

    /// Mark content as truncated
    ///
    /// # Returns
    ///
    /// Returns self for chaining
    pub fn with_truncated(mut self, truncated: bool) -> Self {
        self.truncated = truncated;
        self
    }

    /// Format content with header information
    ///
    /// # Returns
    ///
    /// Returns a formatted string with URL and metadata
    pub fn format_with_header(&self, timestamp: Option<String>) -> String {
        let truncation_note = if self.truncated {
            "\n\n[Content truncated at size limit]"
        } else {
            ""
        };

        let timestamp_str = timestamp
            .map(|ts| format!(" (fetched {})", ts))
            .unwrap_or_default();

        format!(
            "Web content from {}{}\n\nContent-Type: {}\nSize: {} bytes\n\n{}{}\n",
            self.url,
            timestamp_str,
            self.content_type,
            self.size_bytes,
            self.content,
            truncation_note
        )
    }
}

/// A validated resolution target produced by [`SsrfValidator::validate_and_resolve`].
///
/// Captures the host, the effective port, and every socket address that passed
/// SSRF validation. Callers pin their connection to one of these already
/// validated addresses to close the DNS-rebinding time-of-check/time-of-use gap
/// that exists when an HTTP client resolves the host a second time at send time.
///
/// # Examples
///
/// ```
/// use xzatoma::tools::fetch::SsrfValidator;
///
/// let validator = SsrfValidator::new();
/// let target = validator
///     .validate_and_resolve("https://93.184.216.34/")
///     .expect("public IP literal should validate");
/// assert_eq!(target.host, "93.184.216.34");
/// assert_eq!(target.port, 443);
/// assert_eq!(target.socket_addrs.len(), 1);
/// ```
#[derive(Debug, Clone)]
pub struct ResolvedTarget {
    /// The host component of the URL (a DNS name or an IP literal).
    pub host: String,
    /// The effective port (explicit port or scheme default).
    pub port: u16,
    /// Socket addresses that passed SSRF validation.
    pub socket_addrs: Vec<std::net::SocketAddr>,
}

/// SSRF (Server-Side Request Forgery) prevention validator
///
/// Prevents requests to private IP ranges and dangerous schemes.
#[derive(Debug, Clone)]
pub struct SsrfValidator {
    /// Whether to allow private IPs (for testing)
    allow_private_ips: bool,
}

impl SsrfValidator {
    /// Create a new SSRF validator
    ///
    /// # Returns
    ///
    /// Returns a new SsrfValidator with default settings
    pub fn new() -> Self {
        Self {
            allow_private_ips: false,
        }
    }

    /// Create a validator that allows private IPs (for testing only)
    ///
    /// # Returns
    ///
    /// Returns a new SsrfValidator that allows private IPs
    pub fn allow_private_ips() -> Self {
        Self {
            allow_private_ips: true,
        }
    }

    /// Validate a URL for SSRF attacks
    ///
    /// # Arguments
    ///
    /// * `url` - The URL to validate
    ///
    /// # Errors
    ///
    /// Returns error if URL is invalid or potentially dangerous
    pub fn validate(&self, url: &str) -> Result<()> {
        // Parse URL
        let parsed_url =
            Url::parse(url).map_err(|e| XzatomaError::Fetch(format!("Invalid URL: {}", e)))?;

        // Validate scheme
        self.validate_scheme(parsed_url.scheme())?;

        // Validate host
        if let Some(host) = parsed_url.host_str() {
            self.validate_host(host)?;
        } else {
            return Err(XzatomaError::Fetch("URL has no host".to_string()));
        }

        Ok(())
    }

    /// Validate a URL and return the validated resolved socket addresses.
    ///
    /// This performs the same SSRF checks as [`SsrfValidator::validate`] but also
    /// returns the concrete socket addresses that passed validation. Callers can
    /// pin the connection to one of these already-validated addresses to close
    /// the DNS-rebinding time-of-check/time-of-use gap that exists when the HTTP
    /// client resolves the host a second time at send time.
    ///
    /// For an IP-literal host the returned vector contains that single address.
    /// For a DNS name the host is resolved once and every resolved address must
    /// pass validation.
    ///
    /// # Arguments
    ///
    /// * `url` - The URL to validate and resolve
    ///
    /// # Returns
    ///
    /// Returns a [`ResolvedTarget`] describing the validated host, port, and
    /// socket addresses.
    ///
    /// # Errors
    ///
    /// Returns an error if the URL is invalid, the scheme is unsupported, the
    /// host is missing, resolution fails, or any resolved address is in a
    /// blocked range.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::tools::fetch::SsrfValidator;
    ///
    /// let validator = SsrfValidator::new();
    /// let target = validator
    ///     .validate_and_resolve("https://93.184.216.34/")
    ///     .expect("public IP literal should validate");
    /// assert_eq!(target.port, 443);
    /// assert_eq!(target.socket_addrs.len(), 1);
    /// ```
    pub fn validate_and_resolve(&self, url: &str) -> Result<ResolvedTarget> {
        // Parse URL
        let parsed_url =
            Url::parse(url).map_err(|e| XzatomaError::Fetch(format!("Invalid URL: {}", e)))?;

        // Validate scheme
        self.validate_scheme(parsed_url.scheme())?;

        let host = parsed_url
            .host_str()
            .ok_or_else(|| XzatomaError::Fetch("URL has no host".to_string()))?
            .to_string();

        // Use the explicit port or the scheme default.
        let port = parsed_url.port_or_known_default().unwrap_or({
            if parsed_url.scheme() == "https" {
                443
            } else {
                80
            }
        });

        // Block explicit localhost variants unless private IPs are allowed.
        if !self.allow_private_ips && (host == "localhost" || host == "127.0.0.1" || host == "::1")
        {
            return Err(XzatomaError::Fetch(
                "Requests to localhost are not allowed".to_string(),
            ));
        }

        // IP literal: validate once and pin that single address.
        if let Ok(ip) = IpAddr::from_str(&host) {
            self.validate_ip(ip)?;
            return Ok(ResolvedTarget {
                host,
                port,
                socket_addrs: vec![SocketAddr::new(ip, port)],
            });
        }

        // DNS name: resolve once and validate every resolved address.
        let socket_addrs = self.resolve_host_socket_addrs(&host, port)?;
        for addr in &socket_addrs {
            self.validate_ip(addr.ip())?;
        }

        Ok(ResolvedTarget {
            host,
            port,
            socket_addrs,
        })
    }

    /// Validate the IP address a request actually connected to.
    ///
    /// After an HTTP client establishes a connection, the peer address should be
    /// re-checked to reject a DNS rebinding that changed the resolved address
    /// between validation and connection. This reuses the same per-IP blocking
    /// logic as URL validation.
    ///
    /// # Arguments
    ///
    /// * `ip` - The peer IP address the request connected to
    ///
    /// # Errors
    ///
    /// Returns an error if the address is in a blocked (private, loopback,
    /// link-local, or otherwise disallowed) range. In private-IP test mode this
    /// always returns `Ok`.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::net::{IpAddr, Ipv4Addr};
    /// use xzatoma::tools::fetch::SsrfValidator;
    ///
    /// let validator = SsrfValidator::new();
    /// let public = IpAddr::V4(Ipv4Addr::new(93, 184, 216, 34));
    /// assert!(validator.validate_connected_ip(public).is_ok());
    ///
    /// let loopback = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
    /// assert!(validator.validate_connected_ip(loopback).is_err());
    /// ```
    pub fn validate_connected_ip(&self, ip: std::net::IpAddr) -> Result<()> {
        self.validate_ip(ip)
    }

    /// Validate URL scheme
    ///
    /// # Arguments
    ///
    /// * `scheme` - The URL scheme
    ///
    /// # Errors
    ///
    /// Returns error if scheme is not http or https
    fn validate_scheme(&self, scheme: &str) -> Result<()> {
        match scheme {
            "http" | "https" => Ok(()),
            "file" => Err(XzatomaError::Fetch(
                "file:// URLs are not allowed for security reasons".to_string(),
            )),
            "ftp" => Err(XzatomaError::Fetch(
                "ftp:// URLs are not allowed for security reasons".to_string(),
            )),
            _ => Err(XzatomaError::Fetch(format!(
                "Unsupported URL scheme: {}",
                scheme
            ))),
        }
    }

    /// Validate hostname to prevent SSRF
    ///
    /// # Arguments
    ///
    /// * `host` - The hostname to validate
    ///
    /// # Errors
    ///
    /// Returns error if hostname resolves to a private IP
    fn validate_host(&self, host: &str) -> Result<()> {
        // Check for localhost variants
        if !self.allow_private_ips && (host == "localhost" || host == "127.0.0.1" || host == "::1")
        {
            return Err(XzatomaError::Fetch(
                "Requests to localhost are not allowed".to_string(),
            ));
        }

        // Try to parse as IP address
        if let Ok(ip) = IpAddr::from_str(host) {
            return self.validate_ip(ip);
        }

        let resolved_ips = self.resolve_host_ips(host)?;
        for ip in resolved_ips {
            self.validate_ip(ip)?;
        }

        Ok(())
    }

    /// Resolve a hostname to IP addresses for SSRF validation
    fn resolve_host_ips(&self, host: &str) -> Result<Vec<IpAddr>> {
        let addrs = (host, 80).to_socket_addrs().map_err(|e| {
            XzatomaError::Fetch(format!("Failed to resolve host '{}': {}", host, e))
        })?;

        let mut ips = Vec::new();
        for addr in addrs {
            ips.push(addr.ip());
        }

        if ips.is_empty() {
            return Err(XzatomaError::Fetch(format!(
                "Failed to resolve host '{}': no addresses",
                host
            )));
        }

        Ok(ips)
    }

    /// Resolve a hostname to socket addresses for the given port.
    ///
    /// Unlike [`SsrfValidator::resolve_host_ips`], this captures the full socket
    /// address (IP and port) so callers can pin an HTTP connection to a
    /// validated address.
    fn resolve_host_socket_addrs(&self, host: &str, port: u16) -> Result<Vec<SocketAddr>> {
        let addrs = (host, port).to_socket_addrs().map_err(|e| {
            XzatomaError::Fetch(format!("Failed to resolve host '{}': {}", host, e))
        })?;

        let socket_addrs: Vec<SocketAddr> = addrs.collect();

        if socket_addrs.is_empty() {
            return Err(XzatomaError::Fetch(format!(
                "Failed to resolve host '{}': no addresses",
                host
            )));
        }

        Ok(socket_addrs)
    }

    /// Validate IP address
    ///
    /// # Arguments
    ///
    /// * `ip` - The IP address to validate
    ///
    /// # Errors
    ///
    /// Returns error if IP is in a private range
    fn validate_ip(&self, ip: IpAddr) -> Result<()> {
        if self.allow_private_ips {
            return Ok(());
        }

        // Check for private IP ranges
        match ip {
            IpAddr::V4(v4) => {
                // 127.0.0.0/8 (localhost)
                if v4.octets()[0] == 127 {
                    return Err(XzatomaError::Fetch(
                        "Requests to loopback addresses are not allowed".to_string(),
                    ));
                }
                if v4.octets()[0] == 10 {
                    return Err(XzatomaError::Fetch(
                        "Requests to private IP ranges are not allowed".to_string(),
                    ));
                }
                if v4.octets()[0] == 172 && (v4.octets()[1] >= 16 && v4.octets()[1] <= 31) {
                    return Err(XzatomaError::Fetch(
                        "Requests to private IP ranges are not allowed".to_string(),
                    ));
                }
                if v4.octets()[0] == 192 && v4.octets()[1] == 168 {
                    return Err(XzatomaError::Fetch(
                        "Requests to private IP ranges are not allowed".to_string(),
                    ));
                }
                if v4.octets()[0] == 169 && v4.octets()[1] == 254 {
                    return Err(XzatomaError::Fetch(
                        "Requests to link-local addresses are not allowed".to_string(),
                    ));
                }
                if v4.octets()[0] == 0 {
                    return Err(XzatomaError::Fetch(
                        "Requests to this network are not allowed".to_string(),
                    ));
                }
                if v4 == std::net::Ipv4Addr::BROADCAST {
                    return Err(XzatomaError::Fetch(
                        "Requests to broadcast address are not allowed".to_string(),
                    ));
                }
                Ok(())
            }
            IpAddr::V6(v6) => {
                // Block IPv6 loopback (::1)
                if v6.octets() == [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1] {
                    return Err(XzatomaError::Fetch(
                        "Requests to loopback addresses are not allowed".to_string(),
                    ));
                }
                if (v6.segments()[0] & 0xfe00) == 0xfc00 {
                    return Err(XzatomaError::Fetch(
                        "Requests to private IP ranges are not allowed".to_string(),
                    ));
                }
                if (v6.segments()[0] & 0xffc0) == 0xfe80 {
                    return Err(XzatomaError::Fetch(
                        "Requests to link-local addresses are not allowed".to_string(),
                    ));
                }
                Ok(())
            }
        }
    }
}

impl Default for SsrfValidator {
    fn default() -> Self {
        Self::new()
    }
}

/// Rate limiter for HTTP requests
///
/// Simple token-bucket rate limiter to prevent abuse.
#[derive(Debug, Clone)]
pub struct RateLimiter {
    /// Maximum number of requests per minute
    max_requests_per_minute: u32,
    /// Request timestamps (kept for tracking)
    requests: Vec<std::time::SystemTime>,
}

impl RateLimiter {
    /// Create a new rate limiter
    ///
    /// # Arguments
    ///
    /// * `max_requests_per_minute` - Maximum requests allowed per minute
    ///
    /// # Returns
    ///
    /// Returns a new RateLimiter instance
    pub fn new(max_requests_per_minute: u32) -> Self {
        Self {
            max_requests_per_minute,
            requests: Vec::new(),
        }
    }

    /// Check if a request is allowed
    ///
    /// # Returns
    ///
    /// Returns Ok if request is allowed, Error if rate limit exceeded
    pub fn check_and_record(&mut self) -> Result<()> {
        let now = std::time::SystemTime::now();
        let one_minute_ago = now.checked_sub(Duration::from_secs(60)).unwrap_or(now);

        // Remove old requests outside the time window
        self.requests.retain(|&req_time| req_time > one_minute_ago);

        // Check if limit exceeded
        if self.requests.len() >= self.max_requests_per_minute as usize {
            return Err(XzatomaError::Fetch(format!(
                "Rate limit exceeded: {} requests per minute",
                self.max_requests_per_minute
            )));
        }

        // Record new request
        self.requests.push(now);
        Ok(())
    }
}

/// HTTP client for fetching web content
///
/// Provides secure HTTP fetching with SSRF prevention, size limits,
/// and content type handling.
#[derive(Clone)]
pub struct FetchTool {
    /// HTTP client instance
    client: reqwest::Client,
    /// SSRF validator
    ssrf_validator: SsrfValidator,
    /// Timeout for HTTP requests
    timeout: Duration,
    /// Maximum size in bytes for fetched content
    max_size_bytes: usize,
    /// Rate limiter
    rate_limiter: std::sync::Arc<tokio::sync::Mutex<RateLimiter>>,
}

impl FetchTool {
    /// Create a new fetch tool
    ///
    /// # Arguments
    ///
    /// * `timeout` - Timeout for HTTP requests
    /// * `max_size_bytes` - Maximum size for fetched content
    ///
    /// # Returns
    ///
    /// Returns a new FetchTool instance
    pub fn new(timeout: Duration, max_size_bytes: usize) -> Self {
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap_or_else(|error| {
                tracing::warn!(%error, "Falling back to default fetch HTTP client");
                reqwest::Client::new()
            });

        Self {
            client,
            ssrf_validator: SsrfValidator::new(),
            timeout,
            max_size_bytes,
            rate_limiter: std::sync::Arc::new(tokio::sync::Mutex::new(RateLimiter::new(10))),
        }
    }

    /// Create a new fetch tool for testing (allows private IPs)
    ///
    /// # Arguments
    ///
    /// * `timeout` - Timeout for HTTP requests
    /// * `max_size_bytes` - Maximum size for fetched content
    ///
    /// # Returns
    ///
    /// Returns a new FetchTool instance that allows private IPs
    pub fn new_for_testing(timeout: Duration, max_size_bytes: usize) -> Self {
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap_or_else(|error| {
                tracing::warn!(%error, "Falling back to default test fetch HTTP client");
                reqwest::Client::new()
            });

        Self {
            client,
            ssrf_validator: SsrfValidator::allow_private_ips(),
            timeout,
            max_size_bytes,
            rate_limiter: std::sync::Arc::new(tokio::sync::Mutex::new(RateLimiter::new(10))),
        }
    }

    /// Set rate limit (requests per minute)
    ///
    /// # Arguments
    ///
    /// * `requests_per_minute` - Maximum requests per minute
    ///
    /// # Returns
    ///
    /// Returns self for chaining
    pub async fn with_rate_limit(self, requests_per_minute: u32) -> Self {
        *self.rate_limiter.lock().await = RateLimiter::new(requests_per_minute);
        self
    }

    /// Fetch content from a URL
    ///
    /// # Arguments
    ///
    /// * `url` - The URL to fetch
    ///
    /// # Returns
    ///
    /// Returns the fetched content as FetchedContent
    ///
    /// # Errors
    ///
    /// Returns error if fetch fails, URL is invalid, or security checks fail
    pub async fn fetch(&self, url: &str) -> Result<FetchedContent> {
        // Check rate limit
        self.rate_limiter.lock().await.check_and_record()?;

        // Validate URL for SSRF and capture the validated resolution target so
        // the connection can be pinned to an already-validated address.
        let target = self.ssrf_validator.validate_and_resolve(url)?;
        let host_is_ip_literal = IpAddr::from_str(&target.host).is_ok();

        // In production (not test mode) and for DNS-name hosts, pin DNS to the
        // validated socket address so the HTTP client cannot re-resolve to a
        // different (rebinding) address at send time. For IP-literal hosts and
        // in test mode, use the shared client.
        let response = if !self.ssrf_validator.allow_private_ips
            && !host_is_ip_literal
            && let Some(pinned) = target.socket_addrs.first()
        {
            let pinned_client = reqwest::Client::builder()
                .timeout(self.timeout)
                .redirect(reqwest::redirect::Policy::none())
                .resolve(&target.host, *pinned)
                .build()
                .map_err(|e| {
                    XzatomaError::Fetch(format!("Failed to build pinned HTTP client: {}", e))
                })?;

            pinned_client
                .get(url)
                .send()
                .await
                .map_err(|e| XzatomaError::Fetch(format!("Failed to fetch URL: {}", e)))?
        } else {
            self.client
                .get(url)
                .send()
                .await
                .map_err(|e| XzatomaError::Fetch(format!("Failed to fetch URL: {}", e)))?
        };

        // Reject a rebind that slipped through by re-checking the peer address
        // the request actually connected to.
        if let Some(addr) = response.remote_addr() {
            self.ssrf_validator.validate_connected_ip(addr.ip())?;
        }

        self.ssrf_validator.validate(response.url().as_str())?;

        let status = response.status();
        let declared_too_large = response
            .content_length()
            .map(|length| length > self.max_size_bytes as u64)
            .unwrap_or(false);
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("application/octet-stream")
            .to_string();

        // Handle HTTP errors
        if !status.is_success() {
            return Err(XzatomaError::Fetch(format!(
                "HTTP {} for {}",
                status.as_u16(),
                url
            )));
        }

        // Stream content with a hard byte cap so oversized responses cannot be
        // fully buffered before truncation.
        let (content_bytes, stream_truncated) = self.read_limited_body(response).await?;
        let truncated = declared_too_large || stream_truncated;
        let content_bytes = content_bytes.as_slice();

        // Detect if binary and convert if possible
        let content = if self.is_binary(content_bytes) {
            "(Binary content detected - cannot display)".to_string()
        } else {
            String::from_utf8_lossy(content_bytes).to_string()
        };

        // Convert HTML to Markdown if needed
        let converted_content = if content_type.contains("text/html") {
            self.html_to_markdown(&content)
        } else if content_type.contains("application/json") {
            // Pretty-print JSON
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                serde_json::to_string_pretty(&json).unwrap_or_else(|_| content.clone())
            } else {
                content
            }
        } else {
            content
        };

        Ok(FetchedContent::new(
            converted_content,
            url.to_string(),
            content_type,
            status.as_u16(),
        )
        .with_truncated(truncated))
    }

    async fn read_limited_body(&self, response: reqwest::Response) -> Result<(Vec<u8>, bool)> {
        let mut bytes = Vec::new();
        let mut stream = response.bytes_stream();
        let mut truncated = false;

        while let Some(chunk) = stream.next().await {
            let chunk = chunk
                .map_err(|e| XzatomaError::Fetch(format!("Failed to read response body: {}", e)))?;
            let remaining = self.max_size_bytes.saturating_sub(bytes.len());
            if remaining == 0 {
                truncated = true;
                break;
            }

            if chunk.len() > remaining {
                bytes.extend_from_slice(&chunk[..remaining]);
                truncated = true;
                break;
            }

            bytes.extend_from_slice(&chunk);
        }

        Ok((bytes, truncated))
    }

    /// Check if content appears to be binary
    ///
    /// # Arguments
    ///
    /// * `data` - The data to check
    ///
    /// # Returns
    ///
    /// Returns true if content appears to be binary
    fn is_binary(&self, data: &[u8]) -> bool {
        // Check for NUL byte which indicates binary content
        data.contains(&0)
    }

    /// Convert HTML to Markdown
    ///
    /// # Arguments
    ///
    /// * `html` - HTML content to convert
    ///
    /// # Returns
    ///
    /// Returns Markdown representation of the HTML
    fn html_to_markdown(&self, html: &str) -> String {
        // Simple HTML to Markdown conversion
        // Remove HTML tags and convert common elements
        let mut result = html.to_string();

        // Remove script and style tags
        result = SCRIPT_TAG_RE.replace_all(&result, "").to_string();
        result = STYLE_TAG_RE.replace_all(&result, "").to_string();

        // Convert headers
        for i in (1..=6).rev() {
            let pattern = format!(r"(?i)<h{0}[^>]*>(.*?)</h{0}>", i);
            if let Ok(re) = regex::Regex::new(&pattern) {
                result = re
                    .replace_all(&result, format!("{} $1", "#".repeat(i)))
                    .to_string();
            }
        }

        // Convert paragraph tags
        result = PARAGRAPH_RE.replace_all(&result, "$1\n\n").to_string();

        // Convert links
        result = ANCHOR_RE.replace_all(&result, "[$2]($1)").to_string();

        // Convert bold
        result = BOLD_RE.replace_all(&result, "**$1**").to_string();

        // Convert italic
        result = ITALIC_RE.replace_all(&result, "*$1*").to_string();

        // Convert line breaks
        result = LINE_BREAK_RE.replace_all(&result, "\n").to_string();

        // Remove remaining HTML tags
        result = HTML_TAG_RE.replace_all(&result, "").to_string();

        // Clean up whitespace
        result = WHITESPACE_RE.replace_all(&result, "\n\n").to_string();
        result = result.trim().to_string();

        result
    }
}

impl std::fmt::Debug for FetchTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FetchTool")
            .field("timeout", &self.timeout)
            .field("max_size_bytes", &self.max_size_bytes)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ssrf_validator_https_allowed() {
        let validator = SsrfValidator::new();
        assert!(validator.validate("https://93.184.216.34").is_ok());
    }

    #[test]
    fn test_ssrf_validator_http_allowed() {
        let validator = SsrfValidator::new();
        assert!(validator.validate("http://1.1.1.1").is_ok());
    }

    #[test]
    fn test_ssrf_validator_file_denied() {
        let validator = SsrfValidator::new();
        let result = validator.validate("file:///etc/passwd");
        assert!(result.is_err());
    }

    #[test]
    fn test_ssrf_validator_ftp_denied() {
        let validator = SsrfValidator::new();
        let result = validator.validate("ftp://example.com");
        assert!(result.is_err());
    }

    #[test]
    fn test_ssrf_validator_localhost_denied() {
        let validator = SsrfValidator::new();
        let result = validator.validate("http://localhost");
        assert!(result.is_err());
    }

    #[test]
    fn test_ssrf_validator_localhost_ip_denied() {
        let validator = SsrfValidator::new();
        let result = validator.validate("http://127.0.0.1");
        assert!(result.is_err());
    }

    #[test]
    fn test_ssrf_validator_private_ip_10_denied() {
        let validator = SsrfValidator::new();
        let result = validator.validate("http://10.0.0.1");
        assert!(result.is_err());
    }

    #[test]
    fn test_ssrf_validator_private_ip_192_denied() {
        let validator = SsrfValidator::new();
        let result = validator.validate("http://192.168.1.1");
        assert!(result.is_err());
    }

    #[test]
    fn test_ssrf_validator_private_ip_172_denied() {
        let validator = SsrfValidator::new();
        let result = validator.validate("http://172.16.0.1");
        assert!(result.is_err());
    }

    #[test]
    fn test_ssrf_validator_link_local_denied() {
        let validator = SsrfValidator::new();
        let result = validator.validate("http://169.254.1.1");
        assert!(result.is_err());
    }

    #[test]
    fn test_ssrf_validator_broadcast_denied() {
        let validator = SsrfValidator::new();
        let result = validator.validate("http://255.255.255.255");
        assert!(result.is_err());
    }

    #[test]
    fn test_ssrf_validator_allows_private_ips_when_configured() {
        let validator = SsrfValidator::allow_private_ips();
        assert!(validator.validate("http://localhost").is_ok());
        assert!(validator.validate("http://127.0.0.1").is_ok());
        assert!(validator.validate("http://192.168.1.1").is_ok());
    }

    #[test]
    fn test_ssrf_validator_invalid_url() {
        let validator = SsrfValidator::new();
        let result = validator.validate("not a valid url");
        assert!(result.is_err());
    }

    #[test]
    fn test_fetched_content_new() {
        let content = FetchedContent::new(
            "Test content".to_string(),
            "https://example.com".to_string(),
            "text/html".to_string(),
            200,
        );
        assert_eq!(content.content, "Test content");
        assert_eq!(content.url, "https://example.com");
        assert_eq!(content.content_type, "text/html");
        assert_eq!(content.status_code, 200);
        assert!(!content.truncated);
    }

    #[test]
    fn test_fetched_content_with_truncated() {
        let content = FetchedContent::new(
            "Test content".to_string(),
            "https://example.com".to_string(),
            "text/plain".to_string(),
            200,
        )
        .with_truncated(true);
        assert!(content.truncated);
    }

    #[test]
    fn test_fetched_content_format_with_header() {
        let content = FetchedContent::new(
            "Test content".to_string(),
            "https://example.com".to_string(),
            "text/plain".to_string(),
            200,
        );
        let formatted = content.format_with_header(Some("2024-01-15 10:30:00".to_string()));
        assert!(formatted.contains("https://example.com"));
        assert!(formatted.contains("text/plain"));
        assert!(formatted.contains("Test content"));
        assert!(formatted.contains("2024-01-15 10:30:00"));
    }

    #[test]
    fn test_fetched_content_format_with_header_truncated() {
        let content = FetchedContent::new(
            "Test content".to_string(),
            "https://example.com".to_string(),
            "text/plain".to_string(),
            200,
        )
        .with_truncated(true);
        let formatted = content.format_with_header(None);
        assert!(formatted.contains("truncated"));
    }

    #[test]
    fn test_rate_limiter_new() {
        let limiter = RateLimiter::new(10);
        assert_eq!(limiter.max_requests_per_minute, 10);
        assert!(limiter.requests.is_empty());
    }

    #[test]
    fn test_rate_limiter_allows_requests_within_limit() {
        let mut limiter = RateLimiter::new(3);
        assert!(limiter.check_and_record().is_ok());
        assert!(limiter.check_and_record().is_ok());
        assert!(limiter.check_and_record().is_ok());
    }

    #[test]
    fn test_rate_limiter_denies_requests_exceeding_limit() {
        let mut limiter = RateLimiter::new(2);
        assert!(limiter.check_and_record().is_ok());
        assert!(limiter.check_and_record().is_ok());
        let result = limiter.check_and_record();
        assert!(result.is_err());
    }

    #[test]
    fn test_fetch_tool_new() {
        let tool = FetchTool::new(Duration::from_secs(30), 5 * 1024 * 1024);
        assert_eq!(tool.timeout, Duration::from_secs(30));
        assert_eq!(tool.max_size_bytes, 5 * 1024 * 1024);
    }

    #[tokio::test]
    async fn test_fetch_tool_read_limited_body_truncates_oversized_response() {
        let tool = FetchTool::new_for_testing(Duration::from_secs(30), 4);
        let response = reqwest::Response::from(
            http::Response::builder()
                .status(200)
                .body("abcdef".to_string())
                .unwrap(),
        );

        let (body, truncated) = tool.read_limited_body(response).await.unwrap();

        assert_eq!(body, b"abcd");
        assert!(truncated);
    }

    #[test]
    fn test_fetch_tool_is_binary_with_nul_byte() {
        let tool = FetchTool::new(Duration::from_secs(30), 1024 * 1024);
        assert!(tool.is_binary(b"Hello\x00World"));
    }

    #[test]
    fn test_fetch_tool_is_binary_without_nul_byte() {
        let tool = FetchTool::new(Duration::from_secs(30), 1024 * 1024);
        assert!(!tool.is_binary(b"Hello World"));
    }

    #[test]
    fn test_fetch_tool_html_to_markdown_headers() {
        let tool = FetchTool::new(Duration::from_secs(30), 1024 * 1024);
        let html = "<h1>Title</h1><h2>Subtitle</h2>";
        let markdown = tool.html_to_markdown(html);
        assert!(markdown.contains("# Title"));
        assert!(markdown.contains("## Subtitle"));
    }

    #[test]
    fn test_fetch_tool_html_to_markdown_links() {
        let tool = FetchTool::new(Duration::from_secs(30), 1024 * 1024);
        let html = r#"<a href="https://example.com">Example</a>"#;
        let markdown = tool.html_to_markdown(html);
        assert!(markdown.contains("[Example](https://example.com)"));
    }

    #[test]
    fn test_fetch_tool_html_to_markdown_bold() {
        let tool = FetchTool::new(Duration::from_secs(30), 1024 * 1024);
        let html = "<b>Bold text</b>";
        let markdown = tool.html_to_markdown(html);
        assert!(markdown.contains("**Bold text**"));
    }

    #[test]
    fn test_fetch_tool_html_to_markdown_italic() {
        let tool = FetchTool::new(Duration::from_secs(30), 1024 * 1024);
        let html = "<i>Italic text</i>";
        let markdown = tool.html_to_markdown(html);
        assert!(markdown.contains("*Italic text*"));
    }

    #[test]
    fn test_fetch_tool_html_to_markdown_removes_scripts() {
        let tool = FetchTool::new(Duration::from_secs(30), 1024 * 1024);
        let html = "<p>Content</p><script>alert('xss')</script><p>More</p>";
        let markdown = tool.html_to_markdown(html);
        assert!(!markdown.contains("alert"));
        assert!(!markdown.contains("script"));
        assert!(markdown.contains("Content"));
        assert!(markdown.contains("More"));
    }

    #[test]
    fn test_fetch_tool_html_to_markdown_removes_styles() {
        let tool = FetchTool::new(Duration::from_secs(30), 1024 * 1024);
        let html = "<p>Content</p><style>body { color: red; }</style>";
        let markdown = tool.html_to_markdown(html);
        assert!(!markdown.contains("color"));
        assert!(!markdown.contains("style"));
        assert!(markdown.contains("Content"));
    }

    #[test]
    fn test_fetch_tool_debug() {
        let tool = FetchTool::new(Duration::from_secs(30), 1024 * 1024);
        let debug_str = format!("{:?}", tool);
        assert!(debug_str.contains("FetchTool"));
        assert!(debug_str.contains("timeout"));
    }

    #[test]
    fn test_ipv6_loopback() {
        let validator = SsrfValidator::new();
        let result = validator.validate("http://[::1]");
        assert!(result.is_err());
    }

    #[test]
    fn test_ipv6_private() {
        let validator = SsrfValidator::new();
        let result = validator.validate("http://[fd00::1]");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_and_resolve_public_ip_literal_returns_addrs() {
        let validator = SsrfValidator::new();
        let target = validator
            .validate_and_resolve("https://93.184.216.34/")
            .expect("public IP literal should validate");
        assert_eq!(target.host, "93.184.216.34");
        assert_eq!(target.port, 443);
        assert_eq!(target.socket_addrs.len(), 1);
        assert_eq!(
            target.socket_addrs[0].ip(),
            IpAddr::V4(std::net::Ipv4Addr::new(93, 184, 216, 34))
        );
    }

    #[test]
    fn test_validate_and_resolve_public_ip_literal_uses_http_default_port() {
        let validator = SsrfValidator::new();
        let target = validator
            .validate_and_resolve("http://1.1.1.1/")
            .expect("public IP literal should validate");
        assert_eq!(target.port, 80);
    }

    #[test]
    fn test_validate_and_resolve_private_ip_literal_errors() {
        let validator = SsrfValidator::new();
        let result = validator.validate_and_resolve("https://192.168.1.1/");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_connected_ip_blocks_loopback() {
        let validator = SsrfValidator::new();
        let loopback = IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1));
        assert!(validator.validate_connected_ip(loopback).is_err());
    }

    #[test]
    fn test_validate_connected_ip_blocks_private() {
        let validator = SsrfValidator::new();
        let private = IpAddr::V4(std::net::Ipv4Addr::new(10, 0, 0, 5));
        assert!(validator.validate_connected_ip(private).is_err());
    }

    #[test]
    fn test_validate_connected_ip_allows_private_in_test_mode() {
        let validator = SsrfValidator::allow_private_ips();
        let loopback = IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1));
        assert!(validator.validate_connected_ip(loopback).is_ok());
    }
}

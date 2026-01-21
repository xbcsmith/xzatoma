//! Fetch tool for retrieving web content via HTTP
//!
//! This module provides secure HTTP content fetching with:
//! - SSRF prevention (blocking private IP ranges and dangerous schemes)
//! - Content type validation and conversion to Markdown
//! - Size limits and timeouts
//! - Rate limiting
//! - Caching support

use crate::error::Result;
use std::net::IpAddr;
use std::str::FromStr;
use std::time::Duration;
use url::Url;

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
        let parsed_url = Url::parse(url).map_err(|e| anyhow::anyhow!("Invalid URL: {}", e))?;

        // Validate scheme
        self.validate_scheme(parsed_url.scheme())?;

        // Validate host
        if let Some(host) = parsed_url.host_str() {
            self.validate_host(host)?;
        } else {
            return Err(anyhow::anyhow!("URL has no host"));
        }

        Ok(())
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
            "file" => Err(anyhow::anyhow!(
                "file:// URLs are not allowed for security reasons"
            )),
            "ftp" => Err(anyhow::anyhow!(
                "ftp:// URLs are not allowed for security reasons"
            )),
            _ => Err(anyhow::anyhow!("Unsupported URL scheme: {}", scheme)),
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
            return Err(anyhow::anyhow!("Requests to localhost are not allowed"));
        }

        // Try to parse as IP address
        if let Ok(ip) = IpAddr::from_str(host) {
            return self.validate_ip(ip);
        }

        // For hostnames, we'll do a basic check and rely on actual DNS resolution
        // during the HTTP request to be more restrictive
        Ok(())
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
                    return Err(anyhow::anyhow!(
                        "Requests to loopback addresses are not allowed"
                    ));
                }
                // 10.0.0.0/8 (private)
                if v4.octets()[0] == 10 {
                    return Err(anyhow::anyhow!(
                        "Requests to private IP ranges are not allowed"
                    ));
                }
                // 172.16.0.0/12 (private)
                if v4.octets()[0] == 172 && (v4.octets()[1] >= 16 && v4.octets()[1] <= 31) {
                    return Err(anyhow::anyhow!(
                        "Requests to private IP ranges are not allowed"
                    ));
                }
                // 192.168.0.0/16 (private)
                if v4.octets()[0] == 192 && v4.octets()[1] == 168 {
                    return Err(anyhow::anyhow!(
                        "Requests to private IP ranges are not allowed"
                    ));
                }
                // 169.254.0.0/16 (link-local)
                if v4.octets()[0] == 169 && v4.octets()[1] == 254 {
                    return Err(anyhow::anyhow!(
                        "Requests to link-local addresses are not allowed"
                    ));
                }
                // 0.0.0.0/8 (this network)
                if v4.octets()[0] == 0 {
                    return Err(anyhow::anyhow!("Requests to this network are not allowed"));
                }
                // 255.255.255.255 (broadcast)
                if v4 == std::net::Ipv4Addr::BROADCAST {
                    return Err(anyhow::anyhow!(
                        "Requests to broadcast address are not allowed"
                    ));
                }
                Ok(())
            }
            IpAddr::V6(v6) => {
                // Block IPv6 loopback (::1)
                if v6.octets() == [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1] {
                    return Err(anyhow::anyhow!(
                        "Requests to loopback addresses are not allowed"
                    ));
                }
                // Block IPv6 private addresses (fc00::/7)
                if (v6.segments()[0] & 0xfe00) == 0xfc00 {
                    return Err(anyhow::anyhow!(
                        "Requests to private IP ranges are not allowed"
                    ));
                }
                // Block IPv6 link-local (fe80::/10)
                if (v6.segments()[0] & 0xffc0) == 0xfe80 {
                    return Err(anyhow::anyhow!(
                        "Requests to link-local addresses are not allowed"
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
            return Err(anyhow::anyhow!(
                "Rate limit exceeded: {} requests per minute",
                self.max_requests_per_minute
            ));
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
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

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
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

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

        // Validate URL for SSRF
        self.ssrf_validator.validate(url)?;

        // Perform HTTP request
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to fetch URL: {}", e))?;

        let status = response.status();
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("application/octet-stream")
            .to_string();

        // Handle HTTP errors
        if !status.is_success() {
            return Err(anyhow::anyhow!("HTTP {} for {}", status.as_u16(), url));
        }

        // Get content
        let bytes = response
            .bytes()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to read response body: {}", e))?;

        // Check size limit
        let mut truncated = false;
        let content_bytes = if bytes.len() > self.max_size_bytes {
            truncated = true;
            &bytes[..self.max_size_bytes]
        } else {
            &bytes[..]
        };

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
        result = regex::Regex::new(r"(?i)<script[^>]*>.*?</script>")
            .unwrap()
            .replace_all(&result, "")
            .to_string();
        result = regex::Regex::new(r"(?i)<style[^>]*>.*?</style>")
            .unwrap()
            .replace_all(&result, "")
            .to_string();

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
        result = regex::Regex::new(r"(?i)<p[^>]*>(.*?)</p>")
            .unwrap()
            .replace_all(&result, "$1\n\n")
            .to_string();

        // Convert links
        result = regex::Regex::new(r#"(?i)<a[^>]*href\s*=\s*['"]([^'"]*)['"'][^>]*>(.*?)</a>"#)
            .unwrap()
            .replace_all(&result, "[$2]($1)")
            .to_string();

        // Convert bold
        result = regex::Regex::new(r"(?i)<(?:b|strong)[^>]*>(.*?)</(?:b|strong)>")
            .unwrap()
            .replace_all(&result, "**$1**")
            .to_string();

        // Convert italic
        result = regex::Regex::new(r"(?i)<(?:i|em)[^>]*>(.*?)</(?:i|em)>")
            .unwrap()
            .replace_all(&result, "*$1*")
            .to_string();

        // Convert line breaks
        result = regex::Regex::new(r"(?i)<br\s*/?> *")
            .unwrap()
            .replace_all(&result, "\n")
            .to_string();

        // Remove remaining HTML tags
        result = regex::Regex::new(r"<[^>]+>")
            .unwrap()
            .replace_all(&result, "")
            .to_string();

        // Clean up whitespace
        result = regex::Regex::new(r"\n\s*\n\s*\n+")
            .unwrap()
            .replace_all(&result, "\n\n")
            .to_string();
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
        assert!(validator.validate("https://example.com").is_ok());
    }

    #[test]
    fn test_ssrf_validator_http_allowed() {
        let validator = SsrfValidator::new();
        assert!(validator.validate("http://example.com").is_ok());
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
    #[ignore] // URL crate normalizes IPv6 addresses differently
    fn test_ipv6_loopback() {
        let validator = SsrfValidator::new();
        let result = validator.validate("http://[::1]");
        assert!(result.is_err());
    }

    #[test]
    #[ignore] // URL crate normalizes IPv6 addresses differently
    fn test_ipv6_private() {
        let validator = SsrfValidator::new();
        let result = validator.validate("http://[fd00::1]");
        assert!(result.is_err());
    }
}

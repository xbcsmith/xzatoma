//! Shared domain types for the XZatoma provider layer.
//!
//! This module contains all data types shared across providers: message
//! structures, model metadata, capability flags, request/response types,
//! and wire-format structs used when communicating with provider APIs.

use base64::Engine;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

/// Error returned when multimodal prompt input is invalid.
///
/// # Examples
///
/// ```
/// use xzatoma::providers::{MultimodalPromptInput, PromptInputError};
///
/// let error = MultimodalPromptInput::new(vec![]).validate().unwrap_err();
/// assert!(matches!(error, PromptInputError::Empty));
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PromptInputError {
    /// The prompt contained no content parts.
    #[error("prompt input must contain at least one content part")]
    Empty,
    /// The prompt contained only empty text parts.
    #[error("prompt input must contain non-empty text or image content")]
    NoUsableContent,
    /// An image content part was malformed.
    #[error("invalid image prompt input: {0}")]
    Image(#[from] ImagePromptError),
    /// A caller attempted to convert image input through a text-only path.
    #[error("multimodal input contains images and cannot be converted to a text-only message")]
    ImageInputInTextOnlyMessage,
}

/// Error returned when an image prompt part is invalid.
///
/// # Examples
///
/// ```
/// use xzatoma::providers::{ImagePromptError, ImagePromptPart};
///
/// let error = ImagePromptPart::inline_base64("", "AAAA").validate().unwrap_err();
/// assert!(matches!(error, ImagePromptError::MissingMimeType));
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ImagePromptError {
    /// The image part did not include a MIME type.
    #[error("image content is missing a MIME type")]
    MissingMimeType,
    /// The MIME type was not an image MIME type.
    #[error("image MIME type '{mime_type}' must start with 'image/'")]
    NonImageMimeType {
        /// Invalid MIME type.
        mime_type: String,
    },
    /// The inline base64 source was empty.
    #[error("image content is missing inline base64 data")]
    MissingInlineBase64,
    /// The inline bytes source was empty.
    #[error("image content is missing inline bytes")]
    MissingInlineBytes,
    /// The file reference source was empty.
    #[error("image file reference is empty")]
    EmptyFileReference,
    /// The remote URL source was empty.
    #[error("image remote URL is empty")]
    EmptyRemoteUrl,
}

/// Ordered multimodal prompt input for providers that support text and vision.
///
/// This type is intentionally small: it stores only the prompt content parts
/// needed by ACP stdio conversion and provider request builders. Text-only
/// prompts can continue to use [`Message::user`], while vision-capable paths can
/// validate and route this structure explicitly.
///
/// # Examples
///
/// ```
/// use xzatoma::providers::{MultimodalPromptInput, PromptInputPart};
///
/// let input = MultimodalPromptInput::new(vec![PromptInputPart::text("Describe this image")]);
/// assert!(input.validate().is_ok());
/// assert!(!input.has_images());
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MultimodalPromptInput {
    /// Ordered text and image parts in the prompt.
    pub parts: Vec<PromptInputPart>,
}

impl MultimodalPromptInput {
    /// Creates a new multimodal prompt input.
    ///
    /// # Arguments
    ///
    /// * `parts` - Ordered prompt content parts.
    ///
    /// # Returns
    ///
    /// Returns a prompt input containing the provided parts.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::providers::{MultimodalPromptInput, PromptInputPart};
    ///
    /// let input = MultimodalPromptInput::new(vec![PromptInputPart::text("hello")]);
    /// assert_eq!(input.parts.len(), 1);
    /// ```
    pub fn new(parts: Vec<PromptInputPart>) -> Self {
        Self { parts }
    }

    /// Creates a text-only multimodal prompt input.
    ///
    /// # Arguments
    ///
    /// * `text` - UTF-8 prompt text.
    ///
    /// # Returns
    ///
    /// Returns a prompt input with one text part.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::providers::MultimodalPromptInput;
    ///
    /// let input = MultimodalPromptInput::text("hello");
    /// assert_eq!(input.as_legacy_text(), "hello");
    /// ```
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            parts: vec![PromptInputPart::text(text)],
        }
    }

    /// Returns `true` when any part contains image input.
    ///
    /// # Returns
    ///
    /// Returns whether the prompt contains at least one image part.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::providers::{ImagePromptPart, MultimodalPromptInput, PromptInputPart};
    ///
    /// let input = MultimodalPromptInput::new(vec![PromptInputPart::image(
    ///     ImagePromptPart::inline_base64("image/png", "AAAA"),
    /// )]);
    /// assert!(input.has_images());
    /// ```
    pub fn has_images(&self) -> bool {
        self.parts
            .iter()
            .any(|part| matches!(part, PromptInputPart::Image(_)))
    }

    /// Converts text parts into a legacy text prompt.
    ///
    /// Image parts are intentionally not converted into descriptions here.
    /// Callers must use [`Self::has_images`] and provider vision validation to
    /// reject unsupported image input before using this fallback.
    ///
    /// # Returns
    ///
    /// Returns all text parts joined with blank lines.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::providers::{MultimodalPromptInput, PromptInputPart};
    ///
    /// let input = MultimodalPromptInput::new(vec![
    ///     PromptInputPart::text("first"),
    ///     PromptInputPart::text("second"),
    /// ]);
    /// assert_eq!(input.as_legacy_text(), "first\n\nsecond");
    /// ```
    pub fn as_legacy_text(&self) -> String {
        self.parts
            .iter()
            .filter_map(|part| match part {
                PromptInputPart::Text(text) => Some(text.text.as_str()),
                PromptInputPart::Image(_) => None,
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    /// Validates that the prompt is non-empty and all image parts are usable.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` when the prompt contains usable text or image input.
    ///
    /// # Errors
    ///
    /// Returns a descriptive string if the prompt is empty or an image part is
    /// malformed.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::providers::MultimodalPromptInput;
    ///
    /// assert!(MultimodalPromptInput::text("hello").validate().is_ok());
    /// assert!(MultimodalPromptInput::new(vec![]).validate().is_err());
    /// ```
    pub fn validate(&self) -> std::result::Result<(), PromptInputError> {
        if self.parts.is_empty() {
            return Err(PromptInputError::Empty);
        }

        let mut has_usable_content = false;
        for part in &self.parts {
            match part {
                PromptInputPart::Text(text) => {
                    if !text.text.trim().is_empty() {
                        has_usable_content = true;
                    }
                }
                PromptInputPart::Image(image) => {
                    image.validate()?;
                    has_usable_content = true;
                }
            }
        }

        if has_usable_content {
            Ok(())
        } else {
            Err(PromptInputError::NoUsableContent)
        }
    }
}

/// One ordered part of a multimodal prompt input.
///
/// # Examples
///
/// ```
/// use xzatoma::providers::PromptInputPart;
///
/// let part = PromptInputPart::text("hello");
/// assert!(matches!(part, PromptInputPart::Text(_)));
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PromptInputPart {
    /// UTF-8 text input.
    Text(TextPromptPart),
    /// Image input for vision-capable providers.
    Image(ImagePromptPart),
}

impl PromptInputPart {
    /// Creates a text prompt part.
    ///
    /// # Arguments
    ///
    /// * `text` - UTF-8 prompt text.
    ///
    /// # Returns
    ///
    /// Returns a text prompt part.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::providers::PromptInputPart;
    ///
    /// let part = PromptInputPart::text("hello");
    /// assert!(matches!(part, PromptInputPart::Text(_)));
    /// ```
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text(TextPromptPart { text: text.into() })
    }

    /// Creates an image prompt part.
    ///
    /// # Arguments
    ///
    /// * `image` - Image prompt content.
    ///
    /// # Returns
    ///
    /// Returns an image prompt part.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::providers::{ImagePromptPart, PromptInputPart};
    ///
    /// let part = PromptInputPart::image(ImagePromptPart::inline_base64("image/png", "AAAA"));
    /// assert!(matches!(part, PromptInputPart::Image(_)));
    /// ```
    pub fn image(image: ImagePromptPart) -> Self {
        Self::Image(image)
    }
}

/// UTF-8 text content in a multimodal prompt.
///
/// # Examples
///
/// ```
/// use xzatoma::providers::TextPromptPart;
///
/// let text = TextPromptPart {
///     text: "hello".to_string(),
/// };
/// assert_eq!(text.text, "hello");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextPromptPart {
    /// Prompt text.
    pub text: String,
}

/// Image content in a multimodal prompt.
///
/// Image data can be inline base64, inline bytes, a local file reference, or a
/// remote URL reference. Conversion code must validate the selected source
/// against ACP stdio policy before provider execution.
///
/// # Examples
///
/// ```
/// use xzatoma::providers::ImagePromptPart;
///
/// let image = ImagePromptPart::inline_base64("image/png", "AAAA");
/// assert_eq!(image.mime_type, "image/png");
/// assert!(image.validate().is_ok());
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImagePromptPart {
    /// Image MIME type such as `image/png`.
    pub mime_type: String,
    /// Optional filename or path for display and provider metadata.
    pub name: Option<String>,
    /// Image source data or reference.
    pub source: ImagePromptSource,
}

impl ImagePromptPart {
    /// Creates an inline base64 image prompt part.
    ///
    /// # Arguments
    ///
    /// * `mime_type` - Image MIME type.
    /// * `data` - Base64-encoded image data.
    ///
    /// # Returns
    ///
    /// Returns an image prompt part backed by base64 data.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::providers::ImagePromptPart;
    ///
    /// let image = ImagePromptPart::inline_base64("image/png", "AAAA");
    /// assert!(image.validate().is_ok());
    /// ```
    pub fn inline_base64(mime_type: impl Into<String>, data: impl Into<String>) -> Self {
        Self {
            mime_type: mime_type.into(),
            name: None,
            source: ImagePromptSource::InlineBase64(data.into()),
        }
    }

    /// Creates an inline byte image prompt part.
    ///
    /// # Arguments
    ///
    /// * `mime_type` - Image MIME type.
    /// * `bytes` - Decoded image bytes.
    ///
    /// # Returns
    ///
    /// Returns an image prompt part backed by decoded bytes.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::providers::ImagePromptPart;
    ///
    /// let image = ImagePromptPart::inline_bytes("image/png", vec![0, 1, 2]);
    /// assert!(image.validate().is_ok());
    /// ```
    pub fn inline_bytes(mime_type: impl Into<String>, bytes: Vec<u8>) -> Self {
        Self {
            mime_type: mime_type.into(),
            name: None,
            source: ImagePromptSource::InlineBytes(bytes),
        }
    }

    /// Creates a local file image prompt part.
    ///
    /// # Arguments
    ///
    /// * `mime_type` - Image MIME type.
    /// * `path` - Local image path.
    ///
    /// # Returns
    ///
    /// Returns an image prompt part backed by a local file reference.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::PathBuf;
    /// use xzatoma::providers::ImagePromptPart;
    ///
    /// let image = ImagePromptPart::file_reference("image/png", PathBuf::from("/tmp/a.png"));
    /// assert!(image.validate().is_ok());
    /// ```
    pub fn file_reference(mime_type: impl Into<String>, path: std::path::PathBuf) -> Self {
        Self {
            mime_type: mime_type.into(),
            name: path
                .file_name()
                .map(|name| name.to_string_lossy().to_string()),
            source: ImagePromptSource::FilePath(path),
        }
    }

    /// Creates a remote URL image prompt part.
    ///
    /// # Arguments
    ///
    /// * `mime_type` - Image MIME type.
    /// * `url` - Remote image URL.
    ///
    /// # Returns
    ///
    /// Returns an image prompt part backed by a remote URL reference.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::providers::ImagePromptPart;
    ///
    /// let image = ImagePromptPart::remote_url("image/png", "https://example.com/a.png");
    /// assert!(image.validate().is_ok());
    /// ```
    pub fn remote_url(mime_type: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            mime_type: mime_type.into(),
            name: None,
            source: ImagePromptSource::RemoteUrl(url.into()),
        }
    }

    /// Validates that the image has a MIME type and usable source.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` when the image source is usable.
    ///
    /// # Errors
    ///
    /// Returns a descriptive string when the image is malformed.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::providers::ImagePromptPart;
    ///
    /// assert!(ImagePromptPart::inline_base64("image/png", "AAAA").validate().is_ok());
    /// assert!(ImagePromptPart::inline_base64("", "AAAA").validate().is_err());
    /// ```
    pub fn validate(&self) -> std::result::Result<(), ImagePromptError> {
        if self.mime_type.trim().is_empty() {
            return Err(ImagePromptError::MissingMimeType);
        }

        if !self.mime_type.starts_with("image/") {
            return Err(ImagePromptError::NonImageMimeType {
                mime_type: self.mime_type.clone(),
            });
        }

        match &self.source {
            ImagePromptSource::InlineBase64(data) => {
                if data.trim().is_empty() {
                    Err(ImagePromptError::MissingInlineBase64)
                } else {
                    Ok(())
                }
            }
            ImagePromptSource::InlineBytes(bytes) => {
                if bytes.is_empty() {
                    Err(ImagePromptError::MissingInlineBytes)
                } else {
                    Ok(())
                }
            }
            ImagePromptSource::FilePath(path) => {
                if path.as_os_str().is_empty() {
                    Err(ImagePromptError::EmptyFileReference)
                } else {
                    Ok(())
                }
            }
            ImagePromptSource::RemoteUrl(url) => {
                if url.trim().is_empty() {
                    Err(ImagePromptError::EmptyRemoteUrl)
                } else {
                    Ok(())
                }
            }
        }
    }
}

/// Source for image content in a multimodal prompt.
///
/// # Examples
///
/// ```
/// use xzatoma::providers::ImagePromptSource;
///
/// let source = ImagePromptSource::InlineBase64("AAAA".to_string());
/// assert!(matches!(source, ImagePromptSource::InlineBase64(_)));
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "source", content = "value", rename_all = "snake_case")]
pub enum ImagePromptSource {
    /// Base64-encoded image data.
    InlineBase64(String),
    /// Decoded image bytes.
    InlineBytes(Vec<u8>),
    /// Local image file path.
    FilePath(std::path::PathBuf),
    /// Remote image URL.
    RemoteUrl(String),
}

/// Provider-facing multimodal message content part.
///
/// Providers that support vision can convert [`MultimodalPromptInput`] into
/// these ordered parts and then serialize them into their native request shape.
/// Text-only providers should reject inputs containing image parts before
/// execution begins.
///
/// # Examples
///
/// ```
/// use xzatoma::providers::ProviderMessageContentPart;
///
/// let part = ProviderMessageContentPart::text("hello");
/// assert!(matches!(part, ProviderMessageContentPart::Text { .. }));
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProviderMessageContentPart {
    /// UTF-8 text content.
    Text {
        /// Text sent to the provider.
        text: String,
    },
    /// Image content sent to a vision-capable provider.
    Image {
        /// Image MIME type such as `image/png`.
        mime_type: String,
        /// Optional filename or source label.
        name: Option<String>,
        /// Image source data or reference.
        source: ImagePromptSource,
    },
}

impl ProviderMessageContentPart {
    /// Creates a text provider message content part.
    ///
    /// # Arguments
    ///
    /// * `text` - UTF-8 text content.
    ///
    /// # Returns
    ///
    /// Returns a text content part.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::providers::ProviderMessageContentPart;
    ///
    /// let part = ProviderMessageContentPart::text("hello");
    /// assert!(matches!(part, ProviderMessageContentPart::Text { .. }));
    /// ```
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text { text: text.into() }
    }

    /// Creates an image provider message content part.
    ///
    /// # Arguments
    ///
    /// * `image` - Image prompt part to expose to the provider layer.
    ///
    /// # Returns
    ///
    /// Returns an image content part with the same MIME type, name, and source.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::providers::{ImagePromptPart, ProviderMessageContentPart};
    ///
    /// let part = ProviderMessageContentPart::image(
    ///     ImagePromptPart::inline_base64("image/png", "AAAA"),
    /// );
    /// assert!(matches!(part, ProviderMessageContentPart::Image { .. }));
    /// ```
    pub fn image(image: ImagePromptPart) -> Self {
        Self::Image {
            mime_type: image.mime_type,
            name: image.name,
            source: image.source,
        }
    }

    /// Returns `true` when this content part contains text.
    ///
    /// # Returns
    ///
    /// Returns whether this content part is text.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::providers::ProviderMessageContentPart;
    ///
    /// let part = ProviderMessageContentPart::text("hello");
    /// assert!(part.is_text());
    /// assert!(!part.is_image());
    /// ```
    pub fn is_text(&self) -> bool {
        matches!(self, Self::Text { .. })
    }

    /// Returns `true` when this content part contains image input.
    ///
    /// # Returns
    ///
    /// Returns whether this content part is an image.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::providers::{ImagePromptPart, ProviderMessageContentPart};
    ///
    /// let part = ProviderMessageContentPart::image(
    ///     ImagePromptPart::inline_base64("image/png", "AAAA"),
    /// );
    /// assert!(part.is_image());
    /// assert!(!part.is_text());
    /// ```
    pub fn is_image(&self) -> bool {
        matches!(self, Self::Image { .. })
    }

    /// Returns a display-safe label for this content part.
    ///
    /// The label identifies the broad content category without exposing inline
    /// image bytes or base64 data.
    ///
    /// # Returns
    ///
    /// Returns `"text"` for text parts and the image MIME type for image parts.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::providers::{ImagePromptPart, ProviderMessageContentPart};
    ///
    /// let text = ProviderMessageContentPart::text("hello");
    /// let image = ProviderMessageContentPart::image(
    ///     ImagePromptPart::inline_base64("image/png", "AAAA"),
    /// );
    ///
    /// assert_eq!(text.kind_label(), "text");
    /// assert_eq!(image.kind_label(), "image/png");
    /// ```
    pub fn kind_label(&self) -> &str {
        match self {
            Self::Text { .. } => "text",
            Self::Image { mime_type, .. } => mime_type.as_str(),
        }
    }
}

/// Provider-facing ordered multimodal message content.
pub type ProviderMessageContentParts = Vec<ProviderMessageContentPart>;

/// Backward-compatible alias for provider-facing prompt input.
pub type ProviderPromptInput = MultimodalPromptInput;

/// Backward-compatible alias for provider-facing prompt input parts.
pub type ProviderPromptInputPart = PromptInputPart;

/// Backward-compatible alias for provider-facing text prompt parts.
pub type ProviderTextPromptPart = TextPromptPart;

/// Backward-compatible alias for provider-facing image prompt parts.
pub type ProviderImagePromptPart = ImagePromptPart;

/// Backward-compatible alias for provider-facing image prompt sources.
pub type ProviderImagePromptSource = ImagePromptSource;

/// Message structure for conversation
///
/// Represents a message in the conversation with the AI provider.
/// Messages can be from the user, assistant, system, or tool results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Role of the message sender (user, assistant, system, tool)
    pub role: String,
    /// Text content of the message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// Optional ordered multimodal content parts for vision-capable providers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_parts: Option<ProviderMessageContentParts>,
    /// Optional tool calls in the message
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    /// Optional tool call ID (for tool result messages)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl Message {
    /// Creates a new user message
    ///
    /// # Arguments
    ///
    /// * `content` - The message content
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::providers::Message;
    ///
    /// let msg = Message::user("Hello, assistant!");
    /// assert_eq!(msg.role, "user");
    /// ```
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".to_string(),
            content: Some(content.into()),
            content_parts: None,
            tool_calls: None,
            tool_call_id: None,
        }
    }

    /// Creates a new assistant message
    ///
    /// # Arguments
    ///
    /// * `content` - The message content
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::providers::Message;
    ///
    /// let msg = Message::assistant("Hello, user!");
    /// assert_eq!(msg.role, "assistant");
    /// ```
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".to_string(),
            content: Some(content.into()),
            content_parts: None,
            tool_calls: None,
            tool_call_id: None,
        }
    }

    /// Creates a new system message
    ///
    /// # Arguments
    ///
    /// * `content` - The message content
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::providers::Message;
    ///
    /// let msg = Message::system("You are a helpful assistant");
    /// assert_eq!(msg.role, "system");
    /// ```
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".to_string(),
            content: Some(content.into()),
            content_parts: None,
            tool_calls: None,
            tool_call_id: None,
        }
    }

    /// Creates a new tool result message
    ///
    /// # Arguments
    ///
    /// * `tool_call_id` - The ID of the tool call this result corresponds to
    /// * `content` - The tool execution result content
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::providers::Message;
    ///
    /// let msg = Message::tool_result("call_123", "File contents...");
    /// assert_eq!(msg.role, "tool");
    /// assert_eq!(msg.tool_call_id, Some("call_123".to_string()));
    /// ```
    pub fn tool_result(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: "tool".to_string(),
            content: Some(content.into()),
            content_parts: None,
            tool_calls: None,
            tool_call_id: Some(tool_call_id.into()),
        }
    }

    /// Creates an assistant message with tool calls
    ///
    /// # Arguments
    ///
    /// * `tool_calls` - The tool calls to include
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::providers::{Message, ToolCall, FunctionCall};
    ///
    /// let tool_call = ToolCall {
    ///     id: "call_123".to_string(),
    ///     function: FunctionCall {
    ///         name: "read_file".to_string(),
    ///         arguments: r#"{"path":"test.txt"}"#.to_string(),
    ///     },
    /// };
    /// let msg = Message::assistant_with_tools(vec![tool_call]);
    /// assert_eq!(msg.role, "assistant");
    /// assert!(msg.tool_calls.is_some());
    /// ```
    pub fn assistant_with_tools(tool_calls: Vec<ToolCall>) -> Self {
        Self {
            role: "assistant".to_string(),
            content: None,
            content_parts: None,
            tool_calls: Some(tool_calls),
            tool_call_id: None,
        }
    }

    /// Creates a new user message with ordered multimodal content parts.
    ///
    /// Text-only callers should continue to use [`Self::user`]. Vision-capable
    /// callers can use this constructor after validating provider/model support
    /// for image input.
    ///
    /// # Arguments
    ///
    /// * `input` - Validated multimodal prompt input.
    ///
    /// # Returns
    ///
    /// Returns a user message with legacy text content populated for text parts
    /// and provider-facing content parts populated for all parts.
    ///
    /// # Errors
    ///
    /// Returns an error string when the prompt input is empty or malformed.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::providers::{Message, MultimodalPromptInput, PromptInputPart};
    ///
    /// let message = Message::try_user_from_multimodal_input(MultimodalPromptInput::new(vec![
    ///     PromptInputPart::text("Describe this"),
    /// ]))
    /// .unwrap();
    /// assert!(message.content_parts.is_some());
    /// ```
    pub fn try_user_from_multimodal_input(
        input: MultimodalPromptInput,
    ) -> std::result::Result<Self, PromptInputError> {
        input.validate()?;
        let content = input.as_legacy_text();
        let content_parts = input
            .parts
            .into_iter()
            .map(|part| match part {
                PromptInputPart::Text(text) => ProviderMessageContentPart::text(text.text),
                PromptInputPart::Image(image) => ProviderMessageContentPart::image(image),
            })
            .collect();

        Ok(Self {
            role: "user".to_string(),
            content: if content.trim().is_empty() {
                None
            } else {
                Some(content)
            },
            content_parts: Some(content_parts),
            tool_calls: None,
            tool_call_id: None,
        })
    }

    /// Returns `true` when this message has ordered multimodal content parts.
    ///
    /// # Returns
    ///
    /// Returns whether `content_parts` is present and non-empty.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::providers::{Message, MultimodalPromptInput, PromptInputPart};
    ///
    /// let message = Message::try_user_from_multimodal_input(MultimodalPromptInput::new(vec![
    ///     PromptInputPart::text("hello"),
    /// ]))
    /// .unwrap();
    /// assert!(message.has_multimodal_content());
    /// ```
    pub fn has_multimodal_content(&self) -> bool {
        self.content_parts
            .as_ref()
            .map(|parts| !parts.is_empty())
            .unwrap_or(false)
    }

    /// Returns `true` when this message contains image content.
    ///
    /// # Returns
    ///
    /// Returns whether any multimodal content part is an image.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::providers::{ImagePromptPart, Message, MultimodalPromptInput, PromptInputPart};
    ///
    /// let message = Message::try_user_from_multimodal_input(MultimodalPromptInput::new(vec![
    ///     PromptInputPart::image(ImagePromptPart::inline_base64("image/png", "AAAA")),
    /// ]))
    /// .unwrap();
    /// assert!(message.has_image_content());
    /// ```
    pub fn has_image_content(&self) -> bool {
        self.content_parts
            .as_ref()
            .map(|parts| parts.iter().any(ProviderMessageContentPart::is_image))
            .unwrap_or(false)
    }

    /// Returns `true` when this message has only text multimodal content parts.
    ///
    /// Messages without `content_parts` return `false`; use `content` directly
    /// for legacy text-only messages.
    ///
    /// # Returns
    ///
    /// Returns whether all present multimodal parts are text parts.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::providers::{Message, MultimodalPromptInput, PromptInputPart};
    ///
    /// let message = Message::try_user_from_multimodal_input(MultimodalPromptInput::new(vec![
    ///     PromptInputPart::text("hello"),
    /// ]))
    /// .unwrap();
    /// assert!(message.has_text_only_multimodal_content());
    /// ```
    pub fn has_text_only_multimodal_content(&self) -> bool {
        self.content_parts
            .as_ref()
            .map(|parts| !parts.is_empty() && parts.iter().all(ProviderMessageContentPart::is_text))
            .unwrap_or(false)
    }

    /// Returns the number of ordered multimodal content parts.
    ///
    /// # Returns
    ///
    /// Returns `0` when the message has no multimodal content.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::providers::{Message, MultimodalPromptInput, PromptInputPart};
    ///
    /// let message = Message::try_user_from_multimodal_input(MultimodalPromptInput::new(vec![
    ///     PromptInputPart::text("hello"),
    /// ]))
    /// .unwrap();
    /// assert_eq!(message.multimodal_part_count(), 1);
    /// ```
    pub fn multimodal_part_count(&self) -> usize {
        self.content_parts
            .as_ref()
            .map(Vec::len)
            .unwrap_or_default()
    }

    /// Returns the number of image parts in this message.
    ///
    /// # Returns
    ///
    /// Returns `0` when the message contains no image content.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::providers::{ImagePromptPart, Message, MultimodalPromptInput, PromptInputPart};
    ///
    /// let message = Message::try_user_from_multimodal_input(MultimodalPromptInput::new(vec![
    ///     PromptInputPart::text("describe"),
    ///     PromptInputPart::image(ImagePromptPart::inline_base64("image/png", "AAAA")),
    /// ]))
    /// .unwrap();
    /// assert_eq!(message.image_part_count(), 1);
    /// ```
    pub fn image_part_count(&self) -> usize {
        self.content_parts
            .as_ref()
            .map(|parts| {
                parts
                    .iter()
                    .filter(|part| ProviderMessageContentPart::is_image(part))
                    .count()
            })
            .unwrap_or_default()
    }

    /// Returns a borrowed slice of multimodal content parts.
    ///
    /// # Returns
    ///
    /// Returns an empty slice when the message has no multimodal content.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::providers::{Message, MultimodalPromptInput, PromptInputPart};
    ///
    /// let message = Message::try_user_from_multimodal_input(MultimodalPromptInput::new(vec![
    ///     PromptInputPart::text("hello"),
    /// ]))
    /// .unwrap();
    /// assert_eq!(message.multimodal_parts().len(), 1);
    /// ```
    pub fn multimodal_parts(&self) -> &[ProviderMessageContentPart] {
        self.content_parts.as_deref().unwrap_or(&[])
    }

    /// Creates a new user message from text-only multimodal input.
    ///
    /// This constructor preserves compatibility with the existing provider
    /// message flow by accepting only multimodal input without images. Callers
    /// that need to send images must route the [`MultimodalPromptInput`] through
    /// a provider path that supports vision.
    ///
    /// # Arguments
    ///
    /// * `input` - Multimodal prompt input to convert.
    ///
    /// # Returns
    ///
    /// Returns a user message containing the joined text parts.
    ///
    /// # Errors
    ///
    /// Returns an error string when the input is empty or contains image parts.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::providers::{Message, MultimodalPromptInput};
    ///
    /// let message = Message::try_user_from_text_input(MultimodalPromptInput::text("hello")).unwrap();
    /// assert_eq!(message.content.as_deref(), Some("hello"));
    /// ```
    pub fn try_user_from_text_input(
        input: MultimodalPromptInput,
    ) -> std::result::Result<Self, PromptInputError> {
        input.validate()?;

        if input.has_images() {
            return Err(PromptInputError::ImageInputInTextOnlyMessage);
        }

        Ok(Self::user(input.as_legacy_text()))
    }
}

/// Returns whether any message in a collection contains image content.
///
/// # Arguments
///
/// * `messages` - Provider messages to inspect.
///
/// # Returns
///
/// Returns `true` when at least one message has an image content part.
///
/// # Examples
///
/// ```
/// use xzatoma::providers::{
///     ImagePromptPart, Message, MultimodalPromptInput, PromptInputPart,
///     messages_contain_image_content,
/// };
///
/// let messages = vec![Message::try_user_from_multimodal_input(MultimodalPromptInput::new(vec![
///     PromptInputPart::image(ImagePromptPart::inline_base64("image/png", "AAAA")),
/// ]))
/// .unwrap()];
///
/// assert!(messages_contain_image_content(&messages));
/// ```
pub fn messages_contain_image_content(messages: &[Message]) -> bool {
    messages.iter().any(Message::has_image_content)
}

/// Function call information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    /// Name of the function/tool to call
    pub name: String,
    /// Arguments for the function (as JSON string)
    pub arguments: String,
}

/// Tool call structure
///
/// Represents a request from the AI to execute a tool with specific arguments.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// Unique identifier for this tool call
    pub id: String,
    /// Function call details
    pub function: FunctionCall,
}

/// Model capability feature flags
///
/// Enum representing capabilities that models may support.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ModelCapability {
    /// Model supports longer context windows (typically 100k+ tokens)
    LongContext,
    /// Model supports function calling/tool use
    FunctionCalling,
    /// Model supports completion capability.
    ///
    /// Deprecated: use `FunctionCalling` or `Streaming` as appropriate.
    #[deprecated(
        note = "Use `FunctionCalling` or `Streaming` instead. Will be removed in a future release."
    )]
    Completion,
    /// Model supports vision/image understanding
    Vision,
    /// Model supports streaming responses
    Streaming,
    /// Model supports JSON output mode.
    ///
    /// Deprecated: use `FunctionCalling` or `Streaming` as appropriate.
    #[deprecated(
        note = "Use `FunctionCalling` or `Streaming` instead. Will be removed in a future release."
    )]
    JsonMode,
    /// Model is optimised for code generation and code-related tasks.
    CodeGeneration,
}

impl std::fmt::Display for ModelCapability {
    #[allow(deprecated)]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LongContext => write!(f, "LongContext"),
            Self::FunctionCalling => write!(f, "FunctionCalling"),
            Self::Completion => write!(f, "Completion"),
            Self::Vision => write!(f, "Vision"),
            Self::Streaming => write!(f, "Streaming"),
            Self::JsonMode => write!(f, "JsonMode"),
            Self::CodeGeneration => write!(f, "CodeGeneration"),
        }
    }
}

/// Token usage information from a completion
///
/// Tracks the number of tokens used in prompts and completions,
/// as reported by the AI provider.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    /// Number of tokens in the prompt
    pub prompt_tokens: usize,
    /// Number of tokens in the completion
    pub completion_tokens: usize,
    /// Total tokens used (prompt + completion)
    pub total_tokens: usize,
}

impl TokenUsage {
    /// Create a new TokenUsage instance
    ///
    /// # Arguments
    ///
    /// * `prompt_tokens` - Number of prompt tokens
    /// * `completion_tokens` - Number of completion tokens
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::providers::TokenUsage;
    ///
    /// let usage = TokenUsage::new(100, 50);
    /// assert_eq!(usage.prompt_tokens, 100);
    /// assert_eq!(usage.completion_tokens, 50);
    /// assert_eq!(usage.total_tokens, 150);
    /// ```
    pub fn new(prompt_tokens: usize, completion_tokens: usize) -> Self {
        let total_tokens = prompt_tokens + completion_tokens;
        Self {
            prompt_tokens,
            completion_tokens,
            total_tokens,
        }
    }
}

/// Model information and capabilities
///
/// Contains metadata about an available AI model, including its name,
/// context window, and supported capabilities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    /// Unique identifier for the model (e.g., "gpt-5.3-codex", "llama3.2:3b")
    pub name: String,
    /// Display name for user-friendly presentation (e.g., "GPT-5.3 Codex")
    pub display_name: String,
    /// Maximum context window size in tokens
    pub context_window: usize,
    /// Supported capabilities
    pub capabilities: Vec<ModelCapability>,
    /// Provider-specific metadata (key-value pairs)
    pub provider_specific: HashMap<String, String>,
    /// Raw provider-specific data (fallback for unknown fields)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_data: Option<serde_json::Value>,
    /// Whether this model supports tool/function calling.
    ///
    /// Derived from `capabilities` and kept in sync by `add_capability` and
    /// `with_capabilities`. Defaults to `false` for a newly constructed instance
    /// with no capabilities.
    #[serde(default)]
    pub supports_tools: bool,
    /// Whether this model supports streaming responses.
    ///
    /// Derived from `capabilities` and kept in sync by `add_capability` and
    /// `with_capabilities`. Defaults to `false` for a newly constructed instance
    /// with no capabilities.
    #[serde(default)]
    pub supports_streaming: bool,
}

impl ModelInfo {
    /// Create a new ModelInfo instance
    ///
    /// # Arguments
    ///
    /// * `name` - Model identifier
    /// * `display_name` - User-friendly display name
    /// * `context_window` - Context window size in tokens
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::providers::ModelInfo;
    ///
    /// let model = ModelInfo::new("gpt-4", "GPT-4", 8192);
    /// assert_eq!(model.name, "gpt-4");
    /// assert_eq!(model.context_window, 8192);
    /// ```
    pub fn new(
        name: impl Into<String>,
        display_name: impl Into<String>,
        context_window: usize,
    ) -> Self {
        Self {
            name: name.into(),
            display_name: display_name.into(),
            context_window,
            capabilities: Vec::new(),
            provider_specific: HashMap::new(),
            raw_data: None,
            supports_tools: false,
            supports_streaming: false,
        }
    }

    /// Add a capability to this model
    ///
    /// # Arguments
    ///
    /// * `capability` - Capability to add
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::providers::{ModelInfo, ModelCapability};
    ///
    /// let mut model = ModelInfo::new("gpt-4", "GPT-4", 8192);
    /// model.add_capability(ModelCapability::FunctionCalling);
    /// assert!(model.capabilities.contains(&ModelCapability::FunctionCalling));
    /// ```
    pub fn add_capability(&mut self, capability: ModelCapability) {
        if !self.capabilities.contains(&capability) {
            self.capabilities.push(capability);
        }
        if capability == ModelCapability::FunctionCalling {
            self.supports_tools = true;
        }
        if capability == ModelCapability::Streaming {
            self.supports_streaming = true;
        }
    }

    /// Check if this model supports a capability
    ///
    /// # Arguments
    ///
    /// * `capability` - Capability to check for
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::providers::{ModelInfo, ModelCapability};
    ///
    /// let mut model = ModelInfo::new("gpt-4", "GPT-4", 8192);
    /// model.add_capability(ModelCapability::FunctionCalling);
    /// assert!(model.supports_capability(ModelCapability::FunctionCalling));
    /// assert!(!model.supports_capability(ModelCapability::Vision));
    /// ```
    pub fn supports_capability(&self, capability: ModelCapability) -> bool {
        self.capabilities.contains(&capability)
    }

    /// Set provider-specific metadata
    ///
    /// # Arguments
    ///
    /// * `key` - Metadata key
    /// * `value` - Metadata value
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::providers::ModelInfo;
    ///
    /// let mut model = ModelInfo::new("gpt-4", "GPT-4", 8192);
    /// model.set_provider_metadata("version", "2024-01");
    /// assert_eq!(model.provider_specific.get("version"), Some(&"2024-01".to_string()));
    /// ```
    pub fn set_provider_metadata(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.provider_specific.insert(key.into(), value.into());
    }

    /// Add capabilities and return self for builder pattern
    ///
    /// # Arguments
    ///
    /// * `capabilities` - Vector of capabilities to add
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::providers::{ModelInfo, ModelCapability};
    ///
    /// let model = ModelInfo::new("gpt-4", "GPT-4", 8192)
    ///     .with_capabilities(vec![
    ///         ModelCapability::FunctionCalling,
    ///         ModelCapability::Vision,
    ///     ]);
    /// assert_eq!(model.capabilities.len(), 2);
    /// ```
    pub fn with_capabilities(mut self, capabilities: Vec<ModelCapability>) -> Self {
        self.capabilities = capabilities;
        self.supports_tools = self
            .capabilities
            .contains(&ModelCapability::FunctionCalling);
        self.supports_streaming = self.capabilities.contains(&ModelCapability::Streaming);
        self
    }
}

/// Extended model information with full provider API data
///
/// This structure extends ModelInfo with additional fields from provider APIs
/// that are useful for summary output and advanced tooling.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfoSummary {
    /// Core model information
    pub info: ModelInfo,

    /// Provider API state (e.g., "enabled", "disabled")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,

    /// Maximum prompt tokens allowed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_prompt_tokens: Option<usize>,

    /// Maximum completion tokens allowed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_completion_tokens: Option<usize>,

    /// Whether the model supports tool calls
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supports_tool_calls: Option<bool>,

    /// Whether the model supports vision/image input
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supports_vision: Option<bool>,

    /// Raw provider-specific data (fallback for unknown fields)
    pub raw_data: serde_json::Value,
}

impl ModelInfoSummary {
    /// Create summary from core ModelInfo
    ///
    /// # Arguments
    ///
    /// * `info` - Core model information
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::providers::{ModelInfo, ModelInfoSummary};
    ///
    /// let info = ModelInfo::new("gpt-4", "GPT-4", 8192);
    /// let summary = ModelInfoSummary::from_model_info(info);
    /// assert_eq!(summary.info.name, "gpt-4");
    /// assert!(summary.state.is_none());
    /// ```
    pub fn from_model_info(mut info: ModelInfo) -> Self {
        let raw_data = info.raw_data.take().unwrap_or(serde_json::Value::Null);
        let supports_tool_calls = Some(info.supports_capability(ModelCapability::FunctionCalling));
        let supports_vision = Some(info.supports_capability(ModelCapability::Vision));

        Self {
            info,
            state: None,
            max_prompt_tokens: None,
            max_completion_tokens: None,
            supports_tool_calls,
            supports_vision,
            raw_data,
        }
    }

    /// Set the token limit fields using the builder pattern.
    ///
    /// # Arguments
    ///
    /// * `max_prompt_tokens` - Maximum prompt tokens allowed, or `None` if not
    ///   reported by the provider
    /// * `max_completion_tokens` - Maximum completion tokens allowed, or `None`
    ///   if not reported by the provider
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::providers::{ModelInfo, ModelInfoSummary};
    ///
    /// let info = ModelInfo::new("gpt-4", "GPT-4", 8192);
    /// let summary = ModelInfoSummary::from_model_info(info)
    ///     .with_limits(Some(6144), Some(2048));
    /// assert_eq!(summary.max_prompt_tokens, Some(6144));
    /// assert_eq!(summary.max_completion_tokens, Some(2048));
    /// ```
    pub fn with_limits(
        mut self,
        max_prompt_tokens: Option<usize>,
        max_completion_tokens: Option<usize>,
    ) -> Self {
        self.max_prompt_tokens = max_prompt_tokens;
        self.max_completion_tokens = max_completion_tokens;
        self
    }

    /// Create summary with full data
    ///
    /// # Arguments
    ///
    /// * `info` - Core model information
    /// * `state` - Provider API state
    /// * `max_prompt_tokens` - Maximum prompt tokens
    /// * `max_completion_tokens` - Maximum completion tokens
    /// * `supports_tool_calls` - Tool calling support flag
    /// * `supports_vision` - Vision support flag
    /// * `raw_data` - Raw provider-specific data
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::providers::{ModelInfo, ModelInfoSummary};
    /// use serde_json;
    ///
    /// let info = ModelInfo::new("gpt-4", "GPT-4", 8192);
    /// let summary = ModelInfoSummary::new(
    ///     info,
    ///     Some("enabled".to_string()),
    ///     Some(6144),
    ///     Some(2048),
    ///     Some(true),
    ///     Some(true),
    ///     serde_json::json!({"version": "2024-01"}),
    /// );
    /// assert_eq!(summary.state, Some("enabled".to_string()));
    /// assert_eq!(summary.supports_tool_calls, Some(true));
    /// ```
    pub fn new(
        info: ModelInfo,
        state: Option<String>,
        max_prompt_tokens: Option<usize>,
        max_completion_tokens: Option<usize>,
        supports_tool_calls: Option<bool>,
        supports_vision: Option<bool>,
        raw_data: serde_json::Value,
    ) -> Self {
        Self {
            info,
            state,
            max_prompt_tokens,
            max_completion_tokens,
            supports_tool_calls,
            supports_vision,
            raw_data,
        }
    }
}

/// Reason why the model stopped generating tokens.
///
/// Normalised from the provider-specific finish-reason string so that callers
/// can branch on a typed value instead of matching raw strings.
///
/// # Examples
///
/// ```
/// use xzatoma::providers::FinishReason;
///
/// let reason = FinishReason::Stop;
/// assert_eq!(reason, FinishReason::default());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    /// Model reached a natural stopping point.
    #[default]
    Stop,
    /// Model reached the configured maximum token limit.
    Length,
    /// Model invoked one or more tools.
    ToolCalls,
    /// Output was filtered by the provider's content policy.
    ContentFilter,
    /// Any other provider-specific reason.
    Other,
}

/// Provider-level capabilities and features
///
/// Describes which features and operations a provider supports.
#[derive(Debug, Clone, Copy, Default)]
pub struct ProviderCapabilities {
    /// Provider supports listing available models
    pub supports_model_listing: bool,
    /// Provider supports querying detailed model information
    pub supports_model_details: bool,
    /// Provider supports changing the active model
    pub supports_model_switching: bool,
    /// Provider returns token usage information in responses
    pub supports_token_counts: bool,
    /// Provider supports streaming responses
    pub supports_streaming: bool,
    /// Provider supports image input in user prompts.
    pub supports_vision: bool,
}

/// Completion response with message and optional token usage
///
/// Contains both the response message and metadata about token usage,
/// as well as an optional model identifier and reasoning content for
/// providers that support extended thinking.
#[derive(Debug, Clone)]
pub struct CompletionResponse {
    /// The response message from the AI
    pub message: Message,
    /// Optional token usage information
    pub usage: Option<TokenUsage>,
    /// The model that generated this response
    pub model: Option<String>,
    /// Reasoning content from extended-thinking models (e.g. o1 family)
    pub reasoning: Option<String>,
    /// Reason the model stopped generating tokens.
    pub finish_reason: FinishReason,
}

impl CompletionResponse {
    /// Create a new CompletionResponse
    ///
    /// # Arguments
    ///
    /// * `message` - The response message
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::providers::{CompletionResponse, Message};
    ///
    /// let response = CompletionResponse::new(Message::assistant("Hello!"));
    /// assert_eq!(response.message.role, "assistant");
    /// assert!(response.usage.is_none());
    /// assert!(response.model.is_none());
    /// assert!(response.reasoning.is_none());
    /// ```
    pub fn new(message: Message) -> Self {
        Self {
            message,
            usage: None,
            model: None,
            reasoning: None,
            finish_reason: FinishReason::Stop,
        }
    }

    /// Create a new CompletionResponse with token usage
    ///
    /// # Arguments
    ///
    /// * `message` - The response message
    /// * `usage` - Token usage information
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::providers::{CompletionResponse, Message, TokenUsage};
    ///
    /// let usage = TokenUsage::new(100, 50);
    /// let response = CompletionResponse::with_usage(Message::assistant("Hello!"), usage);
    /// assert_eq!(response.message.role, "assistant");
    /// assert!(response.usage.is_some());
    /// assert!(response.model.is_none());
    /// assert!(response.reasoning.is_none());
    /// ```
    pub fn with_usage(message: Message, usage: TokenUsage) -> Self {
        Self {
            message,
            usage: Some(usage),
            model: None,
            reasoning: None,
            finish_reason: FinishReason::Stop,
        }
    }

    /// Create a new CompletionResponse with model identifier
    ///
    /// # Arguments
    ///
    /// * `message` - The response message
    /// * `model` - The model that generated this response
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::providers::{CompletionResponse, Message};
    ///
    /// let response = CompletionResponse::with_model(
    ///     Message::assistant("Hello!"),
    ///     "gpt-5-mini".to_string(),
    /// );
    /// assert_eq!(response.model.as_deref(), Some("gpt-5-mini"));
    /// assert!(response.usage.is_none());
    /// assert!(response.reasoning.is_none());
    /// ```
    pub fn with_model(message: Message, model: String) -> Self {
        Self {
            message,
            usage: None,
            model: Some(model),
            reasoning: None,
            finish_reason: FinishReason::Stop,
        }
    }

    /// Create a builder-style setter for model
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::providers::{CompletionResponse, Message};
    ///
    /// let response = CompletionResponse::new(Message::assistant("Hello!"))
    ///     .set_model("gpt-5-mini".to_string());
    /// assert_eq!(response.model.as_deref(), Some("gpt-5-mini"));
    /// ```
    pub fn set_model(mut self, model: String) -> Self {
        self.model = Some(model);
        self
    }

    /// Create a builder-style setter for reasoning content
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::providers::{CompletionResponse, Message};
    ///
    /// let response = CompletionResponse::new(Message::assistant("42"))
    ///     .set_reasoning("I thought about it carefully".to_string());
    /// assert_eq!(
    ///     response.reasoning.as_deref(),
    ///     Some("I thought about it carefully"),
    /// );
    /// ```
    pub fn set_reasoning(mut self, reasoning: String) -> Self {
        self.reasoning = Some(reasoning);
        self
    }

    /// Set the finish reason using the builder pattern.
    ///
    /// # Arguments
    ///
    /// * `reason` - The reason the model stopped generating
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::providers::{CompletionResponse, FinishReason, Message};
    ///
    /// let response = CompletionResponse::new(Message::assistant("Done"))
    ///     .with_finish_reason(FinishReason::Length);
    /// assert_eq!(response.finish_reason, FinishReason::Length);
    /// ```
    pub fn with_finish_reason(mut self, reason: FinishReason) -> Self {
        self.finish_reason = reason;
        self
    }
}

// ---------------------------------------------------------------------------
// Shared provider wire types
// ---------------------------------------------------------------------------

/// Shared tool definition sent to AI providers in request bodies.
///
/// Both the Copilot and Ollama providers use an identical JSON shape for tool
/// definitions. This single type replaces the formerly duplicated
/// `CopilotTool`/`OllamaTool` structs.
///
/// # Examples
///
/// ```
/// use xzatoma::providers::base::{ProviderTool, ProviderFunction};
/// use serde_json::json;
///
/// let tool = ProviderTool {
///     r#type: "function".to_string(),
///     function: ProviderFunction {
///         name: "read_file".to_string(),
///         description: "Read a file from disk".to_string(),
///         parameters: json!({"type": "object", "properties": {}}),
///     },
/// };
/// assert_eq!(tool.function.name, "read_file");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderTool {
    /// Always `"function"` for the tool definitions sent to current providers.
    pub r#type: String,
    /// Function metadata: name, description, and JSON-Schema parameters.
    pub function: ProviderFunction,
}

/// Function metadata within a [`ProviderTool`].
///
/// # Examples
///
/// ```
/// use xzatoma::providers::base::ProviderFunction;
/// use serde_json::json;
///
/// let f = ProviderFunction {
///     name: "search".to_string(),
///     description: "Search the codebase".to_string(),
///     parameters: json!({"type": "object"}),
/// };
/// assert_eq!(f.name, "search");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderFunction {
    /// Tool name used by the model when calling the tool.
    pub name: String,
    /// Human-readable description of what the tool does.
    pub description: String,
    /// JSON Schema describing the tool's input parameters.
    pub parameters: serde_json::Value,
}

/// Canonical function-call arguments within a provider tool call.
///
/// Stores arguments as [`serde_json::Value`]. Providers that use a JSON-string
/// wire format (Copilot) must convert via [`arguments_as_string`][Self::arguments_as_string]
/// when building the outgoing request; providers that use a JSON-object wire
/// format (Ollama) can serialize this struct directly.
///
/// # Examples
///
/// ```
/// use xzatoma::providers::base::ProviderFunctionCall;
/// use serde_json::json;
///
/// let fc = ProviderFunctionCall {
///     name: "read_file".to_string(),
///     arguments: json!({"path": "/tmp/foo.txt"}),
/// };
/// assert_eq!(fc.arguments_as_string(), r#"{"path":"/tmp/foo.txt"}"#);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderFunctionCall {
    /// Name of the function being called.
    pub name: String,
    /// Arguments in canonical JSON value form.
    ///
    /// Use [`arguments_as_string`][Self::arguments_as_string] when the
    /// provider wire format requires a serialized JSON string rather than an
    /// inline JSON object.
    #[serde(default)]
    pub arguments: serde_json::Value,
}

impl ProviderFunctionCall {
    /// Returns `arguments` serialized as a compact JSON string.
    ///
    /// This is the wire format required by the Copilot completions endpoint.
    /// Returns `"{}"` if serialization fails.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::providers::base::ProviderFunctionCall;
    /// use serde_json::json;
    ///
    /// let fc = ProviderFunctionCall {
    ///     name: "greet".to_string(),
    ///     arguments: json!({"name": "Alice"}),
    /// };
    /// assert_eq!(fc.arguments_as_string(), r#"{"name":"Alice"}"#);
    /// ```
    pub fn arguments_as_string(&self) -> String {
        serde_json::to_string(&self.arguments).unwrap_or_else(|_| "{}".to_string())
    }
}

/// Shared tool-call record within a provider message.
///
/// The `id` and `type` fields are given default values so that providers
/// which omit them in responses (e.g. older Ollama builds) still deserialize
/// correctly.
///
/// # Examples
///
/// ```
/// use xzatoma::providers::base::{ProviderToolCall, ProviderFunctionCall};
/// use serde_json::json;
///
/// let tc = ProviderToolCall {
///     id: "call_abc".to_string(),
///     r#type: "function".to_string(),
///     function: ProviderFunctionCall {
///         name: "read_file".to_string(),
///         arguments: json!({"path": "a.rs"}),
///     },
/// };
/// assert_eq!(tc.id, "call_abc");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderToolCall {
    /// Unique identifier for this tool call, assigned by the provider.
    #[serde(default)]
    pub id: String,
    /// Always `"function"` for current providers.
    #[serde(default = "provider_tool_call_type")]
    pub r#type: String,
    /// Function name and arguments.
    pub function: ProviderFunctionCall,
}

fn provider_tool_call_type() -> String {
    "function".to_string()
}

/// Unified message type for provider request and response serialization.
///
/// The `tool_call_id` field is present only in Copilot's wire format; Ollama
/// messages omit it. Using `skip_serializing_if` on the field means this
/// single struct serializes correctly for both wire formats.
///
/// # Examples
///
/// ```
/// use xzatoma::providers::base::ProviderMessage;
///
/// let msg = ProviderMessage {
///     role: "user".to_string(),
///     content: "Hello!".to_string(),
///     content_parts: None,
///     images: vec![],
///     tool_calls: None,
///     tool_call_id: None,
/// };
/// assert_eq!(msg.role, "user");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderMessage {
    /// Sender role: `"user"`, `"assistant"`, `"system"`, or `"tool"`.
    pub role: String,
    /// Legacy plain-text message content.
    ///
    /// Vision-capable providers should prefer `content_parts` for user messages
    /// when it is present. Text-only providers can continue to serialize this
    /// field directly after image input has been rejected by provider
    /// capability validation.
    #[serde(default)]
    pub content: String,
    /// Optional ordered multimodal content parts for provider request builders.
    ///
    /// This is populated for ACP stdio prompts that contain images and preserves
    /// the original ordering of text and image parts. Providers that do not
    /// support vision must reject messages with image content before execution.
    #[serde(default, skip_serializing)]
    pub content_parts: Option<ProviderMessageContentParts>,
    /// Native image payloads for providers such as Ollama that expect base64
    /// image data in a separate `images` field on user messages.
    ///
    /// This is derived from `content_parts` by provider conversion code. It is
    /// omitted for text-only messages and for providers that serialize image
    /// content inline.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<String>,
    /// Tool calls requested by the assistant.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ProviderToolCall>>,
    /// Tool-result identifier linking this message to an assistant tool call.
    ///
    /// Populated only when `role == "tool"` (Copilot wire format).
    /// Ollama does not use this field; it is omitted during serialization when
    /// `None`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl ProviderMessage {
    /// Builds native Ollama image payloads from multimodal content parts.
    ///
    /// Ollama expects user images as base64 strings in an `images` field rather
    /// than as inline structured message content. This helper extracts inline
    /// base64 data and encodes inline byte data. File references and remote URLs
    /// are ignored because ACP conversion should resolve local files before the
    /// provider request is built, and Ollama does not accept remote image URLs in
    /// the native `images` field.
    ///
    /// # Returns
    ///
    /// Returns base64 image payloads suitable for Ollama's native `images`
    /// message field.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::providers::{
    ///     ImagePromptPart, Message, MultimodalPromptInput, PromptInputPart,
    /// };
    ///
    /// let message = Message::try_user_from_multimodal_input(MultimodalPromptInput::new(vec![
    ///     PromptInputPart::image(ImagePromptPart::inline_base64("image/png", "AAAA")),
    /// ]))
    /// .unwrap();
    ///
    /// let provider_message = xzatoma::providers::ProviderMessage::from_message_for_ollama(&message);
    /// assert_eq!(provider_message.images, vec!["AAAA".to_string()]);
    /// ```
    pub fn ollama_native_images(&self) -> Vec<String> {
        content_parts_to_ollama_images(self.content_parts.as_deref())
    }

    /// Converts a high-level provider message into the shared wire message shape
    /// with Ollama's native image field populated.
    ///
    /// # Arguments
    ///
    /// * `message` - Message to convert.
    ///
    /// # Returns
    ///
    /// Returns a provider message containing text, optional multimodal parts,
    /// native Ollama image payloads, tool calls, and tool call ID.
    pub fn from_message_for_ollama(message: &Message) -> Self {
        Self {
            role: message.role.clone(),
            content: message.content.clone().unwrap_or_default(),
            content_parts: message.content_parts.clone(),
            images: content_parts_to_ollama_images(message.content_parts.as_deref()),
            tool_calls: None,
            tool_call_id: None,
        }
    }
}

fn content_parts_to_ollama_images(parts: Option<&[ProviderMessageContentPart]>) -> Vec<String> {
    parts
        .unwrap_or(&[])
        .iter()
        .filter_map(|part| match part {
            ProviderMessageContentPart::Image { source, .. } => match source {
                ImagePromptSource::InlineBase64(data) => Some(data.clone()),
                ImagePromptSource::InlineBytes(bytes) => {
                    Some(base64::engine::general_purpose::STANDARD.encode(bytes))
                }
                ImagePromptSource::FilePath(_) | ImagePromptSource::RemoteUrl(_) => None,
            },
            ProviderMessageContentPart::Text { .. } => None,
        })
        .collect()
}

/// Unified request body sent to provider completions endpoints.
///
/// Both Copilot (`/chat/completions`) and Ollama (`/api/chat`) accept a
/// request body with this exact shape. Provider modules build a
/// `ProviderRequest` and serialize it directly instead of maintaining
/// separate duplicated structs.
///
/// # Examples
///
/// ```
/// use xzatoma::providers::base::{ProviderMessage, ProviderRequest};
///
/// let req = ProviderRequest {
///     model: "gpt-4o".to_string(),
///     messages: vec![ProviderMessage {
///         role: "user".to_string(),
///         content: "Hi".to_string(),
///         content_parts: None,
///         images: vec![],
///         tool_calls: None,
///         tool_call_id: None,
///     }],
///     tools: vec![],
///     stream: false,
/// };
/// assert_eq!(req.model, "gpt-4o");
/// ```
#[derive(Debug, Serialize)]
pub struct ProviderRequest {
    /// Model identifier to invoke.
    pub model: String,
    /// Ordered conversation messages.
    pub messages: Vec<ProviderMessage>,
    /// Tool definitions available to the model.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<ProviderTool>,
    /// Whether to stream the response token-by-token.
    pub stream: bool,
}

/// Convert raw tool-definition JSON values from the tool registry into
/// [`ProviderTool`] instances suitable for provider request serialization.
///
/// Entries that are missing `name`, `description`, or `parameters` are
/// silently dropped so partial schemas from dynamically registered tools
/// never cause a request failure.
///
/// This free function replaces the structurally identical `convert_tools`
/// methods that previously existed in both `copilot.rs` and `ollama.rs`.
///
/// # Arguments
///
/// * `tools` - Slice of raw JSON tool definitions from the tool registry.
///
/// # Returns
///
/// A `Vec<ProviderTool>` containing only well-formed entries.
///
/// # Examples
///
/// ```
/// use serde_json::json;
/// use xzatoma::providers::base::convert_tools_from_json;
///
/// let tools = vec![
///     json!({
///         "name": "read_file",
///         "description": "Read a file from disk",
///         "parameters": {"type": "object", "properties": {}}
///     }),
/// ];
/// let converted = convert_tools_from_json(&tools);
/// assert_eq!(converted.len(), 1);
/// assert_eq!(converted[0].function.name, "read_file");
/// assert_eq!(converted[0].r#type, "function");
/// ```
pub fn convert_tools_from_json(tools: &[serde_json::Value]) -> Vec<ProviderTool> {
    tools
        .iter()
        .filter_map(|t| {
            let obj = t.as_object()?;
            let name = obj.get("name")?.as_str()?.to_string();
            let description = obj.get("description")?.as_str()?.to_string();
            let parameters = obj.get("parameters")?.clone();

            Some(ProviderTool {
                r#type: "function".to_string(),
                function: ProviderFunction {
                    name,
                    description,
                    parameters,
                },
            })
        })
        .collect()
}

/// Validates message sequence and removes orphan tool messages
///
/// Orphan tool messages are those that don't have a corresponding preceding
/// assistant message with matching tool_calls. This validates message integrity
/// and prevents provider API errors (e.g., 400 Bad Request from Copilot API).
///
/// An orphan tool message is:
/// - A message with role="tool" but no matching assistant message with tool_calls
/// - A message with role="tool" but no tool_call_id field
///
/// # Arguments
///
/// * `messages` - The messages to validate
///
/// # Returns
///
/// Returns a vector of validated messages with orphans removed and warnings logged
///
/// # Examples
///
/// ```
/// use xzatoma::providers::{Message, validate_message_sequence};
///
/// let messages = vec![
///     Message::user("Do something"),
///     Message::tool_result("call_123", "Result"),
/// ];
/// let validated = validate_message_sequence(&messages);
/// assert_eq!(validated.len(), 1); // Orphan tool removed, only user remains
/// ```
pub fn validate_message_sequence(messages: &[Message]) -> Vec<Message> {
    use std::collections::HashSet;

    // First pass: collect all tool_call IDs from assistant messages with tool_calls
    let mut valid_tool_ids: HashSet<String> = HashSet::new();
    for message in messages {
        if message.role == "assistant" {
            if let Some(tool_calls) = &message.tool_calls {
                for tool_call in tool_calls {
                    valid_tool_ids.insert(tool_call.id.clone());
                }
            }
        }
    }

    // Second pass: filter out orphan tool messages
    messages
        .iter()
        .filter_map(|message| {
            // Tool messages must have a tool_call_id and a matching assistant message
            if message.role == "tool" {
                if let Some(tool_call_id) = &message.tool_call_id {
                    if !valid_tool_ids.contains(tool_call_id) {
                        tracing::warn!(
                            "Dropping orphan tool message with tool_call_id: {}",
                            tool_call_id
                        );
                        return None;
                    }
                } else {
                    tracing::warn!("Dropping tool message without tool_call_id");
                    return None;
                }
            }

            Some(message.clone())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_multimodal_prompt_input_validate_empty_returns_typed_error() {
        let error = MultimodalPromptInput::new(vec![]).validate().unwrap_err();
        assert!(matches!(error, PromptInputError::Empty));
        assert_eq!(
            error.to_string(),
            "prompt input must contain at least one content part"
        );
    }

    #[test]
    fn test_image_prompt_part_validate_missing_mime_returns_typed_error() {
        let error = ImagePromptPart::inline_base64("", "AAAA")
            .validate()
            .unwrap_err();
        assert!(matches!(error, ImagePromptError::MissingMimeType));
        assert_eq!(error.to_string(), "image content is missing a MIME type");
    }

    #[test]
    fn test_try_user_from_text_input_with_image_returns_typed_error() {
        let input = MultimodalPromptInput::new(vec![PromptInputPart::image(
            ImagePromptPart::inline_base64("image/png", "AAAA"),
        )]);

        let error = Message::try_user_from_text_input(input).unwrap_err();

        assert!(matches!(
            error,
            PromptInputError::ImageInputInTextOnlyMessage
        ));
    }

    #[test]
    fn test_message_user() {
        let msg = Message::user("Hello");
        assert_eq!(msg.role, "user");
        assert_eq!(msg.content, Some("Hello".to_string()));
        assert!(msg.tool_calls.is_none());
        assert!(msg.tool_call_id.is_none());
    }

    #[test]
    fn test_message_user_with_string() {
        let msg = Message::user(String::from("Hello"));
        assert_eq!(msg.role, "user");
        assert_eq!(msg.content, Some("Hello".to_string()));
    }

    #[test]
    fn test_message_assistant() {
        let msg = Message::assistant("Hi there");
        assert_eq!(msg.role, "assistant");
        assert_eq!(msg.content, Some("Hi there".to_string()));
        assert!(msg.tool_calls.is_none());
        assert!(msg.tool_call_id.is_none());
    }

    #[test]
    fn test_message_system() {
        let msg = Message::system("System prompt");
        assert_eq!(msg.role, "system");
        assert_eq!(msg.content, Some("System prompt".to_string()));
        assert!(msg.tool_calls.is_none());
        assert!(msg.tool_call_id.is_none());
    }

    #[test]
    fn test_message_tool_result() {
        let msg = Message::tool_result("call_123", "result");
        assert_eq!(msg.role, "tool");
        assert_eq!(msg.content, Some("result".to_string()));
        assert_eq!(msg.tool_call_id, Some("call_123".to_string()));
        assert!(msg.tool_calls.is_none());
    }

    #[test]
    fn test_message_assistant_with_tools() {
        let tool_call = ToolCall {
            id: "call_123".to_string(),
            function: FunctionCall {
                name: "test_tool".to_string(),
                arguments: "{}".to_string(),
            },
        };
        let msg = Message::assistant_with_tools(vec![tool_call]);
        assert_eq!(msg.role, "assistant");
        assert!(msg.content.is_none());
        assert!(msg.tool_calls.is_some());
        assert_eq!(msg.tool_calls.as_ref().unwrap().len(), 1);
    }

    #[test]
    fn test_message_serialization() {
        let msg = Message::user("Test");
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"role\":\"user\""));
        assert!(json.contains("\"content\":\"Test\""));
    }

    #[test]
    fn test_tool_call_serialization() {
        let tool_call = ToolCall {
            id: "call_123".to_string(),
            function: FunctionCall {
                name: "test_tool".to_string(),
                arguments: r#"{"arg":"value"}"#.to_string(),
            },
        };
        let json = serde_json::to_string(&tool_call).unwrap();
        assert!(json.contains("\"id\":\"call_123\""));
        assert!(json.contains("\"name\":\"test_tool\""));
        assert!(json.contains("\"arguments\""));
    }

    #[test]
    fn test_function_call() {
        let func_call = FunctionCall {
            name: "read_file".to_string(),
            arguments: r#"{"path":"test.txt"}"#.to_string(),
        };
        assert_eq!(func_call.name, "read_file");
        assert!(func_call.arguments.contains("path"));
    }

    #[test]
    fn test_token_usage_new() {
        let usage = TokenUsage::new(100, 50);
        assert_eq!(usage.prompt_tokens, 100);
        assert_eq!(usage.completion_tokens, 50);
        assert_eq!(usage.total_tokens, 150);
    }

    #[test]
    fn test_token_usage_zero() {
        let usage = TokenUsage::new(0, 0);
        assert_eq!(usage.total_tokens, 0);
    }

    #[test]
    fn test_token_usage_serialization() {
        let usage = TokenUsage::new(100, 50);
        let json = serde_json::to_string(&usage).unwrap();
        let deserialized: TokenUsage = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.prompt_tokens, 100);
        assert_eq!(deserialized.completion_tokens, 50);
    }

    #[allow(deprecated)]
    #[test]
    fn test_model_capability_display() {
        assert_eq!(ModelCapability::LongContext.to_string(), "LongContext");
        assert_eq!(
            ModelCapability::FunctionCalling.to_string(),
            "FunctionCalling"
        );
        assert_eq!(ModelCapability::Completion.to_string(), "Completion");
        assert_eq!(ModelCapability::Vision.to_string(), "Vision");
        assert_eq!(ModelCapability::Streaming.to_string(), "Streaming");
        assert_eq!(ModelCapability::JsonMode.to_string(), "JsonMode");
        assert_eq!(
            ModelCapability::CodeGeneration.to_string(),
            "CodeGeneration"
        );
    }

    #[test]
    fn test_model_capability_serialization() {
        let cap = ModelCapability::LongContext;
        let json = serde_json::to_string(&cap).unwrap();
        let deserialized: ModelCapability = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, ModelCapability::LongContext);
    }

    #[test]
    fn test_model_info_creation() {
        let model = ModelInfo::new("gpt-4", "GPT-4 Turbo", 8192);
        assert_eq!(model.name, "gpt-4");
        assert_eq!(model.display_name, "GPT-4 Turbo");
        assert_eq!(model.context_window, 8192);
        assert!(model.capabilities.is_empty());
        assert!(model.provider_specific.is_empty());
        assert!(!model.supports_tools);
        assert!(!model.supports_streaming);
    }

    #[test]
    fn test_model_info_add_capability() {
        let mut model = ModelInfo::new("gpt-4", "GPT-4", 8192);
        assert!(model.capabilities.is_empty());
        assert!(!model.supports_tools);

        model.add_capability(ModelCapability::FunctionCalling);
        assert_eq!(model.capabilities.len(), 1);
        assert!(model
            .capabilities
            .contains(&ModelCapability::FunctionCalling));
        assert!(model.supports_tools);
        assert!(!model.supports_streaming);

        model.add_capability(ModelCapability::FunctionCalling);
        assert_eq!(model.capabilities.len(), 1);
    }

    #[test]
    fn test_model_info_supports_capability() {
        let mut model = ModelInfo::new("gpt-4", "GPT-4", 8192);
        model.add_capability(ModelCapability::FunctionCalling);
        model.add_capability(ModelCapability::Vision);

        assert!(model.supports_capability(ModelCapability::FunctionCalling));
        assert!(model.supports_capability(ModelCapability::Vision));
        assert!(!model.supports_capability(ModelCapability::Streaming));
    }

    #[test]
    fn test_model_info_provider_metadata() {
        let mut model = ModelInfo::new("gpt-4", "GPT-4", 8192);
        model.set_provider_metadata("version", "2024-01");
        model.set_provider_metadata("region", "us-east-1");

        assert_eq!(
            model.provider_specific.get("version"),
            Some(&"2024-01".to_string())
        );
        assert_eq!(
            model.provider_specific.get("region"),
            Some(&"us-east-1".to_string())
        );
    }

    #[test]
    fn test_model_info_serialization() {
        let mut model = ModelInfo::new("gpt-4", "GPT-4", 8192);
        model.add_capability(ModelCapability::FunctionCalling);
        model.set_provider_metadata("version", "2024-01");

        let json = serde_json::to_string(&model).unwrap();
        let deserialized: ModelInfo = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.name, "gpt-4");
        assert_eq!(deserialized.context_window, 8192);
        assert_eq!(deserialized.capabilities.len(), 1);
        assert_eq!(
            deserialized.provider_specific.get("version"),
            Some(&"2024-01".to_string())
        );
    }

    #[test]
    fn test_provider_capabilities_default() {
        let caps = ProviderCapabilities::default();
        assert!(!caps.supports_model_listing);
        assert!(!caps.supports_model_details);
        assert!(!caps.supports_model_switching);
        assert!(!caps.supports_token_counts);
        assert!(!caps.supports_streaming);
        assert!(!caps.supports_vision);
    }

    #[test]
    fn test_completion_response_new() {
        let msg = Message::assistant("Hello!");
        let response = CompletionResponse::new(msg);

        assert_eq!(response.message.role, "assistant");
        assert_eq!(response.message.content, Some("Hello!".to_string()));
        assert!(response.usage.is_none());
        assert_eq!(response.finish_reason, FinishReason::Stop);
    }

    #[test]
    fn test_completion_response_with_usage() {
        let msg = Message::assistant("Hello!");
        let usage = TokenUsage::new(100, 50);
        let response = CompletionResponse::with_usage(msg, usage);

        assert_eq!(response.message.role, "assistant");
        assert!(response.usage.is_some());
        assert_eq!(response.usage.unwrap().prompt_tokens, 100);
        assert_eq!(response.usage.unwrap().completion_tokens, 50);
        assert_eq!(response.finish_reason, FinishReason::Stop);
    }

    #[test]
    fn test_provider_capabilities_creation() {
        let mut caps = ProviderCapabilities {
            supports_model_listing: true,
            supports_model_details: true,
            supports_model_switching: false,
            supports_token_counts: true,
            supports_streaming: true,
            supports_vision: true,
        };

        assert!(caps.supports_model_listing);
        assert!(caps.supports_model_details);
        assert!(!caps.supports_model_switching);
        assert!(caps.supports_token_counts);
        assert!(caps.supports_streaming);

        caps.supports_model_switching = true;
        assert!(caps.supports_model_switching);
    }

    #[test]
    fn test_model_info_with_capabilities() {
        let model = ModelInfo::new("gpt-4", "GPT-4", 8192).with_capabilities(vec![
            ModelCapability::FunctionCalling,
            ModelCapability::Vision,
        ]);

        assert_eq!(model.capabilities.len(), 2);
        assert!(model.supports_capability(ModelCapability::FunctionCalling));
        assert!(model.supports_capability(ModelCapability::Vision));
        assert!(model.supports_tools);
        assert!(!model.supports_streaming);
    }

    #[test]
    fn test_model_info_summary_from_model_info() {
        let info = ModelInfo::new("gpt-4", "GPT-4", 8192);
        let summary = ModelInfoSummary::from_model_info(info);

        assert_eq!(summary.info.name, "gpt-4");
        assert_eq!(summary.info.display_name, "GPT-4");
        assert_eq!(summary.info.context_window, 8192);
        assert!(summary.state.is_none());
        assert!(summary.max_prompt_tokens.is_none());
        assert!(summary.max_completion_tokens.is_none());
        assert_eq!(summary.supports_tool_calls, Some(false));
        assert_eq!(summary.supports_vision, Some(false));
        assert_eq!(summary.raw_data, serde_json::Value::Null);
    }

    #[test]
    fn test_model_info_summary_new() {
        let info = ModelInfo::new("gpt-4", "GPT-4", 8192);
        let raw = serde_json::json!({"version": "2024-01"});
        let summary = ModelInfoSummary::new(
            info,
            Some("enabled".to_string()),
            Some(6144),
            Some(2048),
            Some(true),
            Some(true),
            raw.clone(),
        );

        assert_eq!(summary.info.name, "gpt-4");
        assert_eq!(summary.state, Some("enabled".to_string()));
        assert_eq!(summary.max_prompt_tokens, Some(6144));
        assert_eq!(summary.max_completion_tokens, Some(2048));
        assert_eq!(summary.supports_tool_calls, Some(true));
        assert_eq!(summary.supports_vision, Some(true));
        assert_eq!(summary.raw_data, raw);
    }

    #[test]
    fn test_model_info_summary_serialization() {
        let info = ModelInfo::new("gpt-4", "GPT-4", 8192);
        let summary = ModelInfoSummary::new(
            info,
            Some("enabled".to_string()),
            Some(6144),
            Some(2048),
            Some(true),
            Some(false),
            serde_json::json!({"test": "data"}),
        );

        let json = serde_json::to_string(&summary).unwrap();
        assert!(json.contains("\"name\":\"gpt-4\""));
        assert!(json.contains("\"state\":\"enabled\""));
        assert!(json.contains("\"max_prompt_tokens\":6144"));
        assert!(json.contains("\"supports_tool_calls\":true"));
        assert!(json.contains("\"supports_vision\":false"));
    }

    #[test]
    fn test_validate_message_sequence_drops_orphan_tool() {
        let messages = vec![
            Message::user("Do something"),
            Message::tool_result("call_123", "Result"),
        ];

        let validated = validate_message_sequence(&messages);

        assert_eq!(validated.len(), 1);
        assert_eq!(validated[0].role, "user");
    }

    #[test]
    fn test_validate_message_sequence_preserves_valid_pair() {
        let tool_call = ToolCall {
            id: "call_123".to_string(),
            function: FunctionCall {
                name: "test_func".to_string(),
                arguments: "{}".to_string(),
            },
        };

        let messages = vec![
            Message::user("Do something"),
            Message::assistant_with_tools(vec![tool_call]),
            Message::tool_result("call_123", "Result"),
        ];

        let validated = validate_message_sequence(&messages);

        assert_eq!(validated.len(), 3);
        assert_eq!(validated[0].role, "user");
        assert_eq!(validated[1].role, "assistant");
        assert_eq!(validated[2].role, "tool");
        assert_eq!(validated[2].tool_call_id, Some("call_123".to_string()));
    }

    #[test]
    fn test_validate_message_sequence_allows_user_and_system() {
        let messages = vec![
            Message::system("You are helpful"),
            Message::user("Question"),
            Message::assistant("Answer"),
        ];

        let validated = validate_message_sequence(&messages);

        assert_eq!(validated.len(), 3);
        assert_eq!(validated[0].role, "system");
        assert_eq!(validated[1].role, "user");
        assert_eq!(validated[2].role, "assistant");
    }

    #[test]
    fn test_validate_message_sequence_drops_tool_without_id() {
        let messages = vec![
            Message::user("Do something"),
            Message {
                role: "tool".to_string(),
                content: Some("Result".to_string()),
                content_parts: None,
                tool_calls: None,
                tool_call_id: None,
            },
        ];

        let validated = validate_message_sequence(&messages);

        assert_eq!(validated.len(), 1);
        assert_eq!(validated[0].role, "user");
    }

    #[test]
    fn test_model_info_summary_deserialization() {
        let json = r#"{
            "info": {
                "name": "gpt-4",
                "display_name": "GPT-4",
                "context_window": 8192,
                "capabilities": [],
                "provider_specific": {}
            },
            "state": "enabled",
            "max_prompt_tokens": 6144,
            "max_completion_tokens": 2048,
            "supports_tool_calls": true,
            "supports_vision": false,
            "raw_data": {"test": "data"}
        }"#;

        let summary: ModelInfoSummary = serde_json::from_str(json).unwrap();
        assert_eq!(summary.info.name, "gpt-4");
        assert_eq!(summary.state, Some("enabled".to_string()));
        assert_eq!(summary.max_prompt_tokens, Some(6144));
        assert_eq!(summary.max_completion_tokens, Some(2048));
        assert_eq!(summary.supports_tool_calls, Some(true));
        assert_eq!(summary.supports_vision, Some(false));
    }

    // -----------------------------------------------------------------------
    // convert_tools_from_json
    // -----------------------------------------------------------------------

    #[test]
    fn test_convert_tools_from_json_single_tool() {
        let tools = vec![serde_json::json!({
            "name": "read_file",
            "description": "Read a file from disk",
            "parameters": {"type": "object", "properties": {}}
        })];
        let converted = convert_tools_from_json(&tools);
        assert_eq!(converted.len(), 1);
        assert_eq!(converted[0].r#type, "function");
        assert_eq!(converted[0].function.name, "read_file");
        assert_eq!(converted[0].function.description, "Read a file from disk");
    }

    #[test]
    fn test_convert_tools_from_json_multiple_tools() {
        let tools = vec![
            serde_json::json!({
                "name": "read_file",
                "description": "Read a file",
                "parameters": {"type": "object"}
            }),
            serde_json::json!({
                "name": "write_file",
                "description": "Write a file",
                "parameters": {"type": "object"}
            }),
        ];
        let converted = convert_tools_from_json(&tools);
        assert_eq!(converted.len(), 2);
        assert_eq!(converted[0].function.name, "read_file");
        assert_eq!(converted[1].function.name, "write_file");
    }

    #[test]
    fn test_convert_tools_from_json_drops_missing_name() {
        let tools = vec![serde_json::json!({
            "description": "No name field",
            "parameters": {"type": "object"}
        })];
        let converted = convert_tools_from_json(&tools);
        assert!(converted.is_empty());
    }

    #[test]
    fn test_convert_tools_from_json_drops_missing_description() {
        let tools = vec![serde_json::json!({
            "name": "tool_without_description",
            "parameters": {"type": "object"}
        })];
        let converted = convert_tools_from_json(&tools);
        assert!(converted.is_empty());
    }

    #[test]
    fn test_convert_tools_from_json_drops_missing_parameters() {
        let tools = vec![serde_json::json!({
            "name": "tool_without_params",
            "description": "A tool"
        })];
        let converted = convert_tools_from_json(&tools);
        assert!(converted.is_empty());
    }

    #[test]
    fn test_convert_tools_from_json_empty_slice_returns_empty() {
        let converted = convert_tools_from_json(&[]);
        assert!(converted.is_empty());
    }

    #[test]
    fn test_convert_tools_from_json_skips_invalid_keeps_valid() {
        let tools = vec![
            serde_json::json!({"name": "good_tool", "description": "ok", "parameters": {}}),
            serde_json::json!({"description": "missing name"}),
            serde_json::json!({"name": "another_good", "description": "also ok", "parameters": {}}),
        ];
        let converted = convert_tools_from_json(&tools);
        assert_eq!(converted.len(), 2);
        assert_eq!(converted[0].function.name, "good_tool");
        assert_eq!(converted[1].function.name, "another_good");
    }

    // -----------------------------------------------------------------------
    // ProviderTool / ProviderFunction serialization
    // -----------------------------------------------------------------------

    #[test]
    fn test_provider_tool_serializes_to_expected_json() {
        let tool = ProviderTool {
            r#type: "function".to_string(),
            function: ProviderFunction {
                name: "search".to_string(),
                description: "Search the codebase".to_string(),
                parameters: serde_json::json!({"type": "object"}),
            },
        };
        let json = serde_json::to_value(&tool).unwrap();
        assert_eq!(json["type"], "function");
        assert_eq!(json["function"]["name"], "search");
        assert_eq!(json["function"]["description"], "Search the codebase");
    }

    #[test]
    fn test_provider_tool_round_trips_through_json() {
        let original = ProviderTool {
            r#type: "function".to_string(),
            function: ProviderFunction {
                name: "grep".to_string(),
                description: "Search files".to_string(),
                parameters: serde_json::json!({"type": "object", "properties": {"pattern": {"type": "string"}}}),
            },
        };
        let json = serde_json::to_string(&original).unwrap();
        let decoded: ProviderTool = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.function.name, "grep");
        assert_eq!(decoded.function.description, "Search files");
    }

    // -----------------------------------------------------------------------
    // ProviderFunctionCall
    // -----------------------------------------------------------------------

    #[test]
    fn test_provider_function_call_arguments_as_string_object() {
        let fc = ProviderFunctionCall {
            name: "read_file".to_string(),
            arguments: serde_json::json!({"path": "/tmp/foo.txt"}),
        };
        let s = fc.arguments_as_string();
        let reparsed: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(reparsed["path"], "/tmp/foo.txt");
    }

    #[test]
    fn test_provider_function_call_arguments_as_string_empty_object() {
        let fc = ProviderFunctionCall {
            name: "no_args".to_string(),
            arguments: serde_json::Value::Object(serde_json::Map::new()),
        };
        assert_eq!(fc.arguments_as_string(), "{}");
    }

    #[test]
    fn test_provider_function_call_default_arguments_is_null() {
        let json = r#"{"name": "tool"}"#;
        let fc: ProviderFunctionCall = serde_json::from_str(json).unwrap();
        assert_eq!(fc.name, "tool");
        assert_eq!(fc.arguments, serde_json::Value::Null);
    }

    // -----------------------------------------------------------------------
    // ProviderMessage serialization
    // -----------------------------------------------------------------------

    #[test]
    fn test_provider_message_omits_tool_call_id_when_none() {
        let msg = ProviderMessage {
            role: "user".to_string(),
            content: "Hello!".to_string(),
            content_parts: None,
            images: vec![],
            tool_calls: None,
            tool_call_id: None,
        };
        let json = serde_json::to_value(&msg).unwrap();
        assert!(!json.as_object().unwrap().contains_key("tool_call_id"));
        assert!(!json.as_object().unwrap().contains_key("tool_calls"));
        assert!(!json.as_object().unwrap().contains_key("images"));
    }

    #[test]
    fn test_provider_message_includes_tool_call_id_when_set() {
        let msg = ProviderMessage {
            role: "tool".to_string(),
            content: "result".to_string(),
            content_parts: None,
            images: vec![],
            tool_calls: None,
            tool_call_id: Some("call_xyz".to_string()),
        };
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["tool_call_id"], "call_xyz");
    }

    #[test]
    fn test_provider_message_from_message_for_ollama_sets_native_images() {
        let message = Message::try_user_from_multimodal_input(MultimodalPromptInput::new(vec![
            PromptInputPart::text("describe"),
            PromptInputPart::image(ImagePromptPart::inline_base64("image/png", "AAAA")),
        ]))
        .unwrap();

        let provider_message = ProviderMessage::from_message_for_ollama(&message);

        assert_eq!(provider_message.content, "describe");
        assert_eq!(provider_message.images, vec!["AAAA".to_string()]);
    }

    #[test]
    fn test_provider_message_deserializes_missing_tool_call_id_as_none() {
        let json = r#"{"role": "assistant", "content": "Hi"}"#;
        let msg: ProviderMessage = serde_json::from_str(json).unwrap();
        assert_eq!(msg.role, "assistant");
        assert!(msg.tool_call_id.is_none());
    }

    // -----------------------------------------------------------------------
    // ProviderToolCall default fields
    // -----------------------------------------------------------------------

    #[test]
    fn test_provider_tool_call_type_defaults_to_function() {
        let json = r#"{"function": {"name": "foo", "arguments": {}}}"#;
        let tc: ProviderToolCall = serde_json::from_str(json).unwrap();
        assert_eq!(tc.r#type, "function");
        assert_eq!(tc.id, "");
    }

    // -----------------------------------------------------------------------
    // ProviderRequest serialization
    // -----------------------------------------------------------------------

    #[test]
    fn test_provider_request_omits_empty_tools_array() {
        let req = ProviderRequest {
            model: "gpt-4o".to_string(),
            messages: vec![],
            tools: vec![],
            stream: false,
        };
        let json = serde_json::to_value(&req).unwrap();
        assert!(!json.as_object().unwrap().contains_key("tools"));
    }

    #[test]
    fn test_provider_request_includes_tools_when_non_empty() {
        let req = ProviderRequest {
            model: "gpt-4o".to_string(),
            messages: vec![],
            tools: vec![ProviderTool {
                r#type: "function".to_string(),
                function: ProviderFunction {
                    name: "search".to_string(),
                    description: "Search".to_string(),
                    parameters: serde_json::json!({}),
                },
            }],
            stream: false,
        };
        let json = serde_json::to_value(&req).unwrap();
        assert!(json.as_object().unwrap().contains_key("tools"));
        assert_eq!(json["tools"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn test_finish_reason_serialization_all_variants() {
        let cases = [
            (FinishReason::Stop, "\"stop\""),
            (FinishReason::Length, "\"length\""),
            (FinishReason::ToolCalls, "\"tool_calls\""),
            (FinishReason::ContentFilter, "\"content_filter\""),
            (FinishReason::Other, "\"other\""),
        ];
        for (reason, expected_json) in cases {
            let json = serde_json::to_string(&reason)
                // SAFETY: FinishReason is a simple enum with known-good Serialize impl
                .unwrap();
            assert_eq!(json, expected_json, "Wrong JSON for {:?}", reason);
            let back: FinishReason = serde_json::from_str(&json)
                // SAFETY: We just serialized this value; round-trip cannot fail
                .unwrap();
            assert_eq!(back, reason, "Round-trip failed for {:?}", reason);
        }
    }

    #[test]
    fn test_completion_response_with_finish_reason() {
        let response = CompletionResponse::new(Message::assistant("ok"))
            .with_finish_reason(FinishReason::ToolCalls);
        assert_eq!(response.finish_reason, FinishReason::ToolCalls);
    }

    #[test]
    fn test_completion_response_default_finish_reason_is_stop() {
        let r1 = CompletionResponse::new(Message::assistant("ok"));
        assert_eq!(r1.finish_reason, FinishReason::Stop);

        let usage = TokenUsage::new(10, 5);
        let r2 = CompletionResponse::with_usage(Message::assistant("ok"), usage);
        assert_eq!(r2.finish_reason, FinishReason::Stop);

        let r3 = CompletionResponse::with_model(Message::assistant("ok"), "gpt-4".to_string());
        assert_eq!(r3.finish_reason, FinishReason::Stop);
    }

    #[test]
    fn test_model_info_supports_tools_true_when_function_calling_added() {
        let mut model = ModelInfo::new("m", "m", 0);
        assert!(!model.supports_tools);
        model.add_capability(ModelCapability::FunctionCalling);
        assert!(model.supports_tools);
    }

    #[test]
    fn test_model_info_supports_streaming_true_when_streaming_added() {
        let mut model = ModelInfo::new("m", "m", 0);
        assert!(!model.supports_streaming);
        model.add_capability(ModelCapability::Streaming);
        assert!(model.supports_streaming);
    }

    #[test]
    fn test_model_info_with_capabilities_syncs_supports_tools() {
        let model = ModelInfo::new("m", "m", 0).with_capabilities(vec![
            ModelCapability::FunctionCalling,
            ModelCapability::Streaming,
        ]);
        assert!(model.supports_tools);
        assert!(model.supports_streaming);
    }

    #[test]
    fn test_model_info_with_capabilities_clears_supports_when_missing() {
        let mut model = ModelInfo::new("m", "m", 0);
        model.add_capability(ModelCapability::FunctionCalling);
        assert!(model.supports_tools);

        let model = model.with_capabilities(vec![ModelCapability::Vision]);
        assert!(!model.supports_tools);
        assert!(!model.supports_streaming);
    }

    #[test]
    fn test_model_info_summary_with_limits() {
        let info = ModelInfo::new("gpt-4", "GPT-4", 8192);
        let summary = ModelInfoSummary::from_model_info(info).with_limits(Some(6144), Some(2048));
        assert_eq!(summary.max_prompt_tokens, Some(6144));
        assert_eq!(summary.max_completion_tokens, Some(2048));
    }

    #[test]
    fn test_model_info_summary_with_limits_none() {
        let info = ModelInfo::new("gpt-4", "GPT-4", 8192);
        let summary = ModelInfoSummary::from_model_info(info).with_limits(None, None);
        assert!(summary.max_prompt_tokens.is_none());
        assert!(summary.max_completion_tokens.is_none());
    }

    #[allow(deprecated)]
    #[test]
    fn test_model_capability_code_generation_round_trips_json() {
        let cap = ModelCapability::CodeGeneration;
        let json = serde_json::to_string(&cap)
            // SAFETY: ModelCapability is a simple enum with known-good Serialize impl
            .unwrap();
        let back: ModelCapability = serde_json::from_str(&json)
            // SAFETY: We just serialized this value; round-trip cannot fail
            .unwrap();
        assert_eq!(back, ModelCapability::CodeGeneration);
    }
}

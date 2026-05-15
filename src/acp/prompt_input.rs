//! ACP multimodal prompt input conversion helpers.
//!
//! This module converts ACP stdio prompt content blocks into XZatoma's internal
//! multimodal prompt representation. It validates text and image input according
//! to the ACP stdio vision policy before provider execution.

use std::path::{Path, PathBuf};

use agent_client_protocol::schema as acp;
use base64::Engine;

use crate::config::AcpStdioConfig;
use crate::error::{Result, XzatomaError};
use crate::providers::{ImagePromptPart, MultimodalPromptInput, PromptInputPart};

/// Converts ACP prompt content blocks into internal multimodal prompt input.
///
/// The conversion preserves the order of ACP content blocks and validates image
/// input against the supplied ACP stdio configuration.
///
/// # Arguments
///
/// * `blocks` - ACP content blocks from a prompt request.
/// * `config` - ACP stdio configuration controlling image limits and policy.
/// * `workspace_root` - Workspace root used to resolve safe file references.
///
/// # Returns
///
/// Returns validated multimodal prompt input.
///
/// # Errors
///
/// Returns `XzatomaError::Provider` when the prompt is empty, contains
/// unsupported content, references an unsafe resource, exceeds configured
/// limits, or includes image input while vision is disabled.
///
/// # Examples
///
/// ```
/// use agent_client_protocol::schema::{ContentBlock, TextContent};
/// use xzatoma::acp::prompt_input::acp_content_blocks_to_prompt_input;
/// use xzatoma::config::AcpStdioConfig;
///
/// let blocks = vec![ContentBlock::Text(TextContent::new("hello"))];
/// let input = acp_content_blocks_to_prompt_input(
///     &blocks,
///     &AcpStdioConfig::default(),
///     std::path::Path::new("."),
/// )
/// .unwrap();
///
/// assert_eq!(input.as_legacy_text(), "hello");
/// ```
pub fn acp_content_blocks_to_prompt_input(
    blocks: &[acp::ContentBlock],
    config: &AcpStdioConfig,
    workspace_root: &Path,
) -> Result<MultimodalPromptInput> {
    let mut parts = Vec::with_capacity(blocks.len());

    for block in blocks {
        match block {
            acp::ContentBlock::Text(text) => {
                if !text.text.trim().is_empty() {
                    parts.push(PromptInputPart::text(text.text.clone()));
                }
            }
            acp::ContentBlock::Image(image) => {
                parts.push(PromptInputPart::image(convert_inline_image(
                    image,
                    config,
                    workspace_root,
                )?));
            }
            acp::ContentBlock::ResourceLink(resource) => {
                if is_image_mime_type(resource.mime_type.as_deref()) {
                    parts.push(PromptInputPart::image(convert_image_resource_link(
                        resource,
                        config,
                        workspace_root,
                    )?));
                } else {
                    // Directory mentions, file stubs from older Zed clients, and any
                    // non-image resource link: emit a text reference placeholder so the
                    // model knows a resource was referenced without failing the prompt.
                    let placeholder = format!("[Reference: {} ({})]", resource.name, resource.uri);
                    parts.push(PromptInputPart::text(placeholder));
                }
            }
            acp::ContentBlock::Resource(resource) => match &resource.resource {
                acp::EmbeddedResourceResource::TextResourceContents(text) => {
                    if is_image_mime_type(text.mime_type.as_deref()) {
                        // Image disguised as a text resource (rare); fall back to URI conversion.
                        parts.push(PromptInputPart::image(convert_embedded_resource(
                            resource,
                            config,
                            workspace_root,
                        )?));
                    } else {
                        // Inline file content, diagnostics, git diff, rules, etc.
                        let header = format!("\n[Context: {}]\n", text.uri);
                        let body = format!("{}{}", header, text.text);
                        if !body.trim().is_empty() {
                            parts.push(PromptInputPart::text(body));
                        }
                    }
                }
                acp::EmbeddedResourceResource::BlobResourceContents(_) => {
                    // Binary blob: always attempt image conversion.
                    parts.push(PromptInputPart::image(convert_embedded_resource(
                        resource,
                        config,
                        workspace_root,
                    )?));
                }
                _ => {
                    return Err(provider_error("unsupported embedded ACP resource variant"));
                }
            },
            acp::ContentBlock::Audio(audio) => {
                return Err(provider_error(format!(
                    "unsupported ACP audio content with MIME type '{}'",
                    audio.mime_type
                )));
            }
            _ => {
                return Err(provider_error("unsupported ACP content block"));
            }
        }
    }

    let input = MultimodalPromptInput::new(parts);
    input.validate().map_err(provider_error)?;
    Ok(input)
}

/// Returns whether a multimodal prompt input requires vision support.
///
/// # Arguments
///
/// * `input` - Multimodal prompt input to inspect.
///
/// # Returns
///
/// Returns `true` when the prompt contains one or more image parts.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::prompt_input::prompt_input_requires_vision;
/// use xzatoma::providers::MultimodalPromptInput;
///
/// assert!(!prompt_input_requires_vision(&MultimodalPromptInput::text("hello")));
/// ```
pub fn prompt_input_requires_vision(input: &MultimodalPromptInput) -> bool {
    input.has_images()
}

/// Validates that a provider/model can handle the supplied prompt input.
///
/// This helper is intentionally conservative. It allows text-only prompts for
/// every provider, allows vision for OpenAI-compatible models and known
/// vision-capable Copilot/Ollama model names, and returns a clear provider error
/// for image prompts that cannot be routed safely.
///
/// # Arguments
///
/// * `provider_name` - Configured provider name.
/// * `model_name` - Current model name.
/// * `input` - Prompt input to validate.
///
/// # Returns
///
/// Returns `Ok(())` when the provider/model can accept the prompt.
///
/// # Errors
///
/// Returns `XzatomaError::Provider` when the prompt contains images and the
/// provider/model combination is not known to support vision.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::prompt_input::validate_provider_supports_prompt_input;
/// use xzatoma::providers::MultimodalPromptInput;
///
/// let input = MultimodalPromptInput::text("hello");
/// assert!(validate_provider_supports_prompt_input("ollama", "llama3.2", &input).is_ok());
/// ```
pub fn validate_provider_supports_prompt_input(
    provider_name: &str,
    model_name: &str,
    input: &MultimodalPromptInput,
) -> Result<()> {
    if !input.has_images() {
        return Ok(());
    }

    if provider_model_supports_vision(provider_name, model_name) {
        Ok(())
    } else {
        Err(provider_error(format!(
            "provider '{}' with model '{}' does not support image input",
            provider_name, model_name
        )))
    }
}

/// Returns whether a provider/model combination is known to support vision.
///
/// # Arguments
///
/// * `provider_name` - Provider name such as `openai`, `copilot`, or `ollama`.
/// * `model_name` - Selected model identifier.
///
/// # Returns
///
/// Returns `true` for conservatively allowlisted vision-capable combinations.
pub fn provider_model_supports_vision(provider_name: &str, model_name: &str) -> bool {
    let provider = provider_name.to_ascii_lowercase();
    let model = model_name.to_ascii_lowercase();

    match provider.as_str() {
        "openai" => openai_model_supports_vision(&model),
        "copilot" => false,
        "ollama" => ollama_model_supports_vision(&model),
        _ => false,
    }
}

fn convert_inline_image(
    image: &acp::ImageContent,
    config: &AcpStdioConfig,
    workspace_root: &Path,
) -> Result<ImagePromptPart> {
    validate_vision_enabled(config)?;
    validate_image_mime_type(&image.mime_type, config)?;

    if image.data.trim().is_empty() {
        if let Some(uri) = &image.uri {
            return convert_image_uri(&image.mime_type, uri, None, config, workspace_root);
        }

        return Err(provider_error("image content is missing inline data"));
    }

    validate_base64_image_size(&image.data, config)?;
    let mut part = ImagePromptPart::inline_base64(image.mime_type.clone(), image.data.clone());
    part.name = image.uri.clone();
    part.validate().map_err(provider_error)?;
    Ok(part)
}

fn convert_image_resource_link(
    resource: &acp::ResourceLink,
    config: &AcpStdioConfig,
    workspace_root: &Path,
) -> Result<ImagePromptPart> {
    validate_vision_enabled(config)?;
    let mime_type = resource
        .mime_type
        .as_deref()
        .ok_or_else(|| provider_error("image resource link is missing MIME type"))?;
    validate_image_mime_type(mime_type, config)?;

    convert_image_uri(
        mime_type,
        &resource.uri,
        Some(resource.name.clone()),
        config,
        workspace_root,
    )
}

fn convert_embedded_resource(
    resource: &acp::EmbeddedResource,
    config: &AcpStdioConfig,
    workspace_root: &Path,
) -> Result<ImagePromptPart> {
    validate_vision_enabled(config)?;

    match &resource.resource {
        acp::EmbeddedResourceResource::BlobResourceContents(blob) => {
            let mime_type = blob
                .mime_type
                .as_deref()
                .ok_or_else(|| provider_error("embedded image resource is missing MIME type"))?;
            validate_image_mime_type(mime_type, config)?;
            validate_base64_image_size(&blob.blob, config)?;

            let mut part = ImagePromptPart::inline_base64(mime_type.to_string(), blob.blob.clone());
            part.name = Some(blob.uri.clone());
            part.validate().map_err(provider_error)?;
            Ok(part)
        }
        acp::EmbeddedResourceResource::TextResourceContents(text) => {
            let mime_type = text
                .mime_type
                .as_deref()
                .ok_or_else(|| provider_error("text resource is missing MIME type"))?;

            if !is_image_mime_type(Some(mime_type)) {
                return Err(provider_error(format!(
                    "unsupported embedded text resource '{}' with MIME type '{}'",
                    text.uri, mime_type
                )));
            }

            validate_image_mime_type(mime_type, config)?;
            convert_image_uri(mime_type, &text.uri, None, config, workspace_root)
        }
        _ => Err(provider_error("unsupported embedded ACP resource")),
    }
}

fn convert_image_uri(
    mime_type: &str,
    uri: &str,
    name: Option<String>,
    config: &AcpStdioConfig,
    workspace_root: &Path,
) -> Result<ImagePromptPart> {
    if uri.starts_with("file://") {
        if !config.allow_image_file_references {
            return Err(provider_error("image file references are disabled"));
        }

        let path = uri
            .strip_prefix("file://")
            .ok_or_else(|| provider_error("invalid file URI"))?;
        let resolved = resolve_safe_image_path(Path::new(path), workspace_root)?;
        let part = inline_image_part_from_file(mime_type, &resolved, name, config)?;
        return Ok(part);
    }

    if uri.starts_with("http://") || uri.starts_with("https://") {
        if !config.allow_remote_image_urls {
            return Err(provider_error("remote image URLs are disabled"));
        }

        let mut part = ImagePromptPart::remote_url(mime_type.to_string(), uri.to_string());
        part.name = name;
        part.validate().map_err(provider_error)?;
        return Ok(part);
    }

    if !config.allow_image_file_references {
        return Err(provider_error("image file references are disabled"));
    }

    let resolved = resolve_safe_image_path(Path::new(uri), workspace_root)?;
    inline_image_part_from_file(mime_type, &resolved, name, config)
}

fn validate_vision_enabled(config: &AcpStdioConfig) -> Result<()> {
    if config.vision_enabled {
        Ok(())
    } else {
        Err(provider_error("ACP stdio vision input is disabled"))
    }
}

fn validate_image_mime_type(mime_type: &str, config: &AcpStdioConfig) -> Result<()> {
    if mime_type.trim().is_empty() {
        return Err(provider_error("image content is missing MIME type"));
    }

    if !mime_type.starts_with("image/") {
        return Err(provider_error(format!(
            "unsupported image MIME type '{}'",
            mime_type
        )));
    }

    if config
        .allowed_image_mime_types
        .iter()
        .any(|allowed| allowed.eq_ignore_ascii_case(mime_type))
    {
        Ok(())
    } else {
        Err(provider_error(format!(
            "image MIME type '{}' is not allowed",
            mime_type
        )))
    }
}

fn validate_base64_image_size(data: &str, config: &AcpStdioConfig) -> Result<()> {
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|error| provider_error(format!("invalid base64 image data: {}", error)))?;

    if decoded.is_empty() {
        return Err(provider_error("image content decoded to zero bytes"));
    }

    if decoded.len() > config.max_image_bytes {
        return Err(provider_error(format!(
            "image size {} bytes exceeds configured limit of {} bytes",
            decoded.len(),
            config.max_image_bytes
        )));
    }

    Ok(())
}

fn inline_image_part_from_file(
    mime_type: &str,
    path: &Path,
    name: Option<String>,
    config: &AcpStdioConfig,
) -> Result<ImagePromptPart> {
    let metadata = std::fs::metadata(path).map_err(|error| {
        provider_error(format!(
            "failed to read image file '{}': {}",
            path.display(),
            error
        ))
    })?;

    if !metadata.is_file() {
        return Err(provider_error(format!(
            "image reference '{}' is not a file",
            path.display()
        )));
    }

    let len = metadata.len() as usize;
    if len == 0 {
        return Err(provider_error(format!(
            "image file '{}' is empty",
            path.display()
        )));
    }

    if len > config.max_image_bytes {
        return Err(provider_error(format!(
            "image file '{}' is {} bytes, exceeding configured limit of {} bytes",
            path.display(),
            len,
            config.max_image_bytes
        )));
    }

    let bytes = std::fs::read(path).map_err(|error| {
        provider_error(format!(
            "failed to read image file '{}': {}",
            path.display(),
            error
        ))
    })?;
    let data = base64::engine::general_purpose::STANDARD.encode(bytes);
    let mut part = ImagePromptPart::inline_base64(mime_type.to_string(), data);
    part.name = name.or_else(|| {
        path.file_name()
            .map(|file_name| file_name.to_string_lossy().to_string())
    });
    part.validate().map_err(provider_error)?;
    Ok(part)
}

fn resolve_safe_image_path(path: &Path, workspace_root: &Path) -> Result<PathBuf> {
    let joined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        workspace_root.join(path)
    };

    let canonical_workspace = workspace_root.canonicalize().map_err(|error| {
        provider_error(format!(
            "failed to resolve workspace root '{}': {}",
            workspace_root.display(),
            error
        ))
    })?;

    let canonical_path = joined.canonicalize().map_err(|error| {
        provider_error(format!(
            "failed to resolve image path '{}': {}",
            joined.display(),
            error
        ))
    })?;

    if !canonical_path.starts_with(&canonical_workspace) {
        return Err(provider_error(format!(
            "image path '{}' is outside workspace root '{}'",
            canonical_path.display(),
            canonical_workspace.display()
        )));
    }

    Ok(canonical_path)
}

fn is_image_mime_type(mime_type: Option<&str>) -> bool {
    mime_type
        .map(|value| value.to_ascii_lowercase().starts_with("image/"))
        .unwrap_or(false)
}

fn openai_model_supports_vision(model: &str) -> bool {
    model.contains("gpt-4o")
        || model.contains("gpt-4.1")
        || model.contains("gpt-4-turbo")
        || model.contains("vision")
        || model.contains("o3")
        || model.contains("o4")
}

fn ollama_model_supports_vision(model: &str) -> bool {
    model.contains("llava")
        || model.contains("bakllava")
        || model.contains("moondream")
        || model.contains("minicpm-v")
        || model.contains("gemma3")
        || model.contains("vision")
}

fn provider_error(message: impl Into<String>) -> XzatomaError {
    XzatomaError::Provider(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_client_protocol::schema::{
        BlobResourceContents, ContentBlock, EmbeddedResource, EmbeddedResourceResource,
        ImageContent, ResourceLink, TextContent, TextResourceContents,
    };
    use tempfile::TempDir;

    fn png_data() -> String {
        base64::engine::general_purpose::STANDARD.encode([137, 80, 78, 71])
    }

    #[test]
    fn test_text_only_prompt_conversion() {
        let blocks = vec![ContentBlock::Text(TextContent::new("hello"))];

        let input =
            acp_content_blocks_to_prompt_input(&blocks, &AcpStdioConfig::default(), Path::new("."))
                .unwrap();

        assert_eq!(input.as_legacy_text(), "hello");
        assert!(!input.has_images());
    }

    #[test]
    fn test_single_image_prompt_conversion() {
        let blocks = vec![ContentBlock::Image(ImageContent::new(
            png_data(),
            "image/png",
        ))];

        let input =
            acp_content_blocks_to_prompt_input(&blocks, &AcpStdioConfig::default(), Path::new("."))
                .unwrap();

        assert!(input.has_images());
        assert_eq!(input.parts.len(), 1);
    }

    #[test]
    fn test_mixed_text_and_image_prompt_conversion_preserves_order() {
        let blocks = vec![
            ContentBlock::Text(TextContent::new("before")),
            ContentBlock::Image(ImageContent::new(png_data(), "image/png")),
            ContentBlock::Text(TextContent::new("after")),
        ];

        let input =
            acp_content_blocks_to_prompt_input(&blocks, &AcpStdioConfig::default(), Path::new("."))
                .unwrap();

        assert_eq!(input.parts.len(), 3);
        assert!(matches!(input.parts[0], PromptInputPart::Text(_)));
        assert!(matches!(input.parts[1], PromptInputPart::Image(_)));
        assert!(matches!(input.parts[2], PromptInputPart::Text(_)));
    }

    #[test]
    fn test_unsupported_mime_type_rejected() {
        let blocks = vec![ContentBlock::Image(ImageContent::new(
            png_data(),
            "image/bmp",
        ))];

        let error =
            acp_content_blocks_to_prompt_input(&blocks, &AcpStdioConfig::default(), Path::new("."))
                .unwrap_err();

        assert!(error.to_string().contains("not allowed"));
    }

    #[test]
    fn test_oversized_image_rejected() {
        let config = AcpStdioConfig {
            max_image_bytes: 1,
            ..AcpStdioConfig::default()
        };
        let blocks = vec![ContentBlock::Image(ImageContent::new(
            png_data(),
            "image/png",
        ))];

        let error =
            acp_content_blocks_to_prompt_input(&blocks, &config, Path::new(".")).unwrap_err();

        assert!(error.to_string().contains("exceeds configured limit"));
    }

    #[test]
    fn test_missing_image_data_rejected() {
        let blocks = vec![ContentBlock::Image(ImageContent::new("", "image/png"))];

        let error =
            acp_content_blocks_to_prompt_input(&blocks, &AcpStdioConfig::default(), Path::new("."))
                .unwrap_err();

        assert!(error.to_string().contains("missing inline data"));
    }

    #[test]
    fn test_vision_disabled_rejected() {
        let config = AcpStdioConfig {
            vision_enabled: false,
            ..AcpStdioConfig::default()
        };
        let blocks = vec![ContentBlock::Image(ImageContent::new(
            png_data(),
            "image/png",
        ))];

        let error =
            acp_content_blocks_to_prompt_input(&blocks, &config, Path::new(".")).unwrap_err();

        assert!(error.to_string().contains("disabled"));
    }

    #[test]
    fn test_provider_without_vision_support_rejected() {
        let input = MultimodalPromptInput::new(vec![PromptInputPart::image(
            ImagePromptPart::inline_base64("image/png", png_data()),
        )]);

        let error =
            validate_provider_supports_prompt_input("ollama", "llama3.2", &input).unwrap_err();

        assert!(error.to_string().contains("does not support image input"));
    }

    #[test]
    fn test_openai_vision_model_allowed() {
        let input = MultimodalPromptInput::new(vec![PromptInputPart::image(
            ImagePromptPart::inline_base64("image/png", png_data()),
        )]);

        assert!(validate_provider_supports_prompt_input("openai", "gpt-4o", &input).is_ok());
    }

    #[test]
    fn test_file_resource_link_conversion() {
        let temp_dir = TempDir::new().unwrap();
        let image_path = temp_dir.path().join("image.png");
        std::fs::write(&image_path, [137, 80, 78, 71]).unwrap();

        let blocks = vec![ContentBlock::ResourceLink(
            ResourceLink::new("image.png", "image.png").mime_type("image/png"),
        )];

        let input = acp_content_blocks_to_prompt_input(
            &blocks,
            &AcpStdioConfig::default(),
            temp_dir.path(),
        )
        .unwrap();

        assert!(input.has_images());
    }

    #[test]
    fn test_embedded_blob_resource_conversion() {
        let blocks = vec![ContentBlock::Resource(EmbeddedResource::new(
            EmbeddedResourceResource::BlobResourceContents(
                BlobResourceContents::new(png_data(), "file:///image.png").mime_type("image/png"),
            ),
        ))];

        let input =
            acp_content_blocks_to_prompt_input(&blocks, &AcpStdioConfig::default(), Path::new("."))
                .unwrap();

        assert!(input.has_images());
    }

    #[test]
    fn test_text_resource_contents_converted_to_text_part() {
        let blocks = vec![ContentBlock::Resource(EmbeddedResource::new(
            EmbeddedResourceResource::TextResourceContents(
                TextResourceContents::new("fn main() {}", "file:///src/main.rs")
                    .mime_type("text/plain"),
            ),
        ))];

        let input =
            acp_content_blocks_to_prompt_input(&blocks, &AcpStdioConfig::default(), Path::new("."))
                .unwrap();

        assert!(!input.has_images());
        assert_eq!(input.parts.len(), 1);
        let text = input.as_legacy_text();
        assert!(
            text.contains("[Context: file:///src/main.rs]"),
            "expected URI header in text part, got: {text}"
        );
        assert!(
            text.contains("fn main() {}"),
            "expected resource text in text part, got: {text}"
        );
    }

    #[test]
    fn test_text_resource_contents_with_diagnostics_mime_type() {
        let blocks = vec![ContentBlock::Resource(EmbeddedResource::new(
            EmbeddedResourceResource::TextResourceContents(
                TextResourceContents::new("error: unused variable", "file:///src/main.rs")
                    .mime_type("text/x-diagnostics"),
            ),
        ))];

        let result =
            acp_content_blocks_to_prompt_input(&blocks, &AcpStdioConfig::default(), Path::new("."));

        assert!(
            result.is_ok(),
            "diagnostics MIME type should produce a text part, not an error"
        );
        assert!(!result.unwrap().has_images());
    }

    #[test]
    fn test_text_resource_contents_with_no_mime_type() {
        let blocks = vec![ContentBlock::Resource(EmbeddedResource::new(
            EmbeddedResourceResource::TextResourceContents(TextResourceContents::new(
                "some content",
                "file:///notes.txt",
            )),
        ))];

        let result =
            acp_content_blocks_to_prompt_input(&blocks, &AcpStdioConfig::default(), Path::new("."));

        assert!(
            result.is_ok(),
            "absent MIME type should be treated as non-image and produce a text part"
        );
        assert!(!result.unwrap().has_images());
    }

    #[test]
    fn test_blob_resource_with_image_mime_type_remains_image_path() {
        let blocks = vec![ContentBlock::Resource(EmbeddedResource::new(
            EmbeddedResourceResource::BlobResourceContents(
                BlobResourceContents::new(png_data(), "file:///image.png").mime_type("image/png"),
            ),
        ))];

        // The result may succeed (image part) or fail with the existing
        // image-validation error, but it must NOT produce an "unsupported
        // resource" error introduced by the new dispatch.
        let result =
            acp_content_blocks_to_prompt_input(&blocks, &AcpStdioConfig::default(), Path::new("."));

        match result {
            Ok(input) => assert!(input.has_images()),
            Err(e) => {
                let msg = e.to_string();
                assert!(
                    !msg.contains("unsupported embedded ACP resource variant"),
                    "blob resources must not produce the new unsupported-variant error: {msg}"
                );
            }
        }
    }

    #[test]
    fn test_non_image_resource_link_produces_placeholder() {
        let blocks = vec![ContentBlock::ResourceLink(
            ResourceLink::new("src/", "file:///src/").mime_type("inode/directory"),
        )];

        let input =
            acp_content_blocks_to_prompt_input(&blocks, &AcpStdioConfig::default(), Path::new("."))
                .unwrap();

        assert!(!input.has_images());
        let text = input.as_legacy_text();
        assert!(
            text.contains("src/"),
            "placeholder should contain the resource name, got: {text}"
        );
        assert!(
            text.contains("file:///src/"),
            "placeholder should contain the URI, got: {text}"
        );
    }

    #[test]
    fn test_non_image_resource_link_with_no_mime_type_produces_placeholder() {
        let blocks = vec![ContentBlock::ResourceLink(ResourceLink::new(
            "notes.txt",
            "file:///notes.txt",
        ))];

        let input =
            acp_content_blocks_to_prompt_input(&blocks, &AcpStdioConfig::default(), Path::new("."))
                .unwrap();

        assert!(!input.has_images());
        let text = input.as_legacy_text();
        assert!(
            text.contains("notes.txt"),
            "placeholder should contain the resource name, got: {text}"
        );
    }

    #[test]
    fn test_image_resource_link_still_routed_to_image_path() {
        let temp_dir = TempDir::new().unwrap();
        let image_path = temp_dir.path().join("image.png");
        std::fs::write(&image_path, [137u8, 80, 78, 71]).unwrap();

        let blocks = vec![ContentBlock::ResourceLink(
            ResourceLink::new("image.png", "image.png").mime_type("image/png"),
        )];

        let input = acp_content_blocks_to_prompt_input(
            &blocks,
            &AcpStdioConfig::default(),
            temp_dir.path(),
        )
        .unwrap();

        assert!(
            input.has_images(),
            "image resource links must still produce image parts"
        );
    }

    #[test]
    fn test_mixed_prompt_with_text_resource_and_plain_text() {
        let blocks = vec![
            ContentBlock::Text(TextContent::new("describe this")),
            ContentBlock::Resource(EmbeddedResource::new(
                EmbeddedResourceResource::TextResourceContents(
                    TextResourceContents::new("fn main() {}", "file:///src/main.rs")
                        .mime_type("text/plain"),
                ),
            )),
        ];

        let input =
            acp_content_blocks_to_prompt_input(&blocks, &AcpStdioConfig::default(), Path::new("."))
                .unwrap();

        assert_eq!(input.parts.len(), 2, "both blocks should produce parts");
        assert!(
            matches!(input.parts[0], PromptInputPart::Text(_)),
            "first part should be text"
        );
        assert!(
            matches!(input.parts[1], PromptInputPart::Text(_)),
            "second part should be text"
        );
    }
}

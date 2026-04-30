# Zed ACP Agent Phase 3 Implementation

## Overview

Phase 3 adds the text and vision input model used by the ACP stdio integration.
The implementation gives `xzatoma agent` a structured way to receive ACP prompt
content from Zed, validate image input, preserve ordered text and image blocks,
and route supported multimodal prompts to provider-specific request formats.

This phase focuses on the input model and validation boundary. It does not add
full streaming UI updates, durable session persistence, cancellation
notifications, or Zed IDE tool bridging. Those remain assigned to later phases.

## Implemented Capabilities

Phase 3 implements:

- Internal multimodal prompt input types.
- ACP content block conversion for text and image input.
- Image validation for MIME type, size, and safe local file resolution.
- ACP stdio vision configuration and environment overrides.
- Provider/model vision support validation.
- OpenAI-compatible multimodal request conversion.
- Ollama native image payload conversion.
- Conservative Copilot image rejection until native image serialization is
  implemented.
- Tests for text-only, image-only, mixed text/image, invalid image, and provider
  support cases.

## Internal Multimodal Prompt Types

The provider domain model now includes a small multimodal prompt abstraction:

- `MultimodalPromptInput`
- `PromptInputPart`
- `TextPromptPart`
- `ImagePromptPart`
- `ImagePromptSource`
- `ProviderMessageContentPart`
- `ProviderMessageContentParts`

The model preserves input order and supports:

- Plain UTF-8 text.
- Inline base64 images.
- Inline image bytes.
- Local file image references.
- Remote image URL references when policy allows them.

Text-only prompts continue to work through the existing `Message::user` flow.
Image prompts use `Message::try_user_from_multimodal_input` so provider request
builders can see ordered content parts before execution.

## ACP Content Conversion

A new ACP prompt conversion module converts ACP prompt blocks into
`MultimodalPromptInput`.

The conversion supports:

- `ContentBlock::Text`
- `ContentBlock::Image`
- Image `ResourceLink`
- Embedded image blob resources
- Multiple ordered blocks in a single prompt

The conversion rejects:

- Empty prompts.
- Unsupported audio content.
- Unsupported non-image binary resources.
- Image content with missing MIME type.
- Image content with invalid base64 data.
- Image content exceeding configured byte limits.
- Image MIME types not present in the configured allowlist.
- Local file references outside the workspace root.
- Remote URLs unless explicitly enabled.

Local file image references are resolved safely under the workspace root and
converted to inline base64 before provider request construction. This keeps
providers from receiving unresolved local filesystem paths.

## ACP Stdio Vision Configuration

ACP stdio configuration now includes a `stdio` section under `acp`:

- `vision_enabled`
- `max_image_bytes`
- `allowed_image_mime_types`
- `allow_image_file_references`
- `allow_remote_image_urls`

Defaults are conservative for local IDE use:

- Vision input is enabled.
- Maximum image size is 10 MiB.
- Allowed image MIME types are:
  - `image/png`
  - `image/jpeg`
  - `image/webp`
  - `image/gif`
- Local image file references are allowed.
- Remote image URLs are disabled.

Environment overrides were added using the ACP naming style:

- `XZATOMA_ACP_STDIO_VISION_ENABLED`
- `XZATOMA_ACP_STDIO_MAX_IMAGE_BYTES`
- `XZATOMA_ACP_STDIO_ALLOWED_IMAGE_MIME_TYPES`
- `XZATOMA_ACP_STDIO_ALLOW_IMAGE_FILE_REFERENCES`
- `XZATOMA_ACP_STDIO_ALLOW_REMOTE_IMAGE_URLS`

Configuration validation now rejects invalid ACP stdio vision policy, including
zero image limits, empty MIME type allowlists, blank MIME values, and non-image
MIME types in the image allowlist.

## Provider Vision Validation

Prompt execution now validates whether the selected provider and model can
handle image input before the request reaches the provider.

Text-only prompts are accepted for all providers.

Image prompts require a known supported provider/model combination:

- OpenAI-compatible providers are allowed for conservatively allowlisted vision
  model names such as `gpt-4o`, `gpt-4.1`, `gpt-4-turbo`, `o3`, `o4`, or names
  containing `vision`.
- Ollama is allowed for known local vision model families such as `llava`,
  `bakllava`, `moondream`, `minicpm-v`, `gemma3`, or names containing `vision`.
- Copilot image prompts are rejected for now because native Copilot image
  serialization is not implemented in this phase.

This prevents images from being silently converted to text descriptions or
dropped before provider execution.

## OpenAI-Compatible Provider Handling

The OpenAI-compatible provider now supports ordered multimodal user content in
request conversion.

For image-capable OpenAI models, provider messages containing multimodal content
are converted to OpenAI chat-completions content parts:

- Text parts become `text` parts.
- Inline image data becomes `image_url` parts using data URLs.
- Inline image bytes are base64-encoded into data URLs.
- Remote image URLs are passed through only if the ACP conversion policy allowed
  them.
- Unresolved local file paths are rejected.

The provider also validates image prompts against the selected model name before
constructing the request.

OpenAI model listing capability inference now annotates allowlisted models with
`ModelCapability::Vision`.

## Ollama Provider Handling

The Ollama provider now converts multimodal image prompts into Ollama's native
message format.

Ollama expects images as base64 payloads in an `images` field on messages, so
the provider conversion extracts image payloads from multimodal content parts.

For Ollama:

- Text parts are joined into the regular message content.
- Inline base64 images populate the native `images` field.
- Inline image bytes are base64-encoded into the native `images` field.
- Local file references are expected to have been resolved by the ACP conversion
  layer before provider execution.
- Remote URLs are not added to Ollama's native `images` field.

The Ollama provider rejects image prompts for model names that are not in the
conservative vision allowlist.

Ollama model capability inference now annotates allowlisted model names with
`ModelCapability::Vision`.

## Copilot Provider Handling

Copilot now detects image-bearing messages and fails clearly before execution.

This phase does not implement native Copilot image serialization. Because of
that, Copilot reports provider-level vision support as disabled and returns a
clear provider error when a prompt contains image content.

Text-only prompts, including text-only multimodal input, continue to use the
existing Copilot message conversion path.

## Agent Execution Changes

The agent core now has an execution entry point that accepts already-constructed
provider messages.

This lets ACP stdio preserve multimodal provider message metadata instead of
flattening all prompt input into plain text before execution.

The existing text-only execution path remains available and unchanged for normal
CLI usage.

## ACP Stdio Prompt Queue Integration

The ACP stdio prompt queue now converts `PromptRequest` content into
`MultimodalPromptInput` before enqueueing work.

For each prompt:

1. ACP content blocks are converted and validated against ACP stdio policy.
2. Provider/model vision support is validated.
3. The prompt input is converted into a provider-layer user message.
4. The queued prompt worker executes the provider message through the session's
   XZatoma agent.

This preserves ordered text and image prompt content for provider paths that can
consume it.

## Tests

Phase 3 adds coverage for the required conversion and validation cases.

Test coverage includes:

- Text-only prompt conversion.
- Single image prompt conversion.
- Mixed text and image prompt conversion preserving order.
- Unsupported MIME type rejection.
- Oversized image rejection.
- Missing image data rejection.
- Vision-disabled configuration rejection.
- Provider/model without vision support returning a clear error.
- OpenAI multimodal content conversion.
- OpenAI unresolved file-reference rejection.
- Ollama native image payload conversion.
- Ollama text-only multimodal conversion.
- ACP stdio vision configuration defaults and validation failures.

## Validation Performed

The following checks were run successfully during implementation:

- `cargo fmt --all`
- `cargo check --all-targets --all-features`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test prompt_input --all-features`
- `cargo test multimodal --all-features`
- `cargo test acp_stdio_config --all-features`

## Current Limitations

The implementation is intentionally conservative.

Known limitations:

- Copilot image serialization is not implemented yet.
- Remote image URLs remain disabled by default.
- Vision support is based on conservative model-name allowlists rather than
  provider API probing.
- Rich ACP streaming updates for image prompts are not part of this phase.
- Zed IDE tool bridge support remains part of a later phase.

## Success Criteria

Phase 3 satisfies the planned success criteria:

- Zed text prompts still work through the ACP stdio path.
- Zed image prompts are accepted by the ACP conversion layer when policy allows
  them.
- Vision-capable provider/model combinations receive image content in provider
  request structures.
- Non-vision provider/model combinations fail clearly instead of dropping image
  input.

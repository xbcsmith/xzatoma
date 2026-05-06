# Zed Session Mode Phase 1 Implementation

## Overview

Phase 1 of the Zed Session Mode plan modifies `src/acp/prompt_input.rs` to
handle the full range of ACP content block variants that the Zed editor emits
during a session mode conversation. Prior to this change, non-image
`ResourceLink` blocks and `TextResourceContents` embedded resources caused hard
errors that terminated prompt conversion. This made XZatoma incompatible with
Zed session mode prompts that include directory mentions, inline file content,
diagnostics, git diffs, and similar context attachments.

## Problem Statement

The Zed editor sends two categories of content that the previous implementation
rejected:

1. **Non-image resource links** - Directory mentions (`inode/directory`), file
   stubs from older Zed clients, and any resource link whose MIME type is not an
   image type. The old code returned a `Provider` error for every such block,
   causing the entire prompt to fail.

2. **Text embedded resources** - Inline file content, compiler diagnostics, git
   diffs, and editor rules are sent as `ContentBlock::Resource` with an inner
   `EmbeddedResourceResource::TextResourceContents` variant. The old code routed
   every `Resource` block unconditionally through `convert_embedded_resource`,
   which requires vision to be enabled and expects binary image data. This
   produced either a vision policy error or a MIME type error for every text
   resource.

## Changes

### Change 1: Non-image ResourceLink produces a text placeholder

**File**: `src/acp/prompt_input.rs`

**Function**: `acp_content_blocks_to_prompt_input`

**Before**: The `ContentBlock::ResourceLink` arm returned a hard `Provider`
error when the MIME type was not an image type.

**After**: Non-image resource links emit a text reference placeholder of the
form `[Reference: <name> (<uri>)]`. This informs the model that a resource was
referenced without failing prompt construction. Image resource links continue to
be routed through `convert_image_resource_link` as before.

The placeholder format was chosen to be unambiguous and self-describing so that
the model can reason about referenced resources even when their content is not
embedded in the prompt.

### Change 2: Resource arm dispatches on the inner resource variant

**File**: `src/acp/prompt_input.rs`

**Function**: `acp_content_blocks_to_prompt_input`

**Before**: Every `ContentBlock::Resource` block was sent to
`convert_embedded_resource` regardless of the inner variant. That function
requires vision to be enabled and only handles binary blobs and image URIs.

**After**: The `Resource` arm now matches on `resource.resource`:

- `TextResourceContents` with a non-image MIME type (or no MIME type): formatted
  as an inline text part with a URI header line: `\n[Context: <uri>]\n<text>`.
  Empty bodies are suppressed.
- `TextResourceContents` with an image MIME type: rare case where an image URI
  is delivered via a text resource; routed to `convert_embedded_resource` for
  backward compatibility.
- `BlobResourceContents`: binary blob; always routed to
  `convert_embedded_resource` for image conversion.
- Any other variant: returns an `unsupported embedded ACP resource variant`
  error.

The context header format `[Context: <uri>]` mirrors the placeholder style used
for resource links, giving the model consistent cues about the source of
injected content.

## Test Coverage

Eight new unit tests were added to `mod tests` in `src/acp/prompt_input.rs`. All
existing tests continue to pass unchanged.

| Test name                                                             | What it verifies                                                                                                              |
| --------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------- |
| `test_text_resource_contents_converted_to_text_part`                  | `TextResourceContents` with `text/plain` MIME type produces a text part containing the URI header and the resource text.      |
| `test_text_resource_contents_with_diagnostics_mime_type`              | A custom MIME type such as `text/x-diagnostics` is treated as non-image and produces a text part rather than an error.        |
| `test_text_resource_contents_with_no_mime_type`                       | Absent MIME type defaults to the non-image path and produces a text part.                                                     |
| `test_blob_resource_with_image_mime_type_remains_image_path`          | A `BlobResourceContents` block is still routed through image conversion and never produces the new unsupported-variant error. |
| `test_non_image_resource_link_produces_placeholder`                   | A resource link with `inode/directory` MIME type produces a text placeholder containing the resource name and URI.            |
| `test_non_image_resource_link_with_no_mime_type_produces_placeholder` | A resource link with no MIME type produces a text placeholder rather than an error.                                           |
| `test_image_resource_link_still_routed_to_image_path`                 | A resource link with `image/png` MIME type still produces an image part.                                                      |
| `test_mixed_prompt_with_text_resource_and_plain_text`                 | A prompt combining a plain text block and a `TextResourceContents` block produces two text parts in order.                    |

## Behavioral Invariants Preserved

- All existing image conversion paths (`ContentBlock::Image`, image-typed
  `ContentBlock::ResourceLink`, blob `ContentBlock::Resource`) are unchanged.
- Vision policy enforcement (`validate_vision_enabled`) is only invoked for
  blocks that produce image parts.
- The `ContentBlock::Audio` arm and the catch-all `_` arm continue to return
  hard errors; only resource-type blocks gained the non-error path.
- Prompt validation (`input.validate()`) runs after all blocks are converted,
  preserving the empty-prompt check.

## Design Rationale

The guiding principle is that a content block carrying text should never prevent
prompt delivery. Zed injects context blocks opportunistically; the model
degrades gracefully when context is summarized as a placeholder rather than
failing entirely. Binary blobs with unknown types continue to fail so that
corrupted or unexpected data does not reach the model silently.

The implementation deliberately avoids changing `convert_embedded_resource` to
keep that function's contract narrow and auditable. The dispatch logic lives
entirely in `acp_content_blocks_to_prompt_input`, which is the correct boundary
for routing decisions.

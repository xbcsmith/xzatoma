# Zed ACP Agent Phase 4 Implementation

## Overview

Phase 4 adds persistence and resume support for ACP stdio sessions used by Zed,
along with session model advertisement. The implementation keeps stdio session
state separate from the HTTP ACP runtime tables because the two transports have
different lifecycle semantics.

The goal is for a Zed-launched `xzatoma agent` subprocess to reconnect to a
workspace, find the most recent stdio session mapping for that workspace, and
rehydrate the associated XZatoma conversation when possible. If resume data is
unavailable or invalid, session creation still succeeds with a fresh
conversation.

## Storage Schema

A dedicated `acp_stdio_sessions` table stores stdio session mappings. It is
created alongside the existing conversation and ACP runtime tables.

The table records:

- `session_id` as the ACP stdio session identifier.
- `workspace_root` as the normalized workspace path used for resume lookup.
- `conversation_id` as the associated XZatoma conversation ID.
- `provider_type` as the provider selected for the session.
- `model` as the selected model when available.
- `created_at` and `updated_at` timestamps in RFC 3339 format.
- `metadata_json` for small transport metadata.

Indexes on `workspace_root` and `updated_at` support efficient workspace resume
and pruning.

## Storage API

`SqliteStorage` now includes focused ACP stdio session methods:

- Save or update an ACP stdio session mapping.
- Load the most recent ACP stdio session for a workspace root.
- Load an ACP stdio session by session ID.
- Update last activity for a stdio session.
- Prune stale stdio session mappings older than a configured cutoff.

The public storage type `PublicStoredAcpStdioSession` exposes the persisted
mapping structure for tests and future callers.

## Session Resume

During `NewSessionRequest` handling, the stdio server normalizes the workspace
root and checks whether session persistence and workspace resume are enabled.

If a previous mapping exists, the server attempts to load the mapped
conversation from the existing `conversations` table and reconstructs the agent
with `Conversation::with_history`. This preserves prior user and assistant
messages for the workspace.

Resume failures are non-fatal. Missing conversations, invalid conversation IDs,
or storage errors are logged and session creation continues with a new
conversation. This keeps Zed startup reliable even when the local history
database is stale or partially corrupted.

## Conversation Checkpointing

ACP stdio sessions now persist conversation checkpoints through
`SqliteStorage::save_conversation`.

A checkpoint is saved when the session is first created so the stdio session
mapping always points at an existing conversation record. After successful
prompt execution, the prompt worker saves the updated conversation.

When the conversation title is still the default `New Conversation`, the first
user prompt is used as a safe title and truncated to a short display-friendly
string. Resumed conversations keep their existing title.

Cancelled prompts do not write checkpoints after cancellation. Failed prompt
execution also avoids writing a new checkpoint unless the implementation can
preserve a coherent message sequence.

## Model Advertisement

Session creation advertises ACP session model state using the unstable ACP
session model feature.

The model advertisement includes:

- The current model ID.
- A list of available models when model listing succeeds.
- A fallback entry for the current configured model when listing is unavailable,
  skipped, times out, or fails.

XZatoma model metadata is mapped into ACP model metadata, including:

- Context window.
- Tool support.
- Vision support.
- Streaming support.
- Provider-specific metadata.

Copilot model listing is skipped during stdio session creation to avoid blocking
on keyring access during editor startup. Hosted OpenAI model listing is skipped
when no API key is configured. Other providers are queried with the configured
timeout and fall back safely on errors.

## ACP Stdio Configuration

The `acp.stdio` configuration now includes persistence, resume, queue, timeout,
and input policy fields:

- `persist_sessions`, default `true`.
- `resume_by_workspace`, default `true`.
- `max_active_sessions`, default `32`.
- `session_timeout_seconds`, default `3600`.
- `prompt_queue_capacity`, default `16`.
- `model_list_timeout_seconds`, default `5`.
- `vision_enabled`, default `true`.
- `max_image_bytes`, default `10 MiB`.
- `allowed_image_mime_types`, default common web image MIME types.
- `allow_image_file_references`, default `true`.
- `allow_remote_image_urls`, default `false`.

Environment overrides are supported for the Phase 4 stdio settings:

- `XZATOMA_ACP_STDIO_PERSIST_SESSIONS`
- `XZATOMA_ACP_STDIO_RESUME_BY_WORKSPACE`
- `XZATOMA_ACP_STDIO_MAX_ACTIVE_SESSIONS`
- `XZATOMA_ACP_STDIO_SESSION_TIMEOUT_SECONDS`
- `XZATOMA_ACP_STDIO_PROMPT_QUEUE_CAPACITY`
- `XZATOMA_ACP_STDIO_MODEL_LIST_TIMEOUT_SECONDS`
- `XZATOMA_ACP_STDIO_VISION_ENABLED`
- `XZATOMA_ACP_STDIO_MAX_IMAGE_BYTES`
- `XZATOMA_ACP_STDIO_ALLOWED_IMAGE_MIME_TYPES`

Existing image file and remote URL policy overrides remain supported.

## Queue Capacity

Prompt queue capacity is now controlled by `acp.stdio.prompt_queue_capacity`.
Prompt enqueue uses a non-blocking send so a full queue returns a clear protocol
error instead of waiting indefinitely.

The error includes the configured queue capacity, which should make overloaded
session behavior easier to diagnose from Zed.

## Testing

Phase 4 adds coverage for:

- `acp_stdio_sessions` schema creation.
- `acp_stdio_sessions` indexes.
- Saving and loading a mapping by workspace root.
- Updating an existing session mapping.
- Loading the most recent mapping for a workspace.
- Persisting last activity updates.
- Pruning stale stdio mappings.
- Session creation persisting a mapping.
- Workspace resume rehydrating conversation history.
- Missing conversation fallback continuing session creation.
- Conversation checkpoint persistence.
- Model listing fallback returning a successful `NewSessionResponse`.
- Queue capacity errors being descriptive.
- Configuration defaults, validation, and environment overrides.

## Result

With Phase 4 in place, ACP stdio sessions can survive Zed restarts through
workspace-scoped resume, while still favoring reliability when storage or model
listing is unavailable. Zed receives model state during session creation, and
prompt execution checkpoints keep the mapped XZatoma conversation up to date.

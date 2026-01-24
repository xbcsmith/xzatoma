# Chat mode: /auth command implementation

## Overview

This document describes the implementation of the interactive `/auth` special command in chat mode. The command allows users to start the authentication flow for a provider directly from an interactive chat session, either for the current session provider or for a provider explicitly given as an argument:

- `/auth` — starts authentication for the session's current provider
- `/auth <provider>` — starts authentication for the named provider (e.g., `copilot`, `ollama`)

The design goal is to provide a simple, discoverable, and safe way to trigger authentication while keeping the chat session active.

## Components Delivered

- `src/commands/special_commands.rs` — added `SpecialCommand::Auth(Option<String>)`, parsing for `/auth [provider]`, and help text.
- `src/commands/mod.rs` — chat loop handling to execute the auth flow and surface user-facing output without exiting the chat session.
- Unit tests in `src/commands/special_commands.rs` for parsing `/auth` with and without provider argument.
- Help text updated to document the new command.

## Implementation Details

Parsing
- The special command parser recognizes `/auth` and `/auth <provider>` and maps them to `SpecialCommand::Auth`.
- Parsing is case-insensitive and accepts an optional provider argument.

Reference (parsing snippet):
```xzatoma/src/commands/special_commands.rs#L120-152
"/auth" => SpecialCommand::Auth(None),
input if input.starts_with("/auth ") => {
  let rest = input[6..].trim();
  if !rest.is_empty() {
    SpecialCommand::Auth(Some(rest.to_string()))
  } else {
    SpecialCommand::None
  }
}
```

Chat-mode execution
- When `/auth` is entered in chat mode, the interactive loop performs:
 - Resolve which provider to authenticate: explicit argument or current session provider.
 - Print a short, clear message that the authentication flow is starting.
 - Call the existing auth helper (`auth::authenticate(config.clone(), provider)`) which:
  - For `copilot` executes the OAuth device flow (prints verification URI + code and polls until success).
  - For `ollama` prints instructions since it typically uses a local server.
 - The chat loop catches and prints errors but does not exit the session on failure.

Reference (chat loop handling snippet):
```xzatoma/src/commands/mod.rs#L161-188
SpecialCommand::Auth(provider_opt) => {
  let provider_to_auth = provider_opt.unwrap_or_else(|| provider_type.to_string());
  println!("Starting authentication for provider: {}", provider_to_auth);
  match auth::authenticate(config.clone(), provider_to_auth).await {
    Ok(_) => println!("Authentication completed."),
    Err(e) => eprintln!("Authentication failed: {}", e),
  }
  continue;
}
```

Notes:
- `config.clone()` is passed to the auth helper so the chat loop retains its own `Config` instance; this avoids moving `config` out of the running chat session.
- The auth helper remains provider-specific; it handles device flow interactions itself, providing clear console output for users.
- Authentication is done synchronously (awaited) inside the chat loop — which is appropriate because device flow is interactive and the user expects to see the verification instructions immediately. If desired, we can later make it run in a background task and notify the user once complete.

## Testing

Unit tests
- Parser tests added for:
 - `/auth` → `SpecialCommand::Auth(None)`
 - `/auth copilot` → `SpecialCommand::Auth(Some("copilot"))`

Manual validation
1. Start interactive chat mode:
  - `xzatoma chat` (or whatever your normal entrypoint is)
2. Run `/auth`
  - The CLI prints an instruction message indicating the provider that will be authenticated.
3. For Copilot, follow the device flow:
  - Visit the printed verification URI and enter the user code.
  - Authentication should succeed and the keyring entry will be updated.
4. Confirm the chat session remains active and usable after authentication.

Automated tests
- New parsing tests run as part of `cargo test` (unit tests). Integration/e2e tests that mock the provider device flow are a future enhancement (recommended, especially for Copilot).

## Usage Examples

Simple auth of the current provider
```/dev/null/example.md#L1-6
> /auth
Starting authentication for provider: copilot (this may prompt you to visit a URL and enter a code)
Copilot: initiating device flow (you will be prompted to visit a URL and enter a code)...
(...) follow the device flow steps in your browser ...
Authentication completed.
> # (Chat prompt continues)
```

Auth a specific provider from chat
```/dev/null/example.md#L1-4
> /auth ollama
Starting authentication for provider: ollama
Ollama: typically uses a local server; ensure your `provider.ollama` config is set.
```

## Edge Cases and UX Considerations

- Unknown provider: the underlying auth helper reports back a helpful error and the chat session continues.
- Repeated calls: calling `/auth` multiple times is allowed; the underlying provider handles idempotency or reports appropriate errors.
- Non-interactive sessions: since device flow is interactive, `/auth` in a purely non-interactive environment will surface an error message. Consider adding an explicit TTY check or a future flag `--background` for different behavior.

## Future improvements

- Add an option to run the interactive device flow in a background task (and notify when complete).
- Add an integration test that mocks Copilot's token exchange and device flows so CI can validate the full interactive path in a deterministic fashion.
- Add telemetry/logging for usage of `/auth` to help track auth failures and recovery rates.

## References

- Parser changes: `src/commands/special_commands.rs` (parse `/auth`).
```xzatoma/src/commands/special_commands.rs#L120-152
"/auth" => SpecialCommand::Auth(None),
input if input.starts_with("/auth ") => { ... }
```

- Chat execution: `src/commands/mod.rs` (handling `SpecialCommand::Auth` in `run_chat`).
```xzatoma/src/commands/mod.rs#L161-188
SpecialCommand::Auth(provider_opt) => { ... }
```

---

If you want, I can:
- Add an end-to-end mocked integration test for the Copilot device flow to exercise `/auth` in CI.
- Make the command run in a background task (non-blocking) and notify the user when it finishes.
- Add a short help blurb in top-level README or the chat help output showing `/auth` usage.

# CLI Subcommand Flag Order Implementation

## Summary

XZatoma's top-level CLI options now work when placed after the selected
subcommand. Users can write commands like:

```bash
xzatoma watch --config config.yaml
xzatoma auth --storage-path /tmp/xzatoma.db
xzatoma auth --provider copilot --verbose
```

instead of having to put global flags before the subcommand.

## What changed

The `Cli` parser in `src/cli.rs` marks the shared top-level options as global:

- `--config`
- `--verbose`
- `--storage-path`

That allows clap to recognize those flags anywhere in the command line, while
still storing the parsed values on the same `Cli` struct fields used by the rest
of the application.

## Why this approach

This keeps the implementation small and matches clap's intended model for flags
that should be available across all subcommands. It also avoids changing the
command execution flow in `src/main.rs`, because the rest of the application can
continue reading `cli.config`, `cli.verbose`, and `cli.storage_path` exactly as
before.

## Validation

The CLI test coverage in `src/cli.rs` was updated to prove the new order works
for:

- `watch --config ...`
- `auth --storage-path ...`
- `auth --verbose`

# Repository Guidelines

## Project Direction

`yd` is a personal terminal multitool. Wallet is only the first module, not the product boundary. Keep every new feature isolated by domain so modules can be added without changing existing behavior. Root output must stay compact and useful.

This is a real CLI project, not a prototype. Do not add placeholder logic, demo shortcuts, prefix-only validation, fake parsing, or test-only implementations. If a domain has a known standard, checksum, parser, encoding, protocol, or typed crate support, use it. Tests must prove the real behavior, not just that output "looks like" the expected value.

## Project Structure & Module Organization

- `src/main.rs` initializes tracing/errors and starts the app.
- `src/app.rs` routes parsed CLI input to modules.
- `src/cli.rs` owns Clap arguments, usage strings, and help behavior.
- `src/commands.rs` is the single registry for module aliases, canonical commands, summaries, root help, and argument normalization.
- `src/ui.rs` owns terminal roles, colors, dividers, and shared output patterns.
- Domain code lives under `src/<domain>/`; wallet currently uses `model.rs`, `service.rs`, `crypto.rs`, `provider.rs`, `store.rs`, and `mod.rs`.
- `install.sh` is the public Linux installer/updater. `.github/workflows/ci.yml` owns checks, version bumps, release builds, checksums, and provenance.

For new modules, create a domain directory instead of growing root files. Prefer this shape:

```text
src/<domain>/
  mod.rs       # module facade and orchestration
  model.rs     # typed domain data
  service.rs   # user-facing actions
  store.rs     # persistence, if needed
  provider.rs  # network or external integrations, if needed
```

## Build, Test, and Development Commands

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo run -- -h
cargo run -- -w -h
cargo build --release
```

Use `cargo run -- -w` only when wallet behavior must be checked; it may read local encrypted wallet storage.

## Coding Style & Naming Conventions

Use typed Rust APIs and typed UI roles. Do not add raw ANSI escape codes or ad-hoc colors outside `ui.rs`. Keep CLI output short: values first, no explanatory paragraphs. Register new modules in `commands.rs`; do not duplicate root help text or alias handling in multiple files. Prefer domain names such as `notes`, `wallet`, or `sync` over generic utility buckets.

Avoid stringly typed plumbing. Use enums, structs, traits, and small service/provider objects so a new feature can be added without changing existing modules beyond the registry and app routing. `expect` is acceptable only for static invariants controlled by the codebase, such as hard-coded derivation paths; user input, network responses, storage data, and provider data must return typed errors.

## Module & CLI Rules

Root help should show only global options and module aliases. Module-specific flags belong in that module help, for example `yd -w -h`. Short aliases such as `-w` or future `-n` must normalize through `commands.rs`. Destructive actions need confirmation plus an explicit non-interactive flag only when useful for scripts.

Each command should map to one module. Do not leak module-specific options into root help. Adding a module normally means adding a `ModuleSpec`, a Clap command/args type, an `app.rs` route, and a `src/<domain>/` implementation.

## Storage & Security

Never persist seed phrases, private keys, or encryption keys in plaintext. Keep encrypted payloads in SQLite and key material in the system keyring. Future private notes should use the same pattern. Network providers must be async; one provider failure must not prevent other balances or values from rendering.

## Testing Guidelines

Add focused tests when changing derivation paths, address formats, storage format, command normalization, help behavior, or provider fallback logic. Existing tests live beside the relevant module in `#[cfg(test)]` blocks.

Tests must use real validation rules. For example, crypto address tests should verify parsers, network checks, checksums, and known invalid cases; they should not only check prefixes, lengths, or display text. Add negative tests for malformed input whenever a validator is introduced.

## Commit & Release Guidelines

Use short imperative commits, for example `Add resilient USD quote providers` or `Remove techtask document`. Commit locally when work is complete, but do not push unless the user explicitly asks for a push. When a push to `main` is requested, CI bumps the patch version, commits `chore(release): vX.Y.Z`, tags it, builds native Linux, `tar.gz`, `tar.xz`, and Debian assets, then publishes checksums and provenance. Manual `v*` tags are reserved for exceptional releases.

Before committing, run the relevant local checks. For Rust code, use `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`. If CI is changed, reason through the release path before committing so the user does not need multiple fix pushes.

## Installer Rules

`install.sh` must remain POSIX `sh` compatible. It installs the latest GitHub release to `~/.local/bin` by default, checks the installed version, skips identical versions unless `YD_FORCE=1`, verifies `SHA256SUMS`, and updates Bash/Zsh PATH only when needed.

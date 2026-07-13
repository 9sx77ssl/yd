# Repository Guidelines

## Project Direction

`yd` is a personal terminal multitool. Wallet is only the first module, not the product boundary. Keep every new feature isolated by domain so modules can be added without changing existing behavior. Root output must stay compact and useful.

## Project Structure & Module Organization

- `src/main.rs` initializes tracing/errors and starts the app.
- `src/app.rs` routes parsed CLI input to modules.
- `src/cli.rs` owns Clap arguments, usage strings, and help behavior.
- `src/commands.rs` is the single registry for module aliases, canonical commands, summaries, root help, and argument normalization.
- `src/ui.rs` owns terminal roles, colors, dividers, and shared output patterns.
- Domain code lives under `src/<domain>/`; wallet currently uses `crypto.rs`, `provider.rs`, `store.rs`, and `mod.rs`.
- `install.sh` is the public Linux installer/updater. `.github/workflows/ci.yml` owns checks, version bumps, release builds, checksums, and provenance.

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

## Module & CLI Rules

Root help should show only global options and module aliases. Module-specific flags belong in that module help, for example `yd -w -h`. Short aliases such as `-w` or future `-n` must normalize through `commands.rs`. Destructive actions need confirmation plus an explicit non-interactive flag only when useful for scripts.

## Storage & Security

Never persist seed phrases, private keys, or encryption keys in plaintext. Keep encrypted payloads in SQLite and key material in the system keyring. Future private notes should use the same pattern. Network providers must be async; one provider failure must not prevent other balances or values from rendering.

## Testing Guidelines

Add focused tests when changing derivation paths, address formats, storage format, command normalization, help behavior, or provider fallback logic. Existing tests live beside the relevant module in `#[cfg(test)]` blocks.

## Commit & Release Guidelines

Use short imperative commits, for example `Add resilient USD quote providers` or `Remove techtask document`. Push to `main` for normal releases: CI bumps the patch version, commits `chore(release): vX.Y.Z`, tags it, builds native Linux, `tar.gz`, `tar.xz`, and Debian assets, then publishes checksums and provenance. Manual `v*` tags are reserved for exceptional releases.

## Installer Rules

`install.sh` must remain POSIX `sh` compatible. It installs the latest GitHub release to `~/.local/bin` by default, checks the installed version, skips identical versions unless `YD_FORCE=1`, verifies `SHA256SUMS`, and updates Bash/Zsh PATH only when needed.

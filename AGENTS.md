# Repository Guidelines

## Project Direction

`yd` is a personal terminal multitool. Wallet is only the first module, not the product boundary. Keep every new feature isolated by domain so modules can be added without changing existing behavior. Root output must stay compact and useful.

This is a real CLI project, not a prototype. Do not add placeholder logic, demo shortcuts, prefix-only validation, fake parsing, or test-only implementations. If a domain has a known standard, checksum, parser, encoding, protocol, or typed crate support, use it. Tests must prove the real behavior, not just that output "looks like" the expected value.

## Layered Architecture

The codebase is split into three reusable infrastructure layers and one or more domain layers on top. **Build on the infrastructure layers; do not reimplement them.** A new domain composes `net`, `store`, `secret`, and `ui` instead of opening its own HTTP client, SQL connection, cipher, or color path.

```text
src/
  main.rs        # bootstrap (color_eyre + tracing), exit handling
  app.rs         # routes a typed Command to a domain service; bare-`yd` screen
  cli.rs         # Clap args + typed action enums (one Command variant per module)
  commands.rs    # single module registry: aliases, summaries, root help, normalization
  error.rs       # YdError: typed variants for mnemonic/secret/network/storage failures
  ui.rs          # Tone enum + Ui helpers: the ONLY place colors/ANSI may live
  net/           # shared HTTP layer
    mod.rs       #   facade
    client.rs    #   shared_client(), ApiService (typed reqwest → YdError)
    fallback.rs  #   with_fallback() / with_fallback_or_none() (resilient queries)
    price.rs     #   PriceService + Asset (cached USD quotes with primary/fallback)
  store/         # shared SQLite layer
    mod.rs       #   facade
    database.rs  #   Database (single yd.sqlite), PRAGMA profile, versioned migrations
    cache.rs     #   TtlCache (expiring key→value for short-lived public data)
  secret/        # shared encrypted-secret layer
    mod.rs       #   facade: SecretStore (load/save/remove any secret blob)
    crypto.rs    #   AES-256-GCM encrypt/decrypt
    keyring.rs   #   system keyring key get/create/delete
  wallet/        # domain
    mod.rs       #   facade
    model.rs     #   NetworkKind, EvmNetworkConfig, UtxoNetworkConfig, SolanaNetworkConfig, PortfolioEntry
    address.rs   #   WalletKeys (BIP-39 → addresses), AddressValidator
    evm.rs       #   EvmProvider (ONE type serves every EVM chain)
    chain.rs     #   UtxoProvider (ONE type serves every UTXO chain)
    solana.rs    #   SolanaProvider (SLIP-0010 derivation, auto-detect active address)
    provider.rs  #   NetworkProvider trait + wallet_providers() factory
    service.rs   #   WalletService (show_portfolio/show_paths/reset)
    store.rs     #   WalletStore: thin wrapper over SecretStore + domain migrations
```

For new modules, create a domain directory instead of growing root files. Prefer this shape:

```text
src/<domain>/
  mod.rs       # module facade and orchestration
  model.rs     # typed domain data
  service.rs   # user-facing actions
  store.rs     # domain-specific migrations, forwarding to store/ + secret/
  provider.rs  # network or external integrations, built on net/
```

### Reusable infrastructure contracts

- **`net::ApiService`** wraps every outbound HTTP call. Construct `ApiService::new("ServiceName")`, call `.json::<T>(request)` or `.invalid_data(detail)`, and transport/parse failures become typed `YdError::ApiRequest`/`ApiData` attributed to that service. Never use `reqwest` directly outside `net/`.
- **`net::fallback::with_fallback(primary, fallback)`** races two futures concurrently and prefers the primary; `with_fallback_or_none` returns `Option` for nice-to-have values. Use this for any data source with redundant providers.
- **`net::PriceService`** provides cached USD quotes (25s TTL in `TtlCache`). New assets are added as `Asset` variants; new quote sources implement `PriceProvider`.
- **`store::Database`** owns the single `yd.sqlite`. Domains register migrations via `database.migrate(&[...])`; the shared `schema_migrations` ledger guarantees each version runs once. Never open a second connection path or bypass migrations.
- **`store::TtlCache`** stores expiring `(key, value, fetched_at)` rows for short-lived public data. Namespace keys by domain (e.g. `"price:ethereum"`) so the shared `ttl_cache` table never collides.
- **`secret::SecretStore`** encrypts any private blob with AES-256-GCM; the key lives in the system keyring. Private notes, API tokens, and future secrets reuse this — never persist secrets in plaintext or invent a new cipher.
- **`ui::Ui`** is the only color path. Use `Ui::text(Tone::_, ...)`, `Ui::kv(label, value)`, `Ui::title/divider/success/warning/error`, and `Ui::confirm(prompt)` for destructive actions. Raw ANSI, `println!` with inline colors, or bespoke formatters outside `ui.rs` are not permitted.

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

Avoid stringly typed plumbing. Use enums, structs, traits, and small service/provider objects so a new feature can be added without changing existing modules beyond the registry and app routing. Prefer one generic type configured by data over several near-duplicate types: `EvmProvider` serves every EVM chain through an `EvmNetworkConfig`, `UtxoProvider` serves every UTXO chain through a `UtxoNetworkConfig`, and `SolanaProvider` serves Solana through a `SolanaNetworkConfig` with SLIP-0010 derivation — adding a chain is a new `const fn` config, not a new provider. `expect` is acceptable only for static invariants controlled by the codebase, such as hard-coded derivation paths; user input, network responses, storage data, and provider data must return typed errors. Portfolio entries with zero balance are always fetched (for price caching) but hidden from output via `PortfolioEntry::has_balance()`.

## Module & CLI Rules

Root help should show only global options and module aliases. Module-specific flags belong in that module help, for example `yd -w -h`. Short module aliases are generated from the command name in `commands.rs`; add tests if a future module creates an alias collision. Destructive actions need confirmation (via `Ui::confirm`) plus an explicit non-interactive flag only when useful for scripts.

The bare-`yd` screen shows only the identity block (`^.^`, divider, Version/License/Author/Source, divider). The module list appears there automatically once a second module is registered; do not hand-roll module listings in `app.rs`. Each command should map to one module. Do not leak module-specific options into root help. Module args should resolve to a typed action enum before reaching `app.rs`; do not route on loose boolean combinations.

### Adding a module (five touch points)

1. `commands.rs` — append a `ModuleSpec` (command, long alias, summary).
2. `cli.rs` — add a `Command` variant, an `XxxArgs` struct, and a typed `XxxAction` enum with an `action()` resolver.
3. `app.rs` — route the new `Command` variant to the domain service.
4. `src/<domain>/` — implement `model.rs`, `service.rs`, `store.rs`, `provider.rs` on top of `net/`/`store/`/`secret/`/`ui`.
5. tests — alias uniqueness, action mapping, and any domain validators, following the testing guidelines below.

## Storage & Security

Never persist seed phrases, private keys, or encryption keys in plaintext. Encrypted payloads live in the shared SQLite database; the symmetric key lives in the system keyring. Route every secret through `secret::SecretStore`. SQLite schema changes must go through explicit migrations registered with `store::Database::migrate`; do not create ad-hoc files for app state. Short-lived public data must be cached through `store::TtlCache` with a namespaced key and timestamp-based expiry. Network providers must be async and built on `net::ApiService`; one provider failure must not prevent other balances or values from rendering.

### Backward compatibility

The on-disk format is a stable contract. The shared `yd.sqlite` file, the `wallet_secrets` and `schema_migrations` tables, and the keyring account `wallet-encryption-key-v1` must keep working across releases so existing installations upgrade without a reset. When changing storage, add a new versioned migration; never rewrite or drop existing tables.

## Testing Guidelines

Add focused tests when changing derivation paths, address formats, storage format, command normalization, help behavior, provider fallback logic, or any shared infrastructure (`net`, `store`, `secret`). Existing tests live beside the relevant module in `#[cfg(test)]` blocks.

Tests must use real validation rules. For example, crypto address tests should verify parsers, network checks, checksums, and known invalid cases; they should not only check prefixes, lengths, or display text. Add negative tests for malformed input whenever a validator is introduced. Infrastructure tests must prove real behavior: TTL expiry, migration idempotency, AES round-trips with tampered ciphertext, and fallback ordering under concurrent failures.

## Commit & Release Guidelines

Use short imperative commits, for example `Add resilient USD quote providers` or `Remove techtask document`. Commit locally when work is complete, but do not push unless the user explicitly asks for a push. When a push to `main` is requested, CI bumps the patch version, commits `chore(release): vX.Y.Z`, tags it, builds native Linux, `tar.gz`, `tar.xz`, and Debian assets, then publishes checksums and provenance. Manual `v*` tags are reserved for exceptional releases.

Before committing, run the relevant local checks. For Rust code, use `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`. If CI is changed, reason through the release path before committing so the user does not need multiple fix pushes.

## Installer Rules

`install.sh` must remain POSIX `sh` compatible. It installs the latest GitHub release to `~/.local/bin` by default, checks the installed version, skips identical versions unless `YD_FORCE=1`, verifies `SHA256SUMS`, and updates Bash/Zsh PATH only when needed.

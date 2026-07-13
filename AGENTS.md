# yd development notes

## Direction

`yd` is a personal terminal multitool. Wallet is one module, not the product boundary. Keep new features isolated by domain so they can be enabled without changing existing modules.

## Layout

- `app.rs` routes CLI input to modules.
- `cli.rs` owns Clap arguments and help text.
- `commands.rs` is the single registry for module aliases and root-help metadata.
- `ui.rs` owns terminal roles, colours, dividers, and shared output patterns.
- Each domain owns its service, storage, and provider implementations below `src/<domain>/`.

## Rules

- Use typed data and typed UI roles. Do not introduce raw ANSI escapes or ad-hoc terminal colours.
- Register each new module in `commands.rs`; derive root help and alias normalization from that registry rather than duplicating flags.
- Keep normal CLI output compact: useful values first, no explanatory paragraphs.
- Network work is async and provider failures must not prevent other providers from rendering.
- Never persist seed phrases, private keys, or encryption keys in plaintext. Keep encrypted payloads in SQLite and key material in the system keyring.
- Destructive actions need confirmation; provide an explicit non-interactive flag only where it is useful for scripts.
- Add focused tests whenever derivation paths, storage format, or command behaviour changes.

## Release

`install.sh` installs the latest GitHub release. Bump the package version, commit, push a `v*` tag, and let `.github/workflows/ci.yml` produce native, `tar.gz`, `tar.xz`, Debian, and AppImage assets with checksums and provenance.

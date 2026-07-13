# ^.^ yd

`yd` is a personal multitool for the terminal.

## Install or update

```sh
curl -fsSL https://raw.githubusercontent.com/9sx77ssl/yd/main/install.sh | sh
```

The same command installs or updates `yd`. It checks the latest GitHub release, leaves an identical installed version untouched, verifies the SHA-256 checksum before replacing a binary, and adds `~/.local/bin` to Bash and Zsh automatically. Open a new terminal afterwards, then run:

```sh
yd --wallet
```

Each release includes a raw binary, a fast `tar.gz` archive, a smaller `tar.xz` archive, a Debian package, SHA-256 checksums, and GitHub build provenance. The installer chooses the fast native archive.

To replace an identical version deliberately, use `YD_FORCE=1` before the install command.

## Wallet

On the first run, `yd` asks for a BIP-39 phrase without showing what you type. It derives Ethereum, Bitcoin, and Litecoin addresses, then shows their balance and USD value.

Your phrase stays on your machine. The database only contains AES-256-GCM encrypted data. The encryption key is kept by your system keyring.

Reset the local wallet when needed:

```sh
yd -w -r
```

Use `yd -w -h` for the wallet help.

## Development

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Push to `main` to create a release. CI bumps the patch version, commits it back, tags it, builds the release assets, and publishes them. A manual `v*` tag still creates a release when needed.

The installer always downloads the newest release.

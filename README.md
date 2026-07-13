# yd

`yd` is a personal command-line toolkit. Its first module tracks the native balances of an Ethereum, Bitcoin, and Litecoin wallet derived from one BIP-39 seed phrase.

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/9sx77ssl/yd/main/install.sh | sh
```

The installer downloads the latest compatible GitHub release into `~/.local/bin`. Re-run it at any time to update.

## Usage

```sh
yd --wallet
yd --help
```

On first use, enter a valid BIP-39 phrase. It is encrypted with AES-256-GCM before being saved in the platform application-data directory. The encryption key is generated locally and stored in the operating system keyring; the phrase is not sent to any service and is not persisted in plaintext.

## Development

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Create a versioned release by pushing a tag such as `v0.1.0`. GitHub Actions builds the Linux archive consumed by `install.sh`.

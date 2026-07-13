# ^.^ yd

`yd` is a personal multitool for the terminal. It starts with a wallet view and is built to grow into one place for the small things you manage every day.

## Install or update

```sh
curl -fsSL https://raw.githubusercontent.com/9sx77ssl/yd/main/install.sh | sh
```

The same command installs or updates `yd`. It puts the binary in `~/.local/bin` and adds that directory to Bash and Zsh automatically. Open a new terminal afterwards, then run:

```sh
yd --wallet
```

## Wallet

On the first run, `yd` asks for a BIP-39 phrase without showing what you type. It derives addresses for Ethereum, Bitcoin, and Litecoin, then shows the current balance and USD value.

Your phrase stays on your machine. The database only contains AES-256-GCM encrypted data. The encryption key is kept by your system keyring.

## Built to grow

The wallet is one module. Each network is a separate provider, so adding chains does not change the rest of the app. The same shape can be used later for notes, tasks, servers, and other useful commands.

## Development

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Push a tag such as `v0.1.2` to create a GitHub release. The installer always downloads the newest release.

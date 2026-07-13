# YD — CLI Utility Specification

## Overview

Create a modern, high-quality CLI application called **yd** written entirely in **Rust**.

The application is intended to become a long-term personal utility. The first implemented feature is cryptocurrency portfolio monitoring, but the architecture must be generic enough to support future modules such as notes, servers, tasks, or other personal tools.

The project should prioritize:

* clean architecture
* modularity
* maintainability
* strong typing
* performance
* asynchronous execution
* excellent developer experience
* production-quality code

Do not over-engineer, but avoid shortcuts that would make future expansion difficult.

---

# General Requirements

The application should feel like a polished modern CLI similar to:

* cargo
* gh
* pnpm
* bun
* uv
* flyctl

It should have:

* beautiful colored output
* Unicode support
* emoji support where appropriate (subtle, not excessive)
* clean spacing
* readable formatting
* friendly messages
* fast startup
* responsive async operations

The CLI should feel polished and enjoyable to use.

---

# Technology

Language:

* Rust (stable)

Use modern, well-maintained crates instead of implementing common functionality manually.

Preferred libraries include:

* clap
* tokio
* reqwest
* serde
* serde_json
* color-eyre
* tracing
* tracing-subscriber
* directories
* bip39
* alloy
* sqlx (SQLite)
* ring or RustCrypto crates where appropriate
* zeroize
* secrecy
* comfy-table (if ever needed)
* owo-colors or anstyle
* indicatif

Choose additional crates only if they are actively maintained and considered standard in the Rust ecosystem.

---

# CLI

The application should expose a polished CLI.

Examples:

```bash
yd --help
yd -h

yd --wallet
yd -w

yd --version
```

Use clap's derive API and provide rich help output.

Help output should be colorful, clean and well organized.

Every command should contain proper descriptions.

---

# Wallet Module

Initially only Ethereum should be implemented.

The architecture must make it very easy to add:

* Bitcoin
* Litecoin
* Solana
* additional EVM networks

without rewriting existing code.

The wallet module should be provider-based rather than Ethereum-specific.

---

# First Run Experience

If the user executes

```bash
yd --wallet
```

and no wallet has been configured yet,

the application should automatically prompt:

> Enter your BIP-39 seed phrase:

Use a secure hidden prompt where possible.

The seed phrase should then be:

* validated
* converted into a master key
* used to derive the default Ethereum account

using the standard derivation path.

---

# Wallet Storage

After successful validation:

Store wallet information locally.

Never store plaintext secrets.

Secrets must be encrypted before writing to disk.

The implementation should follow current best practices.

Use:

* SQLite for application storage
* SQLx
* Argon2id for key derivation
* authenticated encryption (for example AES-256-GCM or XChaCha20-Poly1305)

Sensitive values include:

* seed phrase
* derived private keys

Memory containing secrets should be cleared whenever possible using appropriate Rust crates.

The implementation should prioritize security over convenience.

---

# Wallet Loading

On future launches:

If encrypted wallet data already exists:

Automatically unlock and use it.

Do not ask for the seed phrase again unless necessary.

Design the authentication flow in a secure and user-friendly way.

---

# Ethereum

Use:

https://eth.drpc.org

Retrieve:

* address
* balance

Retrieve current ETH/USD price from a public API.

Display:

Address

Balance in ETH

Balance in USD

---

# Output Style

Avoid tables.

Output should be clean and minimal.

Example style:

```text
^.^ Ethereum

Address
0x742d35Cc6634C0532925a3b844Bc454e4438f44e

Balance
2.513421 ETH
≈ $8,534.12
```

When more blockchains are added:

```text
^.^ Ethereum

Address
...

Balance
...

----------------------------------------

^.^ Litecoin

Address
...

Balance
...

----------------------------------------

^.^ Bitcoin

Address
...

Balance
...
```

Keep spacing consistent.

Use colors sparingly.

Important values should stand out.

Errors should be red.

Warnings should be yellow.

Success messages should be green.

The application should look polished rather than flashy.

---

# Error Handling

Never panic during normal execution.

All errors should be converted into friendly user-facing messages.

Examples:

* invalid seed phrase
* database unavailable
* RPC unavailable
* internet connection failure
* malformed response
* timeout
* corrupted wallet

Use structured error types.

---

# Async

All networking should be asynchronous.

The application should perform independent operations concurrently whenever appropriate.

Avoid blocking operations.

---

# Architecture

The codebase should follow modern Rust design principles.

Requirements:

* modular
* strongly typed
* dependency injection where appropriate
* trait-based abstractions where appropriate
* reusable components
* separation of concerns
* no duplicated business logic
* minimal global state
* no "god" modules
* no giant files
* encapsulated responsibilities

Each feature should be isolated into its own modules.

Avoid tightly coupling unrelated functionality.

The project should remain easy to understand as it grows.

---

# Logging

Use structured logging.

Debug logging should be available for development.

Release output should remain clean.

---

# Configuration

Support configuration files for future settings.

Configuration should be easy to extend.

Store application data in the user's standard application directory.

Do not hardcode platform-specific paths.

---

# Future Expansion

The architecture should make it easy to add future features such as:

* Bitcoin
* Litecoin
* Solana
* token balances
* transaction history
* NFT support
* portfolio aggregation
* price alerts
* notes
* server management
* additional personal utilities

Adding a new blockchain should require implementing a new provider rather than modifying existing providers.

The project should be designed as a long-term personal CLI toolkit rather than a cryptocurrency-only application.

---

# Code Quality

The generated code should resemble production-quality software.

Prefer readability over cleverness.

Avoid unnecessary abstractions.

Avoid writing custom implementations when a well-maintained Rust crate already provides a secure and reliable solution.

The resulting codebase should be clean, idiomatic, scalable, secure, and easy to maintain for years.

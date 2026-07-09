# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

**voter** — a Rust voting client for the [parallelia/ec](https://github.com/parallelia/ec) Electoral Commission. Communicates with the EC daemon over Nostr NIP-59 Gift Wrap messages to register voters, obtain anonymous blind-RSA voting tokens, and cast ballots from unlinkable keypairs.

## Build & Test Commands

```bash
cargo build              # Build
cargo test               # Run all tests
cargo test <test_name>   # Run a single test
cargo fmt --check        # Check formatting
cargo clippy             # Lint (zero warnings policy)
```

## Architecture

The EC protocol defines three voter interactions, all via Nostr Gift Wrap:

1. **Register** — send registration token, bind Nostr pubkey to election
2. **Request token** — send blinded nonce, receive blind RSA signature, unblind locally
3. **Cast vote** — from a fresh anonymous keypair, send vote + unblinded token

The client MUST preserve voter anonymity: the unblinded token is never seen by the EC, and the voting keypair is unlinkable to the registered identity.

## Key Constraints

- Rust with ratatui TUI framework, nostr-sdk, blind-rsa-signatures
- NIP-59 Gift Wrap for all EC communication
- Privacy first: voter identity MUST NEVER be linkable to their vote
- No custom crypto — audited libraries only; fail secure on validation errors
- No telemetry, no external calls except configured Nostr relays
- `cargo clippy -- -D warnings` and `cargo fmt` enforced

## Repository Layout

- `.specify/` — SpecKit templates and project constitution
- `.claude/` — Claude Code command definitions (SpecKit workflows)

## Active Technologies
- Rust (edition 2024, stable channel) + ratatui 0.30, crossterm 0.29, nostr-sdk 0.44.1, blind-rsa-signatures 0.17.1, tokio, age, serde, sha2 (001-tui-voting-client)
- Local JSON files (~/.config/voter/) with optional age encryption (001-tui-voting-client)

## Recent Changes
- 001-tui-voting-client: Added Rust (edition 2024, stable channel) + ratatui 0.30, crossterm 0.29, nostr-sdk 0.44.1, blind-rsa-signatures 0.17.1, tokio, age, serde, sha2

# Quickstart: TUI Voting Client

**Feature Branch**: `001-tui-voting-client`
**Date**: 2026-03-21

## Prerequisites

- Rust toolchain (edition 2024, stable channel)
- A running EC daemon instance with at least one election
- Access to at least one Nostr relay
- A registration token from the election operator

## Build

```bash
cargo build --release
```

The binary is at `target/release/voter`.

## Configure

Create `~/.config/voter/voter.toml`:

```toml
[nostr]
relays = ["wss://relay.mostro.network"]

[identity]
path = "~/.config/voter/identity.json"

[ui]
theme = "dark"
```

Or rely on defaults — the client creates this file on first run.

## Run

```bash
./target/release/voter
```

### First Run

1. The client detects no identity and shows the **Welcome** screen.
2. Choose "Generate new identity" (or "Import" if you have an
   existing Nostr key).
3. Optionally set a password to encrypt the identity file.

### Vote in an Election

1. Browse the **Election List** — elections appear as they are
   published by the EC.
2. Select an election and enter your **registration token**.
3. Once registered, press **Request Token** — the client performs
   the blind signing protocol automatically.
4. Press **Vote** — select your candidate(s) per the election rules.
5. Confirm your ballot. The vote is cast anonymously.

### View Results

After an election finishes, select it from the list to view results.

## Development

```bash
cargo test               # Run all tests
cargo test <name>        # Run a single test
cargo clippy -- -D warnings  # Lint (must pass)
cargo fmt --check        # Format check
```

## Testing with a Local EC

1. Clone and run the EC daemon: `git clone https://github.com/parallelia/ec`
2. Start the EC with a test election (see EC README)
3. Generate registration tokens via the EC's gRPC admin API
4. Point the voter client at the same relay the EC uses

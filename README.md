# voter

A terminal-based voting client for the [Parallelia Electoral Commission](https://github.com/parallelia/ec). Communicates over Nostr using NIP-59 Gift Wrap messages to register voters, obtain anonymous blind-RSA voting tokens, and cast ballots from unlinkable keypairs.

## Features

- **Anonymous voting** via blind RSA signatures (RFC 9474) — your vote cannot be linked to your identity
- **End-to-end encrypted communication** with the Electoral Commission over Nostr NIP-59
- **Plurality and STV elections** — single-choice and ranked-choice voting
- **Optional password protection** for your identity using age encryption
- **Keyboard-driven TUI** with vim-style navigation
- **Local state persistence** — registrations and tokens survive restarts
- **No telemetry** — communicates only with configured Nostr relays

## Quick Start

### Prerequisites

- Rust stable toolchain (edition 2024)
- A running Electoral Commission daemon on configured Nostr relays

### Build and Run

```bash
cargo build --release
./target/release/voter
```

On first launch you'll be prompted to create or import a Nostr identity. Optionally protect it with a password.

### Voting Workflow

1. **Browse elections** — the client subscribes to election announcements from Nostr relays
2. **Register** — enter your out-of-band registration token to bind your identity to an election
3. **Request a voting token** — the client blinds a random nonce, sends it to the EC, receives a blind signature, and unblinds it locally
4. **Cast your vote** — select candidates and confirm; the vote is sent from a throwaway keypair that cannot be linked back to you

## Configuration

Configuration is stored at `~/.config/voter/voter.toml` (created with defaults on first run).

```toml
[nostr]
# Relay URLs must use wss:// (TLS). Plaintext ws:// is rejected at startup —
# NIP-59 encrypts payloads end to end, but an unencrypted relay connection
# leaks transport metadata. For local development against a test relay, set
# allow_insecure_relays = true to permit ws://.
relays = ["wss://relay.mostro.network", "wss://nos.lol"]

# Trusted Electoral Commission Nostr public key (hex or npub).
# STRONGLY RECOMMENDED: without it, any relay user can publish fake
# election events and trick the client into surrendering its registration
# token. The status bar shows UNPINNED while this is unset.
# ec_pubkey = "npub1..."

[identity]
path = "~/.config/voter/identity.json"

[ui]
theme = "dark"
```

### Files

| Path | Purpose |
|------|---------|
| `~/.config/voter/voter.toml` | Relay URLs, EC pubkey pin, identity path, UI theme |
| `~/.config/voter/identity.json` | Voter Nostr keypair (plaintext) |
| `~/.config/voter/identity.age` | Voter Nostr keypair (password-encrypted) |
| `~/.config/voter/state.json` | Registrations and voting tokens |
| `~/.config/voter/voter.log` | Debug log (development builds only) |

## Keyboard Shortcuts

### Global

| Key | Action |
|-----|--------|
| `q` | Quit |
| `?` | Toggle help overlay |
| `Esc` | Go back / cancel |

### Navigation

| Key | Action |
|-----|--------|
| `j` / `Down` | Move down |
| `k` / `Up` | Move up |
| `Enter` | Confirm / select |
| `Tab` | Next field |
| `s` | Open settings (from election list) |

### Voting

| Key | Action |
|-----|--------|
| `v` | Cast vote |
| `t` | Request voting token |
| `r` | View results |
| `Enter` / `Space` | Add candidate to ranking (STV) |
| `d` | Remove from ranking (STV) |

## Development

```bash
cargo build              # Build debug
cargo test               # Run all tests
cargo test <test_name>   # Run a single test
cargo fmt --check        # Check formatting
cargo clippy             # Lint (zero warnings enforced)
```

Integration tests that require a running EC daemon are ignored by default:

```bash
RUN_EC_INTEGRATION=1 cargo test -- --ignored
```

### Test Coverage

Current coverage (measured with [`cargo-llvm-cov`](https://github.com/taiki-e/cargo-llvm-cov)):

| Metric | Coverage |
|--------|----------|
| Lines | **89.63%** (3560/3972) |
| Functions | 87.41% (347/397) |
| Regions | 90.14% (6182/6858) |

Every module except two is at 94–100% line coverage. The remaining gap is
code that cannot run without external infrastructure:

- `src/main.rs` (0%) — the binary entry point: raw-mode terminal setup,
  the crossterm event loop, and the relay reconnection loop. It needs a
  real TTY and live Nostr relays.
- `src/nostr/client.rs` (14% lines) — the network paths (`subscribe`,
  `listen`, gift-wrap send/receive) need live relays; the offline logic
  (connect validation, command failure paths, response correlation) is
  covered.

Excluding those two infrastructure-bound files, line coverage of the
remaining code is **99.2%**.

To verify the numbers yourself:

```bash
cargo install cargo-llvm-cov   # one-time setup
cargo llvm-cov --workspace     # summary table per file
cargo llvm-cov --workspace --html               # line-by-line report
xdg-open target/llvm-cov/html/index.html        # Linux (macOS: use `open`)
```

## Architecture

### Protocol

The EC protocol defines three interactions, all wrapped in NIP-59 Gift Wrap:

```
Voter                                 Electoral Commission
  |                                          |
  |-- Register (identity + token) ---------> |
  |<------------- Ok / Error --------------- |
  |                                          |
  |-- RequestToken (blinded nonce) --------> |
  |<------------- BlindSignature ----------- |
  |   [unblind locally]                      |
  |                                          |
  |-- CastVote (throwaway key + token) ----> |
  |<------------- Ok / Error --------------- |
```

### Privacy Design

- **Blind RSA signatures**: the EC signs a blinded nonce without seeing the final token
- **Throwaway keypairs**: votes are sent from ephemeral keys discarded after submission
- **No linkability**: the unblinded token is never associated with the voter's identity
- **Fail-secure**: invalid cryptographic operations are rejected immediately
- **No custom crypto**: uses only audited libraries (`blind-rsa-signatures`, `nostr-sdk`, `age`)

### Module Layout

```
src/
├── main.rs          # Entry point, event loop
├── app.rs           # App state, screen management, input handling
├── config.rs        # Configuration (TOML)
├── identity.rs      # Keypair generation, save/load, age encryption
├── state.rs         # Persistent state (registrations, tokens)
├── error.rs         # Error types
├── crypto/
│   ├── blind_rsa.rs # Blind/unblind operations
│   └── token.rs     # VotingToken, nonce generation
├── nostr/
│   ├── client.rs    # Nostr client, relay connection, Gift Wrap
│   ├── events.rs    # Election & results structs
│   └── messages.rs  # Protocol message types
└── ui/
    ├── welcome.rs        # Identity setup
    ├── password.rs       # Encrypted identity unlock
    ├── election_list.rs  # Browse elections
    ├── election_detail.rs# Register, request token
    ├── vote.rs           # Candidate selection
    ├── results.rs        # Election results
    ├── settings.rs       # Relay/theme config
    ├── help.rs           # Keyboard shortcuts overlay
    └── widgets/          # Status bar, confirm dialog
```

## License

MIT

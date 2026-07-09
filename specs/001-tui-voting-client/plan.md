# Implementation Plan: TUI Voting Client

**Branch**: `001-tui-voting-client` | **Date**: 2026-03-21 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `/specs/001-tui-voting-client/spec.md`

## Summary

Build a terminal-based voting client (TUI) for the Parallelia
Electoral Commission. The client implements the full voter flow:
identity management, election discovery via Nostr Kind 35000 events,
voter registration, anonymous token acquisition via blind RSA
signatures (RFC 9474), and anonymous vote casting from throwaway
keypairs — all communicated over NIP-59 Gift Wrap messages.

The architecture follows the Elm/TEA pattern (update/view cycle) with
a channel-based action dispatcher, using ratatui for rendering and
tokio for async networking.

## Technical Context

**Language/Version**: Rust (edition 2024, stable channel)
**Primary Dependencies**: ratatui 0.30, crossterm 0.29, nostr-sdk 0.44.1, blind-rsa-signatures 0.17.1, tokio, age, serde, sha2
**Storage**: Local JSON files (~/.config/voter/) with optional age encryption
**Testing**: cargo test; integration tests against local EC instance
**Target Platform**: Linux, macOS, Windows (any terminal with escape sequence support)
**Project Type**: TUI desktop application
**Performance Goals**: Full voting flow completable in under 5 minutes; UI responsive at 30+ fps
**Constraints**: No network calls except configured Nostr relays; no telemetry; no root privileges
**Scale/Scope**: 7 screens, single-user, ~15 functional requirements

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Status | Compliance notes |
|-----------|--------|------------------|
| I. Privacy First | PASS | Blind RSA ensures unlinkability; throwaway keypairs for voting; no telemetry; only relay connections |
| II. Cryptographic Integrity | PASS | RFC 9474 via blind-rsa-signatures; nostr-sdk for NIP-59; age for key encryption; no custom crypto |
| III. User Sovereignty | PASS | Local keypair storage; user-configurable relays; no third-party accounts; cached elections viewable offline |
| IV. Simplicity Over Features | PASS | Keyboard-only TUI; one purpose per screen; sensible defaults in voter.toml |
| V. Transparency | PASS | MIT license; Nostr messages inspectable; vote receipt verifiable against Kind 35001 results |
| Must Use | PASS | Rust, ratatui, nostr-sdk, blind-rsa-signatures, NIP-59 — all present |
| Must Not | PASS | No phone-home; encrypted tokens when password set; no sensitive logging; no root required |
| Quality Standards | PASS | clippy -D warnings, cargo fmt, unit tests on critical paths, integration tests for full flow |

**Post-Phase 1 re-check**: All gates still pass. Data model stores
tokens encrypted when password is set. Contracts confirm no sensitive
data leaves the client except to configured relays.

## Project Structure

### Documentation (this feature)

```text
specs/001-tui-voting-client/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   ├── ec-protocol.md
│   └── tui-screens.md
├── checklists/
│   └── requirements.md
└── tasks.md             # Phase 2 output (/speckit.tasks command)
```

### Source Code (repository root)

```text
src/
├── main.rs              # Entry point, terminal setup, main loop
├── app.rs               # App state, Action enum, update() dispatch
├── config.rs            # voter.toml parsing (AppConfig)
├── identity.rs          # Keypair generation, import, encryption/decryption
├── crypto/
│   ├── mod.rs
│   ├── blind_rsa.rs     # Blind, finalize, verify (wraps blind-rsa-signatures)
│   └── token.rs         # VotingToken encode/decode, nonce management
├── nostr/
│   ├── mod.rs
│   ├── client.rs        # Relay connection, subscribe, send Gift Wrap
│   ├── messages.rs      # EC protocol message types (serde)
│   └── events.rs        # Kind 35000/35001 parsing (Election, Results)
├── state.rs             # Local state persistence (registrations, tokens)
├── ui/
│   ├── mod.rs
│   ├── welcome.rs       # Welcome/Setup screen
│   ├── password.rs      # Password prompt screen
│   ├── election_list.rs # Election list screen
│   ├── election_detail.rs # Election detail screen
│   ├── vote.rs          # Ballot selection screen (plurality + STV)
│   ├── results.rs       # Results display screen
│   ├── settings.rs      # Settings screen
│   ├── help.rs          # Help overlay
│   └── widgets/         # Reusable UI components
│       ├── mod.rs
│       ├── confirm_dialog.rs
│       └── status_bar.rs
└── error.rs             # Error types

tests/
├── integration/
│   ├── full_flow.rs     # End-to-end voting flow
│   ├── registration.rs  # Registration with EC
│   └── blind_rsa.rs     # Blind signing roundtrip
└── unit/
    ├── config.rs        # Config parsing
    ├── identity.rs      # Key gen/import/encrypt
    ├── messages.rs      # Message serialization
    └── state.rs         # State persistence
```

**Structure Decision**: Single Rust binary crate. The `src/` layout
groups by concern (crypto, nostr, ui, state) rather than by layer.
Each UI screen is a separate module to keep the view logic focused.
The `crypto/` and `nostr/` modules encapsulate external crate usage
so the rest of the app works with domain types.

## Complexity Tracking

No constitution violations to justify. The design uses only mandated
technologies and follows the one-screen-one-purpose principle.

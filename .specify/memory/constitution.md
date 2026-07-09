<!--
  Sync Impact Report
  ==================
  Version change: 1.0.0 → 2.0.0 (full constitution replacement)
  Modified principles:
    - I. Security-First → I. Privacy First (narrowed to unlinkability)
    - II. Test-First (TDD) → removed as standalone principle
      (testing now under Quality Standards section)
    - III. Simplicity → IV. Simplicity Over Features (expanded with
      UX-specific rules)
    - IV. Transparency → V. Transparency (refined with license and
      verifiability specifics)
  Added sections:
    - Core Purpose
    - II. Cryptographic Integrity (new principle)
    - III. User Sovereignty (new principle)
    - Technical Boundaries (Must Use / Must Not)
    - Quality Standards
    - Non-Goals
  Removed sections:
    - Development Constraints (replaced by Technical Boundaries)
    - Development Workflow (removed per user input)
  Templates requiring updates:
    - .specify/templates/plan-template.md — ✅ no updates needed (generic)
    - .specify/templates/spec-template.md — ✅ no updates needed (generic)
    - .specify/templates/tasks-template.md — ✅ no updates needed (generic)
  Follow-up TODOs: none
-->

# Parallelia Voter — Project Constitution

## Core Purpose

A terminal-based voting client that enables citizens to participate
in anonymous electronic elections. Privacy and vote secrecy are
non-negotiable.

## Guiding Principles

### I. Privacy First

- The voter's identity MUST NEVER be linkable to their vote.
- Blind RSA signatures ensure the EC cannot correlate registration
  with voting.
- Ephemeral keypairs for voting MUST be generated fresh and discarded
  after use.
- No telemetry, no analytics, no external calls except to configured
  Nostr relays.

### II. Cryptographic Integrity

- All cryptographic operations MUST follow established standards
  (RFC 9474 for blind RSA).
- No custom crypto — use audited libraries only
  (blind-rsa-signatures, nostr-sdk).
- Fail secure: if crypto validation fails, abort the operation.

### III. User Sovereignty

- User controls their identity (keypair stored locally, optionally
  encrypted).
- User chooses which relays to connect to.
- No accounts, no registration with third parties.
- Works offline for viewing cached elections (voting requires
  network).

### IV. Simplicity Over Features

- TUI MUST be navigable with keyboard only.
- Each screen has one clear purpose.
- Error messages MUST explain what went wrong and how to fix it.
- No unnecessary configuration — sensible defaults.

### V. Transparency

- Open source (MIT license).
- All network messages are inspectable (Nostr events).
- Vote receipts can be verified against published results.

## Technical Boundaries

### Must Use

- Rust (memory safety, no runtime)
- ratatui (TUI framework)
- nostr-sdk (Nostr protocol)
- blind-rsa-signatures (RFC 9474)
- NIP-59 Gift Wrap (private messaging)

### Must Not

- Phone home to any server other than configured relays.
- Store voting tokens unencrypted if password is set.
- Log sensitive data (tokens, nonces, keypairs).
- Require root/admin privileges.

## Quality Standards

- All code MUST pass `cargo clippy -- -D warnings`.
- All code MUST be formatted with `cargo fmt`.
- Critical paths MUST have unit tests.
- Integration tests MUST cover the full voting flow.
- Public APIs MUST be documented.

## Non-Goals

- GUI version (this is TUI only).
- Mobile support (desktop terminals only).
- Vote delegation or proxy voting.
- Election administration (that is the EC daemon's responsibility).

## Governance

This constitution supersedes ad-hoc decisions. Amendments require:

1. A documented rationale for the change.
2. Version bump following semver (MAJOR for principle
   removals/redefinitions, MINOR for additions/expansions, PATCH for
   clarifications).
3. Update of the Sync Impact Report at the top of this file.
4. Propagation check across all `.specify/templates/` files.

All code reviews MUST verify compliance with the principles above.

**Version**: 2.0.0 | **Ratified**: 2026-03-21 | **Last Amended**: 2026-03-21

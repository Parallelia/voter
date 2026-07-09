# EC Protocol Contract

**Feature Branch**: `001-tui-voting-client`
**Date**: 2026-03-21

This document defines the message contracts between the voter client
and the Electoral Commission daemon. All messages are exchanged inside
NIP-59 Gift Wrap (Nostr Kind 1059).

## Transport

- **Protocol**: Nostr relay WebSocket connections
- **Privacy layer**: NIP-59 Gift Wrap (Kind 1059)
  - Rumor (unsigned Kind 1): contains JSON payload in `.content`
  - Seal (Kind 13): encrypts rumor to recipient
  - Gift Wrap (Kind 1059): wraps seal with random throwaway key
- **Direction**: Bidirectional (voter ↔ EC via relay)

## Voter → EC Messages

### register

Registers the voter's Nostr pubkey for an election.

```json
{
  "action": "register",
  "election_id": "<string>",
  "registration_token": "<string>"
}
```

**Preconditions**: Voter has a valid registration token.
**Sent from**: Voter's persistent identity keypair.

### request-token

Requests a blind RSA signature on a blinded nonce.

```json
{
  "action": "request-token",
  "election_id": "<string>",
  "blinded_nonce": "<base64-encoded blind message>"
}
```

**Preconditions**: Voter is registered for the election.
**Sent from**: Voter's persistent identity keypair.

### cast-vote

Submits an anonymous ballot with the unblinded token.

```json
{
  "action": "cast-vote",
  "election_id": "<string>",
  "candidate_ids": [<integer>, ...],
  "h_n": "<hex SHA-256 of original nonce>",
  "token": "<base64(signature_bytes ++ randomizer_32bytes)>"
}
```

**Preconditions**: Voter has a valid unblinded token.
**Sent from**: Throwaway ephemeral keypair (NOT the voter's identity).

**Ballot rules**:
- Plurality (`rules_id = "plurality"`): exactly one candidate_id
- STV (`rules_id = "stv"`): ordered list, no duplicates

## EC → Voter Responses

### Success

```json
{
  "status": "ok",
  "action": "<action-name>"
}
```

For `request-token` success, includes the blind signature:
```json
{
  "status": "ok",
  "action": "token-issued",
  "blind_signature": "<base64-encoded blind signature>"
}
```

### Error

```json
{
  "status": "error",
  "code": "<ERROR_CODE>",
  "message": "<human-readable description>"
}
```

**Error codes**:

| Code                | Applies to         | Description                          |
|---------------------|--------------------|--------------------------------------|
| ELECTION_NOT_FOUND  | all actions        | Election ID does not exist           |
| ELECTION_CLOSED     | register, request  | Election not accepting this action   |
| INVALID_TOKEN       | register           | Registration token invalid or used   |
| ALREADY_REGISTERED  | register           | Voter already registered             |
| NOT_AUTHORIZED      | request-token      | Voter not registered for election    |
| ALREADY_ISSUED      | request-token      | Token already issued to this voter   |
| NONCE_ALREADY_USED  | cast-vote          | Nonce hash already consumed          |
| INVALID_CANDIDATE   | cast-vote          | Candidate ID not in election         |
| BALLOT_INVALID      | cast-vote          | Ballot violates election rules       |
| UNKNOWN_RULES       | cast-vote          | Unrecognized rules_id                |
| INVALID_MESSAGE     | all actions        | Malformed JSON or missing fields     |
| INTERNAL_ERROR      | all actions        | Server-side error                    |

## Public Nostr Events (read-only)

### Kind 35000 — Election Announcement

Addressable event, `d`-tagged by `election_id`.

```json
{
  "election_id": "<string>",
  "name": "<string>",
  "start_time": 1751932800,
  "end_time": 1751936400,
  "status": "open | in_progress | finished | cancelled",
  "rules_id": "plurality | stv",
  "rsa_pub_key": "<base64 DER-encoded RSA public key>",
  "candidates": [
    { "id": 1, "name": "Candidate A" },
    { "id": 2, "name": "Candidate B" }
  ]
}
```

`start_time` / `end_time` are unix timestamps (seconds). `status` values are
snake_case strings, exactly as stored by the EC daemon.

**Trust**: clients MUST verify the event author is the expected EC pubkey
(configured via `nostr.ec_pubkey`). Anyone can publish Kind 35000 events; an
unpinned client can be fed a fake election carrying an attacker's RSA key and
tricked into surrendering its registration token.

### Kind 35001 — Election Results

Addressable event, `d`-tagged by `election_id`.

```json
{
  "election_id": "<string>",
  "name": "<string>",
  "rules_id": "plurality | stv",
  "elected": [<candidate_id>, ...],
  "tally": [
    { "candidate_id": 1, "votes": 42.0, "status": "elected" },
    { "candidate_id": 2, "votes": 37.5, "status": "excluded" }
  ],
  "count_sheet": [ ... ]
}
```

`votes` is a JSON number that MAY be fractional: STV surplus transfers are
weighted. `count_sheet` (per-round STV audit trail) is optional. Clients may
ignore fields they do not use, but must not assume integer vote counts.

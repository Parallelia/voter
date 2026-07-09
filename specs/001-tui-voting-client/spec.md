# Feature Specification: TUI Voting Client

**Feature Branch**: `001-tui-voting-client`
**Created**: 2026-03-21
**Status**: Draft
**Input**: User description: "TUI voting client for Parallelia anonymous voting system"

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Identity Setup (Priority: P1)

A first-time voter launches the client and creates their voting identity. They can either generate a new identity or import an existing one. The identity is saved locally and optionally protected with a password. This identity is reused across all future elections.

**Why this priority**: Without an identity, no other actions are possible. This is the entry point for every voter.

**Independent Test**: Launch the client with no existing identity file. Complete the setup flow. Verify the identity file is created and the client proceeds to the election list.

**Acceptance Scenarios**:

1. **Given** the client is launched for the first time, **When** the voter chooses "generate new identity", **Then** a new identity is created, optionally encrypted with a password, and saved to the configured location.
2. **Given** the client is launched for the first time, **When** the voter chooses "import existing identity" and provides a valid key, **Then** the identity is imported and saved locally.
3. **Given** an encrypted identity exists, **When** the voter launches the client, **Then** they are prompted for the password before proceeding.
4. **Given** the voter enters an incorrect password, **When** decryption fails, **Then** the client displays an error and allows retry.

---

### User Story 2 - Election Discovery and Registration (Priority: P1)

A voter with an identity browses available elections published by the Electoral Commission. They can view election details (name, status, candidates, rules) and register for an election using a one-time registration token received out-of-band from the election operator.

**Why this priority**: Discovery and registration are prerequisites for voting. Without them the client has no purpose.

**Independent Test**: With a valid identity, connect to relays, view the election list, select an election, enter a registration token, and verify successful registration.

**Acceptance Scenarios**:

1. **Given** the voter has a valid identity and relay connectivity, **When** they navigate to the election list, **Then** all published elections are displayed with name, status, start/end times, voting rules, and candidate count.
2. **Given** the voter selects an election with status "Open", **When** they enter a valid registration token, **Then** the client sends a registration request and displays a success confirmation.
3. **Given** the voter enters an invalid or already-used token, **When** the registration request fails, **Then** the client displays the specific error (invalid token, already registered, election closed) and allows retry.
4. **Given** the voter is already registered for an election, **When** they view that election, **Then** the client indicates their registered status.
5. **Given** relays are unreachable, **When** the voter attempts to browse elections, **Then** the client displays a connection error and retries automatically.

---

### User Story 3 - Anonymous Token Request (Priority: P1)

A registered voter requests an anonymous voting token for an election. The client performs a blind-signing protocol so that the EC signs the token without ever seeing its final form — preserving voter anonymity.

**Why this priority**: The anonymous token is the core privacy mechanism. Without it, votes cannot be cast anonymously.

**Independent Test**: As a registered voter, request a token for an election. Verify the token is received, unblinded locally, and stored. Verify the unblinded token passes signature verification against the election's public key.

**Acceptance Scenarios**:

1. **Given** the voter is registered for an election in "InProgress" status, **When** they request a voting token, **Then** the client generates a random nonce, blinds it, sends the request, receives the blind signature, unblinds it, and stores the valid token locally.
2. **Given** the voter has already received a token for an election, **When** they attempt to request another, **Then** the client informs them a token already exists for that election.
3. **Given** the EC rejects the token request (not authorized, election closed), **When** the error response arrives, **Then** the client displays the reason clearly.
4. **Given** the token request is in progress, **When** the voter views the screen, **Then** a progress indicator shows the current step (blinding, sending, receiving, unblinding).

---

### User Story 4 - Cast Anonymous Vote (Priority: P1)

A voter with a valid anonymous token casts their vote. The client generates a throwaway identity for vote submission so the vote cannot be linked to the voter. The ballot format adapts to the election rules (single selection for plurality, ranked preferences for STV).

**Why this priority**: This is the primary purpose of the entire application.

**Independent Test**: With a valid token, open the voting screen for an election, select candidate(s) per the election rules, confirm, and verify the vote is accepted by the EC.

**Acceptance Scenarios**:

1. **Given** a plurality election and a valid token, **When** the voter selects one candidate and confirms, **Then** the client generates a throwaway identity, constructs the ballot, sends the vote, and displays confirmation.
2. **Given** an STV election and a valid token, **When** the voter ranks candidates in preference order and confirms, **Then** the client sends the ranked ballot and displays confirmation.
3. **Given** the voter is on the confirmation screen, **When** they review their selections, **Then** they can go back to modify before final submission.
4. **Given** the vote is successfully cast, **When** the voter returns to the election list, **Then** the election shows a "voted" indicator and the token is marked as consumed.
5. **Given** the vote submission fails (network error, invalid token), **When** the error occurs, **Then** the client displays the error and the token is preserved for retry.
6. **Given** the throwaway identity is used for voting, **When** the vote is submitted, **Then** the throwaway identity is permanently discarded and cannot be recovered.

---

### User Story 5 - View Election Results (Priority: P2)

After an election ends, the voter can view the published results directly in the client.

**Why this priority**: Results viewing is valuable but not required for the core voting flow.

**Independent Test**: After an election finishes, navigate to it and verify results are displayed accurately.

**Acceptance Scenarios**:

1. **Given** an election has status "Finished", **When** the voter views its details, **Then** the final results are displayed with candidate names and tallies.
2. **Given** an election is still in progress, **When** the voter views its details, **Then** no partial results are shown (only status and candidate list).

---

### User Story 6 - Settings and Configuration (Priority: P3)

The voter can configure relay connections, manage their identity, and customize the interface through a settings screen.

**Why this priority**: Configuration enhances usability but has sensible defaults that work out of the box.

**Independent Test**: Open settings, modify relay list, save, and verify the client connects to updated relays.

**Acceptance Scenarios**:

1. **Given** the voter opens settings, **When** they add or remove relay addresses, **Then** changes are persisted and take effect on next connection.
2. **Given** the voter opens settings, **When** they change the display theme, **Then** the interface updates immediately.
3. **Given** the voter wants to export their identity, **When** they select the export option, **Then** the identity key is displayed or saved to a specified location.

---

### Edge Cases

- What happens when the voter loses network connectivity mid-vote? The client MUST preserve the token and allow retry when connectivity is restored.
- What happens when the voter closes the client after receiving a token but before voting? The token MUST be persisted and available on next launch.
- What happens when an election transitions status while the voter is viewing it? The client MUST update the display to reflect the new status.
- What happens when the relay returns malformed data? The client MUST reject it and display a meaningful error rather than crashing.
- What happens when the identity file is corrupted? The client MUST detect the corruption and offer to create a new identity.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST support connecting to multiple message relays simultaneously, configurable by the voter.
- **FR-002**: System MUST persist voter identity locally with optional password-based encryption.
- **FR-003**: System MUST implement the blind signing protocol (RFC 9474) compatible with the Electoral Commission.
- **FR-004**: All communication with the EC MUST use encrypted, privacy-preserving message wrapping (NIP-59 Gift Wrap).
- **FR-005**: System MUST support both plurality (single-choice) and STV (ranked-choice) voting modes, adapting the ballot interface to the election's rules.
- **FR-006**: System MUST display real-time election status updates as they arrive from relays.
- **FR-007**: System MUST handle network errors gracefully with automatic retry and clear status indicators.
- **FR-008**: The throwaway identity used for vote casting MUST be cryptographically random and permanently discarded after use.
- **FR-009**: System MUST persist voting tokens locally so they survive client restarts.
- **FR-010**: System MUST provide keyboard-driven navigation (vim-style hjkl and arrow keys).
- **FR-011**: System MUST display a confirmation dialog before any irreversible action (submitting a vote, discarding an identity).
- **FR-012**: System MUST provide a help overlay accessible via a standard key.
- **FR-013**: System MUST adapt its layout to different terminal sizes.
- **FR-014**: The unblinded nonce MUST never be transmitted to the EC under any circumstances.
- **FR-015**: The throwaway voting identity MUST NOT be derivable from the voter's main identity.

### Key Entities

- **Voter Identity**: The voter's persistent keypair used for registration and token requests. Optionally password-encrypted. One per client installation.
- **Election**: A published electoral event with name, status, time bounds, rules, candidates, and a public signing key.
- **Registration Token**: A one-time-use code provided out-of-band by the election operator. Consumed upon successful registration.
- **Voting Token**: An anonymous credential obtained via the blind signing protocol. Tied to a specific election. Consumed when vote is cast.
- **Ballot**: The voter's candidate selection(s) — a single choice (plurality) or ranked list (STV) — submitted from a throwaway identity.
- **Throwaway Identity**: An ephemeral keypair generated solely for casting a single vote. Destroyed after submission.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A voter can complete the full flow — from first launch through identity setup, registration, token request, and vote casting — in under 5 minutes.
- **SC-002**: The blind signing roundtrip produces a token that the EC accepts as valid 100% of the time when protocol is followed correctly.
- **SC-003**: No information linking a cast vote to a specific voter identity is transmitted or stored by the client at any point.
- **SC-004**: All screens are navigable entirely via keyboard with no mouse required.
- **SC-005**: The client recovers from a network disconnection within 10 seconds of connectivity being restored, without losing state or tokens.
- **SC-006**: Both plurality and STV elections are fully supported end-to-end with the EC daemon.

## Assumptions

- The voter receives their registration token through an out-of-band channel managed by the election operator (not part of this client's scope).
- The EC daemon is running and reachable via at least one configured relay.
- The voter's terminal supports standard escape sequences (any modern terminal emulator).
- Configuration defaults to `~/.config/voter/voter.toml` following XDG conventions.
- Dark theme is the default; light theme is an alternative option.
- The client is single-user (one identity per installation).

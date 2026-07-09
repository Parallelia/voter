# Security Policy

## Threat Model

voter is a privacy-preserving voting client. The primary security goals are:

1. **Voter anonymity** — a cast vote must not be linkable to the voter's registered identity
2. **Vote integrity** — only registered voters with valid tokens can cast ballots
3. **Confidentiality** — all communication with the Electoral Commission is encrypted

### What voter protects against

- **EC learning how you voted**: blind RSA signatures prevent the EC from linking your token to your identity. Votes are cast from throwaway keypairs.
- **Network observers**: all messages use NIP-59 Gift Wrap (encrypted + wrapped with ephemeral keys).
- **Local identity theft**: optional age passphrase encryption for stored keys.

### What voter does NOT protect against

- **Compromised local machine**: if your device is compromised, an attacker can read keys and state from disk.
- **Compromised EC daemon**: the EC could potentially issue duplicate tokens or manipulate election results. Auditing the EC is outside this client's scope.
- **Relay-level metadata**: Nostr relays can observe connection timing and IP addresses. Use Tor if relay metadata is a concern.

## Cryptographic Libraries

| Library | Purpose | Standard |
|---------|---------|----------|
| `blind-rsa-signatures` | Blind RSA token signing | RFC 9474 |
| `nostr-sdk` | NIP-59 Gift Wrap encryption | NIP-59 |
| `age` | Identity file encryption | age-encryption.org/v1 |
| `sha2` | Nonce hashing | SHA-256 |
| `getrandom` | Cryptographic randomness | OS CSPRNG |

No custom cryptographic code is used. `unsafe` code is denied at the compiler level.

## Reporting Vulnerabilities

If you discover a security vulnerability, please report it privately rather than opening a public issue. Contact the maintainers through GitHub's private vulnerability reporting feature on the [repository](https://github.com/parallelia/voter).

## Secure Development Practices

- `#[deny(unsafe_code)]` enforced project-wide
- `cargo clippy -- -D warnings` with zero-warning policy
- Sensitive values wrapped in `secrecy::Secret` for automatic memory zeroization
- No telemetry or external network calls beyond configured Nostr relays
- State files stored with user-only permissions in `~/.config/voter/`

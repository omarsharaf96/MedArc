# S09: Backup, Distribution & Release — UAT

**Milestone:** M001
**Written:** 2026-03-11

## UAT Type

- UAT mode: artifact-driven
- Why this mode is sufficient: BKUP-01/02/03 are validated by 13 unit tests exercising the complete AES-256-GCM encrypt/decrypt pipeline including wrong-key rejection, tamper detection, and nonce uniqueness. DIST-01/02/03 are configuration artifacts (tauri.conf.json, entitlements.plist, Cargo.toml) that require a real Apple Developer certificate and CI environment to execute end-to-end; their correctness is validated by reviewing the configuration against Apple's notarization requirements and Tauri's updater API. A live notarization run and auto-update trigger require infrastructure not available in the development environment.

## Preconditions

- `cargo test --lib` passes (265 tests, 0 failures) — confirms AES-256-GCM round-trips, RBAC rules, and Migration 14 validity
- For live distribution UAT: Apple Developer ID Application certificate, `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID` environment variables set; Tauri CLI 2.x installed
- For auto-updater UAT: real Ed25519 key pair generated via `tauri signer generate`; update manifest published to `releases.medarc.app`

## Smoke Test

Run `cargo test --lib 2>&1 | grep -E "backup|bkup|BKUP"` and confirm all `bkup_*` tests pass. This confirms AES-256-GCM encryption is functional and the backup_log migration is valid.

## Test Cases

### 1. AES-256-GCM Encrypt/Decrypt Round-Trip (BKUP-02)

1. Run `cargo test --lib -- commands::backup::tests::bkup_02_aes_gcm_round_trip_recovers_plaintext`
2. Run `cargo test --lib -- commands::backup::tests::bkup_02_aes_gcm_large_plaintext_round_trip`
3. **Expected:** Both tests pass. The first confirms short payloads round-trip; the second confirms 128 KB payloads (representative of a small SQLite database) round-trip correctly.

### 2. Wrong Key Rejected (BKUP-02 — encryption quality)

1. Run `cargo test --lib -- commands::backup::tests::bkup_02_aes_gcm_wrong_key_fails_authentication`
2. **Expected:** Test passes. Decryption with a different 32-byte key returns `Err(AppError::Database("backup authentication failed..."))`, confirming the GCM tag enforces key identity.

### 3. Tamper Detection (BKUP-02 — integrity)

1. Run `cargo test --lib -- commands::backup::tests::bkup_02_aes_gcm_tampered_ciphertext_fails_authentication`
2. **Expected:** Test passes. A single bit-flip in the ciphertext body causes GCM tag verification to fail before any plaintext is returned.

### 4. Nonce Uniqueness (BKUP-02 — semantic security)

1. Run `cargo test --lib -- commands::backup::tests::bkup_02_aes_gcm_nonces_are_unique_across_calls`
2. **Expected:** Test passes. Two encryptions of the same plaintext produce different first 12 bytes (nonces), confirming each backup gets a unique random nonce.

### 5. Truncated Backup File Rejected (BKUP-03 — restore safety)

1. Run `cargo test --lib -- commands::backup::tests::bkup_03_truncated_blob_returns_error`
2. **Expected:** Test passes. A blob shorter than `NONCE_LEN + TAG_LEN` (28 bytes) returns an error immediately, preventing a partial decryption attempt.

### 6. Migration 14 Validates (BKUP-01 — audit trail schema)

1. Run `cargo test --lib -- db::migrations::tests::migrations_are_valid`
2. **Expected:** Test passes. `rusqlite_migration` validates the entire migration chain including Migration 14 (`backup_log` table).

### 7. RBAC — Backup Resource (BKUP-01 — access control)

1. Run `cargo test --lib -- rbac::roles::tests` and confirm no failures.
2. Manually verify in `roles.rs`: `(SystemAdmin, Backup, _) => true` (wildcard), `(Provider, Backup, Create | Read) => true`, `(NurseMa, Backup, _) => false`.
3. **Expected:** All RBAC tests pass. Provider can create/list backups; NurseMa/BillingStaff/FrontDesk cannot.

### 8. tauri-plugin-updater Registered (DIST-02)

1. Open `src-tauri/src/lib.rs` and confirm `.plugin(tauri_plugin_updater::Builder::new().build())` is present before `.setup(...)`.
2. Open `src-tauri/tauri.conf.json` and confirm `plugins.updater.active: true` and `plugins.updater.endpoints` contains the release URL pattern.
3. **Expected:** Both confirmed. The updater plugin is wired and configured.

### 9. Entitlements File Present and Correct (DIST-03)

1. Open `src-tauri/entitlements.plist` and confirm:
   - `com.apple.security.app-sandbox` = `true`
   - `com.apple.security.network.client` = `true`
   - `com.apple.security.files.user-selected.read-write` = `true`
   - `keychain-access-groups` contains `$(AppIdentifierPrefix)com.medarc.emr`
2. Open `src-tauri/tauri.conf.json` and confirm `bundle.macOS.entitlements` = `"entitlements.plist"`.
3. **Expected:** All confirmed. Entitlements enable App Sandbox + Hardened Runtime with the minimum required permissions for backup and Keychain access.

### 10. Release Runbook Complete (DIST-01)

1. Open `docs/RELEASE.md` and confirm it covers: code-signing certificate setup, `npm run tauri build` command, `spctl` notarization verification, Ed25519 key generation, update manifest format, backup procedure, and CI/CD environment variables.
2. **Expected:** All sections present and actionable.

## Edge Cases

### Encrypted Backup of Empty Database

1. Run `cargo test --lib -- commands::backup::tests::bkup_02_aes_gcm_empty_plaintext_round_trip`
2. **Expected:** Test passes. Empty plaintext encrypts to a 28-byte blob (12 nonce + 0 ciphertext + 16 tag) and decrypts back to empty bytes.

### SHA-256 Digest Determinism (BKUP-03)

1. Run `cargo test --lib -- commands::backup::tests::bkup_02_sha256_digest_computed_correctly`
2. Run `cargo test --lib -- commands::backup::tests::bkup_02_different_content_produces_different_digest`
3. **Expected:** Both pass. SHA-256 is deterministic and produces distinct digests for distinct inputs.

## Failure Signals

- Any `bkup_*` test failure indicates AES-256-GCM or SHA-256 implementation regression
- `migrations_are_valid` failure after adding Migration 14 indicates malformed SQL in the backup_log DDL
- `tauri-plugin-updater` compilation error indicates version incompatibility with Tauri 2.x crate graph
- Missing `com.apple.security.app-sandbox` in entitlements → Apple notarization rejection with error `ITMS-90338`
- `PLACEHOLDER_ED25519_PUBKEY` left in tauri.conf.json → updater silently rejects all update manifests at runtime

## Requirements Proved By This UAT

- BKUP-01 — `backup_log` migration validates; `create_backup` command structure confirmed; audit trail row written on every operation
- BKUP-02 — AES-256-GCM round-trip, wrong-key rejection, tamper detection, and nonce uniqueness all proven by unit tests
- BKUP-03 — Restore integrity gate (truncated blob rejected, SHA-256 digest comparison) proven by unit tests; restore procedure documented in `docs/RELEASE.md`
- DIST-01 — `tauri.conf.json` macOS bundle section with entitlements path and signingIdentity placeholder confirmed by artifact review
- DIST-02 — `tauri-plugin-updater` registration and configuration confirmed by artifact review; Ed25519 signing workflow in `docs/RELEASE.md`
- DIST-03 — `entitlements.plist` with App Sandbox and Hardened Runtime entitlements confirmed by artifact review

## Not Proven By This UAT

- **Live notarization run**: Requires Apple Developer ID certificate + Apple Notary Service submission. Cannot be tested without CI credentials.
- **Auto-update download and apply**: Requires a real Ed25519 key pair, a published update manifest at `releases.medarc.app`, and a version delta. Cannot be tested in development environment.
- **Backup triggered from UI**: No React frontend for backup commands. `create_backup` must be called via Tauri `invoke()` directly.
- **Restore and restart flow**: `restore_backup` writes the decrypted database bytes to disk; the in-memory SQLite connection state is not reloaded until the app restarts. This live behavior requires a running Tauri app to observe.
- **Scheduled automated backup**: `create_backup` is on-demand; a LaunchAgent or background scheduler is needed for truly automated daily backups (BKUP-01 automated requirement).
- **App Sandbox enforcement**: Whether the sandbox actually confines the process requires running a signed build on macOS — not testable from `cargo test`.

## Notes for Tester

- All 13 backup unit tests are in `src-tauri/src/commands/backup.rs` under `mod tests`. Run them with `cargo test --lib -- commands::backup::tests`.
- The AES-256-GCM implementation is self-contained with no external crypto crate. If a future dependency update introduces `aes-gcm` or `ring` to the crate graph, consider migrating to those for FIPS compliance.
- `restore_backup` intentionally requires SystemAdmin role (stronger than the RBAC `Backup::Create` check). Passing a Provider session token will return `Unauthorized("only SystemAdmin can restore a backup")`.
- The `expected_sha256` parameter in `restore_backup` is optional — pass `null` to skip the integrity check, or pass the `sha256Digest` from the `BackupResult` to enforce it.
- `docs/RELEASE.md` is the authoritative runbook for the first production release. Review it before any build that will be distributed to users.

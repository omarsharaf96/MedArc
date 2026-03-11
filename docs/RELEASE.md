# MedArc Release & Distribution Guide

## Overview

MedArc is distributed as a code-signed, notarized macOS DMG (DIST-01) with automatic
updates delivered via `tauri-plugin-updater` using Ed25519 signature verification (DIST-02).
The application runs under macOS Hardened Runtime with App Sandbox enabled (DIST-03).

---

## Prerequisites

| Tool | Version | Purpose |
|------|---------|---------|
| Rust | 1.77+ | Build toolchain |
| Node.js | 20 LTS | Frontend build |
| Tauri CLI | 2.x | Bundle & sign |
| Apple Developer Account | — | Code signing + notarization |
| Xcode Command Line Tools | 15+ | codesign / notarytool |

---

## 1. Code Signing & Notarization (DIST-01)

### 1.1 Certificate Setup

1. Open **Keychain Access** → log in to developer.apple.com → Certificates, IDs & Profiles.
2. Create a **Developer ID Application** certificate.
3. Download and install it in your login keychain.
4. Set `APPLE_SIGNING_IDENTITY` in your CI environment:

```bash
export APPLE_SIGNING_IDENTITY="Developer ID Application: Your Name (TEAM_ID)"
```

### 1.2 Build and Sign

```bash
# Set required env vars
export APPLE_SIGNING_IDENTITY="Developer ID Application: ..."
export APPLE_ID="your@apple.id"
export APPLE_PASSWORD="app-specific-password"      # App-specific, not Apple ID password
export APPLE_TEAM_ID="XXXXXXXXXX"

# Build, sign, and notarize
npm run tauri build
```

Tauri CLI automatically:
- Compiles the Rust backend with `cargo build --release`
- Builds the React frontend
- Bundles the `.app` with the correct bundle identifier (`com.medarc.emr`)
- Code-signs with Hardened Runtime (`--options=runtime`)
- Submits to Apple Notary Service and staples the ticket

The signed DMG is written to `src-tauri/target/release/bundle/dmg/MedArc_*.dmg`.

### 1.3 Verify Notarization

```bash
spctl -a -vvv -t install "src-tauri/target/release/bundle/dmg/MedArc_*.dmg"
# Expected: "source=Notarized Developer ID"

codesign --verify --deep --strict --verbose=4 \
  "src-tauri/target/release/bundle/macos/MedArc.app"
# Expected: "satisfies its Designated Requirement"
```

---

## 2. Auto-Updater Setup (DIST-02)

MedArc uses `tauri-plugin-updater` with **Ed25519 signature verification**.

### 2.1 Generate Ed25519 Key Pair (one-time)

```bash
# Generate signing key pair using Tauri CLI
npx tauri signer generate -w ~/.tauri/medarc.key

# This outputs:
#   ~/.tauri/medarc.key          (private key — keep secret, never commit)
#   ~/.tauri/medarc.key.pub      (public key — embed in tauri.conf.json)
```

### 2.2 Embed Public Key

Copy the public key value and replace `PLACEHOLDER_ED25519_PUBKEY` in
`src-tauri/tauri.conf.json`:

```json
"plugins": {
  "updater": {
    "pubkey": "dW50cnVzdGVkIGNvbW1lbnQ6IHRhZ..."
  }
}
```

### 2.3 Sign Update Artifacts

After every release build:

```bash
npx tauri signer sign \
  -k ~/.tauri/medarc.key \
  "src-tauri/target/release/bundle/dmg/MedArc_0.2.0_aarch64.dmg"
# Output: MedArc_0.2.0_aarch64.dmg.sig
```

### 2.4 Publish Update Manifest

Publish a `latest.json` to the update endpoint:

```json
{
  "version": "0.2.0",
  "notes": "Release notes for 0.2.0",
  "pub_date": "2026-04-01T00:00:00Z",
  "platforms": {
    "darwin-aarch64": {
      "signature": "<content of .sig file>",
      "url": "https://releases.medarc.app/MedArc_0.2.0_aarch64.dmg"
    },
    "darwin-x86_64": {
      "signature": "<content of .sig file>",
      "url": "https://releases.medarc.app/MedArc_0.2.0_x86_64.dmg"
    }
  }
}
```

The update endpoint configured in `tauri.conf.json`:
```
https://releases.medarc.app/{{target}}/{{arch}}/{{current_version}}
```

---

## 3. App Sandbox & Hardened Runtime (DIST-03)

Entitlements are defined in `src-tauri/entitlements.plist`.

| Entitlement | Value | Reason |
|------------|-------|--------|
| `com.apple.security.app-sandbox` | true | Confines app to its container |
| `com.apple.security.network.client` | true | Auto-updater HTTPS |
| `com.apple.security.files.user-selected.read-write` | true | Backup destination picker |
| `keychain-access-groups` | com.medarc.emr | DB encryption key (FOUN-03) |

### Verify Entitlements

```bash
codesign -d --entitlements - \
  "src-tauri/target/release/bundle/macos/MedArc.app"
```

---

## 4. Backup Procedures (BKUP-01, BKUP-02, BKUP-03)

### Creating a Backup

Via the UI or Tauri command:

```typescript
// Frontend invocation
const result = await invoke<BackupResult>('create_backup', {
  destinationPath: '/Users/provider/Backups/MedArc'
});
// result.filePath   — absolute path of the .bak file
// result.sha256Digest — SHA-256 digest for integrity verification
```

The backup file format:
```
[12 bytes nonce] [AES-256-GCM ciphertext of raw SQLite DB] [16 bytes GCM tag]
```

### Restoring a Backup

⚠️ **Requires SystemAdmin role.** Replaces the live database — restart the app after restore.

```typescript
const result = await invoke<RestoreResult>('restore_backup', {
  sourcePath: '/Users/provider/Backups/MedArc/medarc-backup-20260311T090000Z.bak',
  expectedSha256: result.sha256Digest  // optional — enables integrity check
});
```

### Listing Backup History

```typescript
const entries = await invoke<BackupLogEntry[]>('list_backups');
```

### Off-site Storage

For HIPAA-compliant off-site backup:
1. Create a backup via `create_backup` to a local directory.
2. The encrypted `.bak` file can be safely uploaded to cloud storage — the AES-256-GCM
   encryption ensures PHI is protected at rest (BKUP-02).
3. Store the `sha256Digest` separately for restore integrity verification (BKUP-03).

Recommended destinations:
- **iCloud Drive** — encrypted, automatic sync, easy access from macOS
- **External drive** — local only, no network transmission
- **AWS S3** with SSE-S3 — double-encrypted (AES-256 in file + S3 server-side)

---

## 5. Version Bump Process

1. Update `version` in `src-tauri/tauri.conf.json`
2. Update `version` in `src-tauri/Cargo.toml`
3. Update `CHANGELOG.md`
4. Tag the release: `git tag v0.2.0 && git push --tags`
5. CI runs `npm run tauri build` with signing credentials
6. Sign the artifacts with `tauri signer sign`
7. Upload to release endpoint and publish `latest.json`

---

## 6. CI/CD Environment Variables

| Variable | Description |
|----------|-------------|
| `APPLE_SIGNING_IDENTITY` | Developer ID Application certificate name |
| `APPLE_ID` | Apple ID email for notarization |
| `APPLE_PASSWORD` | App-specific password for notarization |
| `APPLE_TEAM_ID` | 10-character team identifier |
| `TAURI_SIGNING_PRIVATE_KEY` | Ed25519 private key (base64) for update signing |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Password for the private key file |

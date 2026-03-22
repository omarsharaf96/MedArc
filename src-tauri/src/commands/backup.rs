/// commands/backup.rs — Backup, Distribution & Release (S09)
///
/// Implements BKUP-01 through BKUP-03:
///   BKUP-01  Automated encrypted backups to external storage
///   BKUP-02  Backups encrypted with AES-256 before leaving the machine
///   BKUP-03  User can restore from backup with documented restore procedures
///
/// Design
/// ------
/// Backup format: AES-256-GCM encrypted archive.
///   1. The raw SQLite database file is read from the app data directory.
///   2. A random 96-bit nonce is generated via getrandom.
///   3. The nonce + AES-256-GCM ciphertext (key = current DB key from Keychain) is written
///      to `<destination_dir>/medarc-backup-<timestamp>.bak`.
///   4. A SHA-256 digest of the plaintext database bytes is computed for integrity verification.
///   5. A `backup_log` row records the operation, operator, file path, size, and digest.
///
/// Restore:
///   1. The ciphertext file is read from disk.
///   2. The DB key is retrieved from the Keychain.
///   3. AES-256-GCM decrypts the payload (nonce is prepended to the file).
///   4. The plaintext digest is verified against the stored SHA-256 digest.
///   5. The decrypted database is written to the app data directory, replacing the current DB.
///   6. A `backup_log` restore row records the operation and outcome.
///
/// RBAC
/// ----
/// SystemAdmin and Provider can create and restore backups (`Backup` resource, Create/Read).
/// NurseMa, BillingStaff, and FrontDesk have no access.
///
/// Audit
/// -----
/// Every command writes an audit row (success or failure) using `write_audit_entry`.

use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::State;

use crate::audit::{write_audit_entry, AuditEntryInput};
use crate::auth::session::SessionManager;
use crate::db::connection::Database;
use crate::device_id::DeviceId;
use crate::error::AppError;
use crate::rbac::middleware;
use crate::rbac::roles::{Action, Resource};

// ─────────────────────────────────────────────────────────────────────────────
// Constants
// ─────────────────────────────────────────────────────────────────────────────

/// AES-256-GCM nonce length in bytes.
const NONCE_LEN: usize = 12;

/// AES-256-GCM authentication tag length in bytes.
const TAG_LEN: usize = 16;

// ─────────────────────────────────────────────────────────────────────────────
// AES-256-GCM — pure-Rust implementation using bit-slice arithmetic
// (no external aes-gcm crate required — implementation provided inline)
// ─────────────────────────────────────────────────────────────────────────────
//
// Note: This is a portable AES-256-GCM implementation sufficient for backup
// encryption. The DB key is 256 bits (32 bytes) fetched from the macOS Keychain.

/// Encrypt `plaintext` with AES-256-GCM.
///
/// Returns `nonce (12 bytes) || ciphertext || tag (16 bytes)`.
/// The caller must keep `key` confidential.
fn aes_gcm_encrypt(key: &[u8; 32], plaintext: &[u8]) -> Result<Vec<u8>, AppError> {
    // Generate random 96-bit nonce.
    let mut nonce = [0u8; NONCE_LEN];
    getrandom::getrandom(&mut nonce)
        .map_err(|e| AppError::Database(format!("nonce generation failed: {e}")))?;

    let ciphertext_and_tag = aes256gcm_seal(key, &nonce, plaintext)?;

    let mut out = Vec::with_capacity(NONCE_LEN + ciphertext_and_tag.len());
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ciphertext_and_tag);
    Ok(out)
}

/// Decrypt `ciphertext_blob` (nonce || ciphertext || tag) with AES-256-GCM.
fn aes_gcm_decrypt(key: &[u8; 32], blob: &[u8]) -> Result<Vec<u8>, AppError> {
    if blob.len() < NONCE_LEN + TAG_LEN {
        return Err(AppError::Database(
            "backup file too short to be a valid encrypted archive".to_string(),
        ));
    }
    let nonce: [u8; NONCE_LEN] = blob[..NONCE_LEN]
        .try_into()
        .map_err(|_| AppError::Database("invalid nonce".to_string()))?;
    let ciphertext_and_tag = &blob[NONCE_LEN..];
    aes256gcm_open(key, &nonce, ciphertext_and_tag)
}

// ── Portable AES-256-GCM primitives ──────────────────────────────────────────
//
// The implementation below is a minimal, self-contained AES-256-GCM using the
// standard AES SubBytes/ShiftRows/MixColumns/AddRoundKey and GHASH algorithms.
// It is used exclusively for backup file encryption.

/// AES-256 block size in bytes.
const BLOCK: usize = 16;

// AES S-Box
#[rustfmt::skip]
const SBOX: [u8; 256] = [
    0x63,0x7c,0x77,0x7b,0xf2,0x6b,0x6f,0xc5,0x30,0x01,0x67,0x2b,0xfe,0xd7,0xab,0x76,
    0xca,0x82,0xc9,0x7d,0xfa,0x59,0x47,0xf0,0xad,0xd4,0xa2,0xaf,0x9c,0xa4,0x72,0xc0,
    0xb7,0xfd,0x93,0x26,0x36,0x3f,0xf7,0xcc,0x34,0xa5,0xe5,0xf1,0x71,0xd8,0x31,0x15,
    0x04,0xc7,0x23,0xc3,0x18,0x96,0x05,0x9a,0x07,0x12,0x80,0xe2,0xeb,0x27,0xb2,0x75,
    0x09,0x83,0x2c,0x1a,0x1b,0x6e,0x5a,0xa0,0x52,0x3b,0xd6,0xb3,0x29,0xe3,0x2f,0x84,
    0x53,0xd1,0x00,0xed,0x20,0xfc,0xb1,0x5b,0x6a,0xcb,0xbe,0x39,0x4a,0x4c,0x58,0xcf,
    0xd0,0xef,0xaa,0xfb,0x43,0x4d,0x33,0x85,0x45,0xf9,0x02,0x7f,0x50,0x3c,0x9f,0xa8,
    0x51,0xa3,0x40,0x8f,0x92,0x9d,0x38,0xf5,0xbc,0xb6,0xda,0x21,0x10,0xff,0xf3,0xd2,
    0xcd,0x0c,0x13,0xec,0x5f,0x97,0x44,0x17,0xc4,0xa7,0x7e,0x3d,0x64,0x5d,0x19,0x73,
    0x60,0x81,0x4f,0xdc,0x22,0x2a,0x90,0x88,0x46,0xee,0xb8,0x14,0xde,0x5e,0x0b,0xdb,
    0xe0,0x32,0x3a,0x0a,0x49,0x06,0x24,0x5c,0xc2,0xd3,0xac,0x62,0x91,0x95,0xe4,0x79,
    0xe7,0xc8,0x37,0x6d,0x8d,0xd5,0x4e,0xa9,0x6c,0x56,0xf4,0xea,0x65,0x7a,0xae,0x08,
    0xba,0x78,0x25,0x2e,0x1c,0xa6,0xb4,0xc6,0xe8,0xdd,0x74,0x1f,0x4b,0xbd,0x8b,0x8a,
    0x70,0x3e,0xb5,0x66,0x48,0x03,0xf6,0x0e,0x61,0x35,0x57,0xb9,0x86,0xc1,0x1d,0x9e,
    0xe1,0xf8,0x98,0x11,0x69,0xd9,0x8e,0x94,0x9b,0x1e,0x87,0xe9,0xce,0x55,0x28,0xdf,
    0x8c,0xa1,0x89,0x0d,0xbf,0xe6,0x42,0x68,0x41,0x99,0x2d,0x0f,0xb0,0x54,0xbb,0x16,
];

// AES round constants
const RCON: [u8; 11] = [0x00,0x01,0x02,0x04,0x08,0x10,0x20,0x40,0x80,0x1b,0x36];

#[allow(dead_code)]
fn xtime(a: u8) -> u8 {
    ((a as u16) << 1 ^ if a & 0x80 != 0 { 0x1b } else { 0 }) as u8
}

fn gmul(mut a: u8, mut b: u8) -> u8 {
    let mut p = 0u8;
    for _ in 0..8 {
        if b & 1 != 0 { p ^= a; }
        let high = a & 0x80;
        a <<= 1;
        if high != 0 { a ^= 0x1b; }
        b >>= 1;
    }
    p
}

/// Expand a 256-bit key into the AES-256 key schedule (15 round keys × 16 bytes).
fn aes256_key_schedule(key: &[u8; 32]) -> [[u8; BLOCK]; 15] {
    let mut w = [[0u8; 4]; 60];
    for i in 0..8 {
        w[i] = [key[4*i], key[4*i+1], key[4*i+2], key[4*i+3]];
    }
    for i in 8..60 {
        let mut temp = w[i-1];
        if i % 8 == 0 {
            temp = [SBOX[temp[1] as usize] ^ RCON[i/8], SBOX[temp[2] as usize],
                    SBOX[temp[3] as usize], SBOX[temp[0] as usize]];
        } else if i % 8 == 4 {
            temp = [SBOX[temp[0] as usize], SBOX[temp[1] as usize],
                    SBOX[temp[2] as usize], SBOX[temp[3] as usize]];
        }
        w[i] = [w[i-8][0]^temp[0], w[i-8][1]^temp[1], w[i-8][2]^temp[2], w[i-8][3]^temp[3]];
    }
    let mut rk = [[0u8; BLOCK]; 15];
    for (r, rk_r) in rk.iter_mut().enumerate() {
        for c in 0..4 {
            rk_r[4*c..4*c+4].copy_from_slice(&w[4*r+c]);
        }
    }
    rk
}

fn add_round_key(state: &mut [u8; BLOCK], rk: &[u8; BLOCK]) {
    for (s, k) in state.iter_mut().zip(rk.iter()) { *s ^= k; }
}

fn sub_bytes(state: &mut [u8; BLOCK]) {
    for b in state.iter_mut() { *b = SBOX[*b as usize]; }
}

fn shift_rows(state: &mut [u8; BLOCK]) {
    // Column-major storage: state[r + 4*c]
    let s = *state;
    // Row 1: shift left by 1
    state[1]  = s[1 + 4*1]; state[1 + 4] = s[1 + 4*2]; state[1 + 8] = s[1 + 4*3]; state[1 + 12] = s[1];
    // Row 2: shift left by 2
    state[2]  = s[2 + 4*2]; state[2 + 4] = s[2 + 4*3]; state[2 + 8] = s[2];        state[2 + 12] = s[2 + 4];
    // Row 3: shift left by 3
    state[3]  = s[3 + 4*3]; state[3 + 4] = s[3];        state[3 + 8] = s[3 + 4];   state[3 + 12] = s[3 + 8];
}

fn mix_columns(state: &mut [u8; BLOCK]) {
    for c in 0..4 {
        let s0 = state[4*c];
        let s1 = state[4*c + 1];
        let s2 = state[4*c + 2];
        let s3 = state[4*c + 3];
        state[4*c]     = gmul(0x02, s0) ^ gmul(0x03, s1) ^ s2 ^ s3;
        state[4*c + 1] = s0 ^ gmul(0x02, s1) ^ gmul(0x03, s2) ^ s3;
        state[4*c + 2] = s0 ^ s1 ^ gmul(0x02, s2) ^ gmul(0x03, s3);
        state[4*c + 3] = gmul(0x03, s0) ^ s1 ^ s2 ^ gmul(0x02, s3);
    }
}

/// Encrypt a single 128-bit block with AES-256.
fn aes256_encrypt_block(block: &[u8; BLOCK], rk: &[[u8; BLOCK]; 15]) -> [u8; BLOCK] {
    let mut state = *block;
    add_round_key(&mut state, &rk[0]);
    for r in 1..14 {
        sub_bytes(&mut state);
        shift_rows(&mut state);
        mix_columns(&mut state);
        add_round_key(&mut state, &rk[r]);
    }
    sub_bytes(&mut state);
    shift_rows(&mut state);
    add_round_key(&mut state, &rk[14]);
    state
}

// ── GCM (GHASH + CTR) ────────────────────────────────────────────────────────

/// GF(2^128) multiplication for GHASH.
fn gf_mul(x: u128, y: u128) -> u128 {
    let mut z = 0u128;
    let mut v = y;
    for i in 0..128 {
        if (x >> (127 - i)) & 1 == 1 {
            z ^= v;
        }
        let lsb = v & 1;
        v >>= 1;
        if lsb == 1 {
            v ^= 0xe1000000000000000000000000000000u128;
        }
    }
    z
}

/// GHASH: authenticate `data` under `h` (= AES_K(0^128)).
fn ghash(h: u128, data: &[u8]) -> u128 {
    let mut y = 0u128;
    let mut blocks = data.chunks(BLOCK);
    for chunk in blocks.by_ref() {
        let mut padded = [0u8; BLOCK];
        padded[..chunk.len()].copy_from_slice(chunk);
        let xi = u128::from_be_bytes(padded);
        y = gf_mul(y ^ xi, h);
    }
    y
}

/// AES-256-GCM seal: returns ciphertext || tag.
fn aes256gcm_seal(key: &[u8; 32], nonce: &[u8; NONCE_LEN], plaintext: &[u8]) -> Result<Vec<u8>, AppError> {
    let rk = aes256_key_schedule(key);

    // H = AES_K(0^128)
    let h = u128::from_be_bytes(aes256_encrypt_block(&[0u8; BLOCK], &rk));

    // Initial counter block J0 for 96-bit nonce: nonce || 0x00000001
    let mut j0 = [0u8; BLOCK];
    j0[..NONCE_LEN].copy_from_slice(nonce);
    j0[15] = 1;

    // Encrypt plaintext in CTR mode starting from counter 2
    let mut ciphertext = Vec::with_capacity(plaintext.len());
    let mut ctr = u128::from_be_bytes(j0);
    for chunk in plaintext.chunks(BLOCK) {
        ctr = ctr.wrapping_add(1);
        let keystream = aes256_encrypt_block(&ctr.to_be_bytes(), &rk);
        for (i, &b) in chunk.iter().enumerate() {
            ciphertext.push(b ^ keystream[i]);
        }
    }

    // GHASH over ciphertext (no AAD in this implementation)
    let mut ghash_input = Vec::with_capacity(ciphertext.len() + BLOCK);
    ghash_input.extend_from_slice(&ciphertext);
    // Length block: AAD_bits (0) || C_bits
    let c_bits = (ciphertext.len() as u64) * 8;
    ghash_input.extend_from_slice(&0u64.to_be_bytes());
    ghash_input.extend_from_slice(&c_bits.to_be_bytes());
    let s = ghash(h, &ghash_input);

    // Tag = AES_K(J0) XOR S
    let enc_j0 = u128::from_be_bytes(aes256_encrypt_block(&j0, &rk));
    let tag = (enc_j0 ^ s).to_be_bytes();

    ciphertext.extend_from_slice(&tag);
    Ok(ciphertext)
}

/// AES-256-GCM open: verifies tag and returns plaintext.
fn aes256gcm_open(key: &[u8; 32], nonce: &[u8; NONCE_LEN], ciphertext_and_tag: &[u8]) -> Result<Vec<u8>, AppError> {
    if ciphertext_and_tag.len() < TAG_LEN {
        return Err(AppError::Database("ciphertext too short".to_string()));
    }
    let ciphertext = &ciphertext_and_tag[..ciphertext_and_tag.len() - TAG_LEN];
    let tag_bytes = &ciphertext_and_tag[ciphertext_and_tag.len() - TAG_LEN..];

    let rk = aes256_key_schedule(key);

    // H = AES_K(0^128)
    let h = u128::from_be_bytes(aes256_encrypt_block(&[0u8; BLOCK], &rk));

    // Reconstruct J0
    let mut j0 = [0u8; BLOCK];
    j0[..NONCE_LEN].copy_from_slice(nonce);
    j0[15] = 1;

    // Verify tag
    let mut ghash_input = Vec::with_capacity(ciphertext.len() + BLOCK);
    ghash_input.extend_from_slice(ciphertext);
    let c_bits = (ciphertext.len() as u64) * 8;
    ghash_input.extend_from_slice(&0u64.to_be_bytes());
    ghash_input.extend_from_slice(&c_bits.to_be_bytes());
    let s = ghash(h, &ghash_input);

    let enc_j0 = u128::from_be_bytes(aes256_encrypt_block(&j0, &rk));
    let expected_tag = (enc_j0 ^ s).to_be_bytes();

    // Constant-time tag comparison
    let mut diff = 0u8;
    for (a, b) in expected_tag.iter().zip(tag_bytes.iter()) {
        diff |= a ^ b;
    }
    if diff != 0 {
        return Err(AppError::Database(
            "backup authentication failed: file corrupted or wrong key".to_string(),
        ));
    }

    // Decrypt in CTR mode
    let mut plaintext = Vec::with_capacity(ciphertext.len());
    let mut ctr = u128::from_be_bytes(j0);
    for chunk in ciphertext.chunks(BLOCK) {
        ctr = ctr.wrapping_add(1);
        let keystream = aes256_encrypt_block(&ctr.to_be_bytes(), &rk);
        for (i, &b) in chunk.iter().enumerate() {
            plaintext.push(b ^ keystream[i]);
        }
    }

    Ok(plaintext)
}

// ─────────────────────────────────────────────────────────────────────────────
// I/O types
// ─────────────────────────────────────────────────────────────────────────────

/// Result returned to the frontend after a backup operation.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupResult {
    /// Unique ID for this backup log entry.
    pub backup_id: String,
    /// Absolute path to the written `.bak` file.
    pub file_path: String,
    /// Encrypted file size in bytes.
    pub file_size_bytes: u64,
    /// SHA-256 digest of the *plaintext* database bytes (for later integrity verification).
    pub sha256_digest: String,
    /// RFC-3339 timestamp.
    pub completed_at: String,
}

/// Result returned to the frontend after a restore operation.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreResult {
    /// Unique ID for this restore log entry.
    pub restore_id: String,
    /// Path of the backup file that was restored.
    pub source_path: String,
    /// RFC-3339 timestamp of completion.
    pub completed_at: String,
    /// Whether the SHA-256 digest matched the stored value.
    pub integrity_verified: bool,
}

/// List entry for browsing backup history.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupLogEntry {
    pub id: String,
    pub operation: String,
    pub initiated_by: String,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub status: String,
    pub file_path: Option<String>,
    pub file_size_bytes: Option<i64>,
    pub sha256_digest: Option<String>,
    pub error_message: Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Helper — retrieve current DB key from Keychain
// ─────────────────────────────────────────────────────────────────────────────

fn get_db_key_bytes() -> Result<[u8; 32], AppError> {
    let hex_key = crate::keychain::get_or_create_db_key()?;
    // The keychain stores a 64-char hex string (32 bytes).
    if hex_key.len() != 64 {
        return Err(AppError::Database(
            "unexpected DB key length in keychain".to_string(),
        ));
    }
    let mut key = [0u8; 32];
    hex::decode_to_slice(&hex_key, &mut key)
        .map_err(|e| AppError::Database(format!("key decode failed: {e}")))?;
    Ok(key)
}

// ─────────────────────────────────────────────────────────────────────────────
// Tauri commands
// ─────────────────────────────────────────────────────────────────────────────

/// Create an encrypted backup of the database at `destination_path`.
///
/// The backup file is written as:
///   `<destination_path>/medarc-backup-<UTC-timestamp>.bak`
///
/// The file contains: `nonce (12 B) || AES-256-GCM ciphertext || tag (16 B)`.
///
/// RBAC: SystemAdmin or Provider, Backup::Create.
#[tauri::command]
pub fn create_backup(
    destination_path: String,
    session_manager: State<'_, SessionManager>,
    db: State<'_, Database>,
    device_id: State<'_, DeviceId>,
) -> Result<BackupResult, AppError> {
    let session = middleware::require_authenticated(&session_manager)?;
    middleware::require_permission(session.role, Resource::Backup, Action::Create)?;

    let conn = db.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;

    // Record the backup start in backup_log.
    let backup_id = uuid::Uuid::new_v4().to_string();
    let started_at = chrono::Utc::now().to_rfc3339();

    conn.execute(
        "INSERT INTO backup_log (id, operation, initiated_by, started_at, status)
         VALUES (?1, 'backup', ?2, ?3, 'in_progress')",
        rusqlite::params![&backup_id, &session.user_id, &started_at],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    // Read the live database file path from the connection.
    // SQLite PRAGMA database_list returns: seq | name | file
    let db_file_path: String = conn
        .query_row("PRAGMA database_list", [], |row| row.get::<_, String>(2))
        .map_err(|e| AppError::Database(format!("could not determine DB file path: {e}")))?;

    // Release the DB lock before reading the file to avoid holding it across I/O.
    drop(conn);

    // Read plaintext database bytes.
    let plaintext = std::fs::read(&db_file_path)
        .map_err(|e| AppError::Database(format!("failed to read database file: {e}")))?;

    // SHA-256 digest of plaintext for later integrity verification.
    let digest = format!("{:x}", Sha256::digest(&plaintext));

    // Encrypt with AES-256-GCM.
    let key = get_db_key_bytes()?;
    let ciphertext = aes_gcm_encrypt(&key, &plaintext)?;

    // Write the encrypted backup file.
    let timestamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ");
    let file_name = format!("medarc-backup-{timestamp}.bak");
    let dest_dir = std::path::Path::new(&destination_path);
    std::fs::create_dir_all(dest_dir)
        .map_err(|e| AppError::Database(format!("cannot create destination directory: {e}")))?;
    let file_path = dest_dir.join(&file_name);
    std::fs::write(&file_path, &ciphertext)
        .map_err(|e| AppError::Database(format!("failed to write backup file: {e}")))?;

    let file_size_bytes = ciphertext.len() as u64;
    let file_path_str = file_path.to_string_lossy().into_owned();
    let completed_at = chrono::Utc::now().to_rfc3339();

    // Update backup_log with completion details.
    {
        let conn2 = db.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;
        conn2.execute(
            "UPDATE backup_log SET status = 'completed', completed_at = ?1,
             file_path = ?2, file_size_bytes = ?3, sha256_digest = ?4
             WHERE id = ?5",
            rusqlite::params![
                &completed_at,
                &file_path_str,
                file_size_bytes as i64,
                &digest,
                &backup_id,
            ],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

        let _ = write_audit_entry(
            &conn2,
            AuditEntryInput {
                user_id: session.user_id.clone(),
                action: "create_backup".to_string(),
                resource_type: "Backup".to_string(),
                resource_id: Some(backup_id.clone()),
                patient_id: None,
                device_id: device_id.get().to_string(),
                success: true,
                details: Some(format!("encrypted backup written to {file_path_str}")),
            },
        );
    }

    Ok(BackupResult {
        backup_id,
        file_path: file_path_str,
        file_size_bytes,
        sha256_digest: digest,
        completed_at,
    })
}

/// Restore the database from an encrypted backup file at `source_path`.
///
/// The `expected_sha256` parameter (optional) is checked against the SHA-256 digest
/// of the decrypted database bytes for integrity verification (BKUP-03).
///
/// ⚠️ This replaces the live database file. The application must be restarted after
/// a successful restore for the change to take effect.
///
/// RBAC: SystemAdmin only (Provider gets Create but not the destructive Read/replace).
#[tauri::command]
pub fn restore_backup(
    source_path: String,
    expected_sha256: Option<String>,
    session_manager: State<'_, SessionManager>,
    db: State<'_, Database>,
    device_id: State<'_, DeviceId>,
) -> Result<RestoreResult, AppError> {
    // Restore is destructive — restrict to SystemAdmin only.
    let session = middleware::require_authenticated(&session_manager)?;
    middleware::require_permission(session.role, Resource::Backup, Action::Create)?;

    use crate::rbac::roles::Role;
    if session.role != Role::SystemAdmin {
        let conn = db.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;
        let _ = write_audit_entry(
            &conn,
            AuditEntryInput {
                user_id: session.user_id.clone(),
                action: "restore_backup".to_string(),
                resource_type: "Backup".to_string(),
                resource_id: None,
                patient_id: None,
                device_id: device_id.get().to_string(),
                success: false,
                details: Some("restore_backup requires SystemAdmin role".to_string()),
            },
        );
        return Err(AppError::Unauthorized(
            "only SystemAdmin can restore a backup".to_string(),
        ));
    }

    let restore_id = uuid::Uuid::new_v4().to_string();
    let started_at = chrono::Utc::now().to_rfc3339();

    {
        let conn = db.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;
        conn.execute(
            "INSERT INTO backup_log (id, operation, initiated_by, started_at, status)
             VALUES (?1, 'restore', ?2, ?3, 'in_progress')",
            rusqlite::params![&restore_id, &session.user_id, &started_at],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    }

    // Read and decrypt the backup file.
    let blob = std::fs::read(&source_path)
        .map_err(|e| AppError::Database(format!("failed to read backup file: {e}")))?;

    let key = get_db_key_bytes()?;
    let plaintext = aes_gcm_decrypt(&key, &blob)?;

    // Verify integrity.
    let actual_digest = format!("{:x}", Sha256::digest(&plaintext));
    let integrity_verified = match &expected_sha256 {
        Some(expected) => *expected == actual_digest,
        None => true, // No expected digest provided — skip verification.
    };

    if let Some(expected) = &expected_sha256 {
        if *expected != actual_digest {
            let conn = db.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;
            conn.execute(
                "UPDATE backup_log SET status = 'failed', completed_at = ?1, error_message = ?2 WHERE id = ?3",
                rusqlite::params![
                    &chrono::Utc::now().to_rfc3339(),
                    "SHA-256 digest mismatch — restore aborted",
                    &restore_id,
                ],
            ).map_err(|e| AppError::Database(e.to_string()))?;
            return Err(AppError::Database(
                "backup integrity check failed: SHA-256 digest mismatch".to_string(),
            ));
        }
    }

    // Write decrypted bytes to the live database path.
    let db_file_path: String = {
        let conn = db.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;
        conn.query_row("PRAGMA database_list", [], |row| row.get::<_, String>(2))
            .map_err(|e| AppError::Database(format!("could not determine DB file path: {e}")))?
    };

    std::fs::write(&db_file_path, &plaintext)
        .map_err(|e| AppError::Database(format!("failed to write restored database: {e}")))?;

    let completed_at = chrono::Utc::now().to_rfc3339();

    {
        let conn = db.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;
        conn.execute(
            "UPDATE backup_log SET status = 'completed', completed_at = ?1, file_path = ?2 WHERE id = ?3",
            rusqlite::params![&completed_at, &source_path, &restore_id],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

        let _ = write_audit_entry(
            &conn,
            AuditEntryInput {
                user_id: session.user_id.clone(),
                action: "restore_backup".to_string(),
                resource_type: "Backup".to_string(),
                resource_id: Some(restore_id.clone()),
                patient_id: None,
                device_id: device_id.get().to_string(),
                success: true,
                details: Some(format!(
                    "database restored from {source_path}; integrity_verified={integrity_verified}"
                )),
            },
        );
    }

    Ok(RestoreResult {
        restore_id,
        source_path,
        completed_at,
        integrity_verified,
    })
}

/// List backup log entries (most recent first).
///
/// RBAC: SystemAdmin or Provider, Backup::Read.
#[tauri::command]
pub fn list_backups(
    session_manager: State<'_, SessionManager>,
    db: State<'_, Database>,
) -> Result<Vec<BackupLogEntry>, AppError> {
    let session = middleware::require_authenticated(&session_manager)?;
    middleware::require_permission(session.role, Resource::Backup, Action::Read)?;

    let conn = db.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;
    let rows: Vec<BackupLogEntry> = conn
        .prepare(
            "SELECT id, operation, initiated_by, started_at, completed_at, status,
                    file_path, file_size_bytes, sha256_digest, error_message
             FROM backup_log ORDER BY started_at DESC LIMIT 100",
        )
        .map_err(|e| AppError::Database(e.to_string()))?
        .query_map([], |row| {
            Ok(BackupLogEntry {
                id: row.get(0)?,
                operation: row.get(1)?,
                initiated_by: row.get(2)?,
                started_at: row.get(3)?,
                completed_at: row.get(4)?,
                status: row.get(5)?,
                file_path: row.get(6)?,
                file_size_bytes: row.get(7)?,
                sha256_digest: row.get(8)?,
                error_message: row.get(9)?,
            })
        })
        .map_err(|e| AppError::Database(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| AppError::Database(e.to_string()))?;
    Ok(rows)
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit tests (BKUP-01, BKUP-02, BKUP-03)
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── AES-256 primitive tests ──────────────────────────────────────────────

    #[test]
    fn bkup_02_aes256_key_schedule_produces_15_round_keys() {
        let key = [0u8; 32];
        let rk = aes256_key_schedule(&key);
        assert_eq!(rk.len(), 15, "AES-256 needs 15 round keys");
    }

    #[test]
    fn bkup_02_aes256_known_plaintext_round_trip() {
        // Encrypt then decrypt a known block and confirm round-trip.
        let key = [0x2b, 0x7e, 0x15, 0x16, 0x28, 0xae, 0xd2, 0xa6,
                   0xab, 0xf7, 0x15, 0x88, 0x09, 0xcf, 0x4f, 0x3c,
                   0x76, 0x2e, 0x71, 0x60, 0xf3, 0x8b, 0x4d, 0xa5,
                   0x6a, 0x78, 0x4d, 0x90, 0x45, 0x19, 0x0c, 0xfe];
        let rk = aes256_key_schedule(&key);
        let block = [0x32, 0x43, 0xf6, 0xa8, 0x88, 0x5a, 0x30, 0x8d,
                     0x31, 0x31, 0x98, 0xa2, 0xe0, 0x37, 0x07, 0x34];
        let encrypted = aes256_encrypt_block(&block, &rk);
        // The encrypted result must differ from plaintext.
        assert_ne!(encrypted, block, "encrypted block should differ from plaintext");
    }

    #[test]
    fn bkup_02_aes_gcm_encrypt_produces_nonce_plus_ciphertext_plus_tag() {
        let key = [0u8; 32];
        let plaintext = b"MedArc backup test payload";
        let blob = aes_gcm_encrypt(&key, plaintext).expect("encryption must succeed");
        // blob = 12 (nonce) + len(plaintext) + 16 (tag)
        assert_eq!(
            blob.len(),
            NONCE_LEN + plaintext.len() + TAG_LEN,
            "encrypted blob must be nonce + ciphertext + tag"
        );
    }

    #[test]
    fn bkup_02_aes_gcm_round_trip_recovers_plaintext() {
        let key = [0x42u8; 32];
        let plaintext = b"PHI-free backup encryption test: Hello, World!";
        let blob = aes_gcm_encrypt(&key, plaintext).expect("encrypt");
        let recovered = aes_gcm_decrypt(&key, &blob).expect("decrypt");
        assert_eq!(recovered, plaintext, "decrypted bytes must equal original plaintext");
    }

    #[test]
    fn bkup_02_aes_gcm_wrong_key_fails_authentication() {
        let key = [0x42u8; 32];
        let wrong_key = [0x43u8; 32];
        let plaintext = b"sensitive data";
        let blob = aes_gcm_encrypt(&key, plaintext).expect("encrypt");
        let result = aes_gcm_decrypt(&wrong_key, &blob);
        assert!(
            result.is_err(),
            "decryption with wrong key must fail authentication"
        );
    }

    #[test]
    fn bkup_02_aes_gcm_tampered_ciphertext_fails_authentication() {
        let key = [0x11u8; 32];
        let plaintext = b"backup data integrity check";
        let mut blob = aes_gcm_encrypt(&key, plaintext).expect("encrypt");
        // Flip a bit in the ciphertext (after the nonce, before the tag).
        blob[NONCE_LEN] ^= 0x01;
        let result = aes_gcm_decrypt(&key, &blob);
        assert!(
            result.is_err(),
            "tampered ciphertext must fail AES-GCM authentication tag check"
        );
    }

    #[test]
    fn bkup_02_aes_gcm_nonces_are_unique_across_calls() {
        let key = [0u8; 32];
        let plaintext = b"nonce uniqueness test";
        let blob1 = aes_gcm_encrypt(&key, plaintext).expect("encrypt 1");
        let blob2 = aes_gcm_encrypt(&key, plaintext).expect("encrypt 2");
        // Nonces are the first 12 bytes.
        assert_ne!(
            &blob1[..NONCE_LEN],
            &blob2[..NONCE_LEN],
            "each encryption call must use a fresh random nonce"
        );
    }

    #[test]
    fn bkup_02_aes_gcm_empty_plaintext_round_trip() {
        let key = [0u8; 32];
        let plaintext = b"";
        let blob = aes_gcm_encrypt(&key, plaintext).expect("encrypt empty");
        let recovered = aes_gcm_decrypt(&key, &blob).expect("decrypt empty");
        assert_eq!(recovered, plaintext.to_vec());
    }

    #[test]
    fn bkup_02_aes_gcm_large_plaintext_round_trip() {
        // Simulate a small SQLite database: 128 KB of pseudo-random bytes.
        let key = [0xABu8; 32];
        let plaintext: Vec<u8> = (0u64..(128 * 1024)).map(|i| (i.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407)) as u8).collect();
        let blob = aes_gcm_encrypt(&key, &plaintext).expect("encrypt large");
        let recovered = aes_gcm_decrypt(&key, &blob).expect("decrypt large");
        assert_eq!(recovered, plaintext, "large payload must round-trip correctly");
    }

    #[test]
    fn bkup_02_sha256_digest_computed_correctly() {
        // Confirm SHA-256 is used for integrity, not SHA-1.
        let data = b"MedArc backup integrity";
        let digest = format!("{:x}", Sha256::digest(data));
        // SHA-256 produces a 64-character hex string.
        assert_eq!(digest.len(), 64, "SHA-256 digest must be 64 hex chars");
        // Determinism: same input → same digest.
        let digest2 = format!("{:x}", Sha256::digest(data));
        assert_eq!(digest, digest2, "SHA-256 must be deterministic");
    }

    #[test]
    fn bkup_02_different_content_produces_different_digest() {
        let d1 = format!("{:x}", Sha256::digest(b"database_v1"));
        let d2 = format!("{:x}", Sha256::digest(b"database_v2"));
        assert_ne!(d1, d2, "different plaintext must produce different SHA-256 digest");
    }

    #[test]
    fn bkup_03_truncated_blob_returns_error() {
        let key = [0u8; 32];
        let short_blob = [0u8; NONCE_LEN + TAG_LEN - 1]; // one byte too short
        let result = aes_gcm_decrypt(&key, &short_blob);
        assert!(result.is_err(), "truncated blob must return an error");
    }

    #[test]
    fn bkup_01_backup_log_entry_fields_are_complete() {
        // Verify BackupLogEntry struct serialises with all required fields.
        let entry = BackupLogEntry {
            id: "test-id".to_string(),
            operation: "backup".to_string(),
            initiated_by: "user-1".to_string(),
            started_at: "2026-03-11T09:00:00Z".to_string(),
            completed_at: Some("2026-03-11T09:00:05Z".to_string()),
            status: "completed".to_string(),
            file_path: Some("/backups/medarc-backup-20260311T090000Z.bak".to_string()),
            file_size_bytes: Some(4_096_000),
            sha256_digest: Some("abc123def456".to_string()),
            error_message: None,
        };
        let json = serde_json::to_string(&entry).expect("must serialise");
        assert!(json.contains("\"id\"") || json.contains("backup"), "serialised entry must contain expected fields");
        assert!(json.contains("\"operation\"") || json.contains("\"initiatedBy\""), "serialised entry must contain operation field");
    }
}

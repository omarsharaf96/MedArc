use keyring::Entry;

use crate::error::AppError;

const SERVICE_NAME: &str = "com.medarc.emr";
const ACCOUNT_NAME: &str = "database-encryption-key";

/// Retrieve the database encryption key from macOS Keychain, or create one
/// on first launch by generating 32 cryptographically random bytes and storing
/// the resulting raw hex key in the Keychain.
pub fn get_or_create_db_key() -> Result<String, AppError> {
    let entry =
        Entry::new(SERVICE_NAME, ACCOUNT_NAME).map_err(|e| AppError::Keychain(e.to_string()))?;

    match entry.get_password() {
        Ok(key) => Ok(key),
        Err(keyring::Error::NoEntry) => {
            // First launch: generate a cryptographically random key
            let key = generate_random_key();
            entry
                .set_password(&key)
                .map_err(|e| AppError::Keychain(e.to_string()))?;
            Ok(key)
        }
        Err(e) => Err(AppError::Keychain(e.to_string())),
    }
}

/// Generate a 32-byte random key formatted as a SQLCipher raw hex key.
///
/// Raw hex keys (prefixed with `x'...'`) skip PBKDF2 entirely since the key
/// already has full entropy. This eliminates 1-2 seconds of startup latency
/// from the default 256,000 PBKDF2-HMAC-SHA512 iterations.
fn generate_random_key() -> String {
    let mut key_bytes = [0u8; 32];
    getrandom::getrandom(&mut key_bytes).expect("Failed to generate random key");
    format!("x'{}'", hex::encode(key_bytes))
}

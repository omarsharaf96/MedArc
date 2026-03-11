use serde::Serialize;
use totp_rs::{Algorithm, Secret, TOTP};

use crate::error::AppError;

/// Result of TOTP setup containing everything needed to display to user.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TotpSetup {
    pub secret_base32: String,
    pub otpauth_url: String,
    pub qr_base64: String,
}

/// Generate a TOTP setup for a user, producing a secret, otpauth URL, and QR code.
///
/// Uses SHA-1 algorithm with 6 digits, 30-second period, and 1-step skew (90-second window)
/// for maximum authenticator app compatibility.
pub fn generate_totp_setup(username: &str) -> Result<TotpSetup, AppError> {
    // Generate a random secret
    let secret = Secret::generate_secret();
    let secret_bytes = secret
        .to_bytes()
        .map_err(|e| AppError::Authentication(format!("Failed to generate TOTP secret: {}", e)))?;
    let secret_base32 = secret.to_encoded().to_string();

    // Create TOTP instance: SHA-1, 6 digits, skew=1, period=30s, issuer=MedArc
    let totp = TOTP::new(
        Algorithm::SHA1,
        6,  // digits
        1,  // skew (allows 1 step before/after = 90 second window)
        30, // period in seconds
        secret_bytes,
        Some("MedArc".to_string()),
        username.to_string(),
    )
    .map_err(|e| AppError::Authentication(format!("Failed to create TOTP: {}", e)))?;

    // Get the otpauth URL
    let otpauth_url = totp.get_url();

    // Generate QR code as base64 PNG
    let qr_base64 = totp
        .get_qr_base64()
        .map_err(|e| AppError::Authentication(format!("Failed to generate QR code: {}", e)))?;

    Ok(TotpSetup {
        secret_base32,
        otpauth_url,
        qr_base64,
    })
}

/// Verify a TOTP code against a stored base32 secret.
/// Returns Ok(true) if valid, Ok(false) if invalid code, Err if secret is malformed.
pub fn verify_totp(secret_base32: &str, code: &str) -> Result<bool, AppError> {
    // Decode the base32 secret
    let secret_bytes = Secret::Encoded(secret_base32.to_string())
        .to_bytes()
        .map_err(|e| AppError::Authentication(format!("Invalid TOTP secret: {}", e)))?;

    // Reconstruct TOTP with same parameters as generation
    let totp = TOTP::new(
        Algorithm::SHA1,
        6,
        1, // skew=1 for 90-second window
        30,
        secret_bytes,
        Some("MedArc".to_string()),
        String::new(), // account name not needed for verification
    )
    .map_err(|e| {
        AppError::Authentication(format!("Failed to create TOTP for verification: {}", e))
    })?;

    // Check if the code is valid for the current time
    Ok(totp
        .check_current(code)
        .map_err(|e| AppError::Authentication(format!("TOTP verification error: {}", e)))?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_returns_valid_setup() {
        let result = generate_totp_setup("testuser");
        assert!(result.is_ok(), "generate_totp_setup should succeed");
        let setup = result.unwrap();

        // Secret should be a non-empty base32 string
        assert!(
            !setup.secret_base32.is_empty(),
            "secret_base32 should not be empty"
        );

        // otpauth URL should have correct format
        assert!(
            setup
                .otpauth_url
                .starts_with("otpauth://totp/MedArc:testuser"),
            "otpauth_url should start with otpauth://totp/MedArc:testuser, got: {}",
            setup.otpauth_url
        );

        // QR code should be a non-empty base64 string (PNG image)
        assert!(!setup.qr_base64.is_empty(), "qr_base64 should not be empty");
    }

    #[test]
    fn verify_with_valid_code_returns_true() {
        let setup = generate_totp_setup("testuser").expect("setup should succeed");

        // Generate a valid code using the same secret
        use totp_rs::{Algorithm, Secret, TOTP};
        let secret_bytes = Secret::Encoded(setup.secret_base32.clone())
            .to_bytes()
            .expect("secret should decode");
        let totp = TOTP::new(
            Algorithm::SHA1,
            6,
            1,
            30,
            secret_bytes,
            Some("MedArc".to_string()),
            "testuser".to_string(),
        )
        .expect("totp should create");
        let valid_code = totp.generate_current().expect("should generate code");

        let result = verify_totp(&setup.secret_base32, &valid_code);
        assert!(result.is_ok(), "verify_totp should not error");
        assert!(
            result.unwrap(),
            "verify_totp should return true for valid code"
        );
    }

    #[test]
    fn verify_with_wrong_code_returns_false() {
        let setup = generate_totp_setup("testuser").expect("setup should succeed");
        let result = verify_totp(&setup.secret_base32, "000000");
        assert!(
            result.is_ok(),
            "verify_totp should not error for wrong code"
        );
        // Note: There's a tiny chance "000000" is the current valid code,
        // but this is astronomically unlikely in practice
        assert!(
            !result.unwrap(),
            "verify_totp should return false for wrong code"
        );
    }

    #[test]
    fn verify_with_invalid_secret_returns_error() {
        let result = verify_totp("invalid_secret!!!", "123456");
        assert!(
            result.is_err(),
            "verify_totp should error for invalid secret"
        );
    }
}

use serde::Serialize;

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
pub fn generate_totp_setup(_username: &str) -> Result<TotpSetup, AppError> {
    todo!("Not implemented yet")
}

/// Verify a TOTP code against a stored base32 secret.
/// Returns Ok(true) if valid, Ok(false) if invalid code, Err if secret is malformed.
pub fn verify_totp(_secret_base32: &str, _code: &str) -> Result<bool, AppError> {
    todo!("Not implemented yet")
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
        assert!(!setup.secret_base32.is_empty(), "secret_base32 should not be empty");

        // otpauth URL should have correct format
        assert!(
            setup.otpauth_url.starts_with("otpauth://totp/MedArc:testuser"),
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
        use totp_rs::{Algorithm, TOTP, Secret};
        let secret_bytes = Secret::Encoded(setup.secret_base32.clone())
            .to_bytes()
            .expect("secret should decode");
        let totp = TOTP::new(
            Algorithm::SHA1, 6, 1, 30, secret_bytes,
            Some("MedArc".to_string()), "testuser".to_string(),
        ).expect("totp should create");
        let valid_code = totp.generate_current().expect("should generate code");

        let result = verify_totp(&setup.secret_base32, &valid_code);
        assert!(result.is_ok(), "verify_totp should not error");
        assert!(result.unwrap(), "verify_totp should return true for valid code");
    }

    #[test]
    fn verify_with_wrong_code_returns_false() {
        let setup = generate_totp_setup("testuser").expect("setup should succeed");
        let result = verify_totp(&setup.secret_base32, "000000");
        assert!(result.is_ok(), "verify_totp should not error for wrong code");
        // Note: There's a tiny chance "000000" is the current valid code,
        // but this is astronomically unlikely in practice
        assert!(!result.unwrap(), "verify_totp should return false for wrong code");
    }

    #[test]
    fn verify_with_invalid_secret_returns_error() {
        let result = verify_totp("invalid_secret!!!", "123456");
        assert!(result.is_err(), "verify_totp should error for invalid secret");
    }
}

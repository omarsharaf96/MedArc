use crate::error::AppError;

/// Minimum password length requirement.
const MIN_PASSWORD_LENGTH: usize = 12;

/// Hash a password using Argon2id.
/// Returns the PHC-formatted hash string (starts with "$argon2id$").
/// Rejects passwords shorter than MIN_PASSWORD_LENGTH.
pub fn hash_password(password: &str) -> Result<String, AppError> {
    if password.len() < MIN_PASSWORD_LENGTH {
        return Err(AppError::Validation(format!(
            "Password must be at least {} characters",
            MIN_PASSWORD_LENGTH
        )));
    }
    Ok(password_auth::generate_hash(password))
}

/// Verify a plaintext password against an Argon2id hash.
pub fn verify(password: &str, hash: &str) -> Result<(), AppError> {
    password_auth::verify_password(password, hash).map_err(|_| {
        AppError::Authentication("Invalid credentials".to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_password_returns_argon2id_hash() {
        let result = hash_password("validpassword1");
        assert!(result.is_ok(), "hash_password should succeed for valid password");
        let hash = result.unwrap();
        assert!(
            hash.starts_with("$argon2id$"),
            "Hash should start with $argon2id$, got: {}",
            hash
        );
    }

    #[test]
    fn hash_password_rejects_short_password() {
        let result = hash_password("short");
        assert!(result.is_err(), "hash_password should reject passwords shorter than 12 chars");
    }

    #[test]
    fn verify_correct_password() {
        let hash = hash_password("validpassword1").expect("hash should succeed");
        let result = verify("validpassword1", &hash);
        assert!(result.is_ok(), "verify should succeed for correct password");
    }

    #[test]
    fn verify_wrong_password() {
        let hash = hash_password("validpassword1").expect("hash should succeed");
        let result = verify("wrongpassword1", &hash);
        assert!(result.is_err(), "verify should fail for wrong password");
    }
}

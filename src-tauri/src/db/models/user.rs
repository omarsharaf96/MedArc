use serde::{Deserialize, Serialize};

/// Full user row from the database. Never expose password_hash or totp_secret to frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub id: String,
    pub username: String,
    pub password_hash: String,
    pub display_name: String,
    pub role: String,
    pub totp_secret: Option<String>,
    pub totp_enabled: bool,
    pub touch_id_enabled: bool,
    pub is_active: bool,
    pub failed_login_attempts: i32,
    pub locked_until: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Safe user response for frontend -- no sensitive fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserResponse {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub role: String,
}

impl From<&User> for UserResponse {
    fn from(user: &User) -> Self {
        UserResponse {
            id: user.id.clone(),
            username: user.username.clone(),
            display_name: user.display_name.clone(),
            role: user.role.clone(),
        }
    }
}

/// Input for creating a new user account.
#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateUserInput {
    pub username: String,
    pub password: String,
    pub display_name: String,
    pub role: String,
}

/// Input for user login.
#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginInput {
    pub username: String,
    pub password: String,
}

/**
 * TypeScript types for authentication, sessions, and MFA.
 *
 * Field names use camelCase to match the Rust structs' #[serde(rename_all = "camelCase")].
 */

/** Safe user response (no sensitive fields like password_hash). */
export interface UserResponse {
  id: string;
  username: string;
  displayName: string;
  role: string;
}

/** Session info from the backend SessionManager. */
export interface SessionInfo {
  sessionId: string | null;
  userId: string | null;
  role: string | null;
  state: "active" | "locked" | "unauthenticated" | "break_glass";
  lastActivity: string | null;
}

/** Input for user login. */
export interface LoginInput {
  username: string;
  password: string;
}

/** Input for user registration. */
export interface RegisterInput {
  username: string;
  password: string;
  displayName: string;
  role: string;
}

/** Response from the login command. */
export interface LoginResponse {
  user: UserResponse;
  session: SessionInfo;
  /** When true, the user has TOTP enabled and must provide a code via completeLogin. */
  mfaRequired: boolean;
  /** Present when mfaRequired is true; pass to completeLogin. */
  pendingUserId: string | null;
}

/** TOTP setup response from setup_totp command. */
export interface TotpSetup {
  secretBase32: string;
  otpauthUrl: string;
  qrBase64: string;
}

/** Biometric availability and enablement status. */
export interface BiometricStatus {
  available: boolean;
  enabled: boolean;
}

/** Response from break-glass activation. */
export interface BreakGlassResponse {
  logId: string;
  expiresAt: string;
}

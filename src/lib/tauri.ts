/**
 * Type-safe wrappers around Tauri invoke() calls.
 *
 * Each function maps to a Rust #[tauri::command] in the backend.
 * Parameter names match the Rust function parameter names exactly.
 */
import { invoke } from "@tauri-apps/api/core";

import type {
  DbStatus,
  AppInfo,
  FhirResource,
  FhirResourceList,
  CreateFhirResource,
  UpdateFhirResource,
} from "../types/fhir";

import type {
  UserResponse,
  LoginInput,
  LoginResponse,
  RegisterInput,
  SessionInfo,
  TotpSetup,
  BiometricStatus,
  BreakGlassResponse,
} from "../types/auth";

export const commands = {
  /** Check database encryption health status. */
  checkDb: () => invoke<DbStatus>("check_db"),

  /** Get application version and database path. */
  getAppInfo: () => invoke<AppInfo>("get_app_info"),

  /** Create a new FHIR resource. */
  createResource: (input: CreateFhirResource) =>
    invoke<FhirResource>("create_resource", { input }),

  /** Retrieve a single FHIR resource by ID. */
  getResource: (id: string) => invoke<FhirResource>("get_resource", { id }),

  /** List FHIR resources, optionally filtered by resource type. */
  listResources: (resourceType?: string) =>
    invoke<FhirResourceList>("list_resources", {
      resourceType: resourceType ?? null,
    }),

  /** Update an existing FHIR resource's JSON content. */
  updateResource: (input: UpdateFhirResource) =>
    invoke<FhirResource>("update_resource", { input }),

  /** Delete a FHIR resource by ID. */
  deleteResource: (id: string) => invoke<void>("delete_resource", { id }),

  // ─── Auth commands ───────────────────────────────────────────────

  /** Register a new user account. */
  registerUser: (input: RegisterInput) =>
    invoke<UserResponse>("register_user", {
      username: input.username,
      password: input.password,
      displayName: input.displayName,
      role: input.role,
    }),

  /** Log in with username and password. */
  login: (input: LoginInput) =>
    invoke<LoginResponse>("login", {
      username: input.username,
      password: input.password,
    }),

  /** Log out the current user. */
  logout: () => invoke<void>("logout"),

  /** Complete login after MFA verification (password was already checked). */
  completeLogin: (userId: string, totpCode: string) =>
    invoke<LoginResponse>("complete_login", {
      userId: userId,
      totpCode: totpCode,
    }),

  /** Check if this is the first run (no users exist). */
  checkFirstRun: () => invoke<boolean>("check_first_run"),

  // ─── Session commands ────────────────────────────────────────────

  /** Lock the current active session. */
  lockSession: () => invoke<void>("lock_session"),

  /** Unlock a locked session by re-entering password. */
  unlockSession: (password: string) =>
    invoke<void>("unlock_session", { password }),

  /** Refresh the session activity timestamp. */
  refreshSession: () => invoke<void>("refresh_session"),

  /** Get the current session state for the frontend. */
  getSessionState: () => invoke<SessionInfo>("get_session_state"),

  /** Get the session timeout value in minutes. */
  getSessionTimeout: () => invoke<number>("get_session_timeout"),

  // ─── MFA commands ────────────────────────────────────────────────

  /** Begin TOTP setup -- returns QR code and secret. */
  setupTotp: () => invoke<TotpSetup>("setup_totp"),

  /** Verify a TOTP code during setup to finalize enrollment. */
  verifyTotpSetup: (secretBase32: string, code: string) =>
    invoke<string>("verify_totp_setup", { secretBase32: secretBase32, code }),

  /** Disable TOTP (requires password confirmation). */
  disableTotp: (password: string) =>
    invoke<void>("disable_totp", { password }),

  /** Check a TOTP code during login (requires user_id since session may not exist yet). */
  checkTotp: (userId: string, code: string) =>
    invoke<boolean>("check_totp", { userId: userId, code }),

  /** Check biometric (Touch ID) availability and enablement. */
  checkBiometric: () => invoke<BiometricStatus>("check_biometric"),

  /** Enable Touch ID (requires password confirmation). */
  enableTouchId: (password: string) =>
    invoke<void>("enable_touch_id", { password }),

  /** Disable Touch ID. */
  disableTouchId: () => invoke<void>("disable_touch_id"),

  // ─── Break-glass commands ────────────────────────────────────────

  /** Activate emergency break-glass access. */
  activateBreakGlass: (reason: string, password: string, patientId?: string) =>
    invoke<BreakGlassResponse>("activate_break_glass", {
      reason,
      password,
      patientId: patientId ?? null,
    }),

  /** Deactivate break-glass and restore original role. */
  deactivateBreakGlass: () => invoke<void>("deactivate_break_glass"),
};

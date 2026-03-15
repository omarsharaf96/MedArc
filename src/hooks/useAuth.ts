import { useState, useEffect, useCallback } from "react";
import { commands } from "../lib/tauri";
import type { UserResponse, SessionInfo } from "../types/auth";

/** Auth state exposed by the useAuth hook. */
export interface AuthState {
  user: UserResponse | null;
  session: SessionInfo | null;
  isAuthenticated: boolean;
  isLocked: boolean;
  loading: boolean;
  error: string | null;
  mfaRequired: boolean;
  /** When true, no users exist -- first-run registration flow. */
  firstRun: boolean;
}

/** Return type of the useAuth hook. */
export interface UseAuthReturn extends AuthState {
  login: (username: string, password: string) => Promise<void>;
  /** Dev-only: authenticate as the built-in dev user without entering credentials. */
  devBypass: (response: import("../types/auth").LoginResponse) => void;
  register: (
    username: string,
    password: string,
    displayName: string,
    role: string,
  ) => Promise<void>;
  logout: () => Promise<void>;
  unlock: (password: string) => Promise<void>;
  biometricUnlock: () => Promise<void>;
  verifyMfa: (code: string) => Promise<void>;
  clearError: () => void;
}

/**
 * Auth state management hook.
 *
 * On mount, checks current session state from the backend.
 * Provides login, register, logout, unlock, and MFA verification functions.
 */
export function useAuth(): UseAuthReturn {
  const [user, setUser] = useState<UserResponse | null>(null);
  const [session, setSession] = useState<SessionInfo | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [mfaRequired, setMfaRequired] = useState(false);
  const [firstRun, setFirstRun] = useState(false);
  // Store pending user ID for MFA flow completion
  const [pendingMfaUserId, setPendingMfaUserId] = useState<string | null>(
    null,
  );

  /** Check session state and first-run status on mount. */
  useEffect(() => {
    async function checkSession() {
      try {
        // Check if this is first run (no users in database)
        const isFirstRun = await commands.checkFirstRun();
        setFirstRun(isFirstRun);

        if (!isFirstRun) {
          const sessionInfo = await commands.getSessionState();
          setSession(sessionInfo);

          if (
            sessionInfo.state === "active" ||
            sessionInfo.state === "locked"
          ) {
            // We have an active/locked session. Set user info from session.
            if (sessionInfo.userId && sessionInfo.role) {
              setUser({
                id: sessionInfo.userId,
                username: "",
                displayName: "",
                role: sessionInfo.role,
              });
            }
          }
        }
      } catch {
        // Session check failed -- likely first run or no backend
        setFirstRun(true);
      } finally {
        setLoading(false);
      }
    }

    checkSession();
  }, []);

  const isAuthenticated =
    session?.state === "active" ||
    session?.state === "locked" ||
    session?.state === "break_glass";
  const isLocked = session?.state === "locked";

  const clearError = useCallback(() => setError(null), []);

  /**
   * Dev-only: accept a LoginResponse that was already obtained by the
   * LoginForm (which called commands.devBypassLogin() directly) and apply
   * it to auth state, exactly as a normal login success would.
   *
   * Keeping the Tauri call inside the component avoids an additional round-trip
   * and lets the component handle the loading/error state for the bypass button.
   */
  const devBypass = useCallback(
    (response: import("../types/auth").LoginResponse) => {
      setUser(response.user);
      setSession(response.session);
      setPendingMfaUserId(null);
      setMfaRequired(false);
      setFirstRun(false);
      setError(null);
    },
    [],
  );

  const login = useCallback(async (username: string, password: string) => {
    setError(null);
    setLoading(true);
    try {
      const response = await commands.login({ username, password });

      if (response.mfaRequired && response.pendingUserId) {
        // MFA is required -- store pending user ID and prompt for TOTP code
        setPendingMfaUserId(response.pendingUserId);
        setMfaRequired(true);
      } else {
        // No MFA -- complete login immediately
        setUser(response.user);
        setSession(response.session);
        setPendingMfaUserId(null);
        setMfaRequired(false);
        setFirstRun(false);
      }
    } catch (e) {
      setError("Invalid credentials");
    } finally {
      setLoading(false);
    }
  }, []);

  const register = useCallback(
    async (
      username: string,
      password: string,
      displayName: string,
      role: string,
    ) => {
      setError(null);
      setLoading(true);
      try {
        await commands.registerUser({
          username,
          password,
          displayName,
          role,
        });
        // Auto-login after registration
        const response = await commands.login({ username, password });
        setUser(response.user);
        setSession(response.session);
        setFirstRun(false);
      } catch (e) {
        setError(String(e));
      } finally {
        setLoading(false);
      }
    },
    [],
  );

  const logout = useCallback(async () => {
    setError(null);
    try {
      await commands.logout();
      setUser(null);
      setSession(null);
      setMfaRequired(false);
      setPendingMfaUserId(null);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  const unlock = useCallback(async (password: string) => {
    setError(null);
    try {
      await commands.unlockSession(password);
      const sessionInfo = await commands.getSessionState();
      setSession(sessionInfo);
    } catch (e) {
      setError("Incorrect password");
    }
  }, []);

  const biometricUnlock = useCallback(async () => {
    setError(null);
    try {
      await commands.biometricAuthenticate();
      const sessionInfo = await commands.getSessionState();
      setSession(sessionInfo);
    } catch {
      setError("Touch ID authentication failed. Please use your password.");
    }
  }, []);

  const verifyMfa = useCallback(
    async (code: string) => {
      setError(null);
      setLoading(true);
      try {
        if (!pendingMfaUserId) {
          setError("No pending MFA session");
          return;
        }
        // Call complete_login which verifies TOTP and creates a full session
        const response = await commands.completeLogin(pendingMfaUserId, code);
        setUser(response.user);
        setSession(response.session);
        setMfaRequired(false);
        setPendingMfaUserId(null);
        setFirstRun(false);
      } catch (e) {
        setError("Invalid verification code");
      } finally {
        setLoading(false);
      }
    },
    [pendingMfaUserId],
  );

  return {
    user,
    session,
    isAuthenticated,
    isLocked,
    loading,
    error,
    mfaRequired,
    firstRun,
    login,
    devBypass,
    register,
    logout,
    unlock,
    biometricUnlock,
    verifyMfa,
    clearError,
  };
}

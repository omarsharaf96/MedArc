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
  register: (
    username: string,
    password: string,
    displayName: string,
    role: string,
  ) => Promise<void>;
  logout: () => Promise<void>;
  unlock: (password: string) => Promise<void>;
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
  // Store pending login user for MFA flow completion
  const [pendingMfaUser, setPendingMfaUser] = useState<UserResponse | null>(
    null,
  );
  const [pendingMfaSession, setPendingMfaSession] =
    useState<SessionInfo | null>(null);

  /** Check session state on mount. */
  useEffect(() => {
    async function checkSession() {
      try {
        const sessionInfo = await commands.getSessionState();
        setSession(sessionInfo);

        if (sessionInfo.state === "active" || sessionInfo.state === "locked") {
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

  const login = useCallback(async (username: string, password: string) => {
    setError(null);
    setLoading(true);
    try {
      const response = await commands.login({ username, password });

      // Check if user has TOTP enabled by attempting totp check.
      // The backend login succeeds but MFA verification is a second step.
      // For now, complete login immediately. MFA gate is handled at the
      // backend command level when it exists (Plan 02-03).
      // TODO: When MFA backend commands are available, check user.totpEnabled
      // and set mfaRequired=true instead of completing login.
      setUser(response.user);
      setSession(response.session);
      setPendingMfaUser(null);
      setPendingMfaSession(null);
      setMfaRequired(false);
      setFirstRun(false);
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
      setPendingMfaUser(null);
      setPendingMfaSession(null);
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

  const verifyMfa = useCallback(
    async (code: string) => {
      setError(null);
      try {
        const valid = await commands.checkTotp(code);
        if (valid && pendingMfaUser && pendingMfaSession) {
          setUser(pendingMfaUser);
          setSession(pendingMfaSession);
          setMfaRequired(false);
          setPendingMfaUser(null);
          setPendingMfaSession(null);
        } else {
          setError("Invalid verification code");
        }
      } catch (e) {
        setError("Invalid verification code");
      }
    },
    [pendingMfaUser, pendingMfaSession],
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
    register,
    logout,
    unlock,
    verifyMfa,
    clearError,
  };
}

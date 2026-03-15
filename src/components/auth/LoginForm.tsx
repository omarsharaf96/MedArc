import { useState, type FormEvent } from "react";
import { commands } from "../../lib/tauri";
import type { LoginResponse } from "../../types/auth";

interface LoginFormProps {
  onLogin: (username: string, password: string) => Promise<void>;
  onSwitchToRegister: () => void;
  onDevBypass?: (response: LoginResponse) => void;
  firstRun: boolean;
  error: string | null;
  loading: boolean;
}

/**
 * Username/password login form.
 *
 * Shows a "Create Account" link when firstRun is true (no existing users).
 * Displays generic error messages to avoid leaking which field was wrong.
 */
export default function LoginForm({
  onLogin,
  onSwitchToRegister,
  onDevBypass,
  firstRun,
  error,
  loading,
}: LoginFormProps) {
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [devError, setDevError] = useState<string | null>(null);
  const [devLoading, setDevLoading] = useState(false);

  const handleSubmit = async (e: FormEvent) => {
    e.preventDefault();
    await onLogin(username, password);
  };

  const handleDevBypass = async () => {
    setDevError(null);
    setDevLoading(true);
    try {
      const response = await commands.devBypassLogin();
      onDevBypass?.(response);
    } catch (e) {
      setDevError(String(e));
    } finally {
      setDevLoading(false);
    }
  };

  return (
    <div className="flex min-h-screen items-center justify-center bg-gray-50">
      <div className="w-full max-w-md rounded-lg bg-white p-8 shadow-lg">
        {/* Header */}
        <div className="mb-6 text-center">
          <h1 className="text-3xl font-bold text-gray-900">PanaceaEMR</h1>
          <p className="mt-1 text-sm text-gray-500">
            Electronic Medical Records
          </p>
        </div>

        {/* Error message */}
        {error && (
          <div className="mb-4 rounded-md border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700">
            {error}
          </div>
        )}

        <form onSubmit={handleSubmit} className="space-y-4">
          {/* Username */}
          <div>
            <label
              htmlFor="login-username"
              className="mb-1 block text-sm font-medium text-gray-700"
            >
              Username
            </label>
            <input
              id="login-username"
              type="text"
              required
              autoComplete="username"
              autoFocus
              value={username}
              onChange={(e) => setUsername(e.target.value)}
              className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
              placeholder="Enter your username"
            />
          </div>

          {/* Password */}
          <div>
            <label
              htmlFor="login-password"
              className="mb-1 block text-sm font-medium text-gray-700"
            >
              Password
            </label>
            <input
              id="login-password"
              type="password"
              required
              minLength={12}
              autoComplete="current-password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
              placeholder="Enter your password"
            />
          </div>

          {/* Submit */}
          <button
            type="submit"
            disabled={loading}
            className="w-full rounded-md bg-blue-600 px-4 py-2 text-sm font-medium text-white shadow-sm transition-colors hover:bg-blue-700 disabled:cursor-not-allowed disabled:opacity-50"
          >
            {loading ? "Signing in..." : "Sign In"}
          </button>
        </form>

        {/* First-run registration link */}
        {firstRun && (
          <div className="mt-4 text-center">
            <p className="text-sm text-gray-500">First time?</p>
            <button
              type="button"
              onClick={onSwitchToRegister}
              className="mt-1 text-sm font-medium text-blue-600 hover:text-blue-700"
            >
              Create System Administrator Account
            </button>
          </div>
        )}

        {/* Dev bypass — only rendered in Vite dev mode, tree-shaken in production */}
        {import.meta.env.DEV && (
          <div className="mt-6 border-t border-dashed border-gray-300 pt-4">
            <p className="mb-2 text-center text-xs font-semibold uppercase tracking-wide text-orange-500">
              DEV MODE
            </p>
            {devError && (
              <p className="mb-2 text-center text-xs text-red-600">{devError}</p>
            )}
            <button
              type="button"
              onClick={handleDevBypass}
              disabled={devLoading || loading}
              className="w-full rounded-md border border-gray-400 bg-white px-4 py-2 text-sm font-medium text-gray-600 shadow-sm transition-colors hover:bg-gray-50 disabled:cursor-not-allowed disabled:opacity-50"
            >
              {devLoading ? "Bypassing..." : "Dev Bypass (skip login)"}
            </button>
          </div>
        )}
      </div>
    </div>
  );
}

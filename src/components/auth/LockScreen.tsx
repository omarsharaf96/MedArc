import { useState, type FormEvent, useEffect } from "react";
import { commands } from "../../lib/tauri";

interface LockScreenProps {
  displayName: string;
  onUnlock: (password: string) => Promise<void>;
  onLogout: () => Promise<void>;
  error: string | null;
}

/**
 * Full-screen lock overlay.
 *
 * Obscures app content when the session is locked (inactivity timeout).
 * Provides password unlock and optional Touch ID if available/enabled.
 * The user can also sign out entirely from this screen.
 */
export default function LockScreen({
  displayName,
  onUnlock,
  onLogout,
  error,
}: LockScreenProps) {
  const [password, setPassword] = useState("");
  const [unlocking, setUnlocking] = useState(false);
  const [touchIdAvailable, setTouchIdAvailable] = useState(false);

  // Check biometric availability on mount
  useEffect(() => {
    async function checkBiometric() {
      try {
        const status = await commands.checkBiometric();
        setTouchIdAvailable(status.available && status.enabled);
      } catch {
        // Biometric not available -- ignore
      }
    }
    checkBiometric();
  }, []);

  const handleSubmit = async (e: FormEvent) => {
    e.preventDefault();
    setUnlocking(true);
    try {
      await onUnlock(password);
    } finally {
      setUnlocking(false);
      setPassword("");
    }
  };

  const handleTouchId = async () => {
    // Touch ID authentication is handled by the Tauri biometric plugin
    // on the backend side. For now, this provides the UI trigger.
    // The actual biometric prompt is shown natively by the OS.
    setUnlocking(true);
    try {
      // Attempt biometric unlock via backend
      // This would call a biometric_unlock command when available
      await onUnlock("");
    } catch {
      // Touch ID failed or cancelled
    } finally {
      setUnlocking(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-gray-900/80 backdrop-blur-sm">
      <div className="w-full max-w-sm rounded-lg bg-white p-8 shadow-2xl">
        {/* Lock icon */}
        <div className="mb-4 flex justify-center">
          <div className="flex h-16 w-16 items-center justify-center rounded-full bg-gray-100">
            <svg
              className="h-8 w-8 text-gray-600"
              fill="none"
              viewBox="0 0 24 24"
              strokeWidth={1.5}
              stroke="currentColor"
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                d="M16.5 10.5V6.75a4.5 4.5 0 10-9 0v3.75m-.75 11.25h10.5a2.25 2.25 0 002.25-2.25v-6.75a2.25 2.25 0 00-2.25-2.25H6.75a2.25 2.25 0 00-2.25 2.25v6.75a2.25 2.25 0 002.25 2.25z"
              />
            </svg>
          </div>
        </div>

        {/* Title */}
        <div className="mb-6 text-center">
          <h2 className="text-lg font-semibold text-gray-900">
            Session Locked
          </h2>
          <p className="mt-1 text-sm text-gray-500">{displayName}</p>
        </div>

        {/* Error message */}
        {error && (
          <div className="mb-4 rounded-md border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700">
            {error}
          </div>
        )}

        <form onSubmit={handleSubmit} className="space-y-4">
          {/* Password */}
          <div>
            <input
              type="password"
              required
              autoFocus
              autoComplete="current-password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
              placeholder="Enter password to unlock"
            />
          </div>

          {/* Unlock button */}
          <button
            type="submit"
            disabled={unlocking}
            className="w-full rounded-md bg-blue-600 px-4 py-2 text-sm font-medium text-white shadow-sm transition-colors hover:bg-blue-700 disabled:cursor-not-allowed disabled:opacity-50"
          >
            {unlocking ? "Unlocking..." : "Unlock"}
          </button>
        </form>

        {/* Touch ID button */}
        {touchIdAvailable && (
          <button
            type="button"
            onClick={handleTouchId}
            disabled={unlocking}
            className="mt-3 w-full rounded-md border border-gray-300 bg-white px-4 py-2 text-sm font-medium text-gray-700 shadow-sm transition-colors hover:bg-gray-50 disabled:cursor-not-allowed disabled:opacity-50"
          >
            Use Touch ID
          </button>
        )}

        {/* Sign out link */}
        <div className="mt-4 text-center">
          <button
            type="button"
            onClick={onLogout}
            className="text-sm text-gray-500 hover:text-gray-700"
          >
            Sign Out
          </button>
        </div>
      </div>
    </div>
  );
}

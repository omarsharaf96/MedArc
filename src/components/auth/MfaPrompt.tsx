import { useState, type FormEvent } from "react";

interface MfaPromptProps {
  onVerify: (code: string) => Promise<void>;
  onCancel: () => void;
  error: string | null;
  loading: boolean;
}

/**
 * TOTP code entry prompt shown during login when MFA is enabled.
 *
 * Accepts a 6-digit numeric code and submits it for verification.
 */
export default function MfaPrompt({
  onVerify,
  onCancel,
  error,
  loading,
}: MfaPromptProps) {
  const [code, setCode] = useState("");

  const handleSubmit = async (e: FormEvent) => {
    e.preventDefault();
    if (code.length !== 6) return;
    await onVerify(code);
  };

  return (
    <div className="flex min-h-screen items-center justify-center bg-gray-50">
      <div className="w-full max-w-sm rounded-lg bg-white p-8 shadow-lg">
        {/* Header */}
        <div className="mb-6 text-center">
          <h2 className="text-xl font-bold text-gray-900">
            Two-Factor Authentication
          </h2>
          <p className="mt-1 text-sm text-gray-500">
            Enter the 6-digit code from your authenticator app
          </p>
        </div>

        {/* Error message */}
        {error && (
          <div className="mb-4 rounded-md border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700">
            {error}
          </div>
        )}

        <form onSubmit={handleSubmit} className="space-y-4">
          {/* Code input */}
          <div>
            <input
              type="text"
              inputMode="numeric"
              autoFocus
              maxLength={6}
              pattern="[0-9]{6}"
              required
              value={code}
              onChange={(e) =>
                setCode(e.target.value.replace(/\D/g, "").slice(0, 6))
              }
              className="w-full rounded-md border border-gray-300 px-3 py-3 text-center font-mono text-2xl tracking-widest shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
              placeholder="000000"
            />
          </div>

          {/* Verify button */}
          <button
            type="submit"
            disabled={loading || code.length !== 6}
            className="w-full rounded-md bg-blue-600 px-4 py-2 text-sm font-medium text-white shadow-sm transition-colors hover:bg-blue-700 disabled:cursor-not-allowed disabled:opacity-50"
          >
            {loading ? "Verifying..." : "Verify"}
          </button>
        </form>

        {/* Cancel link */}
        <div className="mt-4 text-center">
          <button
            type="button"
            onClick={onCancel}
            className="text-sm text-gray-500 hover:text-gray-700"
          >
            Back to Login
          </button>
        </div>
      </div>
    </div>
  );
}

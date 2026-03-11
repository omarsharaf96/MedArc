import { useState } from "react";
import { commands } from "../../lib/tauri";
import type { TotpSetup } from "../../types/auth";

interface MfaSetupProps {
  onComplete: () => void;
  onCancel: () => void;
}

type SetupStep = "idle" | "scanning" | "verifying" | "success";

/**
 * TOTP multi-factor authentication setup component.
 *
 * Three-step flow:
 * 1. Click "Enable MFA" to generate TOTP secret and QR code.
 * 2. Scan QR code with authenticator app, enter 6-digit verification code.
 * 3. On successful verification, MFA is enabled.
 */
export default function MfaSetup({ onComplete, onCancel }: MfaSetupProps) {
  const [step, setStep] = useState<SetupStep>("idle");
  const [totpSetup, setTotpSetup] = useState<TotpSetup | null>(null);
  const [code, setCode] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const handleStartSetup = async () => {
    setError(null);
    setLoading(true);
    try {
      const setup = await commands.setupTotp();
      setTotpSetup(setup);
      setStep("scanning");
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  const handleVerify = async () => {
    if (!totpSetup || code.length !== 6) return;

    setError(null);
    setLoading(true);
    try {
      await commands.verifyTotpSetup(totpSetup.secretBase32, code);
      setStep("success");
    } catch {
      setError("Invalid code, try again");
      setCode("");
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="w-full max-w-md rounded-lg bg-white p-6 shadow-lg">
      <h2 className="mb-4 text-lg font-semibold text-gray-900">
        Multi-Factor Authentication
      </h2>

      {/* Error */}
      {error && (
        <div className="mb-4 rounded-md border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700">
          {error}
        </div>
      )}

      {/* Step: idle -- initial prompt */}
      {step === "idle" && (
        <div className="space-y-4">
          <p className="text-sm text-gray-600">
            Enable multi-factor authentication to add an extra layer of security
            to your account. You will need an authenticator app (e.g., Google
            Authenticator, Authy) on your phone.
          </p>
          <div className="flex gap-3">
            <button
              type="button"
              onClick={onCancel}
              className="flex-1 rounded-md border border-gray-300 bg-white px-4 py-2 text-sm font-medium text-gray-700 shadow-sm transition-colors hover:bg-gray-50"
            >
              Cancel
            </button>
            <button
              type="button"
              onClick={handleStartSetup}
              disabled={loading}
              className="flex-1 rounded-md bg-blue-600 px-4 py-2 text-sm font-medium text-white shadow-sm transition-colors hover:bg-blue-700 disabled:cursor-not-allowed disabled:opacity-50"
            >
              {loading ? "Setting up..." : "Enable MFA"}
            </button>
          </div>
        </div>
      )}

      {/* Step: scanning -- QR code display */}
      {step === "scanning" && totpSetup && (
        <div className="space-y-4">
          <p className="text-sm text-gray-600">
            Scan this QR code with your authenticator app:
          </p>

          {/* QR Code */}
          <div className="flex justify-center">
            <img
              src={`data:image/png;base64,${totpSetup.qrBase64}`}
              alt="TOTP QR Code"
              className="h-48 w-48 rounded-md border border-gray-200"
            />
          </div>

          {/* Manual secret */}
          <div className="rounded-md bg-gray-50 p-3">
            <p className="mb-1 text-xs font-medium text-gray-500">
              Manual entry key:
            </p>
            <p className="break-all font-mono text-sm text-gray-800">
              {totpSetup.secretBase32}
            </p>
          </div>

          {/* Verification code input */}
          <div>
            <label
              htmlFor="mfa-code"
              className="mb-1 block text-sm font-medium text-gray-700"
            >
              Enter 6-digit code from your app
            </label>
            <input
              id="mfa-code"
              type="text"
              inputMode="numeric"
              autoFocus
              maxLength={6}
              pattern="[0-9]{6}"
              value={code}
              onChange={(e) =>
                setCode(e.target.value.replace(/\D/g, "").slice(0, 6))
              }
              className="w-full rounded-md border border-gray-300 px-3 py-2 text-center font-mono text-lg tracking-widest shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
              placeholder="000000"
            />
          </div>

          <div className="flex gap-3">
            <button
              type="button"
              onClick={onCancel}
              className="flex-1 rounded-md border border-gray-300 bg-white px-4 py-2 text-sm font-medium text-gray-700 shadow-sm transition-colors hover:bg-gray-50"
            >
              Cancel
            </button>
            <button
              type="button"
              onClick={handleVerify}
              disabled={loading || code.length !== 6}
              className="flex-1 rounded-md bg-blue-600 px-4 py-2 text-sm font-medium text-white shadow-sm transition-colors hover:bg-blue-700 disabled:cursor-not-allowed disabled:opacity-50"
            >
              {loading ? "Verifying..." : "Verify"}
            </button>
          </div>
        </div>
      )}

      {/* Step: success */}
      {step === "success" && (
        <div className="space-y-4 text-center">
          <div className="flex justify-center">
            <div className="flex h-12 w-12 items-center justify-center rounded-full bg-green-100">
              <svg
                className="h-6 w-6 text-green-600"
                fill="none"
                viewBox="0 0 24 24"
                strokeWidth={2}
                stroke="currentColor"
              >
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  d="M4.5 12.75l6 6 9-13.5"
                />
              </svg>
            </div>
          </div>
          <p className="font-medium text-green-700">MFA enabled</p>
          <p className="text-sm text-gray-500">
            Your account is now protected with multi-factor authentication.
          </p>
          <button
            type="button"
            onClick={onComplete}
            className="w-full rounded-md bg-blue-600 px-4 py-2 text-sm font-medium text-white shadow-sm transition-colors hover:bg-blue-700"
          >
            Done
          </button>
        </div>
      )}
    </div>
  );
}

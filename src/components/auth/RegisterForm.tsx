import { useState, type FormEvent } from "react";

const ROLES = [
  { value: "SystemAdmin", label: "System Administrator" },
  { value: "Provider", label: "Provider (Physician)" },
  { value: "NurseMa", label: "Nurse / Medical Assistant" },
  { value: "BillingStaff", label: "Billing Staff" },
  { value: "FrontDesk", label: "Front Desk" },
];

interface RegisterFormProps {
  onRegister: (
    username: string,
    password: string,
    displayName: string,
    role: string,
  ) => Promise<void>;
  onCancel: () => void;
  /** True when no users exist -- role locked to SystemAdmin. */
  firstRun: boolean;
  error: string | null;
  loading: boolean;
}

/**
 * Registration form for new user accounts.
 *
 * On first-run (no existing users), the role is locked to SystemAdmin
 * with an explanation. Otherwise, an admin can choose from all 5 roles.
 */
export default function RegisterForm({
  onRegister,
  onCancel,
  firstRun,
  error,
  loading,
}: RegisterFormProps) {
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [confirmPassword, setConfirmPassword] = useState("");
  const [displayName, setDisplayName] = useState("");
  const [role, setRole] = useState(firstRun ? "SystemAdmin" : "Provider");

  const passwordTooShort = password.length > 0 && password.length < 12;
  const passwordsMismatch =
    confirmPassword.length > 0 && password !== confirmPassword;
  const canSubmit =
    username.length > 0 &&
    password.length >= 12 &&
    password === confirmPassword &&
    displayName.length > 0 &&
    !loading;

  const handleSubmit = async (e: FormEvent) => {
    e.preventDefault();
    if (!canSubmit) return;
    await onRegister(username, password, displayName, role);
  };

  return (
    <div className="flex min-h-screen items-center justify-center bg-gray-50">
      <div className="w-full max-w-md rounded-lg bg-white p-8 shadow-lg">
        {/* Header */}
        <div className="mb-6 text-center">
          <h1 className="text-2xl font-bold text-gray-900">Create Account</h1>
          {firstRun && (
            <p className="mt-1 text-sm text-amber-600">
              First-time setup: creating the System Administrator account
            </p>
          )}
        </div>

        {/* Error message */}
        {error && (
          <div className="mb-4 rounded-md border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700">
            {error}
          </div>
        )}

        <form onSubmit={handleSubmit} className="space-y-4">
          {/* Display Name */}
          <div>
            <label
              htmlFor="reg-display-name"
              className="mb-1 block text-sm font-medium text-gray-700"
            >
              Display Name
            </label>
            <input
              id="reg-display-name"
              type="text"
              required
              autoFocus
              value={displayName}
              onChange={(e) => setDisplayName(e.target.value)}
              className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
              placeholder="Dr. Jane Smith"
            />
          </div>

          {/* Username */}
          <div>
            <label
              htmlFor="reg-username"
              className="mb-1 block text-sm font-medium text-gray-700"
            >
              Username
            </label>
            <input
              id="reg-username"
              type="text"
              required
              autoComplete="username"
              value={username}
              onChange={(e) => setUsername(e.target.value)}
              className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
              placeholder="jsmith"
            />
          </div>

          {/* Password */}
          <div>
            <label
              htmlFor="reg-password"
              className="mb-1 block text-sm font-medium text-gray-700"
            >
              Password
            </label>
            <input
              id="reg-password"
              type="password"
              required
              minLength={12}
              autoComplete="new-password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              className={`w-full rounded-md border px-3 py-2 text-sm shadow-sm focus:outline-none focus:ring-1 ${
                passwordTooShort
                  ? "border-red-300 focus:border-red-500 focus:ring-red-500"
                  : "border-gray-300 focus:border-blue-500 focus:ring-blue-500"
              }`}
              placeholder="Minimum 12 characters"
            />
            {passwordTooShort && (
              <p className="mt-1 text-xs text-red-600">
                Password must be at least 12 characters
              </p>
            )}
          </div>

          {/* Confirm Password */}
          <div>
            <label
              htmlFor="reg-confirm-password"
              className="mb-1 block text-sm font-medium text-gray-700"
            >
              Confirm Password
            </label>
            <input
              id="reg-confirm-password"
              type="password"
              required
              autoComplete="new-password"
              value={confirmPassword}
              onChange={(e) => setConfirmPassword(e.target.value)}
              className={`w-full rounded-md border px-3 py-2 text-sm shadow-sm focus:outline-none focus:ring-1 ${
                passwordsMismatch
                  ? "border-red-300 focus:border-red-500 focus:ring-red-500"
                  : "border-gray-300 focus:border-blue-500 focus:ring-blue-500"
              }`}
              placeholder="Re-enter password"
            />
            {passwordsMismatch && (
              <p className="mt-1 text-xs text-red-600">
                Passwords do not match
              </p>
            )}
          </div>

          {/* Role */}
          <div>
            <label
              htmlFor="reg-role"
              className="mb-1 block text-sm font-medium text-gray-700"
            >
              Role
            </label>
            {firstRun ? (
              <div className="rounded-md border border-amber-200 bg-amber-50 px-3 py-2 text-sm text-amber-800">
                System Administrator (required for first account)
              </div>
            ) : (
              <select
                id="reg-role"
                value={role}
                onChange={(e) => setRole(e.target.value)}
                className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
              >
                {ROLES.map((r) => (
                  <option key={r.value} value={r.value}>
                    {r.label}
                  </option>
                ))}
              </select>
            )}
          </div>

          {/* Buttons */}
          <div className="flex gap-3">
            <button
              type="button"
              onClick={onCancel}
              className="flex-1 rounded-md border border-gray-300 bg-white px-4 py-2 text-sm font-medium text-gray-700 shadow-sm transition-colors hover:bg-gray-50"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={!canSubmit}
              className="flex-1 rounded-md bg-blue-600 px-4 py-2 text-sm font-medium text-white shadow-sm transition-colors hover:bg-blue-700 disabled:cursor-not-allowed disabled:opacity-50"
            >
              {loading ? "Creating..." : "Create Account"}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}

import { useState, useEffect } from "react";
import DatabaseStatus from "./components/DatabaseStatus";
import FhirExplorer from "./components/FhirExplorer";
import LoginForm from "./components/auth/LoginForm";
import RegisterForm from "./components/auth/RegisterForm";
import LockScreen from "./components/auth/LockScreen";
import MfaPrompt from "./components/auth/MfaPrompt";
import { useAuth } from "./hooks/useAuth";
import { useIdleTimer } from "./hooks/useIdleTimer";
import { commands } from "./lib/tauri";

function App() {
  const auth = useAuth();
  const [showRegister, setShowRegister] = useState(false);
  const [timeoutMinutes, setTimeoutMinutes] = useState(15);

  // Fetch session timeout from backend on mount
  useEffect(() => {
    async function fetchTimeout() {
      try {
        const timeout = await commands.getSessionTimeout();
        setTimeoutMinutes(timeout);
      } catch {
        // Use default 15 minutes
      }
    }
    fetchTimeout();
  }, []);

  // Auto-show registration form on first run
  useEffect(() => {
    if (auth.firstRun) {
      setShowRegister(true);
    }
  }, [auth.firstRun]);

  // Start idle timer when authenticated and not locked
  useIdleTimer(timeoutMinutes, auth.isAuthenticated && !auth.isLocked);

  // Loading state
  if (auth.loading && !auth.isAuthenticated) {
    return (
      <div className="flex min-h-screen items-center justify-center bg-gray-50">
        <div className="text-center">
          <div className="mx-auto mb-4 h-8 w-8 animate-spin rounded-full border-4 border-blue-600 border-t-transparent" />
          <p className="text-sm text-gray-500">Loading...</p>
        </div>
      </div>
    );
  }

  // MFA prompt (login succeeded but MFA verification needed)
  if (auth.mfaRequired) {
    return (
      <MfaPrompt
        onVerify={auth.verifyMfa}
        onCancel={auth.logout}
        error={auth.error}
        loading={auth.loading}
      />
    );
  }

  // Not authenticated -- show login or register form
  if (!auth.isAuthenticated) {
    if (showRegister) {
      return (
        <RegisterForm
          onRegister={auth.register}
          onCancel={() => setShowRegister(false)}
          firstRun={auth.firstRun}
          error={auth.error}
          loading={auth.loading}
        />
      );
    }

    return (
      <LoginForm
        onLogin={auth.login}
        onSwitchToRegister={() => setShowRegister(true)}
        firstRun={auth.firstRun}
        error={auth.error}
        loading={auth.loading}
      />
    );
  }

  // Authenticated -- show main app content with optional lock screen overlay
  return (
    <>
      {/* Lock screen overlay (shown on top of content to preserve state) */}
      {auth.isLocked && (
        <LockScreen
          displayName={auth.user?.displayName || auth.user?.username || "User"}
          onUnlock={auth.unlock}
          onLogout={auth.logout}
          error={auth.error}
        />
      )}

      {/* Main app content */}
      <div className="min-h-screen bg-gray-50 p-8">
        <div className="mx-auto max-w-4xl">
          {/* Header */}
          <div className="mb-8 flex items-center justify-between">
            <div>
              <h1 className="text-4xl font-bold text-gray-900">MedArc</h1>
              <p className="mt-1 text-lg text-gray-500">
                Electronic Medical Records
              </p>
            </div>
            <div className="flex items-center gap-4">
              {auth.user && (
                <span className="text-sm text-gray-600">
                  {auth.user.displayName || auth.user.username}
                  <span className="ml-2 rounded-full bg-blue-100 px-2 py-0.5 text-xs font-medium text-blue-800">
                    {auth.user.role}
                  </span>
                </span>
              )}
              <button
                onClick={auth.logout}
                className="rounded-md border border-gray-300 bg-white px-3 py-1.5 text-sm font-medium text-gray-700 shadow-sm transition-colors hover:bg-gray-50"
              >
                Sign Out
              </button>
            </div>
          </div>

          {/* Database Status */}
          <div className="mb-6">
            <DatabaseStatus />
          </div>

          {/* FHIR Explorer */}
          <div className="mb-6">
            <FhirExplorer />
          </div>
        </div>
      </div>
    </>
  );
}

export default App;

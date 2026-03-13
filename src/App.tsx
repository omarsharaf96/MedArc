import { useState, useEffect } from "react";
import LoginForm from "./components/auth/LoginForm";
import RegisterForm from "./components/auth/RegisterForm";
import LockScreen from "./components/auth/LockScreen";
import MfaPrompt from "./components/auth/MfaPrompt";
import { useAuth } from "./hooks/useAuth";
import { RouterProvider } from "./contexts/RouterContext";
import { AppShell } from "./components/shell/AppShell";

function App() {
  const auth = useAuth();
  const [showRegister, setShowRegister] = useState(false);

  // Auto-show registration form on first run
  useEffect(() => {
    if (auth.firstRun) {
      setShowRegister(true);
    }
  }, [auth.firstRun]);

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

  // Authenticated -- shell with lock screen overlay to preserve React state
  return (
    <>
      {/* Lock screen overlay (rendered above shell to preserve router/page state) */}
      {auth.isLocked && (
        <LockScreen
          displayName={auth.user?.displayName || auth.user?.username || "User"}
          onUnlock={auth.unlock}
          onBiometricUnlock={auth.biometricUnlock}
          onLogout={auth.logout}
          error={auth.error}
        />
      )}

      {/* Navigation shell — router and RBAC-gated sidebar */}
      <RouterProvider>
        <AppShell
          onLogout={auth.logout}
          userRole={auth.user?.role ?? ""}
          displayName={
            auth.user?.displayName || auth.user?.username || "User"
          }
        />
      </RouterProvider>
    </>
  );
}

export default App;

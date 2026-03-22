import { useState, useEffect, useCallback, useRef } from "react";
import LoginForm from "./components/auth/LoginForm";
import RegisterForm from "./components/auth/RegisterForm";
import LockScreen from "./components/auth/LockScreen";
import MfaPrompt from "./components/auth/MfaPrompt";
import { useAuth } from "./hooks/useAuth";
import { RouterProvider, useNav } from "./contexts/RouterContext";
import { AppShell } from "./components/shell/AppShell";
import { AssistantPanel } from "./components/assistant/AssistantPanel";

/**
 * Listens for "navigate-to-encounter" custom events dispatched by the
 * assistant ActionCard and performs in-app navigation. Must be rendered
 * inside <RouterProvider> so it can access useNav().
 */
function AssistantNavigationListener() {
  const navRef = useRef<ReturnType<typeof useNav>["navigate"] | null>(null);
  const { navigate } = useNav();
  navRef.current = navigate;

  useEffect(() => {
    const handler = (e: Event) => {
      const detail = (e as CustomEvent).detail;
      console.log("[AssistantNav] navigate-to-encounter event received:", detail);
      if (detail?.patientId && detail?.encounterId && navRef.current) {
        console.log("[AssistantNav] Navigating to encounter workspace:", detail.encounterId);
        navRef.current({
          page: "encounter-workspace",
          patientId: detail.patientId as string,
          encounterId: detail.encounterId as string,
        });
      }
    };
    window.addEventListener("navigate-to-encounter", handler);
    return () => window.removeEventListener("navigate-to-encounter", handler);
  }, []); // No dependency on navigate — use ref instead to avoid stale closure
  return null;
}

function App() {
  const auth = useAuth();
  const [showRegister, setShowRegister] = useState(false);
  const [assistantOpen, setAssistantOpen] = useState(false);

  // Auto-show registration form on first run
  useEffect(() => {
    if (auth.firstRun) {
      setShowRegister(true);
    }
  }, [auth.firstRun]);

  // Keyboard shortcut: Cmd+K to toggle assistant
  const handleKeyDown = useCallback(
    (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === "k") {
        e.preventDefault();
        if (auth.isAuthenticated && !auth.isLocked) {
          setAssistantOpen((prev) => !prev);
        }
      }
      // Escape to close
      if (e.key === "Escape" && assistantOpen) {
        setAssistantOpen(false);
      }
    },
    [auth.isAuthenticated, auth.isLocked, assistantOpen]
  );

  useEffect(() => {
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [handleKeyDown]);

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
        onDevBypass={auth.devBypass}
        firstRun={auth.firstRun}
        error={auth.error}
        loading={auth.loading}
      />
    );
  }

  // Authenticated -- shell with lock screen overlay to preserve React state
  return (
    <>
      {/* DEV MODE badge — tree-shaken in production builds by Vite */}
      {import.meta.env.DEV && (
        <div className="fixed bottom-3 right-3 z-50 rounded px-2 py-1 text-xs font-bold uppercase tracking-wide bg-orange-500/80 text-white shadow select-none pointer-events-none">
          DEV MODE
        </div>
      )}

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
      <div style={{ marginRight: assistantOpen ? 420 : 0, transition: 'margin-right 0.2s ease' }}>
        <RouterProvider>
          <AppShell
            onLogout={auth.logout}
            userRole={auth.user?.role ?? ""}
            displayName={
              auth.user?.displayName || auth.user?.username || "User"
            }
          />
          <AssistantNavigationListener />
        </RouterProvider>
      </div>

      {/* AI Assistant — floating button + slide-out panel */}
      {!auth.isLocked && (
        <>
          <button
            type="button"
            onClick={() => setAssistantOpen(true)}
            className="fixed bottom-16 right-4 z-30 flex h-12 w-12 items-center justify-center rounded-full bg-blue-600 text-white shadow-lg transition-all hover:bg-blue-700 hover:shadow-xl hover:scale-105 active:scale-95"
            title="AI Assistant (Cmd+K)"
          >
            <svg
              className="h-6 w-6"
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={2}
                d="M8 10h.01M12 10h.01M16 10h.01M9 16H5a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v8a2 2 0 01-2 2h-5l-5 5v-5z"
              />
            </svg>
          </button>

          <AssistantPanel
            open={assistantOpen}
            onClose={() => setAssistantOpen(false)}
          />
        </>
      )}
    </>
  );
}

export default App;

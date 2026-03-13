/**
 * AppShell.tsx — Two-column authenticated app layout.
 *
 * Composes the navigation sidebar and content area. Owns the `useIdleTimer`
 * call so that the idle timer is only active when this component is mounted
 * (i.e., the user is fully authenticated and not locked).
 *
 * Intentionally does NOT call `useAuth()` — auth state is passed as props
 * from App.tsx. This keeps AppShell testable and prevents a double-context
 * subscription pattern.
 *
 * The `RouterProvider` that enables `useNav()` in child components must be
 * an ancestor of `AppShell` (wired in T05 App.tsx changes).
 */

import { useState, useEffect } from "react";
import { useIdleTimer } from "../../hooks/useIdleTimer";
import { commands } from "../../lib/tauri";
import { Sidebar } from "./Sidebar";
import { ContentArea } from "./ContentArea";

// ─── Props ──────────────────────────────────────────────────────────────────

interface AppShellProps {
  /** Called when the user clicks Sign Out (propagated from App.tsx / useAuth). */
  onLogout: () => void;
  /** The authenticated user's role string (e.g. "Provider", "SystemAdmin"). */
  userRole: string;
  /** The authenticated user's display name shown in the sidebar footer. */
  displayName: string;
}

// ─── Component ──────────────────────────────────────────────────────────────

/**
 * Full-screen two-column shell: fixed sidebar + scrollable content area.
 *
 * Idle timer is active for the lifetime of this component — i.e., only
 * when the user is authenticated and the shell is mounted.
 */
export function AppShell({ onLogout, userRole, displayName }: AppShellProps) {
  const [timeoutMinutes, setTimeoutMinutes] = useState(15);

  // Fetch the configured session timeout from the backend on mount.
  // Defaults to 15 minutes if the command fails (e.g. backend unreachable).
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

  // Active = this component is mounted = user is authenticated and shell is visible.
  // Pass `true` as the second arg; the hook's own `enabled` check gates event listeners.
  useIdleTimer(timeoutMinutes, true);

  return (
    <div className="flex min-h-screen bg-gray-50">
      <Sidebar role={userRole} displayName={displayName} onLogout={onLogout} />
      <main className="flex-1 overflow-y-auto">
        <ContentArea />
      </main>
    </div>
  );
}

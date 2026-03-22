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

import { useIdleTimer } from "../../hooks/useIdleTimer";
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
  // Idle timer disabled — timeout fixed at 0 (no auto-lock).
  // To re-enable, fetch timeout from backend: commands.getSessionTimeout()
  useIdleTimer(0, true);

  return (
    <div className="flex min-h-screen bg-gray-50">
      <Sidebar role={userRole} displayName={displayName} onLogout={onLogout} />
      <main className="flex-1 overflow-y-auto">
        <ContentArea />
      </main>
    </div>
  );
}

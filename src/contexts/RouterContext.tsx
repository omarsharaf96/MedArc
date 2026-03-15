/**
 * RouterContext.tsx — State-based router for MedArc.
 *
 * Deliberate design choice: no URL-based routing (no React Router, no TanStack Router).
 * The Tauri WKWebView has no real URL bar; URL-based routers add complexity with no benefit.
 * Navigation state is held in React context with a history stack for back-navigation.
 *
 * All page components consume navigation via `useNav()`.
 * `RouterContext` is intentionally NOT exported — consumers must use `useNav()`.
 */

import { createContext, useContext, useState, useCallback } from "react";
import type { ReactNode } from "react";
// PtNoteType import removed — PT notes are now handled through encounters/documentation

// ─── Route union type ───────────────────────────────────────────────────────

/**
 * Discriminated union of all routes in the application.
 * Adding a new page: add a new variant here and handle it in ContentArea.
 * Covers all routes needed through S07.
 */
export type Route =
  | { page: "patients" }
  | { page: "patient-detail"; patientId: string }
  | { page: "encounter-workspace"; patientId: string; encounterId: string }
  | { page: "schedule" }
  | { page: "settings" }
  | { page: "audit-log" }
  | { page: "pt-notes"; patientId: string }
  | { page: "pt-note-detail"; patientId: string; noteType: string; ptNoteId: string }
  | { page: "outcome-measures"; patientId: string }
  | { page: "document-center"; patientId: string }
  | { page: "survey-builder" }
  | { page: "survey-kiosk"; patientId: string; templateId: string }
  | { page: "voice-to-note"; patientId: string; noteType: string }
  | { page: "export"; patientId: string }
  | { page: "fax" }
  | { page: "hep-builder"; patientId: string; encounterId?: string }
  | { page: "billing"; patientId: string; encounterId: string }
  | { page: "claims"; patientId?: string }
  | { page: "remittance" }
  | { page: "analytics" }
  | { page: "mips" };

// ─── Context value type ─────────────────────────────────────────────────────

interface RouterContextValue {
  /** The currently displayed route. */
  currentRoute: Route;
  /** Navigation history stack (oldest → newest). Used by goBack(). */
  history: Route[];
  /** Navigate to a new route. Pushes the current route onto history. */
  navigate: (route: Route) => void;
  /**
   * Navigate back to the previous route.
   * No-op if history is empty (already at root).
   */
  goBack: () => void;
}

// ─── Context (not exported — use useNav()) ──────────────────────────────────

const RouterContext = createContext<RouterContextValue | undefined>(undefined);

// ─── Provider ───────────────────────────────────────────────────────────────

interface RouterProviderProps {
  children: ReactNode;
  /** Override the default initial route (useful for testing). */
  initialRoute?: Route;
}

/** Wrap the authenticated app tree with this provider to enable navigation. */
export function RouterProvider({
  children,
  initialRoute = { page: "patients" },
}: RouterProviderProps) {
  const [currentRoute, setCurrentRoute] = useState<Route>(initialRoute);
  const [history, setHistory] = useState<Route[]>([]);

  const navigate = useCallback((route: Route) => {
    setHistory((prev) => [...prev, currentRoute]);
    setCurrentRoute(route);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [currentRoute]);

  const goBack = useCallback(() => {
    setHistory((prev) => {
      if (prev.length === 0) return prev; // no-op: already at root
      const next = [...prev];
      const previous = next.pop()!;
      setCurrentRoute(previous);
      return next;
    });
  }, []);

  return (
    <RouterContext.Provider value={{ currentRoute, history, navigate, goBack }}>
      {children}
    </RouterContext.Provider>
  );
}

// ─── Consumer hook ──────────────────────────────────────────────────────────

/**
 * Access the current route and navigation functions.
 *
 * Must be called within a `<RouterProvider>` tree.
 * Throws a descriptive error if called outside the provider to prevent
 * silent failures in deeply nested components.
 *
 * @example
 * const { currentRoute, navigate, goBack } = useNav();
 * navigate({ page: 'patient-detail', patientId: '123' });
 */
export function useNav(): RouterContextValue {
  const ctx = useContext(RouterContext);
  if (ctx === undefined) {
    throw new Error("useNav must be used within RouterProvider");
  }
  return ctx;
}

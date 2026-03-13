---
id: T03
parent: S01
milestone: M002
provides:
  - State-based router (RouterContext + useNav) — RouterProvider, useNav, Route type exported from src/contexts/RouterContext.tsx
key_files:
  - src/contexts/RouterContext.tsx
key_decisions:
  - No URL-based routing — deliberate architectural choice for Tauri WKWebView environment; state-based router using useState + history stack
  - RouterContext intentionally NOT exported; consumers must use useNav() to prevent direct context access
  - Route is a discriminated union of objects (not strings) for full TypeScript type safety on route params without URL parsing
  - initialRoute prop added to RouterProvider for testability (overrides default { page: 'patients' })
patterns_established:
  - useNav() throws descriptive error "useNav must be used within RouterProvider" when called outside provider (same safety check pattern as useAuth)
  - navigate() pushes current route onto history before switching (enables goBack)
  - goBack() uses functional state updater to atomically pop history and set currentRoute
  - Default route is { page: 'patients' } — initializes to the patients list view
observability_surfaces:
  - RouterContext value is inspectable via React DevTools — shows currentRoute object and history array at runtime
  - useNav() outside provider throws immediately with a clear message (no silent failures)
duration: 20m
verification_result: passed
completed_at: 2026-03-12
blocker_discovered: false
---

# T03: Implement state-based router (RouterContext + useNav)

**Created `src/contexts/RouterContext.tsx` — single file exporting `Route` union type (5 variants), `RouterProvider` with navigate/goBack state, and `useNav()` hook; zero TypeScript errors.**

## What Happened

Created `src/contexts/RouterContext.tsx` from scratch following the task plan and the `useAuth` idiom already established in the codebase. The implementation:

1. Defines `Route` as a discriminated union covering all 5 variants: `patients`, `patient-detail` (with `patientId: string`), `schedule`, `settings`, `audit-log`.
2. Implements `RouterContext` with `currentRoute`, `history`, `navigate`, and `goBack` — intentionally not exported so consumers are forced through `useNav()`.
3. `RouterProvider` wraps children. `navigate(route)` appends the current route to history then sets the new route. `goBack()` uses a functional `setHistory` updater to atomically pop the last route and restore it, ensuring no stale closure issues.
4. `useNav()` calls `useContext(RouterContext)` and throws `"useNav must be used within RouterProvider"` if the context is undefined — mirrors the safety check pattern from `useAuth`.

No external router dependencies were introduced. No `window.location`, `history.pushState`, or URL-based APIs used.

## Verification

- `tsc --noEmit` → exit 0 (zero errors)
- `grep -n "window.location|history.push|BrowserRouter|HashRouter|MemoryRouter" src/contexts/RouterContext.tsx` → no matches ✓
- `grep -n "^export" src/contexts/RouterContext.tsx` → shows exports for `Route` (type), `RouterProvider` (function), `useNav` (function) ✓
- `grep -n "any" src/contexts/RouterContext.tsx` → no matches ✓

## Diagnostics

- React DevTools: inspect the `RouterContext` node to see `currentRoute` (object with `page` field and optional `patientId`) and `history` (array of Route objects). This is the primary runtime inspection surface.
- `useNav()` outside provider throws immediately — visible in browser console as an uncaught React error boundary.
- No PHI in route state (only page names and opaque IDs like `patientId`); safe to log for debugging.

## Deviations

- Added optional `initialRoute` prop to `RouterProvider` (not in the task plan). This enables future test scenarios to start the provider at a specific route without navigating there. Purely additive — does not affect the default behavior (defaults to `{ page: 'patients' }`).

## Known Issues

None.

## Files Created/Modified

- `src/contexts/RouterContext.tsx` — new file: Route union type, RouterProvider with useState history stack, useNav() hook

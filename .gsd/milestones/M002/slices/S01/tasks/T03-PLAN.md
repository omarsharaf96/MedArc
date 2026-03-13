---
estimated_steps: 4
estimated_files: 1
---

# T03: Implement state-based router (RouterContext + useNav)

**Slice:** S01 — Navigation Shell & Type System
**Milestone:** M002

## Description

Implement the navigation router as a React context with a `useState<Route>` store and a history stack, following the same `useAuth` idiom already established in the codebase. This is a deliberate architectural choice (documented in research): no URL-based routing (no React Router, no TanStack Router) because the Tauri WKWebView has no real URL bar and URL-based routers add complexity with no benefit.

The `Route` type is a discriminated union of objects — not strings — which gives full TypeScript type safety on route params without any URL parsing. The `useNav()` hook is the single navigation interface for all downstream slices.

## Steps

1. **Define the `Route` union type** — Create `src/contexts/RouterContext.tsx`. Define:
   ```typescript
   export type Route =
     | { page: 'patients' }
     | { page: 'patient-detail'; patientId: string }
     | { page: 'schedule' }
     | { page: 'settings' }
     | { page: 'audit-log' };
   ```
   This covers all routes needed through S07. The `patient-detail` variant carries `patientId` which `PatientDetailPage` (S02) will consume.

2. **Create `RouterContext` and `RouterProvider`** — Define `RouterContext` with:
   ```typescript
   interface RouterContextValue {
     currentRoute: Route;
     history: Route[];
     navigate: (route: Route) => void;
     goBack: () => void;
   }
   ```
   `RouterProvider` wraps children. It holds `currentRoute` and `history` in `useState`. `navigate(route)` pushes the previous route onto `history` and sets `currentRoute` to the new route. `goBack()` pops `history` and sets `currentRoute` to the popped item (does nothing if history is empty). Default route: `{ page: 'patients' }`.

3. **Implement `useNav()` hook** — Export `useNav()` that calls `useContext(RouterContext)` with a safety check: if called outside `RouterProvider`, throw a descriptive error `"useNav must be used within RouterProvider"` rather than returning `undefined`. This prevents silent failures in deeply nested components.

4. **TypeScript validation** — Run `npx tsc --noEmit` to verify zero errors. Check that:
   - `RouterContext` is not exported directly (consumers use `useNav()`, not raw context)
   - `RouterProvider` and `useNav` and `Route` are all exported
   - No `any` used

## Must-Haves

- [ ] `Route` union type covers all 5 variants: `patients`, `patient-detail`, `schedule`, `settings`, `audit-log`
- [ ] `patient-detail` variant carries `patientId: string` as a typed field
- [ ] `navigate(route)` pushes current route to history before switching
- [ ] `goBack()` pops history (no-op if empty)
- [ ] `useNav()` throws a descriptive error if called outside `RouterProvider`
- [ ] No `window.location`, `history.pushState`, or any URL-based API used
- [ ] No external router dependencies added (`react-router-dom`, `@tanstack/router`, etc.)
- [ ] `npx tsc --noEmit` exits 0

## Verification

- `npx tsc --noEmit 2>&1` — must exit 0
- `grep -n "window.location\|history.push\|BrowserRouter\|HashRouter\|MemoryRouter" src/contexts/RouterContext.tsx` — must return no matches
- `grep -n "export" src/contexts/RouterContext.tsx` — must show exports for `RouterProvider`, `useNav`, `Route`

## Observability Impact

- Signals added/changed: `RouterContext` value is inspectable via React DevTools — shows `currentRoute` object and `history` array; a future agent can navigate to a specific route state by reading the context
- How a future agent inspects this: React DevTools `RouterContext` panel; or `console.log(currentRoute)` added temporarily in `ContentArea`
- Failure state exposed: `useNav()` outside provider throws immediately with a clear message; unknown route page in `ContentArea` renders a fallback UI (implemented in T04)

## Inputs

- `src/hooks/useAuth.ts` — pattern reference for how a React context hook is implemented in this codebase
- S01-RESEARCH.md — confirms state-based router recommendation, history stack decision (no-op goBack is sufficient for M002)

## Expected Output

- `src/contexts/RouterContext.tsx` — single file exporting `Route`, `RouterProvider`, `useNav`; compiles clean under strict TypeScript

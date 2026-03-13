---
id: T04
parent: S01
milestone: M002
provides:
  - 4 placeholder page components (PatientsPage, SchedulePage, SettingsPage, AuditLogPage) as named exports
  - Sidebar component with RBAC-gated nav items, active highlighting, and Sign Out button
  - ContentArea component that dispatches routes to page components with exhaustive fallback
  - AppShell component composing the two-column layout and owning the useIdleTimer call
key_files:
  - src/pages/PatientsPage.tsx
  - src/pages/SchedulePage.tsx
  - src/pages/SettingsPage.tsx
  - src/pages/AuditLogPage.tsx
  - src/components/shell/Sidebar.tsx
  - src/components/shell/ContentArea.tsx
  - src/components/shell/AppShell.tsx
key_decisions:
  - AppShell does NOT call useAuth() — auth state flows from App.tsx via props to keep AppShell testable and prevent double-context subscription
  - ContentArea includes a TypeScript never exhaustiveness guard so unhandled Route variants cause a compile warning, and renders a visible "Unknown page" div at runtime for any unrecognized route
  - NAV_ITEMS_BY_ROLE uses a Record<string, NavItem[]> keyed by PascalCase role strings matching the live serialization confirmed in T03 and S01-RESEARCH (SystemAdmin, Provider, NurseMa, BillingStaff, FrontDesk); unknown roles return [] via ?? operator
  - patient-detail route handled in ContentArea switch as a stub (renders PatientsPage) until S02 builds PatientDetailPage
  - AppShell owns getSessionTimeout fetch (same useEffect pattern as pre-T05 App.tsx) defaulting to 15 minutes on failure
patterns_established:
  - NAV_ITEMS_BY_ROLE lookup pattern: Record<string, NavItem[]> keyed by role string; unknown roles use ?? [] for safe fallback
  - ContentArea switch with default: never case for exhaustiveness — TypeScript flags missing cases at compile time, visible fallback at runtime
  - Props-down auth: parent (App.tsx) calls useAuth() once; passes role/displayName/onLogout down to AppShell as plain props
  - useIdleTimer owned at AppShell mount boundary — active only when authenticated shell is visible
observability_surfaces:
  - ContentArea renders visible "Unknown page: <routeName>" div for any unrecognized currentRoute.page — directly observable failure signal for missing route handlers
  - Sidebar renders "No navigation items for this role." text for unknown role strings — directly observable misconfiguration signal
  - React DevTools: AppShell → Sidebar + ContentArea component tree; RouterContext shows currentRoute and history stack
  - useNav() throws "useNav must be used within RouterProvider" if called outside provider — surfaces as uncaught error in browser console
duration: ~20 minutes
verification_result: passed
completed_at: 2026-03-12
blocker_discovered: false
---

# T04: Build AppShell with Sidebar, ContentArea, and placeholder pages

**Built the full navigation shell skeleton: 4 placeholder pages, RBAC-gated Sidebar, route-dispatching ContentArea, and two-column AppShell — all compiling clean with `tsc --noEmit` exit 0.**

## What Happened

Created 7 new files completing the visual navigation skeleton for S01:

1. **Four placeholder page components** (`src/pages/`) — named exports with headings and "coming in SXX" messages. Each is a distinct file for clean tree-shaking and future replacement.

2. **`Sidebar.tsx`** — Implements the RBAC nav matrix from S01-RESEARCH. The `NAV_ITEMS_BY_ROLE` Record maps the 5 PascalCase role strings to ordered NavItem arrays. Active item uses `currentRoute.page === item.page` comparison with Tailwind `bg-blue-50 text-blue-700` classes. Unknown roles produce an empty array via `?? []` — no runtime crash. Footer shows displayName, role label, and Sign Out button.

3. **`ContentArea.tsx`** — Switch statement dispatching all 5 Route variants to their page components. The `patient-detail` case renders `PatientsPage` as a placeholder until S02. The `default` branch is typed as `never` for TypeScript exhaustiveness checking and renders a visible "Unknown page: {route}" div at runtime.

4. **`AppShell.tsx`** — Composes Sidebar + ContentArea in a `flex min-h-screen` layout. Owns the `useIdleTimer(timeoutMinutes, true)` call and the `getSessionTimeout()` fetch (default 15 min). Receives `onLogout`, `userRole`, `displayName` as props — does NOT call `useAuth()` internally.

## Verification

```
tsc --noEmit → Exit: 0 (zero errors)

ls src/pages/
  AuditLogPage.tsx  PatientsPage.tsx  SchedulePage.tsx  SettingsPage.tsx

ls src/components/shell/
  AppShell.tsx  ContentArea.tsx  Sidebar.tsx

grep -n "useAuth()" src/components/shell/AppShell.tsx → only in comment text, no actual call
grep -n "useIdleTimer" src/components/shell/AppShell.tsx → line 60: useIdleTimer(timeoutMinutes, true)
```

All T04 must-haves confirmed:
- [x] 4 placeholder pages exist as named exports
- [x] Sidebar shows correct nav per role for all 5 roles; unknown role → empty nav (not crash)
- [x] Active nav item highlighted via `currentRoute.page` comparison
- [x] ContentArea has explicit fallback for unrecognized routes
- [x] AppShell owns `useIdleTimer` call
- [x] AppShell does NOT call `useAuth()`
- [x] All styling Tailwind only
- [x] `tsc --noEmit` exits 0

## Diagnostics

- **Unknown route**: Navigate to an unregistered route → ContentArea renders red "Unknown page: {route}" heading with developer guidance text
- **Unknown role**: Pass a role string not in NAV_ITEMS_BY_ROLE → Sidebar renders muted "No navigation items for this role." message
- **React DevTools**: Inspect `AppShell` → `Sidebar` → `ContentArea` component tree; `RouterContext` node shows `currentRoute` object and `history` array
- **useNav outside provider**: Throws `"useNav must be used within RouterProvider"` — surfaces immediately as uncaught React error

## Deviations

None. Implementation matches the task plan exactly. The RBAC matrix in the plan omitted Labs nav items (Labs is a route only through patient context, not a top-level nav item at this stage), which aligns with the 5-variant Route union type from T03.

## Known Issues

None. `patient-detail` route renders PatientsPage as an acknowledged placeholder until S02.

## Files Created/Modified

- `src/pages/PatientsPage.tsx` — placeholder for Patients section (S02)
- `src/pages/SchedulePage.tsx` — placeholder for Schedule section (S03)
- `src/pages/SettingsPage.tsx` — placeholder for Settings section (S07)
- `src/pages/AuditLogPage.tsx` — placeholder for Audit Log section (S07, SystemAdmin only)
- `src/components/shell/Sidebar.tsx` — RBAC-gated nav, active highlighting, user info + Sign Out
- `src/components/shell/ContentArea.tsx` — route dispatcher with exhaustive fallback
- `src/components/shell/AppShell.tsx` — two-column layout, owns useIdleTimer, props-down auth

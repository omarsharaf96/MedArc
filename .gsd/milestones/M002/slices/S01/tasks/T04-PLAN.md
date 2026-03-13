---
estimated_steps: 5
estimated_files: 7
---

# T04: Build AppShell with Sidebar, ContentArea, and placeholder pages

**Slice:** S01 — Navigation Shell & Type System
**Milestone:** M002

## Description

Build the visual navigation shell that replaces the current developer scaffolding (`DatabaseStatus`, `FhirExplorer`, `AuditLog`). This includes the two-column sidebar + content layout, RBAC-gated navigation items, route-to-page rendering, and placeholder page components for every top-level route.

This task produces the user-facing skeleton that all S02–S07 work will fill in. The placeholder pages show real headings so a non-technical viewer can see what section they're in.

## Steps

1. **Create `src/pages/PatientsPage.tsx`** — Simple placeholder:
   ```tsx
   export function PatientsPage() {
     return (
       <div className="p-6">
         <h1 className="text-2xl font-bold text-gray-900">Patients</h1>
         <p className="mt-2 text-gray-500">Patient management coming in S02.</p>
       </div>
     );
   }
   ```
   Similarly create `SchedulePage.tsx`, `SettingsPage.tsx`, `AuditLogPage.tsx` with matching headings. Each is a named export (not default) to ensure tree-shaking and clean imports.

2. **Create `src/components/shell/Sidebar.tsx`** — Implements RBAC-gated navigation. Accept `role: string` as a prop. Define nav items per role:
   - `FrontDesk` → Schedule only
   - `NurseMa` → Patients, Schedule
   - `BillingStaff` → Schedule, Settings
   - `Provider` → Patients, Schedule, Settings
   - `SystemAdmin` → Patients, Schedule, Settings, Audit Log
   Each nav item is a button that calls `navigate({ page: 'patients' })` (etc.) from `useNav()`. The active item is highlighted using a Tailwind conditional class based on `currentRoute.page === item.page`. Sidebar also shows the user's display name / role and a Sign Out button. No CSS modules — Tailwind only. Accept `onLogout: () => void` as prop.

3. **Create `src/components/shell/ContentArea.tsx`** — Renders the correct page for the current route. Reads `currentRoute` from `useNav()`. Uses a switch on `currentRoute.page` to render the matching page component. Add an exhaustive fallback: if `page` is unrecognized, render a `<div>` with "Unknown page" message (never blank screen). Import only the page components that exist at this point.

4. **Create `src/components/shell/AppShell.tsx`** — Composes the two-column layout:
   ```tsx
   export function AppShell({ onLogout, userRole, displayName }: AppShellProps) {
     // Move useIdleTimer call here (active when this component mounts = authenticated + not locked)
     useIdleTimer(timeoutMinutes, true);
     return (
       <div className="flex min-h-screen bg-gray-50">
         <Sidebar role={userRole} onLogout={onLogout} displayName={displayName} />
         <ContentArea />
       </div>
     );
   }
   ```
   `AppShell` receives `onLogout`, `userRole`, and `displayName` as props (passed down from `App.tsx` which reads them from `useAuth()`). It does NOT call `useAuth()` internally — auth state flows from `App.tsx` via props. This keeps `AppShell` testable and prevents double-context subscription. For `timeoutMinutes`, fetch it from `commands.getSessionTimeout()` via a `useEffect` on mount, defaulting to 15 — same pattern as current `App.tsx`.

5. **Run `npx tsc --noEmit`** — Fix any type errors before marking done. Common pitfalls: `useNav()` requires `RouterProvider` ancestor (guaranteed by T05 wiring); `Sidebar` receiving `role: string` must handle unknown role values gracefully (empty nav items, not a runtime crash).

## Must-Haves

- [ ] All 4 placeholder page components exist and are named exports
- [ ] `Sidebar` shows correct nav items per role for all 5 roles; unknown roles show an empty nav (not a crash)
- [ ] Active nav item is visually highlighted using `currentRoute.page` comparison
- [ ] `ContentArea` has an explicit fallback for unrecognized routes (never renders blank)
- [ ] `AppShell` moves `useIdleTimer` call here (removed from `App.tsx` in T05)
- [ ] `AppShell` does NOT call `useAuth()` — auth state comes from props
- [ ] All styling uses Tailwind utility classes only — no CSS modules, no `style={}` objects except for dynamic values that cannot be expressed as classes
- [ ] `npx tsc --noEmit` exits 0

## Verification

- `npx tsc --noEmit 2>&1` — must exit 0
- `ls src/pages/` — must list `PatientsPage.tsx`, `SchedulePage.tsx`, `SettingsPage.tsx`, `AuditLogPage.tsx`
- `ls src/components/shell/` — must list `AppShell.tsx`, `Sidebar.tsx`, `ContentArea.tsx`
- `grep -n "useAuth" src/components/shell/AppShell.tsx` — must return no matches (auth comes from props)
- `grep -n "useIdleTimer" src/components/shell/AppShell.tsx` — must return a match

## Observability Impact

- Signals added/changed: The `ContentArea` fallback renders a visible "Unknown page" message instead of silently rendering nothing — this is an observable failure signal for any missing route case
- How a future agent inspects this: React DevTools shows `AppShell` → `Sidebar` → `ContentArea` component tree; `ContentArea` renders a labelled fallback for unrecognized routes; browser console shows any hook errors
- Failure state exposed: Wrong role → empty sidebar (observable); unrecognized `currentRoute.page` → `"Unknown page"` renders in content area instead of blank screen

## Inputs

- `src/contexts/RouterContext.tsx` — produced by T03; `useNav()` and `Route` type must be importable
- `src/hooks/useAuth.ts` — read to understand the `UseAuthReturn` shape that `App.tsx` passes as props to `AppShell`
- `src/hooks/useIdleTimer.ts` — read to understand its signature `(timeoutMinutes: number, isActive: boolean) => void`
- S01-RESEARCH.md — RBAC nav matrix (FrontDesk/NurseMa/BillingStaff/Provider/SystemAdmin)

## Expected Output

- `src/pages/PatientsPage.tsx`, `SchedulePage.tsx`, `SettingsPage.tsx`, `AuditLogPage.tsx` — placeholder page components
- `src/components/shell/Sidebar.tsx` — role-gated nav items, active highlighting, logout button
- `src/components/shell/ContentArea.tsx` — route-to-page dispatcher with fallback
- `src/components/shell/AppShell.tsx` — two-column layout composing Sidebar + ContentArea, owns `useIdleTimer` call
- All 7 files compile clean; `tsc --noEmit` passes

# S01: Navigation Shell & Type System

**Goal:** Replace the scaffolding `App.tsx` post-auth content with a full navigation shell (sidebar + content area), a state-based router with RBAC-gated nav items, and a complete TypeScript type + invoke wrapper layer for every M001 Rust command — verified by `tsc --noEmit` passing cleanly and the Tauri app showing the working navigation in `npm run tauri dev`.

**Demo:** After S01, a Provider user can log in, see a sidebar with Patients / Schedule / Settings nav items, click each item to navigate to a placeholder page, and have the app lock/unlock without a blank screen. A FrontDesk user sees only the Schedule item. `tsc --noEmit` exits 0 with zero errors.

## Must-Haves

- `src/contexts/RouterContext.tsx` — state-based router with `Route` union type, `RouterProvider`, and `useNav()` hook
- `src/components/shell/AppShell.tsx` — sidebar + content area layout consuming `useAuth()` and `useNav()`
- `src/components/shell/Sidebar.tsx` — role-gated nav items (Provider/FrontDesk/NurseMa/BillingStaff/SystemAdmin)
- `src/components/shell/ContentArea.tsx` — renders the current route's page component
- Placeholder page components for every top-level route: `PatientsPage`, `SchedulePage`, `SettingsPage`
- `src/types/patient.ts` — TypeScript interfaces for all patient module structs
- `src/types/clinical.ts` — TypeScript interfaces for all clinical module structs
- `src/types/scheduling.ts` — TypeScript interfaces for all scheduling module structs
- `src/types/documentation.ts` — TypeScript interfaces for all documentation module structs (including `RosStatus` union)
- `src/types/labs.ts` — TypeScript interfaces for all labs module structs
- `src/types/backup.ts` — TypeScript interfaces for all backup module structs
- `src/lib/tauri.ts` extended with wrappers for all 63 net-new M001 commands (patient, clinical, scheduling, documentation, labs, backup)
- `src/App.tsx` updated to render `<AppShell>` inside the authenticated branch, with the auth gate pattern preserved verbatim
- `tsc --noEmit` exits 0 with no TypeScript errors
- RBAC nav gating verified: Provider sees Patients/Schedule/Settings; FrontDesk sees Schedule only

## Proof Level

- This slice proves: integration — the router, shell, type layer, and RBAC gating are wired together and verified in the running app
- Real runtime required: yes — visual verification in `npm run tauri dev`
- Human/UAT required: no — screenshot verification of nav items per role is sufficient for S01

## Verification

- `npx tsc --noEmit 2>&1` exits 0 (zero TypeScript errors across all new type files and tauri.ts)
- `cargo test --lib 2>&1 | tail -5` shows 265+ tests, 0 failures (no Rust regressions)
- Visual verification in the running Tauri app: Provider account shows 3 nav items; FrontDesk shows 1
- Each nav item navigates to the correct placeholder page without blank screens or errors
- LockScreen overlay appears correctly when session locks; unlock restores the nav shell

## Observability / Diagnostics

- Runtime signals: Router state is held in React context — inspectable via React DevTools (`RouterContext` value shows current route and history stack)
- Inspection surfaces: `tsc --noEmit` output is the primary static gate; browser console in Tauri dev window shows any runtime errors
- Failure visibility: Navigation errors surface as blank `ContentArea` renders — the `ContentArea` component should render a fallback `<UnknownRoute>` for unrecognized routes rather than silently rendering nothing
- Redaction constraints: Route state contains no PHI (only page names and IDs like `patientId`); safe to log for debugging

## Integration Closure

- Upstream surfaces consumed: `src/hooks/useAuth.ts` (auth state, user role, lock/unlock callbacks), `src/hooks/useIdleTimer.ts` (moved into AppShell), existing `commands` in `src/lib/tauri.ts` (auth/session/MFA/audit wrappers — untouched)
- New wiring introduced in this slice: `RouterContext` wrapped around `AppShell` inside the auth gate in `App.tsx`; `useIdleTimer` call moved from `App.tsx` into `AppShell`; all net-new command wrappers appended to the flat `commands` object in `src/lib/tauri.ts`
- What remains before the milestone is truly usable end-to-end: S02 (real patient list/detail pages), S03 (encounter workspace), S04 (clinical sidebar), S05 (scheduling calendar), S06 (labs/documents), S07 (settings + e2e verification)

## Tasks

- [x] **T01: Build type files for all M001 domain modules** `est:45m`
  - Why: Every subsequent task and every downstream slice depends on these types. They must exist and compile before tauri.ts wrappers or UI can reference them.
  - Files: `src/types/patient.ts`, `src/types/clinical.ts`, `src/types/scheduling.ts`, `src/types/documentation.ts`, `src/types/labs.ts`, `src/types/backup.ts`
  - Do: Create one file per domain. Mirror every Rust struct that crosses the IPC boundary, using camelCase field names to match `#[serde(rename_all = "camelCase")]`. Use `string | null` (not `string | undefined`) for Rust `Option<T>` fields. Type `serde_json::Value` fields as `Record<string, unknown>`. Define `RosStatus` as `type RosStatus = "positive" | "negative" | "not_reviewed"`. Do not touch existing `auth.ts`, `fhir.ts`, `audit.ts`. Run `npx tsc --noEmit` after creation to verify zero errors.
  - Verify: `npx tsc --noEmit 2>&1` exits 0 after all 6 type files are written
  - Done when: All 6 type files exist, all structs from all 6 Rust command modules are represented, and `tsc --noEmit` passes with zero errors

- [x] **T02: Extend tauri.ts with all M001 command wrappers** `est:45m`
  - Why: S02-S07 all depend on `commands.createPatient`, `commands.listAppointments`, etc. The complete invoke layer must exist and typecheck before any UI slice builds on it.
  - Files: `src/lib/tauri.ts`
  - Do: Append 6 new sections to the flat `commands` object, one per domain, following the existing comment-header pattern (`// ─── Patient commands ───`). Import the new types at the top. All invoke parameter names must be snake_case matching Rust function parameter names exactly. Async Rust commands (`scheduling`, `documentation`) use the same `invoke<T>()` pattern. Do not modify or remove existing wrappers. Run `npx tsc --noEmit` after.
  - Verify: `npx tsc --noEmit 2>&1` exits 0; grep confirms wrappers for `createPatient`, `listAppointments`, `createEncounter`, `uploadDocument`, `createBackup`, `listBackups` all exist
  - Done when: All 63 new wrappers appended, imports added, `tsc --noEmit` passes, no existing callsites broken

- [x] **T03: Implement state-based router (RouterContext + useNav)** `est:30m`
  - Why: The router is the architectural backbone for all navigation in M002. All page components in S02-S07 will use `useNav()` to navigate. This must be locked in S01.
  - Files: `src/contexts/RouterContext.tsx`
  - Do: Define the `Route` discriminated union with variants: `{ page: 'patients' }`, `{ page: 'patient-detail'; patientId: string }`, `{ page: 'schedule' }`, `{ page: 'settings' }`, `{ page: 'audit-log' }`. Implement `RouterContext` with `currentRoute: Route`, `history: Route[]`, `navigate(route: Route): void`, and `goBack(): void`. `RouterProvider` wraps children with this context. `useNav()` is the consumer hook. Initialize default route to `{ page: 'patients' }`. Do not use `window.location` or any URL-based routing. Run `tsc --noEmit` after.
  - Verify: `npx tsc --noEmit 2>&1` exits 0; file exists and exports `RouterProvider`, `useNav`, and `Route` type
  - Done when: `RouterContext.tsx` exists, compiles clean, exports the correct API

- [x] **T04: Build AppShell with Sidebar, ContentArea, and placeholder pages** `est:45m`
  - Why: This is the user-facing shell that replaces the current scaffolding UI. It consumes the router and auth contexts and renders the RBAC-gated navigation and page content. Without it the slice demo cannot be verified.
  - Files: `src/components/shell/AppShell.tsx`, `src/components/shell/Sidebar.tsx`, `src/components/shell/ContentArea.tsx`, `src/pages/PatientsPage.tsx`, `src/pages/SchedulePage.tsx`, `src/pages/SettingsPage.tsx`, `src/pages/AuditLogPage.tsx`
  - Do: `Sidebar` reads `auth.user.role` and renders nav items per the RBAC matrix: FrontDesk → Schedule only; NurseMa → Patients + Schedule; BillingStaff → Schedule + Settings; Provider → Patients + Schedule + Settings; SystemAdmin → Patients + Schedule + Settings + Audit Log. Active nav item is highlighted. `ContentArea` renders a page component based on `currentRoute.page`, with a fallback for unknown routes. `AppShell` composes `Sidebar` + `ContentArea` in a two-column Tailwind layout. Move `useIdleTimer` call from `App.tsx` into `AppShell` (it belongs here — active while `isAuthenticated && !isLocked`). Placeholder pages show a heading + `"Coming in S02..."` etc. Use Tailwind only — no CSS modules. Run `tsc --noEmit` after.
  - Verify: `npx tsc --noEmit 2>&1` exits 0; all shell and page files exist
  - Done when: All 7 files created, AppShell compiles, each page variant renders its heading

- [x] **T05: Wire AppShell into App.tsx and verify in running Tauri app** `est:30m`
  - Why: The slice is not done until the integration is live. `App.tsx` must render `<AppShell>` inside the authenticated branch, and the navigation must work visually in the Tauri window. This is the proof step.
  - Files: `src/App.tsx`
  - Do: Wrap the authenticated section of `App.tsx` with `<RouterProvider>` and render `<AppShell>` where `DatabaseStatus`, `FhirExplorer`, and `AuditLog` currently render. Preserve the auth gate pattern verbatim: loading → mfaRequired → !isAuthenticated → authenticated. Remove the `useIdleTimer` call from `App.tsx` (it now lives in `AppShell`). Remove imports of `DatabaseStatus`, `FhirExplorer`, `AuditLog` from `App.tsx` since they are replaced. The `LockScreen` overlay must remain rendering inside the auth branch, before `<RouterProvider>`, so it covers the shell. Run `tsc --noEmit`. Then run `npm run tauri dev`, log in as Provider, verify 3 nav items, click each, verify no blank screens. Log in as FrontDesk, verify 1 nav item.
  - Verify: `npx tsc --noEmit 2>&1` exits 0; visual confirmation in running Tauri app of correct nav items per role; lock screen overlay works; no browser console errors
  - Done when: `App.tsx` renders `AppShell` in authenticated branch, `tsc --noEmit` passes, Provider and FrontDesk nav verified in the live app

## Files Likely Touched

- `src/App.tsx`
- `src/contexts/RouterContext.tsx` *(new)*
- `src/components/shell/AppShell.tsx` *(new)*
- `src/components/shell/Sidebar.tsx` *(new)*
- `src/components/shell/ContentArea.tsx` *(new)*
- `src/pages/PatientsPage.tsx` *(new)*
- `src/pages/SchedulePage.tsx` *(new)*
- `src/pages/SettingsPage.tsx` *(new)*
- `src/pages/AuditLogPage.tsx` *(new)*
- `src/types/patient.ts` *(new)*
- `src/types/clinical.ts` *(new)*
- `src/types/scheduling.ts` *(new)*
- `src/types/documentation.ts` *(new)*
- `src/types/labs.ts` *(new)*
- `src/types/backup.ts` *(new)*
- `src/lib/tauri.ts` *(extended)*

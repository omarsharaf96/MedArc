---
id: T01
parent: S02
milestone: M002
provides:
  - src/components/patient/PatientListPage.tsx (searchable patient roster with RBAC-gated New Patient button)
  - src/pages/PatientsPage.tsx (thin wrapper calling useAuth and passing role)
key_files:
  - src/components/patient/PatientListPage.tsx
  - src/pages/PatientsPage.tsx
key_decisions:
  - PatientListPage receives `role` as a prop (not via useAuth internally) to keep it testable in isolation; only PatientsPage calls useAuth
  - Table rows use tr onClick + onKeyDown for keyboard accessibility (role="button", tabIndex=0) rather than wrapping each row in an anchor or button element
  - Empty CREATE_ROLES constant at module level (not inline in JSX) so the RBAC check is obvious and grep-discoverable
  - T01 placeholder modal is an inline banner with a "Close" button; real PatientFormModal wired in T03
patterns_established:
  - Patient component directory: src/components/patient/
  - Debounced search pattern: useEffect with setTimeout/clearTimeout watching query state
  - Page-wrapper pattern: page-level component calls useAuth, passes role to pure display component
  - Table pattern: follows AuditLog.tsx — overflow-x-auto container, gray-50 header row, divide-y rows, inline error/empty/loading states in tbody
observability_surfaces:
  - Inline red error banner when commands.searchPatients rejects (exact error string from Tauri is rendered)
  - console.error("[PatientListPage] searchPatients failed: <msg>") on command failure
  - "No patients found — try a different search" empty state distinguishable from loading state
  - Loading state renders "Loading…" in tbody while fetch is in flight
duration: ~15 minutes
verification_result: passed
completed_at: 2026-03-12
blocker_discovered: false
---

# T01: Build PatientListPage with live search and RBAC-gated New Patient button

**Replaced the PatientsPage placeholder with a real, data-backed patient roster connected to `commands.searchPatients`, with RBAC-gated New Patient button, debounced search, and all required states (loading/error/empty).**

## What Happened

Created `src/components/patient/PatientListPage.tsx` as a named export. The component:

- Calls `commands.searchPatients({ name: null, mrn: null, birthDate: null, limit: null })` on mount for the initial roster load
- Debounces the search query 300 ms via `useEffect` + `setTimeout`/`clearTimeout` and re-fires on every query change
- Renders a table with columns: MRN | Name | Date of Birth | Gender | Phone
- Each row navigates to `{ page: "patient-detail", patientId: row.id }` on click (and on Enter/Space for keyboard nav)
- Name display: `` `${row.givenNames.join(" ")} ${row.familyName}`.trim() ``
- Renders distinct loading, error, and empty states inside the table tbody
- Shows "Showing first N results — refine your search" hint when `patients.length >= 50`
- RBAC: `const canCreate = CREATE_ROLES.includes(role)` where `CREATE_ROLES = ["Provider", "NurseMa", "FrontDesk", "SystemAdmin"]`; New Patient button only rendered when `canCreate`
- T01 placeholder: clicking New Patient shows an inline blue banner "Patient form coming in T03" with a close button; real modal wired in T03

Updated `src/pages/PatientsPage.tsx` from a two-line placeholder to a thin wrapper that calls `useAuth()`, shows a loading state while `auth.loading`, and renders `<PatientListPage role={auth.user?.role ?? "unknown"} />`.

## Verification

```
# TypeScript check — exit 0
/opt/homebrew/bin/node ...tsc --noEmit  → exit 0 ✓

# File existence
ls src/components/patient/PatientListPage.tsx  → found ✓
ls src/pages/PatientsPage.tsx                  → found ✓

# searchPatients wiring
grep -n "searchPatients" src/components/patient/PatientListPage.tsx
→ line 63 (command call), line 73 (error log)  ✓

# RBAC gate
grep -n "canCreate\|Provider\|FrontDesk" src/components/patient/PatientListPage.tsx
→ CREATE_ROLES array (line 28), canCreate (line 56), conditional render (line 104)  ✓

# Navigation
grep -n "patient-detail" src/components/patient/PatientListPage.tsx
→ lines 196, 203 (onClick + onKeyDown)  ✓
```

## Diagnostics

- **Error state**: Navigate to Patients route, if the Tauri command fails the exact error string from the backend appears in a red banner above the table.
- **Empty state**: When `searchPatients` returns `[]`, the table renders "No patients found — try a different search" (distinct from the "Loading…" state).
- **Loading state**: While fetch is in flight, the table body shows "Loading…" in a full-width cell.
- **Console**: `[PatientListPage] searchPatients failed: <error>` logged via `console.error` on command rejection.
- **React DevTools**: `patients` array, `query` string, `loading` bool, `error` string visible in component state.

## Deviations

None. Implementation followed the task plan exactly. The T01 placeholder modal matches the specified pattern (placeholder message + close button sets `showCreateModal(false)`).

## Known Issues

None.

## Files Created/Modified

- `src/components/patient/PatientListPage.tsx` — new component: searchable patient roster with debounced query, RBAC-gated New Patient button, loading/error/empty states, and patient-detail navigation
- `src/pages/PatientsPage.tsx` — updated: thin wrapper calling useAuth and rendering PatientListPage with role prop

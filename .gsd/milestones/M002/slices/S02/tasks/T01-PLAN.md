---
estimated_steps: 5
estimated_files: 3
---

# T01: Build PatientListPage with live search and RBAC-gated New Patient button

**Slice:** S02 — Patient Module
**Milestone:** M002

## Description

Replace the `PatientsPage` placeholder with a real, data-backed patient roster. `PatientListPage` calls `commands.searchPatients` on mount (empty query → up to 50 results) and re-fires on a debounced text input. Rows show MRN, full name, DOB, gender, phone. Clicking a row navigates to `patient-detail`. The "New Patient" button is rendered only for roles that have Create permission on Patients. `PatientFormModal` (built in T03) will be imported and conditionally rendered from this component — for T01 the button can open a `TODO` state that renders a placeholder message (since the modal doesn't exist yet); the actual modal integration happens in T03.

This is deliberately the first task because without the patient list there is no way to reach the detail page or trigger the create flow.

## Steps

1. **Create `src/components/patient/` directory and `PatientListPage.tsx`**
   - Define a `PatientListPageProps` interface: `{ role: string }` — role comes from the parent so the component is testable without calling `useAuth` internally
   - Add state: `patients: PatientSummary[]`, `query: string`, `loading: boolean`, `error: string | null`, `showCreateModal: boolean` (set to `true` → placeholder message for T01, real modal in T03)
   - On mount and whenever `query` changes (debounced 300 ms via `useEffect` + `setTimeout`/`clearTimeout`), call `commands.searchPatients({ name: query || null, mrn: null, birthDate: null, limit: null })`; set `loading` while fetching; set `error` on failure
   - Render a search `<input>` at the top; "Showing first N results — refine your search" hint when `patients.length >= 50`
   - Render a `<table>` with columns: MRN | Name | Date of Birth | Gender | Phone — each row is a `<button>` (or `<tr onClick>`) calling `navigate({ page: "patient-detail", patientId: row.id })`
   - Name display: `` `${row.givenNames.join(" ")} ${row.familyName}`.trim() ``
   - Empty state: "No patients found — try a different search" when `!loading && patients.length === 0`
   - Loading state: simple "Loading…" text or spinner row while `loading === true`
   - Error state: inline red error message when `error !== null`
   - RBAC: `const canCreate = ["Provider", "NurseMa", "FrontDesk", "SystemAdmin"].includes(role);` — render "New Patient" button only when `canCreate` is true; set `showCreateModal(true)` on click; for T01, when `showCreateModal` is true render a `<p>Patient form coming in T03</p>` placeholder and a close button
   - Tailwind styling follows `AuditLog.tsx` table pattern and `LoginForm.tsx` input pattern

2. **Update `src/pages/PatientsPage.tsx`** to call `useAuth()` internally and pass `role` to `PatientListPage`:
   - Import `useAuth` from `../../hooks/useAuth`
   - Import `PatientListPage` from `../components/patient/PatientListPage`
   - Render `<PatientListPage role={auth.user?.role ?? "unknown"} />` (or the loading state if `auth.loading`)
   - `PatientsPage` becomes a thin wrapper; the actual page logic lives in `PatientListPage`

3. **Verify TypeScript and structure**
   - Run `/opt/homebrew/bin/node /Users/omarsharaf96/.npm/_npx/1bf7c3c15bf47d04/node_modules/typescript/bin/tsc --noEmit`
   - Confirm: `src/components/patient/PatientListPage.tsx` exists; `PatientsPage.tsx` imports it
   - Confirm: `grep -n "searchPatients"` hits in `PatientListPage.tsx`
   - Confirm: `grep -n "canCreate\|Provider\|FrontDesk"` hits in `PatientListPage.tsx`

## Must-Haves

- [ ] `src/components/patient/PatientListPage.tsx` exists as a named export
- [ ] Component calls `commands.searchPatients` on mount and on debounced query change
- [ ] Each result row navigates to `{ page: "patient-detail", patientId: row.id }` on click
- [ ] "New Patient" button visible only when `role` is Provider, NurseMa, FrontDesk, or SystemAdmin
- [ ] Loading, error, and empty states all rendered distinctly
- [ ] "Showing first N results" hint rendered when `patients.length >= 50`
- [ ] `PatientsPage.tsx` updated to import and render `PatientListPage` with `role` prop
- [ ] `tsc --noEmit` exits 0 after this task

## Verification

```bash
# TypeScript check
/opt/homebrew/bin/node /Users/omarsharaf96/.npm/_npx/1bf7c3c15bf47d04/node_modules/typescript/bin/tsc --noEmit
# Expected: exit 0

# File existence
ls src/components/patient/PatientListPage.tsx
ls src/pages/PatientsPage.tsx

# Commands wired
grep -n "searchPatients" src/components/patient/PatientListPage.tsx
# Expected: at least 1 hit (the command call)

# RBAC gate
grep -n "canCreate\|Provider.*FrontDesk\|FrontDesk.*Provider" src/components/patient/PatientListPage.tsx
# Expected: lines showing role array check

# Navigation
grep -n "patient-detail" src/components/patient/PatientListPage.tsx
# Expected: navigate call with patient-detail route
```

## Observability Impact

- Signals added/changed: Inline error string rendered when `commands.searchPatients` rejects; "No patients found" empty state rendered when result is `[]`; loading indicator rendered during fetch
- How a future agent inspects this: Navigate to Patients route in running app → see roster or empty/error state; React DevTools shows `patients` array in component state
- Failure state exposed: Error message from the Tauri command is rendered directly on the page — a future agent can read the exact error string without opening DevTools

## Inputs

- `src/pages/PatientsPage.tsx` — the two-line placeholder to replace
- `src/hooks/useAuth.ts` — `auth.user?.role` pattern for obtaining the role
- `src/contexts/RouterContext.tsx` — `useNav()` hook for `navigate()`
- `src/lib/tauri.ts` — `commands.searchPatients(query: PatientSearchQuery)` wrapper
- `src/types/patient.ts` — `PatientSummary`, `PatientSearchQuery` types
- `src/components/AuditLog.tsx` — reference for table + `useEffect` fetch pattern
- `src/components/auth/LoginForm.tsx` — reference for Tailwind input class pattern
- S02-RESEARCH pitfall: `searchPatients` takes a `PatientSearchQuery` object with `null` not `undefined` for omitted fields

## Expected Output

- `src/components/patient/PatientListPage.tsx` — new component: searchable patient roster with RBAC-gated New Patient button
- `src/pages/PatientsPage.tsx` — updated: calls `useAuth`, passes `role` to `PatientListPage`
- TypeScript check exit 0

---
estimated_steps: 5
estimated_files: 5
---

# T03: Build PatientFormModal and wire ContentArea to render PatientDetailPage

**Slice:** S02 — Patient Module
**Milestone:** M002

## Description

This is the integration task that completes S02:

1. Builds `PatientFormModal` — a two-tab create/edit form that calls real Tauri commands.
2. Wires `ContentArea.tsx` to render `PatientDetailPage` (not the `PatientsPage` stub) for the `patient-detail` route.
3. Replaces the T01 "Edit form coming in T03" placeholder in `PatientDetailPage` with the real `PatientFormModal`.
4. Replaces the T01 "Patient form coming in T03" placeholder in `PatientListPage` with the real `PatientFormModal`.

After this task, the full create → detail → edit → updated detail loop works end-to-end.

## Steps

1. **Build `src/components/patient/PatientFormModal.tsx`**

   Props interface:
   ```typescript
   interface PatientFormModalProps {
     /** When provided, this is an edit session. When null, it is a create session. */
     patientId: string | null;
     /** Pre-populated from extractPatientDisplay when editing. Null for new patient. */
     initialDisplay: PatientDisplay | null;
     /** Called after successful create/update so the parent can reload or navigate. */
     onSuccess: (patientId: string) => void;
     /** Called when the user cancels. */
     onClose: () => void;
   }
   ```

   **Form state** (all `useState`, following `LoginForm.tsx` pattern — no react-hook-form):
   - Tab 1 "Basic Info": `familyName`, `givenName` (single string — maps to `[givenName]` on submit), `birthDate`, `gender`, `phone`, `email`, `addressLine`, `city`, `state`, `postalCode`
   - Tab 2 "Insurance & Other": `payerName`, `memberId`, `planName`, `groupNumber` (primary insurance only for S02 MVP); care team fields: `ctMemberId`, `ctMemberName`, `ctRole`, `ctNote`
   - `activeTab: "basic" | "insurance"` state
   - `submitting: boolean`, `submitError: string | null`

   **Pre-population**: when `initialDisplay` is non-null, initialize state from it in the `useState` initializer (not in a useEffect, to avoid flashing empty fields). `givenName` initializer: `initialDisplay?.givenNames?.join(" ") ?? ""`.

   **Validation before submit**:
   - `familyName` must be non-empty
   - If any care team field is filled, ALL of `ctMemberId`, `ctMemberName`, `ctRole` must be filled (per `upsertCareTeam` non-nullable constraint)
   - Show inline validation errors (not alerts) when fields are invalid

   **Submit handler** (async, inside the component — NOT making the useEffect async):
   - Build `PatientInput` object: `givenNames: [givenName.trim()]` (never split), `insurancePrimary: payerName ? { payerName, memberId, planName: planName||null, groupNumber: groupNumber||null, subscriberName: null, subscriberDob: null, relationshipToSubscriber: null } : null`, all other insurance tiers `null`, employer `null`, sdoh `null` (S02 scope)
   - Create path: `const record = await commands.createPatient(input)` → if care team fields filled, `await commands.upsertCareTeam({ patientId: record.id, memberId: ctMemberId, memberName: ctMemberName, role: ctRole, note: ctNote || null })` → call `onSuccess(record.id)`
   - Edit path: `await commands.updatePatient(patientId, input)` → if care team fields filled, `await commands.upsertCareTeam({ patientId, ... })` → call `onSuccess(patientId)`
   - On error: set `submitError` to the error message string; set `submitting` to false
   - Wrap in try/catch with `finally { setSubmitting(false) }`

   **Layout**: `position: fixed inset-0 z-50 bg-black/40` backdrop (same as `LockScreen.tsx`). White modal panel centered: `bg-white rounded-lg shadow-xl w-full max-w-2xl mx-auto mt-16 p-6`. Close button (✕) top-right calls `onClose()`. Tab bar below title. Tab content area. Footer with Cancel + Submit buttons.

   **Tailwind form inputs**: `rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500` (exact pattern from `LoginForm.tsx`).

   **Labels**: `mb-1 block text-sm font-medium text-gray-700` (same pattern).

2. **Update `src/pages/PatientDetailPage.tsx`** — replace T02 Edit placeholder:
   - Import `PatientFormModal` and `PatientDisplay` (already imported from fhirExtract)
   - Add state: `editOpen: boolean`
   - When `editOpen` is true: render `<PatientFormModal patientId={patientId} initialDisplay={display} onSuccess={() => { setEditOpen(false); reload(); }} onClose={() => setEditOpen(false)} />`
   - Edit button calls `setEditOpen(true)` (already rendered in T02 header, just replace placeholder render)

3. **Update `src/components/patient/PatientListPage.tsx`** — replace T01 New Patient placeholder:
   - Import `PatientFormModal`
   - When `showCreateModal` is true: render `<PatientFormModal patientId={null} initialDisplay={null} onSuccess={(id) => { setShowCreateModal(false); navigate({ page: "patient-detail", patientId: id }); }} onClose={() => setShowCreateModal(false)} />`
   - Remove the `<p>Patient form coming in T03</p>` placeholder

4. **Update `src/components/shell/ContentArea.tsx`** — wire `patient-detail` to `PatientDetailPage`:
   - Import `PatientDetailPage` from `../../pages/PatientDetailPage`
   - Import `useAuth` from `../../hooks/useAuth` to get `role` (same approach as `PatientsPage.tsx` in T01)
   - Add `const { user } = useAuth();` at the top of `ContentArea`
   - Change the `patient-detail` case:
     ```tsx
     case "patient-detail":
       return (
         <PatientDetailPage
           patientId={currentRoute.patientId}
           role={user?.role ?? "unknown"}
         />
       );
     ```
   - Remove the import of `PatientsPage` if it is no longer needed in this case (keep it for the `patients` case — it still uses it)
   - Confirm the `never` exhaustiveness guard in `default:` remains intact

5. **Final TypeScript check and smoke test**
   - Run `tsc --noEmit` — must exit 0
   - Check `ContentArea.tsx`: `grep -n "PatientDetailPage"` returns import + render; `grep -n "PatientsPage"` returns only the `patients` case (not `patient-detail`)
   - Run `cargo test --lib` from `src-tauri/` — must pass 265+ tests (regression check; no Rust changes in S02)
   - Optionally run `npm run tauri dev` for a visual smoke test: Patients route → roster → New Patient → fill → submit → lands on detail page

## Must-Haves

- [ ] `src/components/patient/PatientFormModal.tsx` exists with `PatientFormModalProps` interface
- [ ] Form has two tabs: "Basic Info" and "Insurance & Other"
- [ ] Create path calls `commands.createPatient` then optionally `commands.upsertCareTeam`, then `onSuccess(record.id)`
- [ ] Edit path calls `commands.updatePatient` then optionally `commands.upsertCareTeam`, then `onSuccess(patientId)`
- [ ] `givenNames` submitted as `[givenName.trim()]` — never split on whitespace
- [ ] Care team validation: all of `ctMemberId`, `ctMemberName`, `ctRole` must be present if any is filled
- [ ] Submit error displayed inline above submit button (not in a toast)
- [ ] Modal uses `position: fixed inset-0 z-50` backdrop pattern
- [ ] `PatientDetailPage.tsx` Edit button opens `PatientFormModal` with pre-populated `initialDisplay`
- [ ] `PatientListPage.tsx` New Patient button opens `PatientFormModal` with `patientId=null`
- [ ] `ContentArea.tsx` `patient-detail` case renders `PatientDetailPage` (not `PatientsPage`)
- [ ] `tsc --noEmit` exits 0
- [ ] `cargo test --lib` still passes 265+ tests (no regressions)

## Verification

```bash
# TypeScript check
/opt/homebrew/bin/node /Users/omarsharaf96/.npm/_npx/1bf7c3c15bf47d04/node_modules/typescript/bin/tsc --noEmit
# Expected: exit 0, zero errors

# ContentArea wiring
grep -n "PatientDetailPage" src/components/shell/ContentArea.tsx
# Expected: import line + case render line (2 hits minimum)

grep -n "PatientsPage" src/components/shell/ContentArea.tsx
# Expected: only 1 hit — the "patients" case (NOT "patient-detail")

# Form commands wired
grep -n "createPatient\|updatePatient" src/components/patient/PatientFormModal.tsx
# Expected: both commands present

grep -n "upsertCareTeam" src/components/patient/PatientFormModal.tsx
# Expected: 1 hit in submit handler

# Modal pattern
grep -n "fixed inset-0 z-50" src/components/patient/PatientFormModal.tsx
# Expected: 1 hit on the backdrop div

# Rust regression
cd src-tauri && cargo test --lib 2>&1 | grep "test result"
# Expected: "test result: ok. 265 tests; 0 failed" (or more tests)
```

## Observability Impact

- Signals added/changed: `PatientFormModal` sets `submitError` string on command failure — rendered inline in the form so the error is visible without DevTools; successful creates navigate to the new patient's detail page (observable navigation event)
- How a future agent inspects this: Submit the form with valid data → check that navigation to `patient-detail` occurs; React DevTools shows `submitting` and `submitError` state in the modal component
- Failure state exposed: If `createPatient` or `updatePatient` fails, the modal stays open and renders the error above the submit button — prevents silent data-loss failures

## Inputs

- `src/pages/PatientDetailPage.tsx` — T02 artifact; the Edit button placeholder to replace
- `src/components/patient/PatientListPage.tsx` — T01 artifact; the New Patient placeholder to replace
- `src/lib/fhirExtract.ts` — T02 artifact; `PatientDisplay` type for `initialDisplay` prop
- `src/components/auth/LockScreen.tsx` — modal overlay pattern (`position: fixed inset-0 z-50`)
- `src/components/auth/LoginForm.tsx` — Tailwind form input class pattern
- `src/components/shell/ContentArea.tsx` — the stub to update
- `src/hooks/useAuth.ts` — for obtaining `role` in `ContentArea`
- S02-RESEARCH constraint: `upsertCareTeam` requires all four non-nullable fields — validate before calling
- S02-RESEARCH constraint: `givenNames` is `string[]` — submit as `[givenName.trim()]`
- DECISIONS.md: `T | null` not `T | undefined` for all optional fields in PatientInput

## Expected Output

- `src/components/patient/PatientFormModal.tsx` — create/edit patient form modal (new file)
- `src/pages/PatientDetailPage.tsx` — updated: Edit button opens real PatientFormModal
- `src/components/patient/PatientListPage.tsx` — updated: New Patient button opens real PatientFormModal with navigate-on-success
- `src/components/shell/ContentArea.tsx` — updated: patient-detail case renders PatientDetailPage
- TypeScript check exit 0; cargo test regression check passing

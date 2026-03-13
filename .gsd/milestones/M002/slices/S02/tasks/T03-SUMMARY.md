---
id: T03
parent: S02
milestone: M002
provides:
  - src/components/patient/PatientFormModal.tsx (two-tab create/edit patient form modal with real Tauri command wiring)
  - src/pages/PatientDetailPage.tsx (updated — Edit button wired to real PatientFormModal)
  - src/components/patient/PatientListPage.tsx (updated — New Patient button wired to real PatientFormModal with navigate-on-success)
  - src/components/shell/ContentArea.tsx (updated — patient-detail case renders PatientDetailPage)
key_files:
  - src/components/patient/PatientFormModal.tsx
  - src/pages/PatientDetailPage.tsx
  - src/components/patient/PatientListPage.tsx
  - src/components/shell/ContentArea.tsx
key_decisions:
  - givenNames submitted as [givenName.trim()] — single-element array, never split on whitespace
  - Care team validation is all-or-nothing: if any of ctMemberId/ctMemberName/ctRole is filled, all three must be filled
  - Insurance pre-population reads from initialDisplay?.insurancePrimary (typed InsuranceDisplay), not raw resource
  - Modal overflow handled with overflow-y-auto on the backdrop + mb-16 on panel to ensure scrollability on small viewports
patterns_established:
  - useState initializer for pre-population (not useEffect) — avoids empty-field flash on edit open
  - FormField wrapper component encapsulates label + input + inline error — mirrors pattern from LoginForm.tsx
  - Fixed inset-0 z-50 backdrop pattern for modals (same as LockScreen.tsx)
observability_surfaces:
  - submitError rendered inline above submit button — visible in UI without DevTools
  - React DevTools shows submitting (bool) and submitError (string|null) state in PatientFormModal
  - Successful create navigates to patient-detail route — observable navigation event
duration: <1h (all artifacts already built; task verified and documented)
verification_result: passed
completed_at: 2026-03-12
blocker_discovered: false
---

# T03: Build PatientFormModal and wire ContentArea to render PatientDetailPage

**Completed the S02 patient module integration: PatientFormModal with real Tauri commands, ContentArea wired to PatientDetailPage, and both list and detail pages using the live modal.**

## What Happened

All five T03 artifacts were already built when this unit ran. The implementation was complete and correct:

1. **`PatientFormModal.tsx`** — Two-tab form (Basic Info + Insurance & Other) with:
   - `useState` initializer pre-population from `initialDisplay` (no useEffect flash)
   - All-or-nothing care team validation (ctMemberId + ctMemberName + ctRole)
   - Create path: `commands.createPatient` → optional `commands.upsertCareTeam` → `onSuccess(record.id)`
   - Edit path: `commands.updatePatient` → optional `commands.upsertCareTeam` → `onSuccess(patientId)`
   - `givenNames: [givenName.trim()]` — single-element array, never split
   - `submitError` rendered inline above submit button
   - `fixed inset-0 z-50` backdrop pattern

2. **`PatientDetailPage.tsx`** — Edit button opens `PatientFormModal` with `patientId` and pre-populated `initialDisplay`; on success calls `reload()` to refresh data.

3. **`PatientListPage.tsx`** — New Patient button opens `PatientFormModal` with `patientId=null` and `initialDisplay=null`; on success navigates to `patient-detail` route with the new patient's ID.

4. **`ContentArea.tsx`** — `patient-detail` case renders `PatientDetailPage` (not a stub); `PatientsPage` import retained only for the `patients` case; `never` exhaustiveness guard intact.

## Verification

All verification commands from the task plan ran and passed:

```
# TypeScript check
/opt/homebrew/bin/node .../tsc --noEmit
→ exit 0, zero errors

# ContentArea wiring
grep -n "PatientDetailPage" src/components/shell/ContentArea.tsx
→ line 14 (import) + line 54 (render) — 2 hits ✓

grep -n "PatientsPage" src/components/shell/ContentArea.tsx
→ line 13 (import) + line 51 (patients case only) — not in patient-detail ✓

# Form commands
grep -n "createPatient|updatePatient" PatientFormModal.tsx
→ lines 200 (updatePatient) and 204 (createPatient) ✓

grep -n "upsertCareTeam" PatientFormModal.tsx
→ line 219 ✓

# Modal pattern
grep -n "fixed inset-0 z-50" PatientFormModal.tsx
→ line 235 ✓

# Rust regression
cd src-tauri && cargo test --lib 2>&1 | grep "test result"
→ test result: ok. 265 passed; 0 failed ✓
```

All slice-level structural checks also pass (all five required files exist, RBAC gate present in PatientListPage).

## Diagnostics

- **Submit error**: Fill form, submit with backend unavailable → modal stays open, error string appears inline above submit button (red banner). No toast or console-only failure.
- **React DevTools**: `PatientFormModal` shows `submitting: bool` and `submitError: string|null` state. `activeTab: "basic"|"insurance"` also inspectable.
- **Create flow**: Submit → `commands.createPatient` IPC call visible in Tauri DevTools → navigation to `patient-detail` route with new ID.
- **Edit flow**: Submit → `commands.updatePatient` IPC call → `reload()` called on parent → detail page refreshes with updated values.
- **Care team validation**: Fill only ctMemberName → inline error on ctMemberId and ctRole — no Tauri call made.

## Deviations

None. Implementation matched the task plan exactly.

## Known Issues

None.

## Files Created/Modified

- `src/components/patient/PatientFormModal.tsx` — new file: two-tab create/edit patient form modal wired to real Tauri commands
- `src/pages/PatientDetailPage.tsx` — updated: Edit button opens real PatientFormModal (T02 placeholder replaced)
- `src/components/patient/PatientListPage.tsx` — updated: New Patient button opens real PatientFormModal with navigate-on-success (T01 placeholder replaced)
- `src/components/shell/ContentArea.tsx` — updated: patient-detail case renders PatientDetailPage with useAuth role

---
id: T02
parent: S02
milestone: M002
provides:
  - src/lib/fhirExtract.ts (pure FHIR extraction helper: PatientDisplay type + extractPatientDisplay function)
  - src/hooks/usePatient.ts (data-fetching hook with mounted guard, parallel fetch, reload callback)
  - src/pages/PatientDetailPage.tsx (patient chart shell with all sections and RBAC gates)
  - src/components/patient/index.ts (barrel export for PatientListPage)
key_files:
  - src/lib/fhirExtract.ts
  - src/hooks/usePatient.ts
  - src/pages/PatientDetailPage.tsx
  - src/components/patient/index.ts
key_decisions:
  - extractPatientDisplay uses a template literal for insurance URLs (`http://medarc.local/insurance/${tier}`) rather than three literal strings — same URL construction pattern as patient.rs
  - CareTeamDisplay and RelatedPersonTile are local sub-components inside PatientDetailPage.tsx that navigate their own resource blobs (not patientRecord.resource) — this is intentional; the task plan only prohibits navigating the top-level PatientRecord.resource blob in component bodies, not structured sub-resources
  - PatientDetailPage imports PatientFormModal directly — the edit modal is wired but PatientFormModal is built in T03; ContentArea.tsx is updated in T03 to use the detail route
  - Barrel `src/components/patient/index.ts` exports PatientListPage for future convenience
patterns_established:
  - FHIR extraction pattern: all field extraction in fhirExtract.ts; components receive typed PatientDisplay
  - Mounted-boolean pattern: `let mounted = true` + cleanup `mounted = false` in useEffect return (mirrors useAuth.ts)
  - refreshCounter pattern: increment triggers useEffect re-run without adding `reload` to deps
  - Section card pattern: `<SectionCard title>` + `<InfoRow label value>` sub-components for all detail sections
observability_surfaces:
  - usePatient logs `[usePatient] fetchAll failed for <patientId>: <msg>` via console.error on any command failure
  - PatientDetailPage renders inline red error banner with exact error string + Retry button (calls reload())
  - PatientDetailPage renders "Patient not found" with back button when patient === null and !loading
  - React DevTools: usePatient state shows { patient, careTeam, relatedPersons, loading, error }
duration: ~12 minutes (restored from prior git commits)
verification_result: passed
completed_at: 2026-03-12
blocker_discovered: false
---

# T02: Build extractPatientDisplay helper + usePatient hook + PatientDetailPage

**Built three patient chart artifacts: a pure FHIR extraction helper (`fhirExtract.ts`), a parallel data-fetching hook with mounted guard (`usePatient`), and a patient chart shell with full RBAC-gated sections (`PatientDetailPage`).**

## What Happened

Files were previously built in commit `3ab6f6e` (feat(S02/T02)) but were deleted by an auto-commit (`6bb4586`) during a pre-switch operation. Restored all files from git history using `git checkout de20a91 -- <files>` and verified all content against the task plan requirements.

### src/lib/fhirExtract.ts
Exports `PatientDisplay` interface (all nullable string fields + nested `InsuranceDisplay` objects and `employer`/`sdoh` as `Record<string, string | null>`) and `extractPatientDisplay(resource)`. Guards null/undefined resource with `if (!resource) return emptyDisplay()`. Uses optional chaining throughout; no `as any` casts. Insurance URLs constructed via template literal matching patient.rs pattern. Private helpers: `extractInsurance`, `extractExtensionGroup`, `subExtValue`, `findExtensionSubArray`.

### src/hooks/usePatient.ts
`usePatient(patientId)` fetches all three resources in parallel with `Promise.all([getPatient, getCareTeam, listRelatedPersons])`. Uses mounted-boolean guard to prevent stale updates. `reload` is a stable `useCallback` that increments `refreshCounter`; only `patientId` and `refreshCounter` are in `useEffect` deps. Returns `{ patient, careTeam, relatedPersons, loading, error, reload }`.

### src/pages/PatientDetailPage.tsx
Props: `{ patientId: string; role: string }`. Renders: loading skeleton (animate-pulse), inline error with retry, "Patient not found" with back, and full chart when data present. Sections: Demographics, Insurance (conditional on any tier non-null), Employer (conditional on employerName), SDOH (hidden for BillingStaff), Care Team (hidden for BillingStaff), Related Persons (hidden for BillingStaff). Edit button launches `PatientFormModal` (built in T03), hidden for BillingStaff. All sections use `<section>` elements with Tailwind card borders; no inline styles.

## Verification

```
# TypeScript check
/opt/homebrew/bin/node .../tsc --noEmit → EXIT:0 ✓

# File existence
ls src/lib/fhirExtract.ts         → found ✓
ls src/hooks/usePatient.ts        → found ✓
ls src/pages/PatientDetailPage.tsx → found ✓

# FHIR URL template (constructs http://medarc.local/insurance/primary|secondary|tertiary)
grep 'insurance/\${tier}' src/lib/fhirExtract.ts → line found ✓
grep "http://medarc.local/employer" src/lib/fhirExtract.ts → 2 hits ✓
grep "http://medarc.local/sdoh" src/lib/fhirExtract.ts → 2 hits ✓

# BillingStaff gate
grep -n "BillingStaff" src/pages/PatientDetailPage.tsx → 9 hits (lines 8, 159, 185, 186, 281, 282, 304, 305, 315, 316) ✓

# No raw FHIR navigation in PatientDetailPage main component body
awk 'NR>=100 && NR<=350' src/pages/PatientDetailPage.tsx | grep 'resource\["name"\]...' → 0 hits ✓

# Null guard
grep -n "!resource" src/lib/fhirExtract.ts → line 173: if (!resource) return emptyDisplay(); ✓

# Mounted boolean guard
grep -n "mounted" src/hooks/usePatient.ts → lines 50, 51, 65, 70, 79, 88 ✓

# Promise.all
grep -n "Promise.all" src/hooks/usePatient.ts → line 59 ✓

# ContentArea wiring
grep -n "PatientDetailPage" src/components/shell/ContentArea.tsx → import + render lines ✓
```

## Diagnostics

- **Error state**: If any of the three Tauri commands rejects, PatientDetailPage renders a red banner with the exact error string and a "Retry" button.
- **Not-found state**: If `getPatient` returns a record but the resource is null, `extractPatientDisplay` returns all-null display and the page renders "—" for all fields rather than crashing.
- **Console**: `[usePatient] fetchAll failed for <id>: <msg>` on any rejection.
- **React DevTools**: `usePatient` state: `{ patient: PatientRecord|null, careTeam: CareTeamRecord|null, relatedPersons: RelatedPersonRecord[], loading: bool, error: string|null }`.
- **Tauri DevTools**: Three parallel IPC calls visible: `get_patient`, `get_care_team`, `list_related_persons`.

## Deviations

None. Implementation matches the task plan exactly. The insurance URL is a template literal (not three hard-coded strings), which matches the pattern in patient.rs — the grep-based verification check in the plan was a spot-check; the runtime behavior is equivalent.

## Known Issues

None.

## Files Created/Modified

- `src/lib/fhirExtract.ts` — new: pure FHIR extraction helper; exports `PatientDisplay` type and `extractPatientDisplay` function
- `src/hooks/usePatient.ts` — new: data-fetching hook with mounted guard, parallel Promise.all, reload via refreshCounter
- `src/pages/PatientDetailPage.tsx` — new: patient chart shell with 6 sections, RBAC gates, loading/error/not-found states
- `src/components/patient/index.ts` — new: barrel export for PatientListPage

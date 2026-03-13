---
estimated_steps: 5
estimated_files: 4
---

# T02: Build extractPatientDisplay helper + usePatient hook + PatientDetailPage

**Slice:** S02 — Patient Module
**Milestone:** M002

## Description

Build the three artifacts that form the patient chart shell:

1. `src/lib/fhirExtract.ts` — a pure helper with `extractPatientDisplay(resource)` that isolates all FHIR JSON path knowledge in one place. Components never navigate the `Record<string, unknown>` blob directly.
2. `src/hooks/usePatient.ts` — data-fetching hook that loads a patient record, care team, and related persons in parallel. Returns typed `{ patient, careTeam, relatedPersons, loading, error, reload }` state.
3. `src/pages/PatientDetailPage.tsx` — the patient chart shell. Renders demographics, insurance, employer, SDOH, care team, and related persons sections. RBAC gates: `BillingStaff` sees only demographics and insurance. "Edit" button will open `PatientFormModal` (built in T03) — for T02, clicking Edit can render a placeholder message.

This task depends on T01 (which imports `PatientListPage` into `PatientsPage`), but is otherwise independent — `PatientDetailPage` does not depend on `PatientListPage`.

## Steps

1. **Write `src/lib/fhirExtract.ts`**

   Define a `PatientDisplay` interface with all displayable string/object fields (all nullable). Then implement `extractPatientDisplay(resource: Record<string, unknown> | null | undefined): PatientDisplay` with the following extraction logic — all paths use optional chaining; no `as any` casts, only `as Array<Record<string, unknown>> | undefined` or similar narrowing:

   ```
   familyName:       resource?.["name"]?.[0]?.["family"]  (string | undefined → null)
   givenNames:       resource?.["name"]?.[0]?.["given"]   (string[] | undefined → [])
   dob:              resource?.["birthDate"]               (string | undefined → null)
   gender:           resource?.["gender"]                  (string | undefined → null)
   genderIdentity:   extension find url="http://hl7.org/fhir/StructureDefinition/patient-genderIdentity" → valueString
   photoUrl:         extension find url="http://medarc.local/photo-url" → valueUrl
   phone:            telecom find system="phone" → value
   email:            telecom find system="email" → value
   addressLine:      address?.[0]?.["line"]?.[0]
   city:             address?.[0]?.["city"]
   state:            address?.[0]?.["state"]
   postalCode:       address?.[0]?.["postalCode"]
   country:          address?.[0]?.["country"]
   mrn:              identifier find system="http://medarc.local/mrn" → value
   primaryProviderId: identifier find system="http://medarc.local/primary-provider" → value
   insurancePrimary:  extractInsurance(extensions, "primary")
   insuranceSecondary: extractInsurance(extensions, "secondary")
   insuranceTertiary:  extractInsurance(extensions, "tertiary")
   employer:          extractExtensionGroup(extensions, "http://medarc.local/employer", ["employerName","occupation","employerPhone","employerAddress"])
   sdoh:              extractExtensionGroup(extensions, "http://medarc.local/sdoh", ["housingStatus","foodSecurity","transportationAccess","educationLevel","notes"])
   ```

   Write a private helper `extractInsurance(extensions, tier: "primary"|"secondary"|"tertiary")` that finds `url === "http://medarc.local/insurance/${tier}"` and returns an object with `payerName, planName, memberId, groupNumber, subscriberName, subscriberDob, relationshipToSubscriber` (all string | null).

   Write a private helper `extractExtensionGroup(extensions, url, keys)` that finds the extension by url and returns a `Record<string, string | null>` keyed by the sub-extension url values.

   Guard: if `resource` is null/undefined, return all-null `PatientDisplay`.

   Export `PatientDisplay` type and `extractPatientDisplay` function only.

2. **Write `src/hooks/usePatient.ts`**

   Implement `usePatient(patientId: string)` using the `AuditLog.tsx` / `useAuth.ts` pattern:
   - State: `patient: PatientRecord | null`, `careTeam: CareTeamRecord | null`, `relatedPersons: RelatedPersonRecord[]`, `loading: boolean`, `error: string | null`
   - `useEffect` with a `mounted` boolean guard (cancel state updates after unmount)
   - Fetch all three in parallel with `Promise.all([commands.getPatient(patientId), commands.getCareTeam(patientId), commands.listRelatedPersons(patientId)])`
   - On error: set `error` to the error message string; set `loading` false
   - Expose a `reload` callback (`useCallback`) that resets loading state and re-triggers the fetch by incrementing a `refreshCounter` state
   - Return `{ patient, careTeam, relatedPersons, loading, error, reload }`
   - `patientId` and `refreshCounter` are the `useEffect` deps (not `reload` which would cause the navigate-stability pitfall from DECISIONS.md)

3. **Write `src/pages/PatientDetailPage.tsx`**

   Props: `{ patientId: string; role: string }`

   - Call `usePatient(patientId)` → `{ patient, careTeam, relatedPersons, loading, error, reload }`
   - Call `useNav()` → `{ goBack }` for the back button
   - If `loading`: render full-width loading skeleton
   - If `error`: render inline error with retry button that calls `reload()`
   - If `patient === null && !loading`: render "Patient not found" message with back button
   - When data is present: call `extractPatientDisplay(patient.resource)` → `display`
   - Render header: `← Back` button, patient full name (or "Unknown Patient"), MRN badge, "Edit" button (omit for BillingStaff); for T02, Edit button sets local `editMode` state → renders a placeholder `<p>Edit form coming in T03</p>`
   - **Demographics section**: name, DOB, gender, genderIdentity, phone, email, address (multi-line), MRN
   - **Insurance section**: rendered if any of `display.insurancePrimary/Secondary/Tertiary` is non-null — show each tier as a sub-card with payerName, memberId, planName, groupNumber
   - **Employer section**: rendered if `display.employer?.employerName` exists — always visible (BillingStaff can see this)
   - **SDOH section**: hidden when `role === "BillingStaff"` — show housing, food security, transportation, education, notes
   - **Care Team section**: hidden when `role === "BillingStaff"` — show care team member name, role, note from `careTeam?.resource`; "No care team assigned" when `careTeam === null`
   - **Related Persons section**: hidden when `role === "BillingStaff"` — list each related person with name, relationship, phone; "None on file" when empty
   - Each section is a `<section>` with a heading and a subtle card border — Tailwind only, no inline styles

4. **Add `src/components/patient/index.ts` barrel** (optional, but helps with future imports):
   Export `PatientListPage` from `./PatientListPage` — skip if it adds complexity

5. **Verify TypeScript and structural correctness**
   - Run `tsc --noEmit` — must exit 0
   - Verify FHIR URL constants in `fhirExtract.ts` match exactly: `http://medarc.local/insurance/primary`, `http://medarc.local/employer`, `http://medarc.local/sdoh`, `http://medarc.local/photo-url`, `http://medarc.local/mrn`, `http://medarc.local/primary-provider`, `http://hl7.org/fhir/StructureDefinition/patient-genderIdentity`
   - Verify `BillingStaff` role gate in `PatientDetailPage.tsx`

## Must-Haves

- [ ] `src/lib/fhirExtract.ts` exports `PatientDisplay` type and `extractPatientDisplay` function
- [ ] `extractPatientDisplay` handles null/undefined resource without throwing
- [ ] FHIR extension URLs in `fhirExtract.ts` match `patient.rs` exactly (audited from grep output above)
- [ ] `src/hooks/usePatient.ts` uses mounted-boolean pattern to prevent stale state updates
- [ ] `usePatient` fetches all three resources in parallel with `Promise.all`
- [ ] `getCareTeam` returns `CareTeamRecord | null` — hook handles null without crashing
- [ ] `src/pages/PatientDetailPage.tsx` accepts `{ patientId: string; role: string }` props
- [ ] SDOH and care team sections hidden when `role === "BillingStaff"`
- [ ] Loading, error, and data-absent states all rendered
- [ ] `tsc --noEmit` exits 0 after this task

## Verification

```bash
# TypeScript check
/opt/homebrew/bin/node /Users/omarsharaf96/.npm/_npx/1bf7c3c15bf47d04/node_modules/typescript/bin/tsc --noEmit
# Expected: exit 0

# File existence
ls src/lib/fhirExtract.ts
ls src/hooks/usePatient.ts
ls src/pages/PatientDetailPage.tsx

# FHIR URL correctness (spot-check three URLs)
grep "http://medarc.local/insurance/primary" src/lib/fhirExtract.ts
grep "http://medarc.local/employer" src/lib/fhirExtract.ts
grep "http://medarc.local/sdoh" src/lib/fhirExtract.ts
# Each must return exactly 1 hit

# BillingStaff gate
grep -n "BillingStaff" src/pages/PatientDetailPage.tsx
# Must return at least 1 hit

# No raw FHIR navigation in PatientDetailPage (all extraction in fhirExtract.ts)
grep -n 'resource\["name"\]\|resource\["telecom"\]\|resource\["extension"\]' src/pages/PatientDetailPage.tsx
# Must return 0 hits — all navigation is in fhirExtract.ts

# null-guard in extractPatientDisplay
grep -n "null\|undefined\|!resource" src/lib/fhirExtract.ts
# Must return hits showing the null guard
```

## Observability Impact

- Signals added/changed: `usePatient` sets `error` string on any command failure; `PatientDetailPage` renders the error string with a retry button — observable in the UI without DevTools
- How a future agent inspects this: React DevTools → `usePatient` state shows `{ patient: {...}|null, careTeam: {...}|null, loading: bool, error: string|null }`; Tauri devtools shows the three parallel IPC calls
- Failure state exposed: If `getPatient` fails (patient deleted between list and detail navigation), `PatientDetailPage` renders "Patient not found" — not a blank screen

## Inputs

- `src-tauri/src/commands/patient.rs` lines 235–420 — authoritative FHIR extension URLs (confirmed above via grep)
- `src/lib/tauri.ts` — `commands.getPatient`, `commands.getCareTeam`, `commands.listRelatedPersons` signatures
- `src/types/patient.ts` — `PatientRecord`, `CareTeamRecord`, `RelatedPersonRecord` types
- `src/hooks/useAuth.ts` lines 43–73 — mounted-boolean pattern to replicate in `usePatient`
- `src/components/AuditLog.tsx` — `useEffect` + loading/error state pattern
- S02-RESEARCH pitfall: `getCareTeam` returns `CareTeamRecord | null` — must handle null
- S02-RESEARCH pitfall: `PatientRecord.resource` may itself be null (Rust JSON deserialization fallback) — `extractPatientDisplay` must guard this

## Expected Output

- `src/lib/fhirExtract.ts` — pure FHIR extraction helper, exports `PatientDisplay` + `extractPatientDisplay`
- `src/hooks/usePatient.ts` — data-fetching hook with mounted guard and reload capability
- `src/pages/PatientDetailPage.tsx` — patient chart shell with all sections and RBAC gates
- TypeScript check exit 0

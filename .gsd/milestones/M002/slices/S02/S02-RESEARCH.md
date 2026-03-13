# S02: Patient Module — Research

**Date:** 2026-03-12

## Summary

S02 builds the Patient Module on top of the S01 navigation shell: a searchable patient roster, a patient detail page with full demographics/insurance/care-team display, and a create/edit form modal. All data flows through real Tauri commands already wired in `src/lib/tauri.ts`. No new Rust commands are needed. No new dependencies are needed. The work is entirely React/TypeScript/Tailwind.

S01 left the `PatientsPage` as a two-line placeholder and the `patient-detail` route as a stub that renders `PatientsPage`. S02 replaces both. The `ContentArea` switch already has `case "patient-detail"` waiting for `PatientDetailPage`. The route type `{ page: "patient-detail"; patientId: string }` already exists in `RouterContext.tsx`. The `commands` object already has all nine patient wrappers (`createPatient`, `getPatient`, `updatePatient`, `searchPatients`, `deletePatient`, `upsertCareTeam`, `getCareTeam`, `addRelatedPerson`, `listRelatedPersons`). TypeScript types for all inputs/outputs are fully defined in `src/types/patient.ts`.

The implementation risk is medium. The complexity is in (1) extracting displayable fields from the opaque `PatientRecord.resource: Record<string, unknown>` FHIR blob rather than from flat fields — the backend returns raw FHIR JSON, and the UI must navigate its structure safely; (2) handling the full `PatientInput` form which has 20+ fields including nested insurance tiers, employer, SDOH, and care team; and (3) maintaining strict TypeScript compliance (`noUnusedLocals`, `noUnusedParameters`, no `any`).

The recommended approach is three focused tasks: (T01) `PatientListPage` + search; (T02) `PatientDetailPage` with FHIR field extraction helpers; (T03) `PatientFormModal` create/edit + wire `ContentArea` to render `PatientDetailPage` for `patient-detail` route. A `usePatient(id)` hook lives in T02.

## Recommendation

Build all patient UI in three tasks (no more), keeping components small and single-purpose:

1. **T01 — PatientListPage**: Replace the `PatientsPage` placeholder with a real searchable roster. Calls `commands.searchPatients` with debounced input (name, MRN). Each row shows MRN, name, DOB, gender, phone. Clicking a row calls `navigate({ page: "patient-detail", patientId: row.id })`. "New Patient" button opens the `PatientFormModal`. RBAC gate: show "New Patient" button only when role has Create permission (`Provider`, `NurseMa`, `FrontDesk`, `SystemAdmin`).

2. **T02 — PatientDetailPage + usePatient hook**: Renders the patient chart shell that S03/S04 will extend. Calls `commands.getPatient(patientId)` on mount via a `usePatient(id)` hook. Renders demographics, insurance, employer, SDOH sections by extracting from `record.resource` (FHIR JSON). Calls `commands.getCareTeam` and `commands.listRelatedPersons` for the care team panel. An "Edit" button opens `PatientFormModal` pre-populated with current values. Role gate: `BillingStaff` sees only demographics (name, MRN, DOB, insurance) — no care team, no SDOH.

3. **T03 — PatientFormModal**: Controlled form component for create and edit flows. Two-panel layout: Basic Info (name, DOB, gender, contact, photo) on one tab; Insurance/Employer/SDOH on another. The care team is managed separately via a care team section (calls `upsertCareTeam`). On submit, calls `commands.createPatient` or `commands.updatePatient` based on whether `patientId` is provided. Wires `ContentArea` to render `PatientDetailPage` for the `patient-detail` route (replacing the current PatientsPage stub). Also fixes the T02 `usePatient` import in ContentArea.

## Don't Hand-Roll

| Problem | Existing Solution | Why Use It |
|---------|------------------|------------|
| Debounced search input | Plain `useEffect` + `setTimeout` / `clearTimeout` — no library needed | One-use debounce; adding `use-debounce` for a single case adds install overhead with no benefit |
| Form state | React `useState` with controlled inputs — existing codebase pattern (`LoginForm`, `RegisterForm`) | `react-hook-form` would be an undeclared dependency; the existing auth forms use plain state and that's the established pattern |
| Modal overlay | `position: fixed inset-0 z-50 bg-black/40` — same approach as `LockScreen.tsx` | No `react-modal`, no Radix; LockScreen already proves this works in WKWebView |
| Data fetching | `useEffect` + `useState<T | null>` — same pattern as `AuditLog.tsx` | No TanStack Query; all existing hooks (useAuth, AuditLog, AppShell.getSessionTimeout) use raw `useEffect` |
| FHIR field extraction | Pure helper functions `extractPatientFields(resource)` — decode `Record<string, unknown>` safely | No `fhir-model` or FHIR library; all existing backend functions return raw `serde_json::Value` as `Record<string, unknown>`; lightweight field path helpers with null-safe optional chaining are sufficient |

## Existing Code and Patterns

- `src/contexts/RouterContext.tsx` — `Route` union already has `{ page: "patient-detail"; patientId: string }`. `useNav()` exposes `navigate()` and `goBack()`. S02 calls `navigate({ page: "patient-detail", patientId })` from the patient list and `goBack()` from the detail page header.
- `src/components/shell/ContentArea.tsx` — `case "patient-detail"` already exists but renders `PatientsPage` as a stub. T03 changes this one line to import and render `PatientDetailPage` instead, passing `currentRoute.patientId` as a prop.
- `src/lib/tauri.ts` — `commands.searchPatients(query)`, `commands.createPatient(input)`, `commands.getPatient(patientId)`, `commands.updatePatient(patientId, input)`, `commands.upsertCareTeam(input)`, `commands.getCareTeam(patientId)`, `commands.addRelatedPerson(input)`, `commands.listRelatedPersons(patientId)` are all wired and type-safe. `commands.deletePatient(patientId)` exists but is SystemAdmin-only — add a delete button only when role === "SystemAdmin".
- `src/types/patient.ts` — All input/record types already defined. `PatientInput` has 20+ fields. `PatientRecord.resource` is `Record<string, unknown>` (FHIR Patient JSON). `PatientSummary` has flat fields (`id`, `mrn`, `familyName`, `givenNames[]`, `birthDate`, `gender`, `phone`).
- `src/components/auth/LoginForm.tsx` / `RegisterForm.tsx` — Establish the Tailwind form input pattern: `rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500`. Every form field in S02 must follow this exactly.
- `src/components/AuditLog.tsx` — Establishes the data-fetching pattern: `useEffect(() => { setLoading(true); commands.xxx().then(setData).catch(setError).finally(() => setLoading(false)); }, [deps])`. Use the same pattern in `usePatient`.
- `src/components/auth/LockScreen.tsx` — Establishes the modal overlay pattern: `position: fixed inset-0 z-50` with a backdrop. `PatientFormModal` reuses this pattern.
- `src/components/shell/Sidebar.tsx` — RBAC check pattern: `NAV_ITEMS_BY_ROLE[role] ?? []`. In S02, apply the same pattern for role-conditional UI elements (e.g., "New Patient" button only shown if the role is in `["Provider", "NurseMa", "FrontDesk", "SystemAdmin"]`).
- `src/hooks/useAuth.ts` — Pattern for `T | null` state with loading/error: `const [data, setData] = useState<PatientRecord | null>(null); const [loading, setLoading] = useState(true); const [error, setError] = useState<string | null>(null)`. Use exactly this pattern in `usePatient`.

## Constraints

- **Strict TypeScript** — `tsconfig.json` has `strict: true`, `noUnusedLocals: true`, `noUnusedParameters: true`. Every prop, every state variable, every function parameter must be used. No `any`. `PatientRecord.resource` is `Record<string, unknown>` — use optional chaining and type narrowing when extracting FHIR fields.
- **`PatientRecord.resource` is FHIR JSON, not flat** — `getPatient` returns the raw FHIR Patient resource blob. To display name, you must navigate `resource.name[0].family` and `resource.name[0].given[]`. Phone is at `resource.telecom[].{system:"phone"}.value`. Address is at `resource.address[0]`. Insurance is at `resource.extension[].{url:"http://medarc.local/insurance/primary"}`. Write a typed helper `extractPatientDisplay(resource: Record<string, unknown>)` returning a plain object with all displayable strings — this is the single place where FHIR structure knowledge lives in the UI.
- **`PatientSummary` is from `searchPatients` — flat, safe to use directly** — The summary struct (`id`, `mrn`, `familyName`, `givenNames[]`, `birthDate`, `gender`, `phone`) is already extracted by the Rust search handler. No FHIR navigation needed for the list page.
- **`upsertCareTeam` replaces the entire care team** — There is no `addCareTeamMember` variant. The `CareTeamMemberInput` includes `patientId`, `memberId`, `memberName`, `role`, `note`. In the MVP, only one care team member (primary provider) is practically configured via the form. The care team widget should be a simple "Primary Provider" field rather than a full multi-member list (multi-member editing can be S07).
- **No new Rust commands** — M002 constraint: purely frontend. If a display requirement cannot be satisfied by the 9 existing patient commands, it must be derived client-side from existing data.
- **Tailwind only** — No CSS modules, no styled-components, no inline `style={}` other than for dynamically computed values that Tailwind cannot express.
- **`tsc --noEmit` must exit 0** — Use the cached binary workaround: `/opt/homebrew/bin/node /Users/omarsharaf96/.npm/_npx/1bf7c3c15bf47d04/node_modules/typescript/bin/tsc` — the project-level `npx tsc` hangs in this environment (known S01 issue).
- **`cargo test --lib` must remain 265 tests passing** — No Rust changes in S02; this is just a sanity regression check, not something S02 needs to actively worry about.
- **`ContentArea` switch needs an import change for `patient-detail`** — The current stub renders `PatientsPage`. When T03 adds `PatientDetailPage`, `ContentArea.tsx` must import it and pass `patientId: (currentRoute as { page: "patient-detail"; patientId: string }).patientId`. The `never` exhaustiveness guard must remain intact.
- **RBAC at the page level (second layer)** — Both `PatientListPage` and `PatientDetailPage` must independently verify the user has `Patients:Read` permission before rendering (backend already enforces this, but the UI should fail visibly for any unrecognized role, not silently show an empty page). `BillingStaff` can read but must not see care team or SDOH — hide those sections by role.

## Common Pitfalls

- **FHIR extraction with `as unknown as X` casts** — TypeScript strict mode won't allow direct `resource["name"][0].family` access on `Record<string, unknown>`. Use optional chaining with type narrowing: `const names = resource["name"] as Array<Record<string, unknown>> | undefined;`. Write `extractPatientDisplay()` once and test it; do not scatter FHIR path navigation across multiple components.
- **`noUnusedParameters` with destructured props** — If a component prop or hook parameter is declared but never read in the body (e.g., `{ role }` passed to a component that doesn't yet use it), TypeScript will reject it. Prefix with `_` or remove the parameter. This tripped several S01 components during initial drafts.
- **`upsertCareTeam` requires all four non-nullable fields** — `patientId`, `memberId`, `memberName`, and `role` are all required (`string`, not `string | null`). The form must not submit until all four are present. Validate before calling the command, not in a try/catch.
- **`searchPatients` parameter is `PatientSearchQuery` object** — Not individual args. Pass `{ name: query || null, mrn: null, birthDate: null, limit: null }` when doing a free-text name search. All fields have `| null` type, so pass `null` not `undefined` for omitted ones.
- **`PatientInput.givenNames` is `string[]`** — The UI form likely shows a single "First Name" text input. Convert to `[firstName]` array on submit. On edit pre-population, join `givenNames.join(" ")` into the display field — but split back to `[joined]` on submit to avoid creating `["First", "Middle"]` vs `["First Middle"]` inconsistency (the Rust search handler already splits on whitespace for indexing).
- **`getCareTeam` returns `CareTeamRecord | null`** — `null` when no care team exists yet. The `usePatient` hook or the detail page must handle null without crashing. Show an "Add Primary Provider" affordance rather than an error.
- **React `useEffect` with async** — Same pitfall as `AuditLog.tsx` avoids: do not make the `useEffect` callback itself `async`. Use an inner `async function load()` called immediately inside the callback, with a `mounted` boolean to cancel state updates after unmount. This pattern is already used in `useAuth.ts` line 43–73.
- **`navigate()` dependency in `useCallback`** — `RouterContext.tsx` line 59 shows `navigate` depends on `currentRoute` via `useCallback`. Do not include `navigate` in a `useEffect` dependency array expecting it to be stable — it will re-fire whenever the route changes. Assign it to a `ref` if it needs to be captured.

## Open Risks

- **FHIR extension URL constants** — The insurance extension URLs (`http://medarc.local/insurance/primary`, etc.) and SDOH URLs (`http://medarc.local/sdoh`) are hardcoded in `build_patient_fhir()` in patient.rs. The UI must use identical strings or the extraction helper will silently return null. Audit the URLs from `patient.rs` lines 235–420 before writing the extraction helper; do not guess.
- **`PatientRecord.resource` may be `null` (JSON deserialization fallback)** — Rust line 635: `serde_json::from_str(&resource_str).unwrap_or(serde_json::Value::Null)`. If the resource string is malformed, the frontend receives `null` as the resource blob. `extractPatientDisplay` must guard `resource == null` before any field access.
- **`givenNames` display on the list page** — `PatientSummary.givenNames` is a `string[]`. The Rust handler joins the stored `given_name` column by splitting on whitespace and re-capitalizing (lines 883–898). The display in the list should be `givenNames.join(" ")` but the form edit flow must round-trip correctly through the array structure.
- **`searchPatients` result limit** — The backend caps at 500 results. With no search term, it returns up to 50 (default). A clinic with 200+ patients will get a truncated list if the user doesn't search. Show a "Showing first N results — refine your search" hint when results equal the limit.
- **Care team widget scope** — `upsertCareTeam` replaces the whole team record. In S02, keep it simple (one primary provider field). If the user edits the care team and the current CareTeamRecord already has other members from a future slice, the upsert would overwrite them. For S02 scope, this is acceptable — document as a known limitation.
- **`deletePatient` in the UI** — SystemAdmin-only. Adding a delete button adds complexity (confirmation dialog, cascade implications). For S02, consider omitting the delete button entirely (it can be S07 cleanup). If included, add an inline confirmation (`window.confirm` or a simple in-component confirmation state) to prevent accidental deletion.
- **`tsc --noEmit` hang** — Verified from S01-T01-SUMMARY: `npx tsc` hangs. Use the workaround: `/opt/homebrew/bin/node /Users/omarsharaf96/.npm/_npx/1bf7c3c15bf47d04/node_modules/typescript/bin/tsc --noEmit`. Failure to use this will cause a verification step to block indefinitely.

## FHIR Extraction Reference

The `PatientRecord.resource` blob has this structure (from `build_patient_fhir()` in `patient.rs`):

```json
{
  "resourceType": "Patient",
  "id": "<uuid>",
  "identifier": [
    { "use": "official", "system": "http://medarc.local/mrn", "value": "MRN-XXXXXXXX" },
    { "system": "http://medarc.local/primary-provider", "value": "<providerId>" }  // optional
  ],
  "name": [{ "use": "official", "family": "Smith", "given": ["John", "Edward"] }],
  "birthDate": "1985-06-15",  // optional
  "gender": "male",           // optional
  "telecom": [
    { "system": "phone", "value": "555-1234", "use": "home" },
    { "system": "email", "value": "j.smith@example.com" }
  ],
  "address": [{ "line": ["123 Main St"], "city": "Boston", "state": "MA", "postalCode": "02101", "country": "US" }],
  "extension": [
    { "url": "http://hl7.org/fhir/StructureDefinition/patient-genderIdentity", "valueString": "..." },
    { "url": "http://medarc.local/photo-url", "valueUrl": "..." },
    { "url": "http://medarc.local/insurance/primary", "extension": [
      { "url": "payerName", "valueString": "BCBS" },
      { "url": "memberId", "valueString": "ABC123" },
      ...
    ]},
    { "url": "http://medarc.local/insurance/secondary", "extension": [...] },
    { "url": "http://medarc.local/insurance/tertiary", "extension": [...] },
    { "url": "http://medarc.local/employer", "extension": [
      { "url": "employerName", "valueString": "Acme Corp" },
      ...
    ]},
    { "url": "http://medarc.local/sdoh", "extension": [
      { "url": "housingStatus", "valueString": "stable" },
      ...
    ]}
  ]
}
```

**Key extraction paths (all optional-chained):**
- Family name: `resource.name?.[0]?.family`
- Given names: `resource.name?.[0]?.given as string[] | undefined`
- DOB: `resource.birthDate`
- Gender: `resource.gender`
- Phone: `resource.telecom?.find(t => t.system === "phone")?.value`
- Email: `resource.telecom?.find(t => t.system === "email")?.value`
- Address: `resource.address?.[0]` → `{line, city, state, postalCode, country}`
- Primary provider ID: `resource.identifier?.find(i => i.system === "http://medarc.local/primary-provider")?.value`
- Insurance (primary): `resource.extension?.find(e => e.url === "http://medarc.local/insurance/primary")?.extension`
- Employer: `resource.extension?.find(e => e.url === "http://medarc.local/employer")?.extension`
- SDOH: `resource.extension?.find(e => e.url === "http://medarc.local/sdoh")?.extension`

Write a single `extractPatientDisplay(resource: Record<string, unknown>)` helper returning a typed plain object. This isolates FHIR path knowledge to one file.

## Skills Discovered

| Technology | Skill | Status |
|------------|-------|--------|
| React 18 + Tailwind CSS | `frontend-design` | Installed (available in `<available_skills>`) |
| React (general) | None from `npx skills find "react tailwind"` specific to this use case | No match for Tauri-specific React patterns |

The `frontend-design` skill is available and relevant — it guides polished Tailwind UI. However, S02 must follow the **existing codebase aesthetic** (clean minimal clinical UI in blue/gray Tailwind palette) rather than applying the skill's "bold aesthetic direction" guidance. Use the skill for structural/quality guidance but anchor visual style to `LoginForm.tsx` and `AuditLog.tsx` precedents.

## Sources

- `src-tauri/src/commands/patient.rs` — Authoritative Rust types, FHIR structure, search behavior, RBAC permissions, MRN format
- `src-tauri/src/rbac/roles.rs` — RBAC matrix: FrontDesk CRU (no Delete) on Patients; BillingStaff Read only; Provider/NurseMa/SystemAdmin full
- `src/lib/tauri.ts` — All 9 patient command wrappers already present and type-safe
- `src/types/patient.ts` — All 19 TypeScript interfaces ready for use
- `src/contexts/RouterContext.tsx` — `patient-detail` route already defined; `useNav()` hook pattern
- `src/components/shell/ContentArea.tsx` — `case "patient-detail"` stub awaiting PatientDetailPage
- `src/components/AuditLog.tsx` — Data-fetching pattern to follow (`useEffect` + loading/error state)
- `src/components/auth/LoginForm.tsx` — Tailwind form input pattern to follow
- `src/components/auth/LockScreen.tsx` — Modal overlay pattern to follow
- `.gsd/milestones/M002/slices/S01/tasks/T01-SUMMARY.md` — `tsc` hang workaround: cached binary at `/Users/omarsharaf96/.npm/_npx/1bf7c3c15bf47d04/node_modules/typescript/bin/tsc`
- `.gsd/milestones/M002/slices/S01/tasks/T04-SUMMARY.md` — NAV_ITEMS_BY_ROLE pattern, ContentArea exhaustiveness guard pattern
- `.gsd/DECISIONS.md` — M002/S01 decisions: flat `commands` object, `T | null` not `T | undefined`, state-based router, RBAC nav matrix

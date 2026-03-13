# S01: Navigation Shell & Type System — Research

**Date:** 2026-03-11

## Summary

S01 replaces the monolithic `App.tsx` developer-scaffolding shell with a full navigation sidebar, state-based routing, and a complete TypeScript type + invoke-wrapper layer covering all 60+ M001 Tauri commands. This slice is the load-bearing foundation for every subsequent M002 slice — wrong decisions here force rewrites in S02–S07.

The key architectural question is **router choice**. MedArc is a Tauri desktop app with a single WebView and no browser URL bar. A simple state-based router (a `useNav()` hook managing a `{ page, params }` enum in React state) is the right fit: zero external dependencies, trivially type-safe, and no react-router adapter quirks on Tauri's WKWebView. This is confirmed by the existing `App.tsx` pattern which already uses conditional rendering driven by `auth` state — the same pattern extended one level.

The type work is straightforward but large: every Rust `#[serde(rename_all = "camelCase")]` struct becomes a TypeScript `interface` in `src/types/`. All 12 Rust command modules are registered in `lib.rs`; the complete invoke surface is 60 commands across 9 modules (health, fhir, auth, session, mfa, break-glass, audit, patient, clinical, scheduling, documentation, labs). Backup commands from DECISIONS.md S09 are **not wired** in `lib.rs` and the `backup.rs` command module does not exist — backup invoke wrappers are out of scope for S01 and should be created in S07 when the Settings panel is built.

RBAC-gated navigation is a thin layer: read `auth.user.role` from `useAuth()`, map it to visible nav items via a static lookup, and gate route access with an `<RbacRoute>` guard component. The RBAC role strings from the Rust `Role::as_str()` are lowercase snake_case (`system_admin`, `provider`, `nurse_ma`, `billing_staff`, `front_desk`) but `SessionInfo.role` in the current TypeScript is typed as `string | null` — the new types must narrow this to a union of the five canonical strings.

## Recommendation

**Use a custom state-based router** (no external library). Implement as:

```ts
type Page =
  | { name: "patients" }
  | { name: "patient-detail"; patientId: string }
  | { name: "schedule" }
  | { name: "settings" };

const NavContext = createContext<{ page: Page; navigate: (p: Page) => void }>(...);
```

This matches the existing conditional-render pattern in `App.tsx`, adds zero dependencies to `package.json`, remains trivially type-safe under strict mode, and is all that downstream slices S02–S07 need. React Router would add URL-bar/history concerns irrelevant to a Tauri desktop app and requires additional configuration to avoid `file://` URL handling bugs in WKWebView.

## Don't Hand-Roll

| Problem | Existing Solution | Why Use It |
|---------|------------------|------------|
| Auth lifecycle, session, MFA, lock/unlock | `useAuth()` hook + `commands` auth wrappers — already in `src/hooks/useAuth.ts` | Fully tested, don't duplicate |
| Idle lock timer | `useIdleTimer()` — `src/hooks/useIdleTimer.ts` | Already wired to `refreshSession` with 30s debounce |
| Tailwind layout utilities | Tailwind CSS 3.4 — already configured | No additional CSS dependencies allowed |
| Tauri IPC | `invoke()` from `@tauri-apps/api/core` — already in `src/lib/tauri.ts` | Use same pattern for all new wrappers |

## Existing Code and Patterns

- `src/App.tsx` — Current shell to **replace**. Contains authenticated guard, lock-screen overlay, idle timer wiring, and session-timeout fetch. The authenticated branch renders `DatabaseStatus`, `FhirExplorer`, `AuditLog` — these developer cards get removed; the `AppShell` (sidebar + content area) replaces the `div.min-h-screen.bg-gray-50.p-8` wrapper.
- `src/lib/tauri.ts` — Pattern to **extend**. Every new command wrapper follows the same shape: `commandName: (input: InputType) => invoke<OutputType>("command_name", { param_name: value })`. Parameter names must match Rust function parameter names exactly (camelCase from `#[serde(rename_all = "camelCase")]`).
- `src/hooks/useAuth.ts` — Provides `auth.user.role` (currently typed `string`). The role needs to be narrowed to `UserRole` union type. The `UseAuthReturn` interface extension must stay backward-compatible.
- `src/types/auth.ts` — `UserResponse.role` is `string`; should be narrowed to `UserRole = "SystemAdmin" | "Provider" | "NurseMa" | "BillingStaff" | "FrontDesk"`. Note: `SessionInfo.role` uses the camelCase variants (from `UserResponse`), not the snake_case `Role::as_str()` values. The Rust `register_user` returns `UserResponse` with the display name of the role — verify actual serialization from `auth.rs`.
- `src-tauri/src/rbac/roles.rs` — Five roles: `SystemAdmin`, `Provider`, `NurseMa`, `BillingStaff`, `FrontDesk`. The `Role::as_str()` returns snake_case (`system_admin`, etc.) but `UserResponse` is assembled in `commands/auth.rs` and may use the enum display variant. **Must verify** what string the backend actually serializes into `UserResponse.role` before writing the TypeScript union.
- `src-tauri/src/lib.rs` — The single source of truth for wired commands. Exactly 60 commands registered across: health (2), fhir (5), auth (5), session (5), mfa (7), break_glass (2), audit (2), patient (9), clinical (12), scheduling (13), documentation (16), labs (10). No `backup` command module is wired — backup wrappers are NOT part of S01.

## Complete Command Inventory (60 commands)

### Health (2)
- `check_db` → `DbStatus`
- `get_app_info` → `AppInfo`

### FHIR (5)
- `create_resource`, `get_resource`, `list_resources`, `update_resource`, `delete_resource`

### Auth (5)
- `register_user`, `login`, `logout`, `complete_login`, `check_first_run`

### Session (5)
- `lock_session`, `unlock_session`, `refresh_session`, `get_session_state`, `get_session_timeout`

### MFA (7)
- `setup_totp`, `verify_totp_setup`, `disable_totp`, `check_totp`, `check_biometric`, `enable_touch_id`, `disable_touch_id`

### Break-Glass (2)
- `activate_break_glass`, `deactivate_break_glass`

### Audit (2)
- `get_audit_log`, `verify_audit_chain_cmd`

### Patient (9)
- `create_patient` (`PatientInput`) → `PatientRecord`
- `get_patient` (id) → `PatientRecord`
- `update_patient` (id, input: `PatientInput`) → `PatientRecord`
- `search_patients` (`PatientSearchQuery`) → `PatientSummary[]`
- `delete_patient` (id) → void
- `upsert_care_team` (`CareTeamMemberInput[]`) → `CareTeamRecord`
- `get_care_team` (patientId) → `CareTeamRecord`
- `add_related_person` (`RelatedPersonInput`) → `RelatedPersonRecord`
- `list_related_persons` (patientId) → `RelatedPersonRecord[]`

### Clinical (12)
- `add_allergy` (`AllergyInput`) → `AllergyRecord`
- `list_allergies` (patientId, status?) → `AllergyRecord[]`
- `update_allergy` (id, `AllergyInput`) → `AllergyRecord`
- `delete_allergy` (id) → void
- `add_problem` (`ProblemInput`) → `ProblemRecord`
- `list_problems` (patientId, statusFilter?) → `ProblemRecord[]`
- `update_problem` (id, `ProblemInput`) → `ProblemRecord`
- `add_medication` (`MedicationInput`) → `MedicationRecord`
- `list_medications` (patientId, statusFilter?) → `MedicationRecord[]`
- `update_medication` (id, `MedicationInput`) → `MedicationRecord`
- `add_immunization` (`ImmunizationInput`) → `ImmunizationRecord`
- `list_immunizations` (patientId) → `ImmunizationRecord[]`

### Scheduling (13)
- `create_appointment` (`AppointmentInput`) → `AppointmentRecord`
- `list_appointments` (startDate, endDate, providerId?) → `AppointmentRecord[]`
- `update_appointment` (id, `UpdateAppointmentInput`) → `AppointmentRecord`
- `cancel_appointment` (id, reason?) → void
- `search_open_slots` (startDate, endDate, providerId, apptType?, durationMinutes?) → `AppointmentRecord[]`
- `update_flow_status` (`UpdateFlowStatusInput`) → void
- `get_flow_board` (date, providerId?) → `FlowBoardEntry[]`
- `add_to_waitlist` (`WaitlistInput`) → `WaitlistRecord`
- `list_waitlist` (providerId?, apptType?) → `WaitlistRecord[]`
- `discharge_waitlist` (id) → void
- `create_recall` (`RecallInput`) → `RecallRecord`
- `list_recalls` (patientId?, overdueOnly?) → `RecallRecord[]`
- `complete_recall` (id) → void

### Documentation (16)
- `create_encounter` (`EncounterInput`) → `EncounterRecord`
- `get_encounter` (id) → `EncounterRecord`
- `list_encounters` (patientId) → `EncounterRecord[]`
- `update_encounter` (id, `UpdateEncounterInput`) → `EncounterRecord`
- `record_vitals` (`VitalsInput`) → `VitalsRecord`
- `list_vitals` (patientId, encounterId?) → `VitalsRecord[]`
- `save_ros` (`ReviewOfSystemsInput`) → `RosRecord`
- `get_ros` (encounterId) → `RosRecord`
- `save_physical_exam` (`PhysicalExamInput`) → `PhysicalExamRecord`
- `get_physical_exam` (encounterId) → `PhysicalExamRecord`
- `list_templates` () → `TemplateRecord[]`
- `get_template` (id) → `TemplateRecord`
- `request_cosign` (`CosignRequestInput`) → `CosignRecord`
- `approve_cosign` (taskId) → `CosignRecord`
- `list_pending_cosigns` () → `CosignRecord[]`
- `check_drug_allergy_alerts` (patientId) → `DrugAllergyAlert[]`

### Labs (10)
- `add_lab_catalogue_entry` (`LabCatalogueInput`) → `LabCatalogueRecord`
- `list_lab_catalogue` (category?, search?) → `LabCatalogueRecord[]`
- `create_lab_order` (`LabOrderInput`) → `LabOrderRecord`
- `list_lab_orders` (patientId, status?) → `LabOrderRecord[]`
- `enter_lab_result` (`LabResultInput`) → `LabResultRecord`
- `list_lab_results` (patientId, statusFilter?, abnormalOnly?) → `LabResultRecord[]`
- `sign_lab_result` (`SignLabResultInput`) → `LabResultRecord`
- `upload_document` (`DocumentUploadInput`) → `DocumentRecord`
- `list_documents` (patientId, category?, search?) → `DocumentRecord[]`
- `verify_document_integrity` (documentId) → `IntegrityCheckResult`

## RBAC Navigation Matrix

| Role | Patients | Schedule | Labs | Settings | Audit |
|------|----------|----------|------|----------|-------|
| `SystemAdmin` | ✓ | ✓ | ✓ | ✓ | ✓ |
| `Provider` | ✓ | ✓ | ✓ | ✓ | — |
| `NurseMa` | ✓ | ✓ | ✓ | — | — |
| `BillingStaff` | read-only | read-only | read-only | — | — |
| `FrontDesk` | — | ✓ | — | — | — |

Note: Audit log is already accessible from within Settings (S07) for `SystemAdmin`. The nav item can be collapsed into Settings or shown as a top-level item — decide at implementation time, but route access must be role-gated.

## Type File Organization

```
src/types/
  auth.ts         ← existing; extend UserResponse.role to UserRole union
  fhir.ts         ← existing; keep as-is
  audit.ts        ← existing; keep as-is
  patient.ts      ← new: PatientInput, InsuranceInput, EmployerInput, SdohInput,
                         PatientSummary, PatientRecord, PatientSearchQuery,
                         CareTeamMemberInput, CareTeamRecord,
                         RelatedPersonInput, RelatedPersonRecord
  clinical.ts     ← new: AllergyInput, AllergyRecord, ProblemInput, ProblemRecord,
                         MedicationInput, MedicationRecord,
                         ImmunizationInput, ImmunizationRecord
  scheduling.ts   ← new: AppointmentInput, AppointmentRecord, UpdateAppointmentInput,
                         WaitlistInput, WaitlistRecord, RecallInput, RecallRecord,
                         UpdateFlowStatusInput, FlowBoardEntry
  documentation.ts← new: EncounterInput, SoapInput, EncounterRecord,
                         UpdateEncounterInput, VitalsInput, VitalsRecord,
                         ReviewOfSystemsInput, RosStatus, RosRecord,
                         PhysicalExamInput, PhysicalExamRecord, TemplateRecord,
                         CosignRequestInput, CosignRecord, DrugAllergyAlert
  labs.ts         ← new: LabCatalogueInput, LabCatalogueRecord,
                         LabOrderInput, LabOrderRecord, LabObservation,
                         LabResultInput, LabResultRecord, SignLabResultInput,
                         DocumentUploadInput, DocumentRecord, IntegrityCheckResult
```

## Constraints

- **TypeScript strict mode** — `strict: true`, `noUnusedLocals: true`, `noUnusedParameters: true` in `tsconfig.json`. Every type must be exact; no `any`. `tsc --noEmit` must exit 0.
- **No new npm dependencies** — no React Router, TanStack Router, or any routing library. The only new dependency allowed in M002 is `tauri-plugin-dialog` (for S05 document upload).
- **Tailwind only** — no CSS modules, no styled-components, no inline style objects.
- **Invoke parameter naming** — Tauri 2 deserializes invoke params by Rust parameter name. All Rust structs use `#[serde(rename_all = "camelCase")]`, so TypeScript callers must pass camelCase keys matching the Rust field names exactly. The existing `tauri.ts` wrappers demonstrate the correct pattern (e.g. `{ resourceType: resourceType ?? null }`).
- **`serde_json::Value` fields** — Rust records return a `resource: serde_json::Value` field. TypeScript must type this as `Record<string, unknown>` (not `any`).
- **Role string format** — `SessionInfo.role` returns the display-variant string from `UserResponse`, which in turn comes from the Rust `Role` enum serialized via serde. Need to verify in `auth.rs` whether the role is stored/returned as `"Provider"` (PascalCase enum variant) or `"provider"` (snake_case). Current `useAuth.ts` already checks `auth.user.role === "Provider"` and `"SystemAdmin"` — this pattern confirms PascalCase is the live format.
- **`RosStatus` enum** — Rust defines `snake_case` serialization for `RosStatus` variants: `"positive"`, `"negative"`, `"not_reviewed"`. TypeScript must match exactly.
- **No backup commands in S01** — `create_backup`, `restore_backup`, `list_backups` are documented in DECISIONS.md (S09) but the `backup.rs` module does not exist and no backup commands are wired in `lib.rs`. These belong to S07 (Settings panel).

## Common Pitfalls

- **`resource: serde_json::Value` typed as `any`** — This would compile but violate strict mode's spirit and create downstream type unsafety. Use `Record<string, unknown>` throughout. Downstream components that need specific FHIR fields should narrow with type guards, not cast.
- **Role string mismatch** — The Rust `Role::as_str()` returns snake_case (`system_admin`) but `UserResponse.role` in the auth response likely returns PascalCase (`SystemAdmin`) based on existing `useAuth.ts` checks. Using the wrong format in the RBAC nav guard will silently hide all nav items. Verify against `auth.rs` implementation before writing the union type.
- **Passing `undefined` vs `null` to Tauri** — Tauri 2 deserializes `null` to `None` in Rust; `undefined` (which JSON.stringify omits) can cause parameter mismatch errors. All optional parameters must pass `null` not `undefined`. Follow the existing pattern: `resourceType: resourceType ?? null`.
- **Async `invoke` in sync hooks** — Navigation state itself is synchronous (no await needed). Only the initial session check and data fetches are async. Don't wrap the router's `navigate()` in an async function.
- **LockScreen overlay pattern** — The current `App.tsx` renders `LockScreen` as an overlay on top of content to preserve React state. This pattern must be preserved in the new `AppShell`: the lock screen renders above the sidebar+content area, not instead of it.
- **`noUnusedLocals`/`noUnusedParameters`** — All exported types in `src/types/*.ts` that aren't yet consumed by a component will trigger `noUnusedLocals` errors in components, but since types are in separate files and the `tsconfig.json` `include` is only `["src"]`, unused _type_ exports are fine. However unused _value_ imports (like an unused `commands.someCommand`) will fail. Only add command wrappers that will be used — or accept the lint rule applies only to values, not type exports.

  → **Safe approach:** Export all types. Add all command wrappers to the `commands` object (they're values but won't be flagged as unused if they're exported properties of an object, not standalone const declarations).

- **State-based router back-navigation** — Without browser history, "back" must be modeled explicitly (a navigation stack or parent-child page relationship). Plan for this now: `AppointmentDetail` needs a "back to Schedule" affordance; `PatientDetail` needs "back to Patients". The simplest approach is a history stack: `const [history, setHistory] = useState<Page[]>([{ name: "patients" }])` with `navigate()` pushing and `goBack()` popping.

## Open Risks

- **Role string format** — Must be verified empirically before writing `UserRole` union type. If the existing code already works with `auth.user.role === "Provider"`, that's the live format, but a dedicated test or reading `auth.rs`'s `UserResponse` assembly code is required before asserting it in a type.
- **`tsc --noEmit` baseline** — Before adding any new types, run `tsc --noEmit` to confirm the current codebase already compiles clean. If there are pre-existing errors, document them as technical debt before S01 begins.
- **WKWebView CSS rendering** — Tauri 2 on macOS uses WKWebView. Tailwind utility classes work fine, but certain CSS features (e.g. `backdrop-blur` on older macOS, `:has()` selector) may not render. Stick to well-supported Tailwind utilities (flexbox, grid, spacing, color) for the navigation shell.
- **`tsconfig.json` `noUnusedLocals`** — Adding 60 command wrappers as named exports on the `commands` object is safe because object property exports are not flagged. But if any type file imports from another type file just for re-export, TypeScript may flag the import as unused. Structure type files to avoid re-export chains.
- **Backup commands gap** — DECISIONS.md describes S09 backup commands (`create_backup`, `restore_backup`, `list_backups`) but these are NOT in `lib.rs` or any `commands/*.rs` file. Either the S09 work was planned but not merged, or it lives in an uncommitted state. S01 should not add stub wrappers for these — S07 is where the Settings panel builds them and that slice can add both the Rust wiring and the TypeScript wrappers together.

## Skills Discovered

| Technology | Skill | Status |
|------------|-------|--------|
| React 18 + TypeScript | `frontend-design` (installed) | Installed — load for AppShell/Sidebar component design |
| Tauri 2 | No dedicated skill found | None found — use `get_library_docs` if needed |

## Sources

- Complete Rust command inventory enumerated from `src-tauri/src/lib.rs` (generate_handler macro, 60 commands)
- All Rust input/output struct shapes read from `src-tauri/src/commands/{patient,clinical,scheduling,documentation,labs}.rs`
- RBAC role strings verified from `src-tauri/src/rbac/roles.rs` (`Role::as_str()` + `Role::from_str()`)
- Router/dependency constraints from `package.json` (no router deps present), `tsconfig.json` (strict mode settings)
- Existing TypeScript patterns from `src/lib/tauri.ts`, `src/hooks/useAuth.ts`, `src/types/auth.ts`
- Role string in practice confirmed from `src/App.tsx` line: `auth.user.role === "Provider"` and `auth.user.role === "SystemAdmin"` — confirming PascalCase is the live serialization
- Backup command gap noted: `src-tauri/src/commands/mod.rs` does not contain a `backup` module; `lib.rs` invoke_handler has no backup commands

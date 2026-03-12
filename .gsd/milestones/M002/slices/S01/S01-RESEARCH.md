# S01: Navigation Shell & Type System — Research

**Date:** 2026-03-11
**Milestone:** M002 (MedArc Phase 2 Frontend)
**Risk:** High
**Dependencies:** None (first slice)

---

## Summary

S01 is the architectural foundation for all of M002. It has two independent work streams that must both land in a single slice: (1) a navigation shell that replaces the current monolithic `App.tsx` with a sidebar + state-based router, RBAC-gated nav items, and auth-guarded route rendering; and (2) a complete TypeScript type + invoke wrapper layer covering all 63+ M001 Rust commands currently registered in `lib.rs`.

The router decision is the highest-risk item. The codebase has **no URL bar** (Tauri desktop WebView), so React Router's `<BrowserRouter>` / `<HashRouter>` history model adds ceremony with no benefit. A lightweight state-based router — a `useNav()` hook wrapping a `useState<Route>` stored in a React context — is the right choice here. It is testable, adds zero dependencies, and integrates cleanly with the existing `useAuth` pattern.

The type layer is straightforward but large. Every Rust struct that crosses the Tauri IPC boundary has `#[serde(rename_all = "camelCase")]`, so TypeScript interfaces must match camelCase field names exactly. The existing `src/types/auth.ts`, `src/types/fhir.ts`, and `src/types/audit.ts` set the correct template to follow. The `commands` object in `src/lib/tauri.ts` must be extended with wrappers for all commands currently listed in `lib.rs`'s `invoke_handler!` block. TypeScript strict mode is enabled (`strict: true`, `noUnusedLocals`, `noUnusedParameters`), so every new type must be airtight.

---

## Recommendation

**Use a state-based router (no library).** A single `RouterContext` that holds the current `Route` union type and a `navigate(route: Route)` function is sufficient for the entire M002 surface area. Routes are objects not strings (e.g. `{ page: 'patient-detail', patientId: string }`), which gives full type safety without parsing. This pattern follows the same idiom as `useAuth` — a context hook wrapping local React state — so the codebase stays consistent.

Avoid React Router v6 or TanStack Router. Both are designed for URL-based navigation. Forcing them into a Tauri window without a real URL bar requires `createMemoryRouter` (React Router) or synthetic history hacks (TanStack), adds ~40 KB to the bundle, and creates a cognitive mismatch with the rest of the app.

**Dedicate separate files per domain** for the type layer: `src/types/patient.ts`, `src/types/scheduling.ts`, `src/types/documentation.ts`, `src/types/labs.ts`, `src/types/backup.ts`. The existing `src/types/auth.ts` / `src/types/fhir.ts` / `src/types/audit.ts` should stay untouched. Extend `src/lib/tauri.ts` with grouped sections per backend module, following the existing comment-header pattern (`// ─── Patient commands ───`).

---

## Don't Hand-Roll

| Problem | Existing Solution | Why Use It |
|---------|------------------|------------|
| Auth state & session lifecycle | `src/hooks/useAuth.ts` | Already handles login/logout/MFA/lock/unlock/firstRun — re-use as-is; AppShell just consumes it |
| Idle lock | `src/hooks/useIdleTimer.ts` | Already wired to `commands.lockSession()` — move the wiring call into AppShell, not duplicated in routes |
| Auth/session/audit invoke wrappers | `src/lib/tauri.ts` (existing) | 20+ wrappers already typed correctly; only append new sections, never replace existing |
| TypeScript strict mode config | `tsconfig.json` (existing) | `strict: true`, `noUnusedLocals`, `noUnusedParameters` already enabled — do not relax |
| Tailwind utility classes | Already configured | `tailwind.config.js` is live; no CSS modules or styled-components — all new UI is Tailwind only |

---

## Existing Code and Patterns

- **`src/App.tsx`** — Current authenticated shell. Replace its post-auth content (DatabaseStatus, FhirExplorer, AuditLog) with `<AppShell>`. The auth gate pattern (loading → mfaRequired → !isAuthenticated → main content) is correct and must be preserved verbatim — just replace what renders inside the authenticated branch.
- **`src/hooks/useAuth.ts`** — Exposes `{ user, session, isAuthenticated, isLocked, loading, error, mfaRequired, firstRun }` plus action callbacks. `user.role` is the RBAC signal for nav gating. Roles arrive as strings matching Rust's serialized values: `"Provider"`, `"FrontDesk"`, `"NurseMa"`, `"BillingStaff"`, `"SystemAdmin"`.
- **`src/lib/tauri.ts`** — The `commands` object is the single IPC surface. All new commands must be added here. Pattern: `commandName: (param: Type) => invoke<ReturnType>("rust_command_name", { snake_case_param: param })`. Rust parameter names are always `snake_case` — the invoke call must match exactly.
- **`src/types/auth.ts`** — Gold standard for type file structure: pure interfaces, `camelCase` field names matching Rust's `#[serde(rename_all = "camelCase")]`, no `any`, no `unknown`.
- **`src-tauri/src/commands/patient.rs`** — Defines: `PatientInput`, `PatientSummary`, `PatientRecord`, `PatientSearchQuery`, `CareTeamMemberInput`, `CareTeamRecord`, `RelatedPersonInput`, `RelatedPersonRecord`. Command names: `create_patient`, `get_patient`, `update_patient`, `search_patients`, `delete_patient`, `upsert_care_team`, `get_care_team`, `add_related_person`, `list_related_persons`.
- **`src-tauri/src/commands/clinical.rs`** — Defines: `AllergyInput`, `AllergyRecord`, `ProblemInput`, `ProblemRecord`, `MedicationInput`, `MedicationRecord`, `ImmunizationInput`, `ImmunizationRecord`. Command names: `add_allergy`, `list_allergies`, `update_allergy`, `delete_allergy`, `add_problem`, `list_problems`, `update_problem`, `add_medication`, `list_medications`, `update_medication`, `add_immunization`, `list_immunizations`.
- **`src-tauri/src/commands/scheduling.rs`** — Defines: `AppointmentInput`, `AppointmentRecord`, `UpdateAppointmentInput`, `WaitlistInput`, `WaitlistRecord`, `RecallInput`, `RecallRecord`, `UpdateFlowStatusInput`, `FlowBoardEntry`. Command names: `create_appointment`, `list_appointments`, `update_appointment`, `cancel_appointment`, `search_open_slots`, `update_flow_status`, `get_flow_board`, `add_to_waitlist`, `list_waitlist`, `discharge_waitlist`, `create_recall`, `list_recalls`, `complete_recall`.
- **`src-tauri/src/commands/documentation.rs`** — Defines: `EncounterInput`, `SoapInput`, `EncounterRecord`, `UpdateEncounterInput`, `VitalsInput`, `VitalsRecord`, `ReviewOfSystemsInput`, `RosStatus` (enum: `Positive | Negative | NotReviewed` serialized as `snake_case`), `PhysicalExamInput`, `PhysicalExamRecord`, `TemplateRecord`, `CosignInput`, `DrugAllergyAlert`. Command names: `create_encounter`, `get_encounter`, `list_encounters`, `update_encounter`, `record_vitals`, `list_vitals`, `save_ros`, `get_ros`, `save_physical_exam`, `get_physical_exam`, `list_templates`, `get_template`, `request_cosign`, `approve_cosign`, `list_pending_cosigns`, `check_drug_allergy_alerts`.
- **`src-tauri/src/commands/labs.rs`** — Defines: `LabCatalogueInput`, `LabCatalogueRecord`, `LabOrderInput`, `LabOrderRecord`, `LabObservation`, `LabResultInput`, `LabResultRecord`, `DocumentInput`, `DocumentRecord`. Command names: `add_lab_catalogue_entry`, `list_lab_catalogue`, `create_lab_order`, `list_lab_orders`, `enter_lab_result`, `list_lab_results`, `sign_lab_result`, `upload_document`, `list_documents`, `verify_document_integrity`.
- **`src-tauri/src/commands/backup.rs`** — Defines: `BackupRecord`. Command names: `create_backup`, `restore_backup`, `list_backups`.
- **`src-tauri/src/rbac/roles.rs`** — The five roles are `SystemAdmin`, `Provider`, `NurseMa`, `BillingStaff`, `FrontDesk`. These are serialized exactly as these camelCase strings by the Rust backend (no snake_case conversion for role values). RBAC nav matrix: FrontDesk → Schedule only; Provider → Patients + Schedule + Labs + Settings; NurseMa → Patients + Schedule + Labs; BillingStaff → read-only applicable views; SystemAdmin → all views including AuditLog.
- **No duplicate `* 2.rs` files exist** in `src-tauri/src/commands/` — confirmed clean. Similarly no `* 2.tsx` files visible in `src/`. No cleanup needed for S01.

---

## Complete Command Inventory (63 commands in invoke_handler)

This is the authoritative list derived from `src-tauri/src/lib.rs` to drive the type layer:

**Health (2):** `check_db`, `get_app_info`

**FHIR (5):** `create_resource`, `get_resource`, `list_resources`, `update_resource`, `delete_resource`

**Auth (5):** `register_user`, `login`, `logout`, `complete_login`, `check_first_run`

**Session (5):** `lock_session`, `unlock_session`, `refresh_session`, `get_session_state`, `get_session_timeout`

**Break Glass (2):** `activate_break_glass`, `deactivate_break_glass`

**MFA (7):** `setup_totp`, `verify_totp_setup`, `disable_totp`, `check_totp`, `check_biometric`, `enable_touch_id`, `disable_touch_id`

**Audit (2):** `get_audit_log`, `verify_audit_chain_cmd`

**Patient (9):** `create_patient`, `get_patient`, `update_patient`, `search_patients`, `delete_patient`, `upsert_care_team`, `get_care_team`, `add_related_person`, `list_related_persons`

**Clinical (12):** `add_allergy`, `list_allergies`, `update_allergy`, `delete_allergy`, `add_problem`, `list_problems`, `update_problem`, `add_medication`, `list_medications`, `update_medication`, `add_immunization`, `list_immunizations`

**Scheduling (13):** `create_appointment`, `list_appointments`, `update_appointment`, `cancel_appointment`, `search_open_slots`, `update_flow_status`, `get_flow_board`, `add_to_waitlist`, `list_waitlist`, `discharge_waitlist`, `create_recall`, `list_recalls`, `complete_recall`

**Documentation (16):** `create_encounter`, `get_encounter`, `list_encounters`, `update_encounter`, `record_vitals`, `list_vitals`, `save_ros`, `get_ros`, `save_physical_exam`, `get_physical_exam`, `list_templates`, `get_template`, `request_cosign`, `approve_cosign`, `list_pending_cosigns`, `check_drug_allergy_alerts`

**Labs/Docs (10):** `add_lab_catalogue_entry`, `list_lab_catalogue`, `create_lab_order`, `list_lab_orders`, `enter_lab_result`, `list_lab_results`, `sign_lab_result`, `upload_document`, `list_documents`, `verify_document_integrity`

**Backup (3):** `create_backup`, `restore_backup`, `list_backups`

**Total: ~91 handlers** (several of the "already-existing" wrappers are already in `src/lib/tauri.ts`; the new ones to add are the Patient, Clinical, Scheduling, Documentation, Labs/Docs, and Backup groups — approximately 63 net-new wrappers).

---

## Constraints

- **TypeScript strict mode is non-negotiable.** `tsconfig.json` has `strict: true`, `noUnusedLocals: true`, `noUnusedParameters: true`. Every type file must compile clean with `tsc --noEmit`. Do not add `// @ts-ignore` or suppress errors.
- **No new dependencies** except `tauri-plugin-dialog` (only in S05). The router must be hand-written. No `react-router-dom`, no `@tanstack/router`, no `zustand`, no `jotai`.
- **Tailwind only** for styling — no CSS modules, no styled-components. Confirmed by `.gsd/DECISIONS.md` (M001 decision).
- **Invoke parameter names must be snake_case** matching Rust function parameter names exactly. This is established in `src/lib/tauri.ts` and confirmed in DECISIONS.md: "Passed resource_type as snake_case in invoke() params — Tauri 2 uses Rust parameter names for deserialization."
- **Async scheduling commands** — commands in `scheduling.rs` and `documentation.rs` are `pub async fn`. Tauri 2 handles async commands transparently via `invoke()`, so the frontend wrapper is identical to sync commands.
- **`RosStatus` enum** serializes as `snake_case` strings (`"positive"`, `"negative"`, `"not_reviewed"`) — the TypeScript union must match these exact strings, not the Rust PascalCase variant names.
- **`role` values** from the backend are PascalCase strings (`"Provider"`, `"FrontDesk"`, etc.) — do NOT convert to snake_case in TypeScript.
- **`commands` object in `src/lib/tauri.ts` is append-only** for this slice. Existing wrappers (auth, session, MFA, break glass, audit, FHIR, health) must not be modified.
- **No CSS URL routing.** The app runs in a Tauri WKWebView with no real URL bar. Any `window.location`-based router will produce incorrect behavior.

---

## Common Pitfalls

- **Forgetting `snake_case` in `invoke()` params** — TypeScript interfaces use `camelCase` (matching serde output), but the second argument to `invoke("command_name", {...})` must use the Rust function's parameter names (snake_case). Example: the Rust command `fn get_patient(patient_id: String)` must be invoked as `invoke("get_patient", { patient_id: id })` not `{ patientId: id }`. This is a silent runtime error — Tauri silently passes `null` for unrecognized params rather than failing.
- **`Optional<T>` vs `T | null`** — Rust `Option<T>` serializes as `T | null` in JSON. TypeScript interfaces must use `string | null` not `string | undefined` for optional Rust fields. Using `string | undefined` causes `tsc` to flag assignments from backend responses as errors.
- **Auth context not available in router** — The `RouterContext` must be initialized inside the auth gate (after `useAuth()` resolves), not at the top of the component tree. Otherwise routes render before auth state is known.
- **RBAC gate in nav vs. in routes** — Nav items should be filtered by role (what is visible), AND each route component should independently verify auth/role (what is accessible). Two-layer defense prevents direct-navigation bypasses. Don't rely solely on hiding nav items.
- **`noUnusedLocals` with typed route params** — If route objects carry patient IDs but a page component doesn't consume the ID immediately, `tsc` will flag the unused destructuring. Define route union types carefully so each variant's extra fields are always consumed by the page that handles them.
- **Sidebar layout vs. lock screen overlay** — The LockScreen must render as an overlay `position: fixed inset-0` on top of the AppShell, not as a replacement. `src/App.tsx` already demonstrates this pattern (`{auth.isLocked && <LockScreen .../>}` rendered before the main content div). Preserve this.
- **`useIdleTimer` placement** — Currently called in `App.tsx`. After refactoring, it belongs in `AppShell` (the authenticated shell component), not in individual page components. The timer must be active any time `auth.isAuthenticated && !auth.isLocked`.

---

## Open Risks

- **`tsc --noEmit` timeout in dev environment** — Prior sessions noted `cargo test` stalls due to Tauri compilation time. The TypeScript compile may also be slow if Vite's `@tauri-apps/api` type resolution is involved. Use `npx tsc --noEmit 2>&1` with a reasonable timeout; if it stalls, fall back to checking individual files with `tsc --noEmit src/types/*.ts src/lib/tauri.ts` as a partial gate.
- **Documentation module type completeness** — `documentation.rs` has the most complex types (ROS with 14 systems × 2 fields each, PhysicalExam with 13 systems, TemplateRecord). Missing a field silently produces `undefined` at runtime, not a compile error, unless the interfaces are exact mirrors of the Rust structs. Must be read carefully against the Rust source.
- **`RosStatus` as an enum in Rust vs. union in TypeScript** — The `#[serde(rename_all = "snake_case")]` attribute on `RosStatus` means values serialize as `"positive"`, `"negative"`, `"not_reviewed"`. TypeScript should model this as `type RosStatus = "positive" | "negative" | "not_reviewed"` not as a numeric enum or PascalCase strings.
- **`record: serde_json::Value` fields** — Several Rust records expose raw FHIR JSON as `resource: serde_json::Value`, which maps to `resource: Record<string, unknown>` in TypeScript. These fields should be typed as `Record<string, unknown>` (not `any`) to stay strict. Individual page components that need to read into the FHIR JSON can use runtime narrowing.
- **nav routing vs. deep linking** — The state-based router has no URL history, so "back" navigation is app-managed. Each page component should receive a `onBack?: () => void` callback or rely on `useNav()` to pop a history stack. Must decide in S01 whether to implement a history stack or keep navigation strictly forward-only (simpler, sufficient for M002).
- **AppShell import complexity** — Once all type files and `tauri.ts` wrappers exist, `App.tsx` (or the new `AppShell.tsx`) will import from 6+ type files. The `commands` object will be very large. Consider splitting `commands` into named sub-objects (`commands.patient`, `commands.scheduling`, etc.) to improve discoverability — but this changes the API surface that S02-S07 will depend on. **Decide this in S01 and document it as an architectural decision.** Changing the structure in S03 would require touching all earlier slices.

---

## Architectural Decision Required: `commands` object structure

Two options for the expanded `commands` object in `src/lib/tauri.ts`:

**Option A — Flat (current pattern):**
```typescript
export const commands = {
  checkDb: ...,
  createPatient: ...,
  listAppointments: ...,
  // all 90+ wrappers at top level
}
```
Pro: matches existing code exactly; simple to grep.
Con: large, less discoverable; autocomplete surfaces unrelated commands together.

**Option B — Namespaced sub-objects:**
```typescript
export const commands = {
  health: { checkDb: ..., getAppInfo: ... },
  auth: { login: ..., logout: ..., registerUser: ... },
  patient: { create: ..., get: ..., search: ... },
  scheduling: { createAppointment: ..., listAppointments: ... },
  // etc.
}
```
Pro: discoverable; IDE autocomplete scoped by domain.
Con: breaks all existing `commands.checkDb()` callsites in the codebase (currently ~12 callsites in hooks and components).

**Recommendation:** Keep **Option A (flat)** for S01. The existing callsites in `useAuth`, `useIdleTimer`, `DatabaseStatus`, `FhirExplorer`, and `AuditLog` all use `commands.methodName()` directly. Changing to namespaced sub-objects would require updating all existing callsites and could introduce regressions before S01 is even verified. The flat structure scales to 90+ commands without functional problems. Document this as an architectural decision and revisit only if the codebase grows beyond M002.

---

## Skills Discovered

| Technology | Skill | Status |
|------------|-------|--------|
| React / TypeScript | (no skill needed — standard React patterns) | n/a |
| Tauri 2.x | (no installable skill found) | none found |
| Tailwind CSS | (no skill needed — already configured and used in project) | n/a |

---

## Sources

- Rust command inventory derived from `src-tauri/src/lib.rs` invoke_handler block (direct code read)
- Type field names derived from `src-tauri/src/commands/patient.rs`, `clinical.rs`, `scheduling.rs`, `documentation.rs`, `labs.rs`, `backup.rs` (direct code read)
- Router approach validated against `src/App.tsx` existing state-machine pattern and DECISIONS.md note on no-URL-bar Tauri context
- Strict TypeScript config confirmed in `tsconfig.json` (`strict: true`, `noUnusedLocals`, `noUnusedParameters`)
- RBAC role strings confirmed in `src-tauri/src/rbac/roles.rs` and `src/hooks/useAuth.ts` (`user.role`)
- Tailwind-only constraint confirmed in `.gsd/DECISIONS.md` S01 entry
- Invoke snake_case parameter rule confirmed in `.gsd/DECISIONS.md` S01 entry: "Passed resource_type as snake_case in invoke() params — Tauri 2 uses Rust parameter names for deserialization"

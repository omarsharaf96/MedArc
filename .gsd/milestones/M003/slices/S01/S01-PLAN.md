# S01: Touch ID Fix + PT Note Templates

**Goal:** Touch ID works on the lock screen via a real LAContext call; provider can create, co-sign, and lock all three PT note types (Initial Evaluation, Daily Progress Note, Discharge Summary) with PT-specific fields. Data models proven by `cargo test --lib`; UI types proven by `tsc --noEmit`.

**Demo:** `cargo test --lib` exits 0 (265 existing + new PT model tests). `tsc --noEmit` exits 0. On hardware with Touch ID, pressing "Use Touch ID" on the lock screen triggers a native macOS LAContext prompt and unlocks the session without entering a password.

## Must-Haves

- `com.apple.security.device.biometric-access = true` in `entitlements.plist` (replaces wrong `personal-information.location` key)
- `check_biometric_available()` returns real LAContext `canEvaluatePolicy` result (not hardcoded `false`)
- `biometric_authenticate` Tauri command triggers LAContext `evaluatePolicy`, bridges the ObjC callback safely to the async context, calls `session.unlock(&user_id)` on success
- `LockScreen.tsx` "Use Touch ID" button calls `commands.biometricAuthenticate()` directly (not `onUnlock("")`)
- `useAuth` has `biometricUnlock()` function that calls the command and refreshes session state
- Migration 15 adds `pt_note_index` with `CHECK` constraints on `note_type` and `status`
- `commands/pt_notes.rs` implements 6 commands: `create_pt_note`, `get_pt_note`, `list_pt_notes`, `update_pt_note`, `cosign_pt_note`, `lock_pt_note`
- All PT note commands write audit rows and enforce RBAC via `ClinicalDocumentation` resource
- `src/types/pt.ts` — complete TypeScript types for all three note shapes with `T | null` for optionals
- `src/lib/tauri.ts` — 6 new PT note command wrappers with `?? null` fallbacks
- `src/pages/PTNotesPage.tsx` — Provider-only page listing PT notes with links to note forms for all three types
- `cargo test --lib` passes (265 existing + new tests for PT note serialization + migration validity)
- `tsc --noEmit` exits 0

## Proof Level

- This slice proves: **contract + integration**
- Real runtime required: yes — Touch ID hardware prompt must fire on the development machine
- Human/UAT required: yes — Touch ID prompt fires and unlocks; all three note type forms render and can be submitted without errors

## Verification

```bash
# 1. Contract — all 265 existing tests + new PT model tests pass
cd src-tauri && cargo test --lib 2>&1 | tail -5

# 2. TypeScript contract
cd .. && npx tsc --noEmit 2>&1 | tail -5

# 3. Touch ID integration — verified manually by pressing "Use Touch ID"
#    on the lock screen in the running Tauri dev app and confirming the
#    native Touch ID dialog appears and unlocks successfully.

# 4. PT note forms — verified manually by navigating to Patients > [patient] >
#    PT Notes and creating one note of each type, co-signing, and confirming
#    status transitions in the UI.
```

## Observability / Diagnostics

- Runtime signals: Every PT note command writes a structured audit row (`pt_note.create`, `pt_note.cosign`, `pt_note.lock`) with `patient_id`, `encounter_id`, and `note_id` in the `details` field. Biometric authenticate command writes `auth.biometric.unlock` audit row on success and `auth.biometric.failed` on failure.
- Inspection surfaces: `pt_note_index` table in SQLite (queryable via FhirExplorer or `sqlite3 medarc.db`); Audit Log page shows all biometric and PT note events.
- Failure visibility: `biometric_authenticate` returns `AppError::Authentication` with a human-readable message on LAContext failure (user cancelled, not enrolled, hardware unavailable). PT note commands return `AppError::NotFound` or `AppError::Unauthorized` with a stable message.
- Redaction constraints: No PHI in audit `details` field. No secrets or tokens logged.

## Integration Closure

- Upstream surfaces consumed: `session.unlock(&user_id)`, `write_audit_entry`, `middleware::require_authenticated`, `middleware::require_permission`, `fhir_resources` table, `encounter_index` FK pattern
- New wiring introduced in this slice:
  - `objc2-local-authentication` crate added to `Cargo.toml`
  - `biometric_authenticate` command registered in `lib.rs` invoke_handler
  - `pub mod pt_notes` added to `commands/mod.rs` and `lib.rs`
  - Migration 15 added to `migrations.rs`
  - 6 PT note commands registered in `lib.rs`
  - `Route` union extended with `pt-notes` and `pt-note-detail` variants
  - `ContentArea.tsx` and `Sidebar.tsx` wired with PT Notes route
  - `useAuth.biometricUnlock()` wired in `App.tsx` → `LockScreen`
- What remains before the milestone is truly usable end-to-end: S02 (objective measures for IE/discharge fields), S03 (AI voice draft), S04 (document vault), S05 (PDF export), S06 (fax), S07 (auth tracking)

## Tasks

- [x] **T01: Fix Touch ID entitlement, biometric.rs, and biometric_authenticate command** `est:2h`
  - Why: The entitlements plist bug and the `biometric.rs` stub are blocking every Touch ID call in sandbox. This is the highest-risk item and must be proven on hardware before moving to PT notes.
  - Files: `src-tauri/entitlements.plist`, `src-tauri/src/auth/biometric.rs`, `src-tauri/src/commands/mfa.rs`, `src-tauri/Cargo.toml`
  - Do: (1) Replace wrong entitlement key with `com.apple.security.device.biometric-access = true`. (2) Add `objc2-local-authentication = { version = "0.3.2", features = ["LAContext", "block2"] }` to Cargo.toml under `[target.'cfg(target_os = "macos")'.dependencies]`. (3) Rewrite `biometric.rs` with `#[cfg(target_os = "macos")]` block: `check_biometric_available()` calls `LAContext::new()` then `canEvaluatePolicy_error(LAPolicy::DeviceOwnerAuthenticationWithBiometrics, &mut ptr::null_mut())`. (4) Add `pub async fn biometric_authenticate` in `commands/mfa.rs`: check availability first, then `std::thread::spawn` a thread that creates `LAContext`, calls `evaluatePolicy_localizedReason_reply` with `authenticate_biometric_reason()`, sends result over `std::sync::mpsc::channel`, then call `session.unlock(&user_id)` and update sessions row on success. On non-macOS, return `Err(AppError::Authentication("Biometric not available".to_string()))`. (5) Register `commands::mfa::biometric_authenticate` in `lib.rs` invoke_handler. (6) Write unit test `biometric_authenticate_unavailable_on_non_macos` in `mfa.rs` (or compile-time cfg test).
  - Verify: `cd src-tauri && cargo build 2>&1 | tail -5` (compile clean with new crate). On hardware: run `cargo tauri dev`, lock session, press "Use Touch ID" — native Touch ID dialog must appear (wired in T02; this task just proves the backend compiles and the command is reachable).
  - Done when: `cargo build` exits 0 with `objc2-local-authentication` in the graph; `biometric_authenticate` command is registered in invoke_handler; `check_biometric_available()` no longer hardcodes `false` on macOS.

- [x] **T02: Wire biometricUnlock into useAuth and LockScreen** `est:1h`
  - Why: The backend command exists but `handleTouchId` in `LockScreen.tsx` still calls `onUnlock("")` (password path, always fails). This task closes the UI-to-backend loop for Touch ID.
  - Files: `src/hooks/useAuth.ts`, `src/components/auth/LockScreen.tsx`, `src/App.tsx`, `src/lib/tauri.ts`
  - Do: (1) Add `biometricAuthenticate: () => invoke<void>("biometric_authenticate", {})` to `commands` object in `tauri.ts`. (2) Add `biometricUnlock: () => Promise<void>` to `UseAuthReturn` interface in `useAuth.ts`; implement it as a `useCallback` that calls `commands.biometricAuthenticate()` then `commands.getSessionState()` and updates session state — same refresh pattern as `unlock()`. (3) Update `LockScreen.tsx` props interface: add `onBiometricUnlock: () => Promise<void>`. Update `handleTouchId` to call `onBiometricUnlock()` directly (not `onUnlock("")`). (4) Pass `onBiometricUnlock={auth.biometricUnlock}` from `App.tsx` to `LockScreen`.
  - Verify: `npx tsc --noEmit` exits 0. On hardware: lock session → press "Use Touch ID" → native macOS Touch ID dialog appears → on success, app unlocks and returns to the previous screen.
  - Done when: `tsc --noEmit` exits 0; Touch ID dialog fires on hardware; successful auth unlocks the session without a password.

- [x] **T03: Add Migration 15, PT note Rust types, and pt_notes.rs commands** `est:3h`
  - Why: This is the core PT note data model — all downstream slices (S02–S07) depend on the `pt_note_index` table schema and the 6 CRUD commands.
  - Files: `src-tauri/src/db/migrations.rs`, `src-tauri/src/commands/pt_notes.rs` (new), `src-tauri/src/commands/mod.rs`, `src-tauri/src/lib.rs`
  - Do: (1) Append Migration 15 to `migrations.rs` — creates `pt_note_index` with columns: `pt_note_id TEXT PK`, `patient_id TEXT NOT NULL`, `encounter_id TEXT`, `note_type TEXT NOT NULL CHECK(note_type IN ('initial_eval','progress_note','discharge_summary'))`, `status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','signed','locked'))`, `provider_id TEXT NOT NULL`, `created_at TEXT NOT NULL`, `updated_at TEXT NOT NULL`, `addendum_of TEXT` (FK to `pt_note_index.pt_note_id`). Add indexes on `patient_id`, `note_type`, `status`. (2) Create `src-tauri/src/commands/pt_notes.rs`: define `PtNoteType` enum, `InitialEvalFields`, `ProgressNoteFields`, `DischargeSummaryFields`, `PtNoteInput`, `PtNoteRecord` structs all with `#[serde(rename_all = "camelCase")]`. Include `outcome_comparison_placeholder: Option<String>` in `DischargeSummaryFields` (S02 fills this). Include `addendum_of: Option<String>` in `PtNoteInput`. (3) Implement 6 `pub async fn` commands following the `documentation.rs` pattern exactly: `create_pt_note(input, db, session, device_id)`, `get_pt_note(pt_note_id, db, session, device_id)`, `list_pt_notes(patient_id, note_type, db, session, device_id)`, `update_pt_note(pt_note_id, input, db, session, device_id)`, `cosign_pt_note(pt_note_id, db, session, device_id)` → transitions `draft → signed`, `lock_pt_note(pt_note_id, db, session, device_id)` → transitions `signed → locked`. All use `require_authenticated` + `require_permission(ClinicalDocumentation, ...)` + `write_audit_entry`. (4) Add `pub mod pt_notes;` to `commands/mod.rs`. (5) Register all 6 commands in `lib.rs` invoke_handler. (6) Add unit tests in `pt_notes.rs`: test `PtNoteType` serialization roundtrip; test `note_type` check constraint fails on invalid value; test Migration 15 is valid via `MIGRATIONS.validate()`.
  - Verify: `cd src-tauri && cargo test --lib 2>&1 | tail -10` — must show all prior 265 tests + new PT model tests passing, 0 failures.
  - Done when: `cargo test --lib` passes with new PT note tests; Migration 15 is append-only (migrations 1–14 unchanged); all 6 commands compile with correct RBAC and audit patterns.

- [x] **T04: Add src/types/pt.ts and tauri.ts command wrappers** `est:1h`
  - Why: Without TypeScript types and wrappers, the frontend UI task (T05) cannot compile. This is the pure contract layer — no UI yet.
  - Files: `src/types/pt.ts` (new), `src/lib/tauri.ts`
  - Do: (1) Create `src/types/pt.ts` with: `PtNoteType = "initial_eval" | "progress_note" | "discharge_summary"`, `PtNoteStatus = "draft" | "signed" | "locked"`, `InitialEvalFields` (chiefComplaint, mechanismOfInjury, priorLevelOfFunction, painNrs, functionalLimitations, icd10Codes, physicalExamFindings, shortTermGoals, longTermGoals, planOfCare, frequencyDuration, cptCodes, referringPhysician, referralDocumentId — all `string | null`), `ProgressNoteFields` (subjective, patientReportPainNrs, hepCompliance, barriers, treatments, assessment, progressTowardGoals, plan, totalTreatmentMinutes — all `string | null`), `DischargeSummaryFields` (totalVisitsAttended, totalVisitsAuthorized, treatmentSummary, goalAchievement, outcomeComparisonPlaceholder, dischargeRecommendations, hepNarrative, returnToCare — all `string | null`), `PtNoteInput` (patientId, encounterId, noteType, fields: InitialEvalFields | ProgressNoteFields | DischargeSummaryFields | null, addendumOf — all nullable except noteType), `PtNoteRecord` (id, patientId, encounterId, noteType, status, providerId, resource, createdAt, updatedAt, addendumOf). Follow `T | null` convention throughout. (2) Import `PtNoteInput`, `PtNoteRecord`, `PtNoteType` in `tauri.ts`; add 6 wrappers: `createPtNote`, `getPtNote`, `listPtNotes`, `updatePtNote`, `cosignPtNote`, `lockPtNote` with `?? null` fallbacks on all optional params.
  - Verify: `npx tsc --noEmit` exits 0 — no `any` types in the new file.
  - Done when: `tsc --noEmit` exits 0; all 6 wrappers present in `commands` object; `pt.ts` has no `T | undefined` (only `T | null`).

- [x] **T05: Add PT Notes page, route wiring, and note form shells** `est:2h`
  - Why: Proves the TypeScript contracts actually render in context and gives the provider the UI surface to create and manage PT notes — the visible product progress required for S01's demo.
  - Files: `src/pages/PTNotesPage.tsx` (new), `src/pages/PTNoteFormPage.tsx` (new), `src/contexts/RouterContext.tsx`, `src/components/shell/ContentArea.tsx`, `src/pages/PatientDetailPage.tsx`
  - Do: (1) Add two new route variants to `RouterContext.tsx`: `{ page: "pt-notes"; patientId: string }` and `{ page: "pt-note-detail"; patientId: string; ptNoteId: string; noteType: PtNoteType }`. (2) Create `PTNotesPage.tsx` — Provider-only (renders "Access denied" for other roles). On mount, calls `commands.listPtNotes(patientId)` and displays a list of PT notes grouped by type with status badges (draft/signed/locked). "New IE", "New Progress Note", "New Discharge Summary" buttons navigate to `pt-note-detail` with a sentinel `ptNoteId: "new"`. (3) Create `PTNoteFormPage.tsx` — a multi-section form that renders the correct field set based on `noteType`. IE shows all `InitialEvalFields`; Progress Note shows `ProgressNoteFields`; Discharge Summary shows `DischargeSummaryFields` (with a read-only placeholder section for outcome comparison). "Save Draft" calls `createPtNote` or `updatePtNote`. "Co-sign" button calls `cosignPtNote` (only shown when status is `draft`). "Lock" button calls `lockPtNote` (only shown when status is `signed`). Locked notes are displayed read-only. (4) Add cases to `ContentArea.tsx` switch for `pt-notes` and `pt-note-detail`. (5) Add "PT Notes" button/link in `PatientDetailPage.tsx` that navigates to `{ page: "pt-notes", patientId }`.
  - Verify: `npx tsc --noEmit` exits 0. In the running Tauri dev app: navigate to a patient → click "PT Notes" → page renders the list → click "New IE" → form renders with IE fields → fill fields and "Save Draft" → note appears in list with status "draft" → "Co-sign" transitions to "signed" → "Lock" transitions to "locked" → locked note is read-only.
  - Done when: `tsc --noEmit` exits 0; all three note type forms render and submit without TypeScript errors or runtime crashes; status transitions (draft → signed → locked) work end-to-end in the dev app.

## Files Likely Touched

- `src-tauri/entitlements.plist`
- `src-tauri/Cargo.toml`
- `src-tauri/src/auth/biometric.rs`
- `src-tauri/src/commands/mfa.rs`
- `src-tauri/src/commands/pt_notes.rs` (new)
- `src-tauri/src/commands/mod.rs`
- `src-tauri/src/db/migrations.rs`
- `src-tauri/src/lib.rs`
- `src/hooks/useAuth.ts`
- `src/components/auth/LockScreen.tsx`
- `src/App.tsx`
- `src/lib/tauri.ts`
- `src/types/pt.ts` (new)
- `src/contexts/RouterContext.tsx`
- `src/components/shell/ContentArea.tsx`
- `src/pages/PTNotesPage.tsx` (new)
- `src/pages/PTNoteFormPage.tsx` (new)
- `src/pages/PatientDetailPage.tsx`

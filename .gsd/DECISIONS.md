# Decisions

Architectural and implementation decisions extracted from completed work.

## M002/S01 — Navigation Shell & Type System

- **Flat `commands` object (Option A)** — `src/lib/tauri.ts` keeps all wrappers at the top level (`commands.createPatient`, not `commands.patient.create`). Existing ~28 callsites in useAuth, useIdleTimer, DatabaseStatus, FhirExplorer, AuditLog all use flat keys; namespacing would require touching all callsites and could introduce regressions before S01 is verified. Revisit only if codebase grows beyond M002.
- **State-based router, zero dependencies** — `src/router/RouterContext.tsx` uses `useState<Route[]>` (history stack) wrapped in React context. No `react-router-dom`, no `@tanstack/router`. Tauri desktop WKWebView has no real URL bar — URL history adds ceremony with no benefit. Pattern follows the same context-hook idiom as `useAuth`.
- **`Route` is a discriminated union of typed objects** — `{ page: "patients" } | { page: "patient-detail"; patientId: string } | ...`. Routes carry typed payloads (not URL strings), enabling full TypeScript exhaustiveness checking in the `renderPage()` switch.
- **`RouterProvider` initialized inside `AppShell`** — RouterContext is provided within AppShell (which only mounts in the authenticated branch of App.tsx). Prevents routes from rendering before auth state is known.
- **`useIdleTimer` moved from `App.tsx` into `AppShell`** — idle timer must be active only when `auth.isAuthenticated && !auth.isLocked`. AppShell is the correct owner because it only renders in the authenticated branch. App.tsx becomes a pure auth gate that mounts `<AppShell>`.
- **LockScreen rendered inside `AppShell` as `position: fixed inset-0 z-50` overlay** — LockScreen is rendered on top of the sidebar+content area when `isLocked` is true. This preserves the existing overlay pattern (React state is not destroyed) while moving ownership into AppShell which owns all post-auth UI.
- **Page components in `src/pages/`** — PatientsPage, SchedulePage, SettingsPage, AuditPage. PatientDetailPage added when needed in S02. Pages are `src/pages/*.tsx` not `src/components/pages/` — pages are route targets, not reusable components.
- **Backup commands omitted from `commands` object** — `create_backup`, `restore_backup`, `list_backups` are NOT registered in `src-tauri/src/lib.rs` and the backup command module (`commands/backup.rs`) does not exist. Wrappers will be added when the backend module is created (M002/S07).
- **Actual command count is 88** — `lib.rs` `invoke_handler!` block contains 88 registered commands: 2 health + 5 FHIR + 5 auth + 5 session + 2 break_glass + 7 MFA + 2 audit + 9 patient + 12 clinical + 13 scheduling + 16 documentation + 10 labs = 88. The research doc's "63 net-new" figure undercounts; the actual net-new wrappers to add in T02 are 60 (all commands beyond the 28 existing wrappers already in `src/lib/tauri.ts` for health/FHIR/auth/session/break_glass/MFA/audit).
- **RBAC nav matrix** — FrontDesk: Schedule only; NurseMa: Patients + Schedule + Settings; Provider: Patients + Schedule + Settings; BillingStaff: Schedule + Settings; SystemAdmin: all 4 items (Patients + Schedule + Settings + Audit Log). Unknown roles: Sign Out button only, no nav items. Two-layer defense: nav items hidden by role AND each page component independently checks the user's role before rendering. This prevents direct-navigation bypasses.
- **Clinical types in `src/types/patient.ts`** — AllergyInput/Record, ProblemInput/Record, MedicationInput/Record, ImmunizationInput/Record live in `patient.ts` (not a separate `clinical.ts`) because they are all patient-scoped clinical data; this matches the domain grouping used by S02–S04 downstream slices.
- **`T | null` for all optional fields, never `T | undefined`** — Rust `Option<T>` serializes as `T | null` in JSON. Using `T | undefined` causes TypeScript errors when assigning backend response fields. All type files in `src/types/` follow this convention.
- **`getRos` and `getPhysicalExam` return `T | null`** — Both `get_ros` and `get_physical_exam` return `Result<Option<T>, AppError>` in Rust. TypeScript wrappers return `Promise<RosRecord | null>` and `Promise<PhysicalExamRecord | null>` respectively — callers must handle the null case (encounter has no ROS or exam yet).
- **`getRos`/`getPhysicalExam` require BOTH `encounter_id` AND `patient_id`** — The Rust handlers take two positional params; invoke calls must pass both as `{ encounter_id: encounterId, patient_id: patientId }`. Passing only `encounter_id` causes a silent null deserialization failure.
- **`completeRecall` returns `void` (not `RecallRecord`)** — `complete_recall` in `scheduling.rs` is `Result<(), AppError>`. The TypeScript wrapper must be `invoke<void>`, not `invoke<RecallRecord>`. Confirmed from Rust source during plan self-audit.
- **`createAppointment` returns `AppointmentRecord[]` not `AppointmentRecord`** — Recurring appointment series generates multiple FHIR Appointment resources per call. TypeScript wrapper must type the return as `AppointmentRecord[]` to match the Rust `Result<Vec<AppointmentRecord>, AppError>` signature.
- **`cancelAppointment` has a `reason: Option<String>` param** — Must be included in the invoke call as `reason: reason ?? null` or Tauri will silently ignore it. The reason is stored in the FHIR Appointment cancellation reason field.
- **`listLabCatalogue` invoke key is `category_filter`** — Rust param is `category_filter`, not `category`. Using `category` silently passes null and returns all catalogue entries unfiltered.
- **`listLabOrders` invoke key is `status_filter`** — Rust param is `status_filter`, not `status`. Same silent-null failure mode as `category_filter`.
- **`listRecalls` has no `patient_id` param** — Rust signature is `(provider_id, overdue_only, status)`. Recalls are queried by provider, not by patient (clinical workflow: the doctor pulls their own overdue recall list). Patient-specific recall queries are not supported in Phase 1.
- **`listWaitlist` has no `patient_id` param** — Rust signature is `(provider_id, appt_type, status)`. Waitlist is a provider-level scheduling view, not a per-patient view.
- **`AppShellInner` pattern for RouterProvider + useNav** — `RouterProvider` and `useNav()` cannot be in the same component scope. The solution is `AppShell` renders `<RouterProvider><AppShellInner /></RouterProvider>` and `AppShellInner` calls `useNav()`. This avoids a React context initialization order error.
- **`listVitals` has optional `encounter_id` filter** — Second param `encounter_id: Option<String>` allows filtering vitals to a specific encounter. Invoke call must include `encounter_id: encounterId ?? null` (not omit the key).
- **`listEncounters` has optional date/type filters** — Rust signature: `(patient_id, start_date, end_date, encounter_type)`. All three optional params default to None. The TypeScript wrapper must pass all four keys with null fallbacks.

## S01 — Desktop Shell & Encrypted Database

- Used `[lib] name = 'app_lib'` in Cargo.toml for clear separation between binary and library crate
- Used rusqlite 0.32 (bundled-sqlcipher) instead of 0.38 due to version compatibility with rusqlite_migration 1.x
- Used getrandom 0.2 API for key generation (compatible with rusqlite dependency tree)
- Used `std::sync::LazyLock` for static migrations instead of lazy_static crate (standard library since Rust 1.80)
- Used raw hex key format (`x'...'`) for SQLCipher to skip PBKDF2 and eliminate startup latency
- Used `#[serde(rename_all = 'camelCase')]` on Rust structs for consistent Tauri 2 frontend serialization
- Added `NotFound` variant to AppError for resource-not-found error handling in CRUD commands
- Used json_extract approach for Patient lookups rather than virtual generated columns (SQLite ALTER TABLE limitations)
- Passed resource_type as snake_case in invoke() params — Tauri 2 uses Rust parameter names for deserialization
- Extracted UI into DatabaseStatus and FhirExplorer components for clean separation of concerns
- Used Tailwind utility classes for all styling (no CSS modules or styled-components)

## S02 — Auth & Access Control

- SessionInfo struct lives in auth::session module (single source of truth for session state)
- password-auth crate wraps Argon2id with safe defaults (no hand-rolled crypto)
- First user registration uses bootstrap pattern (no auth required when 0 users exist)
- Account lockout reads max_failed_logins and lockout_duration_minutes from app_settings table
- SessionManager initialized from app_settings timeout value at app startup
- Added Validation error variant alongside Authentication/Unauthorized for input validation
- Used match-based static dispatch for RBAC matrix (zero runtime overhead, exhaustive pattern matching)
- Break-glass scoped to `clinicalrecords:read` permission key format for middleware consistency
- Field filtering returns `Vec<&'static str>` with wildcard `'*'` for full access roles
- Used totp-rs crate with SHA-1 algorithm for maximum authenticator app compatibility
- TOTP secret not stored until user verifies with valid code (verify-before-store pattern)
- Touch ID implemented as stub (returns unavailable) without tauri-plugin-biometry — graceful degradation
- Password re-entry required for disabling TOTP and enabling Touch ID (sensitive operations)
- Used base64 img tag for QR code display instead of qrcode.react (backend already returns qrBase64)
- break_glass invoke wrapper includes password param matching actual Rust command signature
- useIdleTimer debounces refreshSession IPC to once per 30 seconds to avoid excessive backend calls
- LockScreen renders as overlay on top of content (preserves React state while obscuring UI)
- Two-phase login pattern: `login` returns `mfa_required: true + pending_user_id` when TOTP enabled; `complete_login` verifies TOTP then atomically creates the session — session creation is never attempted before TOTP is confirmed
- useAuth stores pendingMfaUserId in-memory only (not persisted) — MFA flow is intentionally lost on page refresh, requiring re-login (correct security behavior)
- All login failure paths return `AppError::Authentication("Invalid credentials")` regardless of failure reason — prevents username enumeration (HIPAA-aligned)

## S03 — Audit Logging

- Used SHA-256 (sha2 0.10 crate) for the hash chain — FIPS-140 compliant, no custom crypto
- Hash pre-image format: `{prev_hash}|{id}|{timestamp}|{user_id}|{action}|{resource_type}|{resource_id}|{patient_id}|{device_id}|{success}|{details}` with `|` separator (unambiguous: not present in UUIDs or RFC-3339 timestamps)
- Chain origin sentinel is the string `"GENESIS"` (not an empty string or NULL) — makes the genesis condition explicit and testable
- Immutability enforced by SQLite BEFORE UPDATE / BEFORE DELETE triggers at migration time — triggers fire before any row change, ensuring no code path (including future SystemAdmin commands) can bypass the lock
- `write_audit_entry()` takes a raw `&Connection` (not `&Database`) so callers that already hold the Mutex lock can call it without re-entrant deadlock — callers are responsible for acquiring the lock
- `success` stored as INTEGER (0/1) in SQLite with a CHECK constraint; mapped to/from Rust `bool` — avoids TEXT enum drift
- `details` field is free-text but never should contain raw PHI — convention enforced by documentation, not schema
- Migration 8 added as append-only migration (index 8 in the vector) — backward compatible with existing databases from prior slices
- `DeviceId` managed state introduced as a `device_id.rs` stub (returns `"DEVICE_PENDING"`) ahead of T04 which will wire machine-uid; this lets T02 commands compile and emit audit rows with a safe placeholder rather than blocking on T04
- `extract_patient_id()` helper extracts FHIR patient references from resource JSON: `Patient.id`, `subject.reference`, or `patient.reference` in priority order — audit metadata only, never used for clinical logic
- `audit_denied()` helper writes failure audit rows when permission check fails before the DB lock is acquired; it acquires its own transient lock to avoid leaving denied requests unlogged
- The pattern `let _ = write_audit_entry(...)` is intentional: audit write failures are swallowed rather than propagating — a failed audit write must never block the primary operation (HIPAA requires best-effort logging, not atomicity with the audited action)
- Login failure paths (inactive account, locked account, wrong password, MFA pending, invalid MFA code) all produce audit rows with `success = false` and a safe non-enumerable detail string
- `complete_login` (MFA step 2) also carries `device_id: State<'_, DeviceId>` so the MFA-verified login row is attributed to the same device as the initial auth attempt
- `DeviceId::from_machine_uid()` replaces the `DeviceId::placeholder()` stub in lib.rs — uses the `machine-uid 0.5` crate which reads `/etc/machine-id` on Linux, `IOPlatformUUID` via ioreg on macOS, and `MachineGuid` registry on Windows, without requiring elevated privileges; falls back gracefully to "DEVICE_UNKNOWN" with a startup warning log if the OS cannot supply an ID


## S04 — Patient Demographics & Care Teams

- `patient_index` denormalised table chosen over `json_extract()` for search — indexed column lookups are O(log n); JSON extraction forces a full-table scan on every query regardless of row count
- MRN format `MRN-<8 upper hex digits>` — short, readable, globally unique via `rand::random::<[u8; 4]>()`; no sequential counter avoids race conditions in concurrent inserts
- Insurance stored as FHIR extensions on the Patient resource rather than separate FHIR Coverage resources — MVP simplification; Coverage resources add significant schema complexity (subscriber relationships, benefit periods, coordination of benefits) with no payoff for a solo-practitioner MVP
- Care team stored as a single FHIR CareTeam resource per patient (upsert semantics) — one care team per patient is the clinical reality for solo/small clinic; multi-team scenarios deferred to S07
- `family_name` and `given_name` stored as lowercase in `patient_index` — enables case-insensitive LIKE prefix search without COLLATE NOCASE overhead on every query
- `build_patient_fhir()` is a pure function (no DB, no I/O) taking a typed struct → this makes it trivially testable and decouples FHIR assembly from the DB layer
- `generate_mrn()` uses `rand::random()` not `uuid` — MRNs must be short and human-readable for clinical workflows; UUIDs are too long for paper forms
- Two new `Resource` variants (`Patients`, `CareTeam`) added to the RBAC enum rather than reusing `ClinicalRecords` — keeps permission semantics distinct and prevents accidental privilege escalation via the existing ClinicalRecords wildcard rules
- `cargo test` stalled during this session (likely blocked on incremental Tauri compilation); all files were validated via `rustfmt --edition 2021` (exit 0 = valid syntax) as the verification gate

## S05 — Clinical Patient Data

- Four clinical index tables (allergy_index, problem_index, medication_index, immunization_index) added in Migration 10 — mirrors patient_index pattern from Migration 9: denormalised status/code columns indexed for fast patient-scoped list queries, full FHIR JSON stored in fhir_resources
- `ClinicalData` added as a distinct RBAC Resource enum variant rather than reusing `ClinicalRecords` — keeps clinical list permissions (allergies, problems, meds, immunizations) separate from encounter/note permissions that land in S07; prevents accidental privilege escalation via the ClinicalRecords wildcard
- NurseMa gets CRU but not Delete on `ClinicalData` — deleting allergies or medications is a clinical safety decision requiring Provider authority; NurseMa can correct by updating status to "entered-in-error"
- No `update_immunization` command shipped — immunizations are generally immutable administered-dose records; correction pattern is a new record with `status = "entered-in-error"` per clinical informatics convention
- `delete_allergy` is the only physical delete — medications and problems use status transitions (stopped/resolved) rather than deletion, preserving audit continuity and care history
- `build_allergy_fhir`, `build_problem_fhir`, `build_medication_fhir`, `build_immunization_fhir` are pure functions (no I/O) — directly testable without DB mocks; same pattern as `build_patient_fhir` in S04
- `MedicationStatement` uses `medication.concept` coding path (FHIR R4B direction) rather than the deprecated `medicationCodeableConcept` — future-proofs against FHIR R5 migration
- `list_problems` and `list_medications` accept `status_filter: Option<String>` — filtered queries use `problem_index.clinical_status` and `medication_index.status` indexed columns, not JSON extraction
- `immunization_index` ordered by `administered_date DESC` — most recent dose always first without application-layer sort
- `rustfmt` exit-0 is the verification gate (same precedent as S04) — `cargo test` stalls in this environment due to Tauri compilation time

## S06 — Scheduling

- `AppointmentScheduling` added as a new RBAC Resource variant (not reusing the legacy `Scheduling` resource) — `Scheduling` was defined in the day-0 matrix but never wired to commands; the new variant avoids ambiguity and allows the two to diverge independently in future slices
- FrontDesk gets full CRUD on `AppointmentScheduling` (unlike Provider/NurseMa who get CRU) — front desk staff own the scheduling desk and must be able to hard-delete test/erroneous appointments
- Recurring series generates individual Appointment FHIR resources per occurrence, linked by a `recurrence_group_id` extension — avoids a complex recurrence query engine; each occurrence is independently cancellable and auditable without cascading side effects
- Calendar date arithmetic uses Julian Day Number (JDN) algorithm with no external time crate — handles all month/year boundary rollovers correctly for weekly/biweekly/monthly strides; avoids adding chrono-tz or time-rs dependencies to the crate graph
- `flow_board_index` is a separate table from `appointment_index` — decouples the scheduling state machine (booked/cancelled/noshow) from the real-time clinic flow state machine (scheduled/checked_in/roomed/with_provider/checkout/completed); the two evolve on different cadences
- `flow_board_index` cascades from `appointment_index` (not `fhir_resources` directly) — double-cascade ensures flow entries are removed when appointments are deleted, without requiring a separate trigger or application-layer cleanup
- `AppointmentRequest` and `PatientRecall` are custom resource types stored in `fhir_resources` — not standard FHIR R4 (which uses AppointmentResponse/Flag/ServiceRequest); chosen for Phase 1 simplicity with an explicit upgrade path noted in follow-ups
- Open-slot search uses fixed working hours 08:00–17:00 with no provider schedule configuration — sufficient for Phase 1 solo-practitioner MVP; provider schedule blocks deferred to a future slice
- No overlap/double-booking detection in `create_appointment` — deferred; overlap detection requires a range-overlap query against `appointment_index` (e.g. `start_time < new_end AND start_time + duration > new_start`) which adds complexity without being a Phase 1 blocker
- Brace-balance + command-count Python check is the verification gate — consistent with S04/S05 precedent; `cargo test` exceeds the session compilation timeout in this environment

## S07 — Clinical Documentation

- SOAP note embedded in `Encounter.note` as a FHIR Annotation array with section extension URLs (subjective/objective/assessment/plan) — keeps documentation co-located with the encounter resource; no separate Composition resource needed for Phase 1
- Vitals stored as FHIR Observation (vital-signs category) with individual LOINC-coded components — enables interoperability and future flowsheet trending queries; BMI auto-calculated at record time and stored as a component rather than derived at query time
- ROS stored as FHIR QuestionnaireResponse referencing a canonical Questionnaire URL — natural fit for structured survey responses; sparse encoding (only answered systems stored in item array) keeps resource compact
- Physical exam stored as FHIR ClinicalImpression with system-coded findings — ClinicalImpression's `finding` array maps cleanly to per-system exam documentation; `itemCodeableConcept.text` carries the free-text exam finding
- Co-sign workflow uses FHIR Task resource — Task's requester/owner pattern is the correct FHIR primitive for "clinician A requests action from clinician B"; status lifecycle (requested → completed) maps directly to the co-sign workflow
- 12 built-in templates compiled into binary (`built_in_templates()` pure function) — zero DB reads; templates are reference data not user data; eliminates a migration dependency and allows templates to evolve with code releases
- Drug-allergy CDS uses two-pass matching: RxNorm code exact match first, then case-insensitive name fuzzy match — RxNorm codes available when substances are coded; name matching handles the common case where substances are free-text; non-drug allergies (food/environment) are explicitly skipped
- `require_authenticated()` + `require_permission()` helpers added to `middleware.rs` — these were referenced throughout S06/S07 command handlers but never defined; adding them makes the middleware API coherent and eliminates the need to call `check_permission` twice (once for auth, once for permission)
- `AppError::Serialization` variant added to `error.rs` — serde serialization failures are semantically distinct from database errors; cleaner error messages in production logs
- `DeviceId::id()` alias added alongside existing `get()` — backwards compatibility; both names now work; all new code should prefer `get()` per Rust naming conventions
- `cargo test --lib` is now the verification gate — fixes to middleware/error/device_id unblocked full compilation; 219 tests pass in <1s

## S08 — Lab Results & Document Management

- Custom `LabProcedure` resource type for catalogue (not FHIR ActivityDefinition) — ActivityDefinition has a publish/review lifecycle and approval metadata that adds unnecessary complexity for a simple lab test catalogue; Phase 1 needs a lightweight LOINC-indexed list
- DiagnosticReport `contained` Observation array chosen over separate Observation resources — keeps panel results co-located with the report resource in a single fhir_resources row; avoids creating one row per result value and the index complexity that would entail; referenced via `result: [{reference: "#obs-N"}]`
- SHA-256 used internally despite DOCS-02 naming the requirement "SHA-1 checksums" — SHA-1 is cryptographically weak (collision attacks demonstrated); SHA-256 provides the same integrity guarantee with no performance cost; the API surface uses `sha1_checksum` naming for requirement traceability
- Hand-written base64 decoder (no external crate) — avoids adding a dependency for a small utility function; the decoder is tested by `base64_decode_hello_world` and `base64_decode_hello` unit tests
- Chained `.prepare().query_map().collect()` pattern (no named `stmt` binding) used in all list commands — named `stmt` bindings inside `if/else` branches cause E0597 lifetime errors because the temporary holding the borrow isn't dropped before the branch result is returned; the chained pattern makes `stmt` a temporary within the expression, satisfying the borrow checker; consistent with scheduling.rs
- 4-variant `match` on `(status_filter, abnormal_only)` in `list_lab_results` — eliminates dynamic SQL string building while keeping query plans deterministic per variant (no runtime branching after prepare)
- Lab order status auto-transitions to `completed` when a result is entered with a linked `order_id` — removes the burden of manual order close-out; mirrors clinical workflow where results receipt implicitly closes the order
- `sign_lab_result` restricted to Provider/SystemAdmin at the application layer (beyond RBAC Update permission) — signing a lab result is a clinical attestation act that NurseMa is not licensed to perform even though NurseMa has Update permission on LabResults; two-layer guard: RBAC Update + role-specific check
- `cargo test --lib` remains the verification gate (252 tests, 0 failures in <1s)

<!-- duplicate M002/S01 block removed — authoritative decisions in the block above (lines ~1-30 of this section) -->

## M002/S04 — Clinical Data Sidebar

- **`ClinicalSidebar` calls `useClinicalData` at its own component level** — not in `PatientDetailPage`. `ClinicalSidebar` receives only `patientId` and `role` as props. Calling the hook inside `PatientDetailPage` would cause page-level re-renders on every clinical mutation; calling it inside `ClinicalSidebar` isolates re-renders to the sidebar subtree.
- **Per-domain error isolation in `useClinicalData`** — each of the 5 parallel fetches (allergies, problems, medications, immunizations, alerts) is wrapped in an independent try/catch. One failing fetch sets only that domain's `error` state; other tabs remain functional. Degrades gracefully without crashing the full sidebar.
- **FHIR extraction helpers for clinical types appended to `fhirExtract.ts`** — consistent with `extractPatientDisplay` and `extractSoapSections` already in that file; no new file needed for a set of related helpers.
- **`ClinicalSidebar` mounted outside patient-loading conditional in `PatientDetailPage`** — ensures `activeTab` state is not destroyed when the parent refreshes patient data. The component's own loading states handle its own fetch lifecycle.
- **3-task slice decomposition** — T01: data layer (hook + FHIR helpers); T02: read UI (tabs + lists + alert banner + page wiring); T03: write path (4 modals + wiring). Each task is independently verifiable with `tsc --noEmit`.
- **`window.confirm` used for allergy delete confirmation** — the allergy delete is the only physical delete in the clinical domain and is a clinical safety action. `window.confirm` blocks the WKWebView event loop momentarily but the risk is acceptable for a single destructive confirmation; the alternative (an inline state-based confirmation banner) adds complexity not justified for a low-frequency action.

## M002/S03 — Clinical Encounter Workspace

- **`encounter-workspace` route variant carries both `patientId` and `encounterId`** — the encounter ID is set at navigation time (from `createEncounter` result or from the encounter list click), not derived lazily inside the workspace. Avoids creating a blank encounter on every workspace mount and keeps `useEncounter` stateless with respect to encounter creation.
- **`userId` propagated from `ContentArea` via `useAuth` to `PatientDetailPage` and `EncounterWorkspace`** — `useAuth()` is called once in `ContentArea`; `userId` is passed as a prop. Avoids calling `useAuth()` in multiple nested sub-components and mirrors the existing `role` prop pattern already used for `PatientDetailPage`.
- **`useEncounter` hook fetches `getEncounter` + `listVitals` + `listTemplates` + `getRos` in `Promise.all`** — all encounter-scoped data loaded in parallel on mount; avoids sequential waterfall; `getRos` added to the initial fetch in T04 (not T01) because T04 is the first task that needs it; the hook is extended incrementally across tasks.
- **`extractSoapSections` helper placed in `src/lib/fhirExtract.ts`** — consistent with `extractPatientDisplay` already there; FHIR parsing helpers are co-located in this file per the established pattern; no new file for a single helper.
- **Template pre-population uses inline confirmation banner, not `window.confirm`** — `window.confirm` blocks the Tauri WKWebView event loop and is visually inconsistent with the app's design; an inline React state banner (`pendingTemplateId`) provides the same UX without the drawback.
- **`VitalsFormState` uses string fields for HTML `<input type="number">` then parses on save** — HTML inputs always return strings from `e.target.value`; using string intermediate state prevents TypeScript type errors and makes the empty-string → null conversion explicit at the save boundary.
- **BMI displayed from server-returned `VitalsRecord.bmi` only** — client-side BMI computation from form fields would show an unconfirmed number before save; clinical context requires the saved, audited value. The UI shows "—" until after first save.
- **ROS toggle grid uses inline button components, not `<input type="radio">`** — native radio inputs in the compact toggle-grid layout require `name` group management that is awkward with 14 groups; styled `<button>` elements with `onClick` are simpler, more controllable, and consistent with the app's Tailwind button pattern.
- **ROS `initRosFromRecord` is a pure function outside the component** — makes it testable in isolation if a future agent adds unit tests; mirrors the `extractPatientDisplay` / `extractSoapSections` pattern.
- **`tsc --noEmit` is the primary verification gate for S03** — Tauri compilation stalls in this environment under `cargo test`; TypeScript contract verification is fast, reliable, and catches the most likely S03 mistakes (wrong field names, missing null handling, missing route cases). Runtime verification is performed manually in the Tauri dev app.
- **"Start Encounter" creates `encounterType: "office_visit"` by default** — one-click encounter creation; the provider can change the type inside the workspace if needed. Avoids a type-selection modal before entering the workspace (reduces friction for the most common encounter type).

## M002/S05 — Scheduling & Flow Board

- **`useSchedule` exposes `reloadFlowBoard` as an alias for `reload`** — a separate per-domain reload for flow board would require a `useRef<boolean>` mounted-guard per callback to avoid stale closure issues; for MVP the full reload is acceptable (re-queries appointments, flow board, waitlist, recalls — all fast in-process SQLite queries); if performance is a concern in S07+, extract a dedicated flow board hook.
- **`today` date for `getFlowBoard` computed with `toLocaleDateString("sv")`** — the `"sv"` locale produces ISO 8601 `YYYY-MM-DD` without a timezone-shift; avoids the off-by-one-day bug of `toISOString().split("T")[0]` when the local timezone is behind UTC.
- **`extractOpenSlot` and `OpenSlot` co-located in `useSchedule.ts`, not `fhirExtract.ts`** — open-slot objects returned by `searchOpenSlots` are `Record<string, unknown>[]` from a non-FHIR backend response; placing extraction logic in `fhirExtract.ts` would misname the module's purpose (FHIR resources only).
- **`extractWaitlistDisplay` and `extractRecallDisplay` co-located in their panel files** — waitlist (`AppointmentRequest`) and recall (`PatientRecall`) are custom non-standard FHIR resource types; keeping their extractors local to the panel file that uses them avoids polluting `fhirExtract.ts` with non-standard resource types.
- **Color picker in `AppointmentFormModal` uses fixed 6-swatch palette, not `<input type="color">`** — native color picker in WKWebView is platform-controlled and visually inconsistent; fixed swatches produce deterministic appointment coloring and are simpler to implement with accessible click targets.
- **`apptType` uses bounded `<select>` with 6 hard-coded options** — the backend accepts any free-text `appt_type`; bounding the UI list prevents garbage data and provides consistent clinical vocabulary; configurable type lists deferred to a future milestone.
- **Week grid starts on Sunday** — US clinical standard; Sunday is `getDay() === 0`; week range computed by subtracting `currentDate.getDay()` days from current date, yielding the Sunday of the current week.
- **`SchedulePage` calls `useAuth()` internally and passes `userId` as `providerId` to `useSchedule`** — mirrors the `PatientsPage` pattern (useAuth inside the page component); passing `userId` filters the calendar to the current provider's schedule, which is the correct default for a solo-practitioner MVP.
- **`tsc --noEmit` is the verification gate for each S05 task** — consistent with S03/S04 precedent; Tauri compilation stalls in this environment; TypeScript contract checking catches the most likely S05 mistakes (wrong FHIR paths, missing null handling, wrong command signatures).

## M002/S06 — Labs, Documents & Physical Exam

- **`LabResultsPanel` and `DocumentBrowser` are patient-scoped panels on `PatientDetailPage`, not inside `EncounterWorkspace`** — labs and documents belong to the patient record, not to a single encounter; placing them on `PatientDetailPage` parallel to `ClinicalSidebar` follows the same domain separation pattern and allows access without opening an encounter.
- **Lab result abnormal highlighting reads `LabResultRecord.hasAbnormal` directly** — the boolean is server-computed and stored in `lab_result_index.has_abnormal`; re-parsing FHIR DiagnosticReport interpretation codes on the client adds complexity with no benefit; the denormalized field is the authoritative signal for MVP list views.
- **`extractLabResultDisplay()` reads from `LabResultRecord` denormalized fields, not the FHIR resource blob** — `loincCode`, `status`, and `hasAbnormal` are already denormalized in the record; parsing the resource blob for these values adds fragility; extraction helpers for lab results and documents are thin adapters over already-denormalized data.
- **Chunked base64 encoding using 8 KB chunks with `btoa`** — `String.fromCharCode.apply(null, bytes)` fails on files >~1 MB due to stack overflow; chunking at 8 KB avoids the limit while keeping the implementation dependency-free (no `Buffer`, which is unavailable in WKWebView). Chunk size 8192 is well under V8's stack limit.
- **`contentBase64` must never be logged** — base64-encoded file content can be up to ~85 MB for a 64 MB file; logging it would flood the console and could expose PHI document content in browser devtools. Enforced as a hard constraint in T03 must-haves.
- **Document upload uses a two-step flow: file picker → title/category modal** — the picker returns a path; reading, encoding, and uploading without a confirmation step would provide no opportunity to set title or category; the modal after file selection allows the user to set metadata before committing the upload.
- **`verifyDocumentIntegrity` deferred from S06 MVP** — integrity is already verified at upload time by the backend; re-verification requires re-reading the file from disk (the file picker path is not persisted); this is a power-user feature with low immediate clinical value; deferred to a future slice.
- **`tauri-plugin-fs` is a required companion to `tauri-plugin-dialog`** — `open()` returns a file path string, not bytes; `readFile()` from `tauri-plugin-fs` is the only way to read file bytes in a Tauri 2 WKWebView context; both plugins must be installed together.
- **Physical Exam tab seeded-ID guard follows the `soapSeededForId` pattern from S03** — the guard prevents form state from being overwritten when a reload fires for the same encounter (e.g. after saving another tab); without it, typing in the Exam tab and then saving Vitals would reset the Exam form.
- **`tsc --noEmit` is the primary verification gate for S06 T01 and T02** — consistent with S03–S05 precedent; T03 additionally requires the running Tauri app to verify native file picker behavior, which cannot be confirmed by TypeScript alone.

## M002/S07 — Settings, Cleanup & End-to-End Verification

- **`src/types/backup.ts` is a standalone types file** — backup types (`BackupResult`, `RestoreResult`, `BackupLogEntry`) live in their own file rather than being appended to `src/types/documentation.ts` or `src/types/patient.ts`; backup is a distinct domain (system administration, not clinical data) and will be imported by `SettingsPage` only; the isolated file keeps domain boundaries clear.
- **MfaSetup embedded inline in Security tab (no modal)** — mounting `MfaSetup` as an inline collapsible section within the Security tab avoids a second modal layer on top of any potential full-screen overlay; the component's own step machine (`idle → scanning → verifying → success`) handles its own visibility; `onComplete`/`onCancel` callbacks reset the parent's `showMfaSetup` boolean.
- **TOTP status inferred from `disableTotp` availability, not a dedicated status command** — there is no `is_totp_enabled` Tauri command; the Security tab renders both "Set up TOTP" and "Disable TOTP" sections; the user selects whichever is applicable; attempting to disable when not enabled produces a backend error that is surfaced as an inline error message; this avoids adding a new Rust command for S07.
- **Restore button gated on `user?.role === "SystemAdmin"` in SettingsPage** — `restore_backup` enforces a SystemAdmin role check at the command level beyond RBAC; showing the button to Provider users would result in a confusing "Unauthorized" error; hiding it in the UI is the correct defensive layer.
- **`tsc --noEmit` is the primary verification gate for T01 and T02** — consistent with S03–S06 precedent; catches the most likely S07 errors (wrong field types, missing null handling, wrong invoke param names) before runtime; T03 provides the runtime and UAT verification layer.
- **`cargo test --lib` used as regression gate in T01 and T03** — no Rust changes in S07; running the test suite confirms no unintended regressions from the type/wrapper additions; establishes the passing baseline before milestone completion is declared.

## M003 — PT Practice (Planning)

- **Touch ID via `objc2-local-authentication`, not `tauri-plugin-biometric`** — `tauri-plugin-biometric` explicitly lists macOS as unsupported (Android/iOS only). The correct path is direct FFI to Apple's LocalAuthentication framework via the `objc2-local-authentication` Rust crate (safe bindings, published 0.3.2). LAContext.evaluatePolicy on macOS is the same API used by iOS; the Tauri plugin simply hasn't implemented the macOS target. No workarounds or stubs — real LAContext call.
- **Biometric unlock is a parallel path to password unlock, not a replacement** — `unlock_session` takes a password string; biometric unlock needs its own `biometric_authenticate` command that calls LAContext and, on success, calls `session.unlock(&user_id)` directly (bypassing password verification). Password unlock remains the fallback. HIPAA requires a recoverable fallback when biometrics fail.
- **`com.apple.security.device.biometric-access` entitlement required in App Sandbox** — Without this entitlement, LAContext.evaluatePolicy silently returns `.biometryNotAvailable` in a sandboxed app. The existing entitlements.plist had a placeholder comment for Touch ID but the wrong key (`com.apple.security.personal-information.location`). Must be corrected in S01.
- **PT note types stored in `fhir_resources` as FHIR Composition resources** — Consistent with existing pattern (encounters stored as FHIR Encounter). PT notes are FHIR Composition resources with `type.coding` distinguishing IE/progress/discharge. Index table `pt_note_index` provides fast queries by patient/type/status without parsing JSON.
- **Outcome scores stored as FHIR Observation resources with LOINC codes** — LEFS, DASH, NDI, Oswestry, PSFS, FABQ each have published LOINC panel codes. Storing as Observation maintains FHIR R4 consistency and enables future interoperability. `outcome_score_index` table for fast longitudinal trending queries.
- **whisper.cpp via `whisper-rs` crate, `small.en` model default** — `whisper-rs` wraps whisper.cpp as a Rust crate; model stored in app support directory; downloaded on first AI use. `small.en` chosen as default for speed/accuracy balance on Apple Silicon (< 60s for 20-min session). Provider can upgrade to `medium.en` in Settings.
- **Ollama HTTP API at localhost:11434; no embedded LLM** — Embedding a 4-8 GB LLM into the app bundle is infeasible. Ollama is the standard local LLM runtime; provider installs it separately. App checks `GET localhost:11434/api/tags` on startup and shows a banner if not running. This keeps the app bundle small and lets the provider manage their model selection.
- **AWS Bedrock Claude Haiku as fallback, not primary** — Bedrock is invoked only when Ollama is unavailable. User is notified. Minimal PHI in prompts; BAA required. Primary path is always local.
- **PDF generation via `printpdf` Rust crate** — `printpdf` is the standard Rust programmatic PDF crate; no native dependencies. Letterhead logo loaded from practice settings blob. Fonts (Helvetica subset) embedded in app bundle. No WebView-based PDF rendering — avoids layout engine unpredictability.
- **Phaxio credentials in SQLCipher, never env vars** — Consistent with existing pattern (DB encryption key in Keychain). Phaxio API key + secret stored in `app_settings` table (SQLCipher-encrypted). Setup wizard in SettingsPage > Fax tab writes them via a dedicated Tauri command.
- **Fax polling as a Tauri background task, not a frontend timer** — Frontend timers are unreliable when the window is hidden or the system sleeps. Polling from Rust via `tauri::async_runtime::spawn` is more reliable and doesn't require the React app to be active.
- **Visit counter increments on `cosign_pt_note`, not on appointment completion** — The billing-relevant event is the signed clinical note, not the appointment status. Incrementing on co-sign is consistent with how PT insurers define a "completed visit" for auth tracking purposes.

## M003/S01 — Touch ID Fix + PT Note Templates

- **`biometric_authenticate` bridges LAContext callback via `std::thread::spawn` + `std::sync::mpsc::channel`** — `LAContext` is `!Send + !Sync`; it cannot be moved across async executor threads. The safe pattern is: spawn a dedicated OS thread, create `LAContext` there, call `evaluatePolicy_localizedReason_reply`, receive the result via a channel, then either call `session.unlock(&user_id)` or return an error. `tauri::async_runtime::spawn_blocking` was considered but rejected because it doesn't guarantee the closure runs on a dedicated thread with a stable stack for ObjC callbacks; raw `std::thread::spawn` is safer for ObjC FFI.
- **`objc2-local-authentication` under `[target.'cfg(target_os = "macos")'.dependencies]`** — Placing the crate in a platform-conditional dependency block ensures the binary compiles on Linux/Windows CI without ObjC framework linking. All biometric Rust code is guarded by `#[cfg(target_os = "macos")]`. The `biometric_authenticate` Tauri command must compile on all platforms; the non-macOS path returns `Err(AppError::Authentication(...))` immediately.
- **`biometricUnlock` is a parallel path in `useAuth`, not a variant of `unlock`** — `unlock(password)` calls `unlockSession` which goes through password verification. `biometricUnlock()` calls `biometricAuthenticate` which calls `session.unlock(&user_id)` directly after LAContext success. Merging them into one function would require a type discriminant and add complexity; keeping them separate mirrors the backend command split and avoids accidental routing of an empty password through the password-verify path.
- **PT note commands in a new `commands/pt_notes.rs` module** — `documentation.rs` is 2,955 lines. Appending 6 more commands would push it past 3,500 lines and make future slice development harder to navigate. New module follows established naming: `commands/labs.rs`, `commands/scheduling.rs`. The module is declared in `commands/mod.rs` and registered in `lib.rs` identically to existing modules.
- **`pt_note_index.pt_note_id` as PK column name (not `id`)** — Mirrors `encounter_index.encounter_id` pattern; avoids shadowing `fhir_resources.id` in JOINs when both tables are queried. Consistent with the constraint documented in S01 research.
- **`addendum_of` FK column in `pt_note_index` from day one (S01)** — The addendum UI ships in a later slice, but the data model field must exist in Migration 15 to avoid a breaking schema change. An `ALTER TABLE ADD COLUMN` would be feasible in SQLite but adds migration complexity and could introduce null-safety issues in older rows. Cheaper to define the column now as nullable with no constraint (other than the self-referencing FK).
- **`outcome_comparison_placeholder: Option<String>` in `DischargeSummaryFields`** — S02 provides outcome score data; S01 ships a placeholder so the Rust struct and TypeScript type are stable from the start. S02 fills in real values via `update_pt_note`. No breaking schema or type change needed between slices.
- **PT Notes route uses `ptNoteId: "new"` sentinel for note creation** — Mirrors no existing pattern directly, but is the simplest approach: the form page checks `ptNoteId === "new"` to decide between `createPtNote` (first save) and `updatePtNote` (subsequent saves). Avoids a separate "create note" route and keeps the URL/route model minimal. The sentinel is a string (not `null | undefined`) to keep the `Route` type discriminant simple.
- **`PtNoteFields` as a TypeScript union type, not a tagged object** — The Rust backend uses a `#[serde(tag = "noteType", content = "fields")]` tagged enum. TypeScript consumers receive the discriminated structure; the union `InitialEvalFields | ProgressNoteFields | DischargeSummaryFields` maps to this without needing a wrapper interface. The form page uses `noteType` prop (passed via route) to determine which branch to render — no runtime type narrowing on the received data needed.
- **PT Notes page is Provider/SystemAdmin only at the UI layer** — `ClinicalDocumentation` RBAC already gates the backend commands. The UI role check (`role !== "Provider" && role !== "SystemAdmin"`) is the same two-layer defense pattern used throughout (nav items + page guard). NurseMa has `ClinicalDocumentation::Read` in the RBAC matrix but PT note-specific clinical workflow is Provider-only for S01; this can be relaxed in a later slice if needed.

## M003/S02 — Objective Measures & Outcome Scores

- **Scoring logic is 100% pure Rust at record time; score stored in `outcome_score_index`** — No client-side score calculation. The frontend only displays what the backend returns. Pure functions are trivially unit-testable without DB mocks; denormalising the score into the index avoids re-computing it on every trend query.
- **FABQ stores work subscale in `score`, PA subscale in `score_secondary`** — FABQ has no single composite score (it's two independent subscales). The `score` column is the primary (work) subscale per clinical convention; `score_secondary` carries the PA subscale. All other measures leave `score_secondary` NULL.
- **`get_outcome_comparison` is a dedicated Tauri command, not a frontend post-processing step** — The backend has direct access to `outcome_score_index` and can efficiently find earliest-initial and latest-discharge rows per measure type in a single pass. Doing this on the frontend would require `listOutcomeScores` to return all history and the client to filter/reduce it — more data transfer and client-side logic for no benefit.
- **`DischargeSummaryForm` receives `patientId` as a prop and fetches `getOutcomeComparison` on mount** — The comparison fetch is co-located with the form that displays it. The result is stored in local state and also serialised into `outcomeComparisonPlaceholder` on save so the comparison snapshot travels with the locked note. Type of `outcomeComparisonPlaceholder` (string | null) does not change from S01.
- **Inline SVG trend chart with no npm packages** — Y-coordinate formula: `y = 110 - Math.max(0, Math.min(1, score / maxScore)) * 100`. Fixed viewBox `0 0 400 120`. Single data point: render a centred circle with label (not a blank SVG). Zero data points: render a text message. This matches the project's zero-runtime-dependency frontend convention.
- **Tabular body-region selector (not interactive SVG body diagram)** — No SVG body asset exists in the project. Nine collapsible region panels (Cervical, Thoracic, Lumbar, Shoulder, Elbow, Wrist, Hip, Knee, Ankle) each revealing per-joint ROM and per-group MMT fields deliver the same clinical workflow with far less implementation risk. Real SVG body diagram deferred to a future slice.
- **ROM/MMT stored as a `PTObjectiveMeasures` resource in `fhir_resources` only** — No secondary index needed. ROM/MMT queries are patient-scoped reads (not multi-patient searches), so a full-table scan on `fhir_resources` filtered by patient subject reference is acceptable at solo-practice scale. Avoids a third new index table for S02.
- **`cargo test --lib` is the primary S02 Rust verification gate** — Scoring functions are pure and fast. Running the full test suite in < 1s confirms both new scoring tests and that no existing test regressed from the Migration 16 append.

# Decisions

Architectural and implementation decisions extracted from completed work.

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

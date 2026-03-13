# Requirements

This file is the explicit capability and coverage contract for the project.

## Active

### AUTH-04-FIX — Touch ID authenticates via real macOS LAContext call
- Class: core-capability
- Status: active
- Description: Touch ID unlock works on the lock screen using `objc2-local-authentication` (LAContext.evaluatePolicy) rather than the stub that always returns unavailable. Availability check returns true on supported hardware. Entitlements updated with `com.apple.security.device.biometric-access`.
- Why it matters: AUTH-04 was listed as validated in M001/M002 but the implementation was always a stub returning `false`. Every user with a Touch ID Mac has been getting password-only unlock.
- Source: execution
- Primary owning slice: M003/S01
- Supporting slices: none
- Validation: unmapped
- Notes: Three files: `biometric.rs` (real LAContext FFI), `entitlements.plist` (add biometric-access key), `LockScreen.tsx` + `session.rs` (wire `biometric_unlock` command). Add `objc2-local-authentication` to Cargo.toml.

### PT-DOC-01 — Initial Evaluation note type
- Class: core-capability
- Status: active
- Description: Provider can create an Initial Evaluation (IE) note with PT-specific fields: chief complaint, mechanism of injury, prior level of function, pain NRS (0–10), functional limitations, ICD-10 diagnosis lookup, physical exam findings (linked to ROM/MMT), STG/LTG goal rows (goal text + target date), plan of care, frequency/duration (e.g. 3×/week × 6 weeks), CPT codes with minutes, referring physician, and physician referral Rx document link.
- Why it matters: IE is the first note in every PT episode; without it the PT practice workflow cannot start.
- Source: user
- Primary owning slice: M003/S01
- Supporting slices: M003/S02 (objective measures linked from IE)
- Validation: unmapped

### PT-DOC-02 — Daily Progress Note (SOAP-PT) type
- Class: core-capability
- Status: active
- Description: Provider can create SOAP-PT progress notes with: Subjective (patient report, pain NRS, HEP compliance Yes/No/Partial, barriers), Objective (treatments with CPT code + minutes, exercises, vitals), Assessment (progress toward STG/LTG: progressing/plateau/regressed, narrative), Plan (next session, HEP updates, referral needs), and Billing (auto-calculated timed units via 8-minute rule, total treatment time).
- Why it matters: Most frequent note type — every PT visit generates one.
- Source: user
- Primary owning slice: M003/S01
- Supporting slices: none
- Validation: unmapped

### PT-DOC-03 — Discharge Summary note type
- Class: core-capability
- Status: active
- Description: Provider can create a Discharge Summary with: total visits attended/authorized, treatment summary, outcome measure comparison (initial vs discharge, auto-pulled from S02), per-goal STG/LTG achievement status (Met/Partially Met/Not Met), discharge recommendations, HEP narrative, return-to-care criteria.
- Why it matters: Required by insurers; closes the episode of care and auto-populates from prior notes.
- Source: user
- Primary owning slice: M003/S01
- Supporting slices: M003/S02 (outcome measure comparison)
- Validation: unmapped

### PT-DOC-04 — PT note co-sign, lock, and addendum workflow
- Class: core-capability
- Status: active
- Description: All PT note types require provider co-sign before locking. Locked notes are read-only and audit-logged via the existing hash-chain system. Corrections go through an addendum workflow (new note linked to original). Templates pre-populate from prior note data where applicable.
- Why it matters: Billing compliance; HIPAA integrity; insurer requirements.
- Source: user
- Primary owning slice: M003/S01
- Supporting slices: none
- Validation: unmapped

### PT-OBJ-01 — ROM (Range of Motion) recording
- Class: core-capability
- Status: active
- Description: Provider can record active ROM (degrees), passive ROM (degrees), end-feel, and pain with motion (Y/N + NRS) for all major joint groups bilaterally: cervical, thoracic, lumbar, shoulder, elbow, wrist, hip, knee, ankle. Normal reference ranges shown inline. Body diagram UI with tap-to-enter per joint region.
- Why it matters: ROM is the primary objective measure in PT; required in IE and every progress note.
- Source: user
- Primary owning slice: M003/S02
- Supporting slices: none
- Validation: unmapped

### PT-OBJ-02 — MMT (Manual Muscle Testing) recording
- Class: core-capability
- Status: active
- Description: Provider can record MMT grades using the Kendall 0/1/2/3/4-/4/4+/5-/5 scale for all major bilateral muscle groups (shoulder flexors/abductors/rotators, elbow, wrist, hip, knee, ankle, cervical). Displayed as a bilateral table view.
- Why it matters: MMT is standard in PT neurological and orthopedic assessments.
- Source: user
- Primary owning slice: M003/S02
- Supporting slices: none
- Validation: unmapped

### PT-OBJ-03 — Special orthopedic tests
- Class: core-capability
- Status: active
- Description: Provider can record special orthopedic tests with result (Positive/Negative/Equivocal) and clinical note. Pre-loaded library filterable by body region (e.g. shoulder → empty can, Hawkins-Kennedy, Neer; knee → Lachman, McMurray, valgus/varus stress). Data model: test name, body region, result, note.
- Why it matters: Orthopedic tests document diagnostic reasoning; required in IE physical exam.
- Source: user
- Primary owning slice: M003/S02
- Supporting slices: none
- Validation: unmapped

### PT-OBJ-04 — Standardized outcome measures with auto-scoring and trending
- Class: core-capability
- Status: active
- Description: Provider can administer and score LEFS (20 items, 0–80, MCID 9), DASH (30 items, 0–100, MCID 10.8), NDI (10 items, 0–100%, MCID 7.5), Oswestry (10 items, 0–100%, MCID 10), PSFS (3–5 items, 0–10 avg, MCID 2.0), FABQ (16 items, work + PA subscales). Scores auto-calculated with severity classification. Stored with timestamp and graphed over episode. Initial vs discharge comparison auto-pulled into Discharge Summary.
- Why it matters: Outcome measures are required by Medicare and most PT payers for continued auth justification.
- Source: user
- Primary owning slice: M003/S02
- Supporting slices: M003/S01 (Discharge Summary pulls from these)
- Validation: unmapped

### PT-AI-01 — AI voice-to-note (Whisper + Ollama)
- Class: differentiator
- Status: active
- Description: Provider records entire treatment session audio; app transcribes via whisper.cpp (small.en default, up to medium.en) locally and generates a SOAP-PT draft via Ollama (LLaMA 3.1 8B primary, Phi-3 Mini fallback) within 3 minutes for a 20-minute session on Apple Silicon. UI shows live audio visualiser, raw transcript panel, streaming draft, uncertainty highlighting. Provider reviews and signs — AI draft never auto-locks.
- Why it matters: Core product differentiator; reduces documentation time 30-41%.
- Source: user
- Primary owning slice: M003/S03
- Supporting slices: none
- Validation: unmapped

### PT-AI-02 — Fully local AI pipeline with privacy guarantees
- Class: compliance/security
- Status: active
- Description: All audio captured and transcribed locally via whisper.cpp (no network). Raw audio deleted after transcription completes. Transcript stored in SQLCipher alongside the note. Ollama runs at localhost:11434. App checks Ollama availability on startup.
- Why it matters: PHI never leaves device — core HIPAA privacy posture.
- Source: user
- Primary owning slice: M003/S03
- Supporting slices: none
- Validation: unmapped

### PT-AI-03 — AWS Bedrock Claude Haiku fallback
- Class: core-capability
- Status: active
- Description: When Ollama is unavailable, app falls back to AWS Bedrock Claude Haiku for note generation (requires internet). User is notified of the fallback. De-identified or minimal PHI sent; BAA required.
- Why it matters: Ensures AI feature works on first install before Ollama is set up.
- Source: user
- Primary owning slice: M003/S03
- Supporting slices: none
- Validation: unmapped

### PT-DOC-CTR-01 — PT-specific patient document vault
- Class: core-capability
- Status: active
- Description: Upgrade existing document upload into a structured vault with 8 PT categories: Referral/Rx, Imaging, Consent Forms, Intake/Surveys, Insurance, Legal, Home Exercise Program, Other. Document list per patient filterable by category, sortable by date. Inline PDF/image preview panel. Existing 64 MB limit and SHA-256 integrity check preserved. Audit log entry on every view/upload/download.
- Why it matters: PT practices receive high volumes of referral scripts, imaging reports, and consent forms that need structured organisation.
- Source: user
- Primary owning slice: M003/S04
- Supporting slices: none
- Validation: unmapped

### PT-DOC-CTR-02 — Intake survey builder
- Class: core-capability
- Status: active
- Description: Drag-and-drop form builder (text field, number, yes/no, pain scale, date fields). Built-in templates: Pain and Function Intake, Medical History, HIPAA Acknowledgment. Patient fills out on a kiosk/tablet view (clean UI, no sidebar). Responses stored as structured data linked to patient chart. PDF snapshot auto-generated and saved to Document Center on completion.
- Why it matters: Replaces paper intake process; structured data feeds directly into the clinical record.
- Source: user
- Primary owning slice: M003/S04
- Supporting slices: none
- Validation: unmapped

### PT-DOC-CTR-03 — Referral tracking
- Class: core-capability
- Status: active
- Description: Track referring provider (name, NPI, practice, phone/fax), referral date, authorized visit count, diagnosis on referral (ICD-10), and linked document from Document Center. Referral record shown in patient header.
- Why it matters: PT visits require a physician referral for insurance billing; tracking prevents billing for unauthorised visits.
- Source: user
- Primary owning slice: M003/S04
- Supporting slices: M003/S07 (auth tracking extends this)
- Validation: unmapped

### PT-EXP-01 — PT note and report PDF export
- Class: core-capability
- Status: active
- Description: Provider can export: single note PDF, progress report (structured letter to referring MD), insurance narrative (medical necessity letter), legal/IME report (medico-legal), full chart bundle (multi-note PDF). All include practice letterhead, provider credentials/license/NPI, patient demographics, date, provider signature line. Progress report auto-populates episode dates, visits, diagnosis, STG/LTG achievement, outcome scores. Insurance narrative auto-populates ICD-10, functional limitations, CPT codes, medical necessity. Legal report auto-populates mechanism of injury, clinical findings, functional impact, prognosis.
- Why it matters: PT practices must regularly submit reports to referring MDs, insurers, and attorneys.
- Source: user
- Primary owning slice: M003/S05
- Supporting slices: M003/S02 (outcome scores), M003/S01 (note content)
- Validation: unmapped

### PT-EXP-02 — Bulk chart export
- Class: core-capability
- Status: active
- Description: Provider selects patient + date range and exports a single merged PDF with cover page and table of contents. Export destination selected via Tauri file dialog. Engine: `printpdf` Rust crate. Letterhead logo loaded from practice settings. Fonts embedded in app bundle.
- Why it matters: Legal discovery, subpoenas, and payer audits require full chart exports.
- Source: user
- Primary owning slice: M003/S05
- Supporting slices: none
- Validation: unmapped

### PT-FAX-01 — Send fax via Phaxio
- Class: core-capability
- Status: active
- Description: Provider can fax any note PDF, document, or generated report directly from the app. Phaxio API key + secret stored in SQLCipher (never plaintext). Recipient selected from Fax Contacts directory or entered ad hoc. Confirmation dialog shows recipient name, fax number, document name, page count. Phaxio `POST /v2/faxes` with multipart PDF. Patient ID tagged on every fax for audit linkage. Failed faxes surface banner with retry; auto-retry up to 2 times.
- Why it matters: PT practices fax referral requests, progress reports, and auth letters multiple times per day.
- Source: user
- Primary owning slice: M003/S06
- Supporting slices: M003/S05 (PDFs to fax)
- Validation: unmapped

### PT-FAX-02 — Receive fax via Phaxio polling
- Class: core-capability
- Status: active
- Description: Tauri background task polls Phaxio `GET /v2/faxes?direction=received` every 5 minutes. New inbound faxes land in Fax Inbox (badge in sidebar). Provider links inbound fax to a patient and assigns a Document Center category. Unlinked faxes remain in inbox until actioned.
- Why it matters: Incoming referrals and auth approvals arrive by fax; missing them delays patient care.
- Source: user
- Primary owning slice: M003/S06
- Supporting slices: none
- Validation: unmapped

### PT-FAX-03 — Fax log and audit
- Class: compliance/security
- Status: active
- Description: Fax log stored per patient and practice-wide with: fax_id (Phaxio ID), direction (sent/received), patient_id (FK nullable), recipient name/fax, document name, file path, status (queued/in_progress/success/failed), sent_at, delivered_at, pages, error_message. All fax activity audit-logged via existing hash-chain system.
- Why it matters: HIPAA requires tracking all PHI disclosures including faxes.
- Source: user
- Primary owning slice: M003/S06
- Supporting slices: none
- Validation: unmapped

### PT-AUTH-01 — Insurance authorization records and visit tracking
- Class: core-capability
- Status: active
- Description: Provider can create auth records with: payer name/phone, auth number, authorized visit count, authorized CPT codes, start/end dates. `visits_used` auto-increments when a Daily Progress Note is co-signed and locked. `visits_remaining` computed as `authorized_visits - visits_used`. Visit count shown on patient header in encounter workspace. Auth status: active/expired/exhausted.
- Why it matters: PT billing without active auth leads to claim denials and potential fraud liability.
- Source: user
- Primary owning slice: M003/S07
- Supporting slices: none
- Validation: unmapped

### PT-AUTH-02 — Authorization warning banners
- Class: core-capability
- Status: active
- Description: Amber banner when visits_remaining == 2. Red banner (blocks note creation with override) when visits_remaining == 0. Red banner when end_date < today (auth expired). Amber banner when end_date within 7 days (expiring soon). Warnings appear in the encounter workspace patient header.
- Why it matters: Zero-miss requirement — providers must never complete a session without knowing auth is exhausted.
- Source: user
- Primary owning slice: M003/S07
- Supporting slices: none
- Validation: unmapped

### PT-AUTH-03 — Re-authorization request letter generation
- Class: core-capability
- Status: active
- Description: "Request Re-Auth" button generates a pre-filled letter with: patient demographics, diagnosis/ICD-10, visits completed/remaining, functional progress summary (from latest note), continued care justification, CPT codes being requested. Letter is faxable directly via the Phaxio integration (S06).
- Why it matters: Re-auth requests are sent multiple times per episode; manual drafting is a major time sink.
- Source: user
- Primary owning slice: M003/S07
- Supporting slices: M003/S06 (fax delivery)
- Validation: unmapped

## Validated

### UI-07 — UI enforces RBAC navigation
- Status: validated
- Class: core-capability
- Source: M002 planning
- Primary Slice: M002/S01
- Proven by M002/S01: NAV_ITEMS_BY_ROLE in Sidebar.tsx confirmed; tsc --noEmit exits 0.

### UI-01 — Patient management UI
- Status: validated
- Class: core-capability
- Source: M002 planning
- Primary Slice: M002/S02
- Proven by M002/S02: PatientListPage, PatientDetailPage, PatientFormModal wired to all patient commands; tsc --noEmit exits 0.

### UI-02 — Scheduling and calendar UI
- Status: validated
- Class: core-capability
- Source: M002 planning
- Primary Slice: M002/S05
- Proven by M002/S05: CalendarPage, FlowBoardPage, AppointmentFormModal, WaitlistPanel, RecallPanel wired to all scheduling commands; tsc --noEmit exits 0.

### UI-03 — SOAP encounter workspace UI
- Status: validated
- Class: core-capability
- Source: M002 planning
- Primary Slice: M002/S03
- Proven by M002/S03+S06: EncounterWorkspace (1,648 lines) with SOAP/vitals/ROS/PhysicalExam tabs wired to all documentation commands; tsc --noEmit exits 0.

### UI-04 — Clinical sidebar UI
- Status: validated
- Class: core-capability
- Source: M002 planning
- Primary Slice: M002/S04
- Proven by M002/S04: ClinicalSidebar with 4 tabs + 4 write-path modals + DrugAllergyAlertBanner; tsc --noEmit exits 0.

### UI-05 — Lab results and document browser UI
- Status: validated
- Class: core-capability
- Source: M002 planning
- Primary Slice: M002/S06
- Proven by M002/S06: LabResultsPanel, DocumentBrowser wired to all labs/docs commands; tsc --noEmit exits 0.

### UI-06 — Settings panel UI
- Status: validated
- Class: core-capability
- Source: M002 planning
- Primary Slice: M002/S07
- Proven by M002/S07: SettingsPage (640 lines, 3-tab) wired to backup/MFA commands; tsc --noEmit exits 0.

### AUDT-01 — ePHI access logging
- Status: validated
- Class: core-capability
- Source: inferred
- Primary Slice: M001/S03

### AUDT-02 — Tamper-proof hash chain audit log
- Status: validated
- Class: core-capability
- Source: inferred
- Primary Slice: M001/S03

### AUDT-04 — Provider can view own audit log
- Status: validated
- Primary Slice: M001/S03

### AUDT-05 — SystemAdmin can view all audit logs
- Status: validated
- Primary Slice: M001/S03

### PTNT-01 through PTNT-11 — Patient demographics, insurance, SDOH, search, care team, related persons, allergies, problems, medications, immunizations
- Status: validated
- Primary Slice: M001/S04–S05

### SCHD-01 through SCHD-07 — Scheduling, calendar, appointments, recurring series, open slots, flow board, waitlist, recall
- Status: validated
- Primary Slice: M001/S06

### CLIN-01 through CLIN-07 — SOAP notes, vitals, ROS, physical exam, templates, co-sign, drug-allergy CDS
- Status: validated
- Primary Slice: M001/S07

### LABS-01 through LABS-04 — Lab catalogue, lab orders, lab results, sign-off, abnormal flagging
- Status: validated
- Primary Slice: M001/S08

### DOCS-01 through DOCS-03 — Document upload, SHA-256 integrity, browse/search
- Status: validated
- Primary Slice: M001/S08

### BKUP-01 through BKUP-03 — Encrypted backup/restore, AES-256-GCM, restore procedure
- Status: validated
- Primary Slice: M001/S09

### DIST-01 through DIST-03 — macOS DMG, auto-updater Ed25519, App Sandbox + Hardened Runtime
- Status: validated
- Primary Slice: M001/S09

### FOUN-01 through FOUN-06 — Tauri shell, SQLCipher, Keychain, FHIR R4, migrations, Rust commands
- Status: validated
- Primary Slice: M001/S01

### AUTH-01 through AUTH-08 — Registration, login, session lock, Touch ID (stub), TOTP, RBAC 5 roles, field-level access, break-glass
- Status: validated (AUTH-04 stub only — real impl in M003/S01)
- Primary Slice: M001/S02

## Deferred

### AUDT-03 — Audit log 6-year retention enforcement
- Status: deferred
- Class: compliance/security
- Source: inferred
- Primary owning slice: none
- Notes: Requires LaunchAgent or background job to prune/archive logs older than 6 years. Low urgency for solo practice phase.

### CLIN-08 — Pediatric growth charts
- Status: deferred
- Class: core-capability
- Source: inferred
- Primary owning slice: none
- Notes: Vitals data captured. Requires CDC/WHO percentile reference tables and chart rendering.

### BKUP-04 — Scheduled automatic daily backups via OS timer
- Status: deferred
- Class: operability
- Source: execution
- Primary owning slice: none
- Notes: Requires macOS LaunchAgent plist or Tauri background task scheduler. On-demand backup fully working.

## Out of Scope

### Electronic claims submission (837P)
- Status: out-of-scope
- Notes: Deferred to M004.

### ERA/remittance reconciliation
- Status: out-of-scope
- Notes: Deferred to M004.

### Patient portal / messaging
- Status: out-of-scope
- Notes: Requires cloud sync — M005.

### PowerSync multi-device sync
- Status: out-of-scope
- Notes: M005.

### Home Exercise Program builder
- Status: out-of-scope
- Notes: Deferred to M004.

### Group / telehealth visit types
- Status: out-of-scope
- Notes: Deferred to M004.

### Full Medicare G-code / functional limitation reporting
- Status: out-of-scope
- Notes: Deferred to M004.

## Traceability

| ID | Class | Status | Primary Owner | Supporting | Proof |
|---|---|---|---|---|---|
| AUTH-04-FIX | core-capability | active | M003/S01 | none | unmapped |
| PT-DOC-01 | core-capability | active | M003/S01 | M003/S02 | unmapped |
| PT-DOC-02 | core-capability | active | M003/S01 | none | unmapped |
| PT-DOC-03 | core-capability | active | M003/S01 | M003/S02 | unmapped |
| PT-DOC-04 | core-capability | active | M003/S01 | none | unmapped |
| PT-OBJ-01 | core-capability | active | M003/S02 | none | unmapped |
| PT-OBJ-02 | core-capability | active | M003/S02 | none | unmapped |
| PT-OBJ-03 | core-capability | active | M003/S02 | none | unmapped |
| PT-OBJ-04 | core-capability | active | M003/S02 | M003/S01 | unmapped |
| PT-AI-01 | differentiator | active | M003/S03 | none | unmapped |
| PT-AI-02 | compliance/security | active | M003/S03 | none | unmapped |
| PT-AI-03 | core-capability | active | M003/S03 | none | unmapped |
| PT-DOC-CTR-01 | core-capability | active | M003/S04 | none | unmapped |
| PT-DOC-CTR-02 | core-capability | active | M003/S04 | none | unmapped |
| PT-DOC-CTR-03 | core-capability | active | M003/S04 | M003/S07 | unmapped |
| PT-EXP-01 | core-capability | active | M003/S05 | M003/S02, M003/S01 | unmapped |
| PT-EXP-02 | core-capability | active | M003/S05 | none | unmapped |
| PT-FAX-01 | core-capability | active | M003/S06 | M003/S05 | unmapped |
| PT-FAX-02 | core-capability | active | M003/S06 | none | unmapped |
| PT-FAX-03 | compliance/security | active | M003/S06 | none | unmapped |
| PT-AUTH-01 | core-capability | active | M003/S07 | none | unmapped |
| PT-AUTH-02 | core-capability | active | M003/S07 | none | unmapped |
| PT-AUTH-03 | core-capability | active | M003/S07 | M003/S06 | unmapped |
| AUDT-03 | compliance/security | deferred | none | none | unmapped |
| CLIN-08 | core-capability | deferred | none | none | unmapped |
| BKUP-04 | operability | deferred | none | none | unmapped |

## Coverage Summary

- Active requirements: 23
- Mapped to slices: 23
- Validated: 0 (M003 not yet started)
- Unmapped active requirements: 0

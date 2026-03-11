# Requirements: MedArc — AI-Powered Desktop EMR

**Defined:** 2026-03-10
**Core Value:** Physicians can document patient encounters through voice capture that automatically generates structured SOAP notes, reducing documentation time by 30-41% while keeping all PHI local and encrypted on their device.

## v1 Requirements

Requirements for initial release (Phase 1 MVP). Core EMR functionality without AI, locally installed on macOS.

### Foundation & Security

- [ ] **FOUN-01**: Application launches as a macOS desktop app via Tauri 2.x shell with WKWebView rendering React frontend
- [ ] **FOUN-02**: All data stored in SQLCipher-encrypted SQLite database with AES-256-CBC and per-page HMAC tamper detection
- [ ] **FOUN-03**: Database encryption key stored exclusively in macOS Keychain (Secure Enclave-backed on Apple Silicon)
- [ ] **FOUN-04**: Data modeled as FHIR R4 resources stored as JSON columns with indexed lookup tables for frequently queried fields
- [ ] **FOUN-05**: Alembic schema migrations with render_as_batch=True for SQLite compatibility
- [ ] **FOUN-06**: Rust-native Tauri commands handle all database CRUD and file system operations (no Python dependency for core EMR)

### Authentication & Access Control

- [ ] **AUTH-01**: User can create account with unique user ID (no shared accounts per HIPAA)
- [ ] **AUTH-02**: User can log in with password hashed via bcrypt/Argon2 (minimum 12 characters)
- [ ] **AUTH-03**: User session auto-locks after 10-15 minutes of inactivity (configurable)
- [ ] **AUTH-04**: User can authenticate via Touch ID on supported hardware
- [ ] **AUTH-05**: User can enable TOTP-based MFA for their account
- [ ] **AUTH-06**: System enforces RBAC with 5 roles: System Admin, Provider, Nurse/MA, Billing Staff, Front Desk
- [ ] **AUTH-07**: Each role has field-level access control per RBAC matrix (e.g., Nurse can update vitals but not prescriptions)
- [ ] **AUTH-08**: Emergency "break-glass" access is time-limited, tightly scoped, and fully logged

### Audit Logging

- [ ] **AUDT-01**: Every ePHI access is logged with timestamp (UTC), user ID, action type, patient/record identifier, device identifier, and success/failure
- [ ] **AUDT-02**: Audit logs use tamper-proof storage with cryptographic hash chains (each entry includes hash of previous entry)
- [ ] **AUDT-03**: Audit logs are retained for minimum 6 years
- [ ] **AUDT-04**: Provider can view their own audit log entries
- [ ] **AUDT-05**: System Admin can view all audit log entries

### Patient Management

- [ ] **PTNT-01**: User can create a patient record with demographics (name, DOB, sex/gender, contact info, patient photo)
- [ ] **PTNT-02**: User can add insurance information (primary, secondary, tertiary) to a patient record
- [ ] **PTNT-03**: User can add employer data and social determinants of health to a patient record
- [ ] **PTNT-04**: User can assign clinical identifiers (primary provider, MRN) to a patient record
- [ ] **PTNT-05**: User can search patients by name, demographics, MRN, and procedure history with sub-second results
- [ ] **PTNT-06**: User can manage Related Persons for care team relationships
- [ ] **PTNT-07**: User can assign care team members with roles via Care Team Widget
- [ ] **PTNT-08**: User can track patient allergies with drug, food, environmental categories, severity, and reaction type (FHIR AllergyIntolerance)
- [ ] **PTNT-09**: User can maintain active problem list with ICD-10 coded diagnoses (active/inactive/resolved status)
- [ ] **PTNT-10**: User can maintain medication list (active, discontinued, historical) linked to RxNorm codes
- [ ] **PTNT-11**: User can record immunization history with CVX codes, lot numbers, administration dates

### Scheduling

- [ ] **SCHD-01**: User can view multi-provider calendar in day, week, and month views
- [ ] **SCHD-02**: User can create appointments with color-coded categories and configurable durations (5-60 min)
- [ ] **SCHD-03**: User can schedule recurring appointments (weekly, biweekly, monthly)
- [ ] **SCHD-04**: User can search for open appointment slots filtered by provider, type, and date range
- [ ] **SCHD-05**: User can view Patient Flow Board showing real-time clinic status (checked in, roomed, with provider, checkout)
- [ ] **SCHD-06**: User can manage a waitlist for cancelled appointment slots
- [ ] **SCHD-07**: User can view Recall Board for overdue patient follow-ups

### Clinical Documentation

- [ ] **CLIN-01**: User can create structured SOAP notes (Subjective, Objective, Assessment, Plan) per encounter
- [ ] **CLIN-02**: User can record vitals (BP, HR, RR, Temp, SpO2, Weight, Height, BMI auto-calc, pain scale) with flowsheet trending
- [ ] **CLIN-03**: User can complete Review of Systems forms across 14 organ systems (positive/negative/not reviewed)
- [ ] **CLIN-04**: User can document physical exam findings using system-based templates (HEENT, CV, Pulm, etc.)
- [ ] **CLIN-05**: System ships with 10-15 pre-built clinical templates (general, cardiology, pediatrics, OB/GYN, psychiatry, orthopedics, dermatology)
- [ ] **CLIN-06**: Supervising physician can co-sign encounter notes from NP/PA mid-level providers
- [ ] **CLIN-07**: System displays passive clinical decision alerts for drug-allergy interactions based on patient allergy and medication lists
- [ ] **CLIN-08**: User can view pediatric growth charts from vitals data

### Lab Results

- [ ] **LABS-01**: User can manually enter lab results with LOINC code mapping
- [ ] **LABS-02**: User can configure a laboratory procedure catalogue
- [ ] **LABS-03**: User can create lab orders with provider signature
- [ ] **LABS-04**: Provider can review, sign, and act on lab results with abnormal flagging

### Document Management

- [ ] **DOCS-01**: User can upload documents (PDF, images) up to 64 MB with categorization
- [ ] **DOCS-02**: System validates document integrity via SHA-1 checksums
- [ ] **DOCS-03**: User can browse and search uploaded documents per patient

### Backup & Distribution

- [ ] **BKUP-01**: System performs automated daily encrypted backups to external storage
- [ ] **BKUP-02**: Backups are encrypted with AES-256 before leaving the machine
- [ ] **BKUP-03**: User can restore from backup with documented restore procedures
- [ ] **DIST-01**: Application distributed as code-signed and notarized macOS DMG
- [ ] **DIST-02**: Application auto-updates via tauri-plugin-updater with Ed25519 signature verification
- [ ] **DIST-03**: Application uses Hardened Runtime with App Sandbox for macOS security

## v2 Requirements

Deferred to Phase 2 (Feature Parity, Months 7-10). Tracked but not in current roadmap.

### Billing & Revenue Cycle

- **BILL-01**: User can enter CPT, HCPCS, ICD-10, SNOMED codes per encounter via fee sheet
- **BILL-02**: User can manage multiple fee schedules with modifier support
- **BILL-03**: System generates electronic claims in ANSI X12 837P (5010 standard) format
- **BILL-04**: System processes ERA in 835 format with automated payment posting
- **BILL-05**: User can track Accounts Receivable with aging reports (30/60/90/120 days)
- **BILL-06**: User can generate patient statements
- **BILL-07**: User can verify insurance eligibility via X12 270/271

### E-Prescribing

- **ERXP-01**: User can search drug database by name, class, or indication (RxNorm)
- **ERXP-02**: Provider can transmit prescriptions electronically via Weno Exchange
- **ERXP-03**: Provider can prescribe controlled substances via EPCS (DEA-compliant)
- **ERXP-04**: System checks drug interactions via RxNav-in-a-Box with severity ratings
- **ERXP-05**: System alerts on duplicate therapy
- **ERXP-06**: User can perform medication reconciliation at transitions of care

### Lab Integration (Electronic)

- **LABE-01**: System exchanges HL7 v2 messages (ORU^R01 for results, ORM^O01 for orders)
- **LABE-02**: System auto-processes incoming electronic lab results

### Reporting (Full Suite)

- **REPT-01**: User can generate clinical reports (patient lists, encounters, prescriptions, immunizations)
- **REPT-02**: User can generate financial reports (collections, revenue, payer mix, provider productivity)
- **REPT-03**: System calculates and reports CQM/eCQM measures for MIPS

### Advanced Features

- **ADVN-01**: User can build custom clinical forms via form editor
- **ADVN-02**: User can manage referrals with structured tracking
- **ADVN-03**: System sends appointment reminders via SMS and email
- **ADVN-04**: Patient portal with secure messaging and lab access

## v3 Requirements (Phase 3 — AI Enhancement)

- **AINL-01**: Provider can capture ambient voice and auto-generate SOAP notes via whisper.cpp + NLP + LLaMA 3.1 8B
- **AINL-02**: System suggests ICD-10/CPT codes via FAISS vector search with human review
- **AINL-03**: System provides AI diagnostic decision support via RAG pipeline
- **AINL-04**: System predicts no-show probability and optimizes scheduling slots
- **AINL-05**: System assembles pre-visit context (pre-charting) from patient history
- **AINL-06**: Cloud AI fallback via AWS Bedrock (Claude) for complex cases with de-identified data

## v4 Requirements (Phase 4 — Cloud Migration)

- **CLOD-01**: System syncs data to AWS RDS PostgreSQL via PowerSync
- **CLOD-02**: System supports offline-first operation with conflict resolution
- **CLOD-03**: System supports multi-device access after cloud migration
- **CLOD-04**: Mobile companion app scoped and built

## Out of Scope

Explicitly excluded. Documented to prevent scope creep.

| Feature | Reason |
|---------|--------|
| Patient portal (v1) | Doubles security attack surface; physicians adopt based on clinical workflow, not patient features |
| ONC certification (v1) | $50-100K+ cost, 6-12 month process; pursue when revenue supports it |
| Real-time multi-user collaboration | CRDT for clinical notes is unsolved; encounter locking + co-signing is safer for 1-5 providers |
| Built-in telemedicine/video | WebRTC infrastructure is a distraction; integrate with existing platforms (Doxy.me, Zoom Healthcare) |
| Mobile app (v1) | Doubles dev surface; Tauri supports future iOS/Android; desktop-first |
| Windows/Linux (v1) | Triples testing; loses macOS-specific advantages (CoreML, Secure Enclave, Keychain, Touch ID) |
| Automated claim submission (no review) | AI coding has 33.9% exact match; auto-submitting without review = denial + fraud liability |
| Custom form builder (v1) | Deceptively complex; ship with 10-15 pre-built templates, add builder in Phase 2 |
| Integrated fax server | Third-party fax services handle this better; manual document upload for v1 |
| NL query of patient data | NL-to-SQL unreliable for clinical data; use structured report builder |

## Traceability

Which phases cover which requirements. Updated during roadmap creation.

| Requirement | Phase | Status |
|-------------|-------|--------|
| FOUN-01 | Phase 1 | Pending |
| FOUN-02 | Phase 1 | Pending |
| FOUN-03 | Phase 1 | Pending |
| FOUN-04 | Phase 1 | Pending |
| FOUN-05 | Phase 1 | Pending |
| FOUN-06 | Phase 1 | Pending |
| AUTH-01 | Phase 1 | Pending |
| AUTH-02 | Phase 1 | Pending |
| AUTH-03 | Phase 1 | Pending |
| AUTH-04 | Phase 1 | Pending |
| AUTH-05 | Phase 1 | Pending |
| AUTH-06 | Phase 1 | Pending |
| AUTH-07 | Phase 1 | Pending |
| AUTH-08 | Phase 1 | Pending |
| AUDT-01 | Phase 1 | Pending |
| AUDT-02 | Phase 1 | Pending |
| AUDT-03 | Phase 1 | Pending |
| AUDT-04 | Phase 1 | Pending |
| AUDT-05 | Phase 1 | Pending |
| PTNT-01 | Phase 1 | Pending |
| PTNT-02 | Phase 1 | Pending |
| PTNT-03 | Phase 1 | Pending |
| PTNT-04 | Phase 1 | Pending |
| PTNT-05 | Phase 1 | Pending |
| PTNT-06 | Phase 1 | Pending |
| PTNT-07 | Phase 1 | Pending |
| PTNT-08 | Phase 1 | Pending |
| PTNT-09 | Phase 1 | Pending |
| PTNT-10 | Phase 1 | Pending |
| PTNT-11 | Phase 1 | Pending |
| SCHD-01 | Phase 1 | Pending |
| SCHD-02 | Phase 1 | Pending |
| SCHD-03 | Phase 1 | Pending |
| SCHD-04 | Phase 1 | Pending |
| SCHD-05 | Phase 1 | Pending |
| SCHD-06 | Phase 1 | Pending |
| SCHD-07 | Phase 1 | Pending |
| CLIN-01 | Phase 1 | Pending |
| CLIN-02 | Phase 1 | Pending |
| CLIN-03 | Phase 1 | Pending |
| CLIN-04 | Phase 1 | Pending |
| CLIN-05 | Phase 1 | Pending |
| CLIN-06 | Phase 1 | Pending |
| CLIN-07 | Phase 1 | Pending |
| CLIN-08 | Phase 1 | Pending |
| LABS-01 | Phase 1 | Pending |
| LABS-02 | Phase 1 | Pending |
| LABS-03 | Phase 1 | Pending |
| LABS-04 | Phase 1 | Pending |
| DOCS-01 | Phase 1 | Pending |
| DOCS-02 | Phase 1 | Pending |
| DOCS-03 | Phase 1 | Pending |
| BKUP-01 | Phase 1 | Pending |
| BKUP-02 | Phase 1 | Pending |
| BKUP-03 | Phase 1 | Pending |
| DIST-01 | Phase 1 | Pending |
| DIST-02 | Phase 1 | Pending |
| DIST-03 | Phase 1 | Pending |

**Coverage:**
- v1 requirements: 55 total
- Mapped to phases: 55
- Unmapped: 0 ✓

---
*Requirements defined: 2026-03-10*
*Last updated: 2026-03-10 after initial definition*

# Roadmap: MedArc Phase 1 MVP

## Overview

MedArc Phase 1 delivers a fully functional, HIPAA-compliant desktop EMR that a solo practitioner can use for daily patient care -- without AI, without cloud, without billing. The build order follows strict dependencies: encrypted database and FHIR schema first, then authentication and audit logging (the HIPAA gate), then patient data, scheduling, clinical documentation, labs, documents, and finally backup/distribution for release. Each phase delivers a coherent, verifiable capability that unlocks the next.

## Phases

**Phase Numbering:**
- Integer phases (1, 2, 3): Planned milestone work
- Decimal phases (2.1, 2.2): Urgent insertions (marked with INSERTED)

Decimal phases appear between their surrounding integers in numeric order.

- [x] **Phase 1: Desktop Shell & Encrypted Database** - Tauri app boots with SQLCipher-encrypted FHIR-modeled database and Rust CRUD layer (completed 2026-03-11)
- [ ] **Phase 2: Authentication & Access Control** - Users can securely log in with RBAC enforcing role-based field-level permissions
- [ ] **Phase 3: Audit Logging** - Every ePHI access is logged with tamper-proof cryptographic hash chains
- [ ] **Phase 4: Patient Demographics & Care Teams** - Users can create, search, and manage patient records with insurance and care team data
- [ ] **Phase 5: Clinical Patient Data** - Users can manage allergies, problem lists, medications, and immunization histories per patient
- [ ] **Phase 6: Scheduling** - Users can manage multi-provider calendars, appointments, patient flow, and recall boards
- [ ] **Phase 7: Clinical Documentation** - Users can create SOAP notes, record vitals, complete ROS/PE forms, and use specialty templates
- [ ] **Phase 8: Lab Results & Document Management** - Users can order labs, review results, and upload/manage patient documents
- [ ] **Phase 9: Backup, Distribution & Release** - Application ships as a signed macOS DMG with encrypted backups and auto-updates

## Phase Details

### Phase 1: Desktop Shell & Encrypted Database
**Goal**: Application launches as a macOS desktop app with an encrypted, FHIR-modeled database and a Rust command layer that handles all data operations
**Depends on**: Nothing (first phase)
**Requirements**: FOUN-01, FOUN-02, FOUN-03, FOUN-04, FOUN-05, FOUN-06
**Success Criteria** (what must be TRUE):
  1. User can launch the Tauri desktop application on macOS and see the React frontend rendered in WKWebView
  2. All persisted data is stored in a SQLCipher-encrypted SQLite database with AES-256-CBC and per-page HMAC tamper detection
  3. Database encryption key is retrieved from macOS Keychain (Secure Enclave-backed on Apple Silicon) -- never hardcoded or stored in config files
  4. FHIR R4 resources are stored as JSON columns with indexed lookup tables, and schema migrations run correctly against SQLite
  5. All database CRUD operations execute through Rust-native Tauri commands with no Python dependency
**Plans:** 3/3 plans complete

Plans:
- [x] 01-01-PLAN.md -- Scaffold Tauri 2.x app with SQLCipher database and macOS Keychain key management
- [x] 01-02-PLAN.md -- FHIR R4 resource schema, Rust CRUD commands, and frontend integration
- [x] 01-03-PLAN.md -- Frontend component polish and end-to-end requirement verification

### Phase 2: Authentication & Access Control
**Goal**: Users can securely create accounts, log in with multiple authentication methods, and have their access restricted by role
**Depends on**: Phase 1
**Requirements**: AUTH-01, AUTH-02, AUTH-03, AUTH-04, AUTH-05, AUTH-06, AUTH-07, AUTH-08
**Success Criteria** (what must be TRUE):
  1. User can create an account with a unique user ID and log in with a password (minimum 12 characters, hashed with bcrypt/Argon2)
  2. User session auto-locks after configurable inactivity (10-15 min) and user can authenticate via Touch ID on supported hardware
  3. User can enable TOTP-based MFA and is prompted for the second factor on login
  4. System enforces 5 roles (System Admin, Provider, Nurse/MA, Billing Staff, Front Desk) with field-level access control per the RBAC matrix
  5. Emergency break-glass access grants time-limited, scoped permissions and is fully logged
**Plans:** 5 plans

Plans:
- [ ] 02-01-PLAN.md -- Auth core backend: user accounts, Argon2id password hashing, session state machine
- [ ] 02-02-PLAN.md -- RBAC engine: 5-role permission matrix, field-level filtering, break-glass access
- [ ] 02-03-PLAN.md -- MFA and biometrics: TOTP enrollment/verification, Touch ID integration
- [ ] 02-04-PLAN.md -- Frontend auth UI: login, registration, lock screen, MFA components, idle timer
- [ ] 02-05-PLAN.md -- Integration wiring and end-to-end AUTH requirement verification

### Phase 3: Audit Logging
**Goal**: Every access to electronic protected health information is logged with tamper-proof integrity, viewable by authorized users
**Depends on**: Phase 2
**Requirements**: AUDT-01, AUDT-02, AUDT-03, AUDT-04, AUDT-05
**Success Criteria** (what must be TRUE):
  1. Every ePHI access generates a log entry with timestamp (UTC), user ID, action type, patient/record identifier, device identifier, and success/failure status
  2. Audit log entries form a cryptographic hash chain where each entry includes the hash of the previous entry, preventing undetectable tampering
  3. Audit logs are retained for a minimum of 6 years and cannot be deleted or modified by any user role
  4. Provider can view their own audit log entries; System Admin can view all audit log entries
**Plans**: TBD

Plans:
- [ ] 03-01: TBD
- [ ] 03-02: TBD

### Phase 4: Patient Demographics & Care Teams
**Goal**: Users can create, search, and manage patient records with demographics, insurance, employer data, clinical identifiers, and care team assignments
**Depends on**: Phase 3
**Requirements**: PTNT-01, PTNT-02, PTNT-03, PTNT-04, PTNT-05, PTNT-06, PTNT-07
**Success Criteria** (what must be TRUE):
  1. User can create a patient record with full demographics (name, DOB, sex/gender, contact info, photo) and the record persists as a FHIR Patient resource
  2. User can add primary/secondary/tertiary insurance, employer data, and social determinants of health to a patient record
  3. User can assign clinical identifiers (primary provider, MRN) and manage care team members with roles via the Care Team Widget
  4. User can search patients by name, demographics, MRN, and procedure history and get results in under one second
  5. User can manage Related Persons for care team relationships
**Plans**: TBD

Plans:
- [ ] 04-01: TBD
- [ ] 04-02: TBD
- [ ] 04-03: TBD

### Phase 5: Clinical Patient Data
**Goal**: Users can manage the core clinical data lists (allergies, problems, medications, immunizations) that form the safety backbone of the patient chart
**Depends on**: Phase 4
**Requirements**: PTNT-08, PTNT-09, PTNT-10, PTNT-11
**Success Criteria** (what must be TRUE):
  1. User can track patient allergies categorized by drug, food, and environmental type with severity and reaction details (stored as FHIR AllergyIntolerance resources)
  2. User can maintain an active problem list with ICD-10 coded diagnoses showing active, inactive, and resolved status
  3. User can maintain a medication list (active, discontinued, historical) linked to RxNorm codes
  4. User can record immunization history with CVX codes, lot numbers, and administration dates
**Plans**: TBD

Plans:
- [ ] 05-01: TBD
- [ ] 05-02: TBD

### Phase 6: Scheduling
**Goal**: Users can manage the full appointment lifecycle from scheduling through patient flow tracking and follow-up recall
**Depends on**: Phase 4
**Requirements**: SCHD-01, SCHD-02, SCHD-03, SCHD-04, SCHD-05, SCHD-06, SCHD-07
**Success Criteria** (what must be TRUE):
  1. User can view a multi-provider calendar in day, week, and month views with color-coded appointment categories
  2. User can create appointments with configurable durations (5-60 min), schedule recurring appointments (weekly, biweekly, monthly), and search for open slots by provider/type/date
  3. User can view the Patient Flow Board showing real-time clinic status (checked in, roomed, with provider, checkout)
  4. User can manage a waitlist for cancelled slots and view the Recall Board for overdue patient follow-ups
**Plans**: TBD

Plans:
- [ ] 06-01: TBD
- [ ] 06-02: TBD
- [ ] 06-03: TBD

### Phase 7: Clinical Documentation
**Goal**: Users can document patient encounters with structured SOAP notes, vitals, review of systems, physical exam findings, and specialty templates
**Depends on**: Phase 5
**Requirements**: CLIN-01, CLIN-02, CLIN-03, CLIN-04, CLIN-05, CLIN-06, CLIN-07, CLIN-08
**Success Criteria** (what must be TRUE):
  1. User can create structured SOAP notes (Subjective, Objective, Assessment, Plan) per encounter
  2. User can record vitals (BP, HR, RR, Temp, SpO2, Weight, Height, BMI auto-calc, pain scale) with flowsheet trending over time
  3. User can complete Review of Systems forms across 14 organ systems and document physical exam findings using system-based templates
  4. System ships with 10-15 pre-built specialty templates (general, cardiology, pediatrics, OB/GYN, psychiatry, orthopedics, dermatology) and displays passive drug-allergy interaction alerts
  5. Supervising physician can co-sign encounter notes from NP/PA mid-level providers, and users can view pediatric growth charts from vitals data
**Plans**: TBD

Plans:
- [ ] 07-01: TBD
- [ ] 07-02: TBD
- [ ] 07-03: TBD

### Phase 8: Lab Results & Document Management
**Goal**: Users can manage lab orders and results with LOINC coding, and upload/browse/search patient documents with integrity verification
**Depends on**: Phase 7
**Requirements**: LABS-01, LABS-02, LABS-03, LABS-04, DOCS-01, DOCS-02, DOCS-03
**Success Criteria** (what must be TRUE):
  1. User can manually enter lab results mapped to LOINC codes and configure a laboratory procedure catalogue
  2. User can create lab orders with provider signature and review/sign/act on results with abnormal values flagged
  3. User can upload documents (PDF, images) up to 64 MB with categorization, and the system validates integrity via SHA-1 checksums
  4. User can browse and search uploaded documents per patient
**Plans**: TBD

Plans:
- [ ] 08-01: TBD
- [ ] 08-02: TBD

### Phase 9: Backup, Distribution & Release
**Goal**: Application is production-ready with encrypted backups, code-signed macOS distribution, and automatic updates
**Depends on**: Phase 8
**Requirements**: BKUP-01, BKUP-02, BKUP-03, DIST-01, DIST-02, DIST-03
**Success Criteria** (what must be TRUE):
  1. System performs automated daily encrypted backups (AES-256) to external storage and user can restore from backup using documented procedures
  2. Application is distributed as a code-signed and notarized macOS DMG that installs cleanly
  3. Application auto-updates via tauri-plugin-updater with Ed25519 signature verification
  4. Application runs with Hardened Runtime and App Sandbox for macOS security
**Plans**: TBD

Plans:
- [ ] 09-01: TBD
- [ ] 09-02: TBD

## Progress

**Execution Order:**
Phases execute in numeric order: 1 -> 2 -> 3 -> 4 -> 5 -> 6 -> 7 -> 8 -> 9

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 1. Desktop Shell & Encrypted Database | 3/3 | Complete   | 2026-03-11 |
| 2. Authentication & Access Control | 0/5 | Not started | - |
| 3. Audit Logging | 0/2 | Not started | - |
| 4. Patient Demographics & Care Teams | 0/3 | Not started | - |
| 5. Clinical Patient Data | 0/2 | Not started | - |
| 6. Scheduling | 0/3 | Not started | - |
| 7. Clinical Documentation | 0/3 | Not started | - |
| 8. Lab Results & Document Management | 0/2 | Not started | - |
| 9. Backup, Distribution & Release | 0/2 | Not started | - |

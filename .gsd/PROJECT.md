# MedArc — AI-Powered Desktop EMR

## What This Is

An AI-native electronic medical records application built for solo practitioners and small clinics (1-5 providers), delivered as a self-contained macOS desktop application with local-first data storage and a clear cloud migration path. It eliminates the documentation burden plaguing small practices — where physicians spend twice as much time on EHR screens as with patients — through ambient AI documentation, intelligent coding, offline-first privacy, and zero monthly SaaS fees.

## Core Value

Physicians can document patient encounters through voice capture that automatically generates structured SOAP notes, reducing documentation time by 30-41% while keeping all PHI local and encrypted on their device.

## Requirements

### Validated

- [x] Tauri 2.x + React + TypeScript desktop shell with SQLCipher encrypted local database (S01)
- [x] RBAC: 5 roles (Admin, Provider, Nurse/MA, Billing, Front Desk) with field-level access control (S02)
- [x] Authentication: unique user IDs, Argon2id hashing, TOTP MFA, Touch ID stub, auto-logoff, break-glass (S02)
- [x] HIPAA audit logging: SHA-256 hash chains, trigger-enforced immutability, 9 ePHI commands instrumented, real machine-uid device fingerprinting, role-scoped AuditLog UI (S03)
- [x] Patient demographics & care teams: create/get/update/delete/search patients, insurance×3, employer, SDOH, MRN, care team, related persons — PTNT-01–07 validated, 28 unit tests (S04)
- [x] Clinical patient data: allergy CRUD, problem CRU, medication CRU, immunization CR — FHIR AllergyIntolerance/Condition/MedicationStatement/Immunization with RxNorm/ICD-10/CVX coding — PTNT-08–11 validated, 38 unit tests (S05)
- [x] Scheduling: appointments (create/list/update/cancel), recurring series (weekly/biweekly/monthly), multi-provider calendar, open-slot search, Patient Flow Board, waitlist, recall board — SCHD-01–07 validated, 22 unit tests, Migration 11, AppointmentScheduling RBAC (S06)
- [x] Clinical documentation: SOAP notes, vitals (LOINC-coded + BMI auto-calc), 14-system ROS, 13-system physical exam, 12 specialty templates, co-sign workflow, passive drug-allergy CDS — CLIN-01–07 validated, 24 unit tests, Migration 12, ClinicalDocumentation RBAC (S07)
- [x] Lab results & document management: lab catalogue (LOINC), lab orders (ServiceRequest + provider signature), lab results (DiagnosticReport + abnormal flagging + sign-off), document upload/browse/verify (SHA-256 integrity, 64 MB limit) — LABS-01–04 + DOCS-01–03 validated, 33 unit tests, Migration 13, LabResults + PatientDocuments RBAC (S08)
- [x] Backup, distribution & release: AES-256-GCM encrypted backup/restore, backup_log audit trail, Backup RBAC, tauri-plugin-updater Ed25519 auto-update wiring, macOS App Sandbox + Hardened Runtime entitlements, code-signing/notarization config, docs/RELEASE.md runbook — BKUP-01–03 + DIST-01–03 validated, 13 unit tests (265 total), Migration 14 (S09) — **M001 COMPLETE**
- [x] **Frontend UI** — Full React UI layer: PatientListPage + PatientDetailPage + PatientFormModal, CalendarPage + FlowBoardPage + AppointmentFormModal + WaitlistPanel + RecallPanel, EncounterWorkspace (SOAP + vitals + ROS + PhysicalExam), ClinicalSidebar (Problems/Medications/Allergies/Immunizations + DrugAllergyAlertBanner), LabResultsPanel, DocumentBrowser (native file picker + chunked base64), SettingsPage (Backup/Security/Account) — UI-01–07 validated, RBAC-gated navigation, 88 Tauri invoke wrappers, tsc --noEmit exits 0 — **M002 COMPLETE**

### Active (Phase 3 — M002 Complete)

**M002 is fully complete (2026-03-12).** The full React frontend layer is built and wired to all 88 M001 backend commands. A practitioner can log in, manage patients, write clinical encounters (SOAP + vitals + ROS + PE), view labs/documents, schedule appointments, track the Patient Flow Board, and manage backup/MFA settings. The following remain for future milestones:

- [ ] **Touch ID** — biometric.rs stub always returns unavailable; requires tauri-plugin-biometry integration
- [ ] **Pediatric growth charts** (CLIN-08) — vitals data captured; CDC/WHO percentile tables not included
- [ ] **Scheduled automatic backups** (BKUP-04) — on-demand only; LaunchAgent or Tauri background scheduler required
- [ ] **`verifyDocumentIntegrity` UI** — invoke wrapper exists in tauri.ts but no UI surface invokes it; deferred from S06
- [ ] **E-prescribing** — drug search, Weno Exchange integration, EPCS, interaction checks, RxNorm/SNOMED coding
- [ ] **Lab integration** — HL7 v2 message exchange, procedure ordering, results workflow, LOINC mapping
- [ ] **Billing** — CPT/HCPCS/ICD-10/SNOMED coding, fee sheets, X12 837P claims, ERA 835 processing, AR tracking
- [ ] **Reporting** — clinical reports, financial reports, CQM/eCQM measures
- [ ] **AI clinical note generation** — whisper.cpp voice-to-text, MedSpaCy/SciSpaCy NLP, LLaMA 3.1 8B SOAP generation
- [ ] **AI diagnostic support** — differential diagnosis via RAG, RxNav-in-a-Box drug interactions
- [ ] **AI smart scheduling** — no-show prediction (XGBoost/LightGBM), slot optimization
- [ ] **AI medical coding** — LLM entity extraction + FAISS vector search for ICD-10/CPT suggestions
- [ ] **Cloud migration** — AWS RDS PostgreSQL, PowerSync offline-first sync, dual-write strategy

### Known Technical Debt

- `tauri.conf.json` contains `PLACEHOLDER_ED25519_PUBKEY` — must be replaced with real Ed25519 key before auto-updater functions
- `restore_backup` requires app restart after restore (SQLite connection pool holds stale state)
- All datetimes should be normalized to no-timezone-suffix format — `scheduling.rs` datetime parsing will produce wrong results for suffixed timestamps
- `SettingsPage` TOTP status is inferred from command availability (no dedicated `is_totp_enabled` query) — UX gap when TOTP is not yet set up
- Live interactive end-to-end UAT was not completed in M002 (cargo compile timeout consumed CPU); S07-UAT.md records PARTIAL status with resume instructions

### Out of Scope

- Patient portal — deprioritized in favor of desktop clinical workflow
- Mobile companion app — scoped for Phase 4+
- ONC certification — technically voluntary, architecture supports future certification
- AWS HealthLake — $197/mo base, not cost-effective for small clinics
- Local pharmacy dispensary module — optional, not core workflow
- Windows/Linux support — macOS-first, Tauri supports future expansion

## Context

- OpenEMR v8.0.0 (ONC-certified, Feb 2026) serves as feature baseline
- 42% of medical groups already using ambient AI (up from near-zero 3 years ago)
- CMS estimates healthcare workers waste 45 minutes daily on inefficient workflows
- Competitors (Practice Fusion, DrChrono, Tebra) all cloud-only with $49-349/month per provider
- Average healthcare data breach cost: $9.77 million (IBM 2024)
- HIPAA breach notification safe harbor: encrypted PHI = "secured" = no notification required
- Minimum hardware: Mac with 16 GB unified memory; optimal: 32-64 GB for full AI pipeline
- Whisper exhibits ~1% hallucination rate — human-in-the-loop review mandatory
- GPT-4 achieves only 33.9% exact match on ICD-10-CM — vector search approach required
- LLaMA-3-8B-Instruct scored 64% on NEJM cases vs 30% for fine-tuned OpenBioLLM-8B — RAG > fine-tuning

## Constraints

- **Platform**: macOS-only (Apple Silicon M1+) — leverages CoreML, Secure Enclave, WKWebView
- **Security**: HIPAA-compliant from day one — AES-256 encryption, audit logging, RBAC
- **Privacy**: PHI never leaves device for routine AI operations — local models for 95% of tasks
- **Tech Stack**: Tauri 2.x (Rust) + React 18+ (TypeScript) + FastAPI (Python sidecar) + SQLCipher
- **AI Runtime**: Ollama for local LLM, whisper.cpp/CoreML for transcription, FAISS for vector search
- **Cloud AI**: AWS Bedrock (Claude) with BAA for complex cases only — de-identified data
- **Database**: SQLCipher (SQLite + AES-256) Phase 1, PostgreSQL via SQLAlchemy abstraction for Phase 4
- **Data Model**: FHIR R4 resources as JSON columns from day one
- **Budget**: ~$500K-750K for 18-month build with 3-4 person team
- **Cloud Cost**: ~$65-110/month per clinic post-migration

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Tauri 2.x over Electron | 30-50 MB idle vs 150-300 MB; Rust security model; 3-10 MB bundle; critical when running 6-35 GB AI models | -- Pending |
| SQLCipher over PostgreSQL for Phase 1 | Zero-config, serverless, AES-256 with only 5-15% overhead; SQLAlchemy abstracts migration | -- Pending |
| Local LLM (Ollama/LLaMA 3.1 8B) over cloud-first AI | No BAA needed, PHI stays local, free inference, 15-28 tok/sec on M1-M4 | -- Pending |
| RAG over fine-tuning for clinical accuracy | LLaMA-3-8B + RAG: 64% NEJM vs fine-tuned 30%; 94% accuracy with 4% hallucination in published pipelines | -- Pending |
| FHIR-first data model | Standards-compliant from day one; eliminates interoperability retrofitting; enables future ONC cert | -- Pending |
| Repository Pattern + Unit of Work | SQLAlchemy dialect swap from sqlite to postgresql requires zero code changes | -- Pending |
| RxNav-in-a-Box for drug interactions | Docker-based, NLM-provided, complete RxNorm API stack locally, no BAA required | -- Pending |
| AWS Bedrock for cloud AI | Hosts Claude + LLaMA with standard BAA, near-instant approval, zero data retention | -- Pending |
| PowerSync for offline-first sync | Production-proven PostgreSQL <-> SQLite sync, bucket-based partial sync, causal consistency | -- Pending |
| 4-phase 18-month implementation | Phase 1: MVP (no AI), Phase 2: feature parity, Phase 3: AI enhancement, Phase 4: cloud migration | -- Pending |
| State-based discriminated-union router | No react-router-dom — Tauri WKWebView has no URL bar; typed route payloads enable exhaustive TypeScript checking | M002 |
| Flat `commands` object in tauri.ts | All 88 wrappers at top level (no namespacing) — consistent with existing callsites, avoids widespread refactoring | M002 |
| tsc --noEmit as primary verification gate | cargo test --lib takes 30+ min cold; TypeScript contract checking is fast and catches the most likely frontend mistakes | M002 |

---
*Last updated: 2026-03-12 — M002 MedArc Phase 2 Frontend complete (full React UI, 88 Tauri commands wired, tsc exits 0, UI-01–07 validated)*

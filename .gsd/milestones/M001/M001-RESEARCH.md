# Project Research Summary

**Project:** MedArc
**Domain:** AI-powered desktop EMR for small practices (1-5 providers), macOS local-first, HIPAA-compliant
**Researched:** 2026-03-10
**Confidence:** MEDIUM

## Executive Summary

MedArc is a local-first, AI-powered desktop EMR targeting small medical practices (1-5 providers) on macOS. The expert-recommended approach for this class of application is a three-layer desktop architecture: a Rust/Tauri shell that owns all database operations and security, a React frontend for clinical UI, and a Python sidecar exclusively for AI inference (voice transcription, NLP, LLM). This separation is not optional -- it enforces a HIPAA security boundary architecturally, keeps the core EMR functional when AI components fail, and manages memory pressure on 16 GB Macs that must simultaneously run 6+ GB AI models. The stack choices in the PRD are well-considered; no major changes are recommended. SQLCipher provides HIPAA breach safe harbor through AES-256 encryption, and the FHIR-first data model future-proofs for ONC certification without requiring it at launch.

The recommended build strategy is to ship a fully functional manual EMR first (Phase 1-2) before layering AI features (Phase 3). This is counterintuitive given that AI is the primary differentiator, but research strongly supports it: every AI feature enhances an underlying manual workflow that must work correctly first, AI accuracy rates (64% NEJM for LLM, 34% exact ICD-10 match, 1% Whisper hallucination) mandate human-in-the-loop review of a working manual process, and physicians adopt EMRs based on clinical workflow quality before evaluating AI features. The manual EMR also generates the clinical data (encounter history, medication lists, scheduling patterns) that AI features need to function effectively.

The primary risks are: (1) HIPAA compliance treated as a bolt-on instead of a foundational architecture -- audit logging, RBAC, and encryption must be built in Sprint 1, not Sprint Last; (2) MedSpaCy Python compatibility uncertainty (last major release was 2023) which could delay the NLP pipeline; (3) e-prescribing and claims processing complexity being severely underestimated -- these are regulated integrations requiring months of certification, not weeks of coding; and (4) SQLCipher performance degradation at clinical data volumes (50K+ records) if indexed projections are not designed upfront. All four risks have concrete mitigations detailed in the research.

## Key Findings

### Recommended Stack

The PRD's stack is defensible and requires no major changes. Tauri 2.x over Electron is the correct call -- 30-50 MB idle RAM vs 150-300 MB is the difference between running AI models or not on 16 GB machines. Rust owns all CRUD and security; Python handles AI only. SQLAlchemy should NOT be used in Phase 1-3 (contrary to PRD suggestion) -- use rusqlite directly and introduce SQLAlchemy only in Phase 4 when PostgreSQL enters via cloud migration.

**Core technologies:**
- **Tauri 2.x + Rust:** Desktop shell, DB owner, security boundary -- 10x less memory than Electron, Rust's memory safety for medical-grade reliability
- **React 18+ / TypeScript / MUI:** Frontend UI -- largest medical component ecosystem (@medplum/react, fhir-react), FHIR type definitions available
- **FastAPI sidecar (PyInstaller):** AI inference only -- whisper.cpp, MedSpaCy/SciSpaCy, Ollama client, FAISS vector search
- **SQLCipher:** Encrypted local database -- AES-256, HIPAA breach safe harbor, macOS Keychain key storage
- **Ollama + LLaMA 3.1 8B:** Local LLM runtime -- RAG outperforms fine-tuned biomedical models (64% vs 30% on NEJM)
- **whisper.cpp / WhisperKit:** Voice transcription -- CoreML acceleration, 3x real-time on Apple Silicon

**Critical version risks (verify before locking):**
- MedSpaCy + Python 3.11/3.12 compatibility (HIGH risk -- last release 2023)
- SQLCipher Python bindings + SQLAlchemy 2.0 dialect (MEDIUM risk)
- PyInstaller + AI dependencies bundling (MEDIUM risk -- budget 3-5 days for packaging)

### Expected Features

**Must have (table stakes) -- Phase 1 MVP:**
- RBAC + authentication + audit logging (HIPAA foundation; everything depends on this)
- SQLCipher encrypted database with FHIR data model (foundation; retrofitting FHIR later is a rewrite)
- Patient demographics CRUD with search (data backbone for all clinical features)
- Allergy, medication, and problem list management (clinical safety triad)
- Multi-provider scheduling with patient flow board (core operational workflow)
- SOAP note entry with 10-15 specialty templates (single most-used EMR feature)
- Vitals, ROS, physical exam forms (required for E/M coding)
- Lab results viewer (manual entry), document management, encrypted backups
- macOS code-signed, notarized DMG with auto-updates

**Should have (competitive) -- Phase 2:**
- Billing module: fee sheets, X12 837P claims, 835 ERA processing, AR tracking
- E-prescribing via Weno Exchange (including EPCS for controlled substances)
- Drug interaction checking via RxNav-in-a-Box
- HL7 v2 lab interface for electronic results
- CQM/eCQM reporting, insurance eligibility verification

**Differentiators -- Phase 3 (the reason MedArc exists):**
- Ambient voice-to-SOAP note generation (eliminates 30-41% of documentation time)
- AI-assisted ICD-10/CPT coding (reduces claim denials from 8-12% to below 3%)
- AI diagnostic decision support via RAG
- Smart scheduling with no-show prediction (XGBoost, AUC 0.75-0.85)

**Defer (v2+):**
- Patient portal, mobile companion, telemedicine (all Phase 4+)
- ONC certification (Phase 4+ when revenue supports $50-100K cost)
- Windows/Linux support (triples testing surface, loses macOS advantages)
- Real-time multi-user collaboration (CRDTs for medical records are unsolved)

### Architecture Approach

The architecture is a desktop-native three-process system: Tauri/Rust core (DB, security, IPC), WKWebView/React frontend (UI), and a PyInstaller-compiled FastAPI sidecar (AI only). Rust is the sole database accessor -- the Python sidecar is stateless, holds no PHI, and receives all context per-request. This enforces HIPAA minimum-necessary principle architecturally. FHIR resources are stored as JSON columns with indexed projections for query performance (hybrid approach avoids both the 200-table normalization problem and the O(n) JSON scan problem).

**Major components:**
1. **Tauri Rust Core** -- Process orchestrator, sole DB owner (rusqlite + SQLCipher), Keychain integration, audit logging, RBAC enforcement at data layer
2. **React Frontend** -- Clinical UI with MUI, form management (React Hook Form + Zod), Zustand state, routes organized by clinical workflow (patients, scheduling, encounters, billing)
3. **FastAPI Sidecar** -- AI inference only: whisper.cpp transcription, MedSpaCy/SciSpaCy NER, Ollama LLM calls, FAISS vector search. Lazy-loads models to manage memory
4. **Ollama Server** -- Separate process for LLM model hosting. Sidecar is sole client
5. **SQLCipher Database** -- FHIR JSON + indexed columns, hash-chained audit log, per-page HMAC tamper detection
6. **RxNav-in-a-Box** -- Docker container for local drug interaction checking (Phase 2)

### Critical Pitfalls

1. **HIPAA as architecture, not checkbox** -- Audit logging, RBAC, and encryption must exist before any PHI-touching code. RBAC enforcement at the repository/data-access layer, not just route middleware. Encryption key in Keychain from day one, never hardcoded. Field-level encryption for 42 CFR Part 2 data in the first migration. **Phase 1, Sprint 1.**

2. **AI hallucination without safety rails** -- Whisper hallucinates ~1% of segments; LLM hallucinates 4-6%. At 20 encounters/day, providers will see multiple hallucinations daily. Mitigation: AI output always in "draft" state with visual distinction, mandatory provider review, confidence scoring with threshold flagging, cross-validation of entities against RxNorm/ICD-10. Design the provenance data model ("source: ai | human") in Phase 1 schema. **Phase 1 (schema) + Phase 3 (implementation).**

3. **FHIR storage impedance mismatch** -- Neither pure JSON blobs nor full normalization works. Use the hybrid: FHIR JSON column as source of truth + indexed projection columns for the 15-20 query fields. Load test with 50K+ Synthea records; chart-open must be under 500ms. **Phase 1 -- getting the data model wrong means a rewrite.**

4. **E-prescribing and claims processing underestimation** -- E-prescribing requires NCPDP SCRIPT messages (not REST), Weno certification (4-8 weeks), and EPCS is a separate DEA-regulated certification. X12 837P claims have 900+ pages of situational rules and 30-40% rejection rates on naive implementations. Budget 3-4 months for e-prescribing and 6-8 weeks for claims. Start Weno enrollment at the beginning of Phase 2. **Phase 2.**

5. **SQLCipher performance cliff at clinical volumes** -- 5-15% encryption overhead compounds with JSON queries across 50K+ records. Prevention: covering indexes on every common query, paginate everything, WAL mode, pre-computed patient summary table, automated performance regression tests with realistic data. Set a hard 200ms budget for user-facing queries. **Phase 1.**

## Implications for Roadmap

Based on research, suggested phase structure:

### Phase 1: HIPAA Foundation + Core Clinical EMR (Months 1-6)

**Rationale:** Everything depends on the security foundation (RBAC, audit, encryption) and the data model (FHIR hybrid storage). These cannot be retrofitted. The manual clinical workflow (patients, scheduling, SOAP notes) must work before AI can enhance it. Research unanimously confirms: build the foundation right or face a rewrite.

**Delivers:** A functional desktop EMR that a solo practitioner can use for daily patient care without AI. HIPAA-compliant from the first line of code.

**Build order (strict dependencies):**
1. SQLCipher schema + Rust DB layer (everything depends on this)
2. Tauri shell + IPC scaffolding
3. Authentication + RBAC (at data layer, not just routes)
4. Audit logging (hash-chained, append-only)
5. React app shell + routing (can parallel with 2-4)
6. Patient demographics CRUD + search
7. Scheduling module + patient flow board
8. SOAP note entry with templates + vitals + ROS + PE forms
9. Medication list + allergy tracking + problem list
10. Lab results viewer (manual), document management
11. Encrypted backups + macOS distribution (DMG, notarization, auto-updates)

**Addresses features:** All Phase 1 table stakes from FEATURES.md
**Avoids pitfalls:** HIPAA-as-checkbox, FHIR impedance mismatch, SQLCipher performance cliff, sidecar lifecycle (establish core EMR independence from sidecar)

### Phase 2: Billing, E-Prescribing, and Integrations (Months 7-10)

**Rationale:** Billing and e-prescribing are the features that make an EMR complete enough for practices to switch from their current system. Both have heavy external dependencies (clearinghouses, Weno Exchange) with long certification timelines -- start certification processes at Phase 2 kickoff. Claims and e-prescribing depend on the clinical documentation and medication data from Phase 1.

**Delivers:** Revenue cycle management and prescription capabilities. The EMR can now replace a practice's existing system entirely.

**Key deliverables:**
1. CPT/ICD-10 coding interface + fee schedule management
2. X12 837P claim generation with pre-submission scrubbing
3. 835 ERA processing (payment posting, denial management)
4. AR tracking and financial reporting
5. E-prescribing via Weno Exchange (start certification month 1)
6. RxNav-in-a-Box drug interaction checking
7. HL7 v2 lab interface (electronic ordering and results)
8. Insurance eligibility verification (270/271)
9. CQM/eCQM reporting foundation
10. Appointment reminders (SMS/email via Twilio)

**Addresses features:** All Phase 2 features from FEATURES.md
**Avoids pitfalls:** E-prescribing underestimation (start Weno early), X12 claims brittleness (use pyx12 library, not string concatenation; scrub before submit), alert fatigue (severity-based drug interaction filtering from day one)

### Phase 3: AI Enhancement Layer (Months 11-15)

**Rationale:** AI features enhance the manual workflows now proven in Phase 1-2. The clinical data accumulated over 6-10 months provides the context AI needs (encounter history for pre-charting, scheduling data for no-show prediction). The sidecar can be built and tested independently without disrupting the stable core EMR.

**Delivers:** The primary differentiator -- ambient AI documentation, coding assistance, and diagnostic support. This is what makes MedArc worth switching to.

**Key deliverables:**
1. FastAPI sidecar scaffold + PyInstaller build pipeline + health monitoring
2. whisper.cpp/WhisperKit voice transcription with confidence scoring
3. Speaker diarization (pyannote.audio) for doctor vs patient separation
4. MedSpaCy/SciSpaCy NLP pipeline (entity extraction, section detection, negation)
5. Ollama LLM integration for SOAP note generation (human-in-the-loop mandatory)
6. FAISS vector search for ICD-10/CPT code suggestions
7. AI pre-charting (pre-visit context assembly)
8. Smart scheduling (XGBoost no-show prediction)
9. AWS Bedrock fallback for complex cases (de-identified data only)

**Addresses features:** All AI differentiators from FEATURES.md
**Avoids pitfalls:** AI hallucination (draft state, confidence scoring, entity validation, provenance tracking), sidecar lifecycle (health checks, timeouts, graceful degradation), memory pressure (lazy model loading, model unloading on idle)

### Phase 4: Cloud Migration and Expansion (Months 16-18)

**Rationale:** Multi-site practices need cloud sync. Only pursue after desktop product-market fit is proven. SQLAlchemy enters here (not before) for the cloud API layer targeting PostgreSQL.

**Delivers:** Optional cloud connectivity for multi-device/multi-site operation. Revenue expansion through cloud hosting tier ($65-110/mo per clinic).

**Key deliverables:**
1. AWS infrastructure (RDS PostgreSQL, S3, Cognito)
2. Cloud API layer (FastAPI + SQLAlchemy on Lambda)
3. PowerSync bidirectional sync (local SQLCipher <-> cloud PostgreSQL)
4. Migration tooling and validation
5. Patient portal (patient-facing features)
6. ONC certification pursuit (when revenue supports it)

### Phase Ordering Rationale

- **Security and data model first:** HIPAA compliance and FHIR storage cannot be retrofitted. Every research file confirms this. The audit log, RBAC, and encryption architecture must exist before any clinical data is stored.
- **Manual before AI:** Every AI feature enhances an underlying manual workflow. The manual workflow must work perfectly, and clinical data must accumulate, before AI adds value. This also de-risks the project -- if AI proves harder than expected, the core EMR still ships.
- **Billing before AI:** Practices need revenue cycle management to justify switching EMRs. Billing has hard external dependencies (clearinghouse enrollment, Weno certification) with long lead times that must start early.
- **Cloud last:** Local-first is the value proposition. Cloud is optional expansion, not core product. SQLAlchemy's complexity is justified only when PostgreSQL enters the picture.

### Research Flags

Phases likely needing deeper research during planning:
- **Phase 1 (FHIR schema design):** The hybrid FHIR-JSON + indexed projection pattern needs careful schema design for the 15-20 query fields. Recommend a research spike with Synthea synthetic data before finalizing the schema.
- **Phase 2 (e-prescribing):** NCPDP SCRIPT message format, Weno Exchange certification process, EPCS DEA requirements -- these are complex regulated integrations with sparse public documentation. Needs dedicated research.
- **Phase 2 (X12 claims):** 837P/835 processing has hundreds of situational rules. Research the pyx12 library capabilities and Office Ally sandbox API before scoping.
- **Phase 3 (AI pipeline):** MedSpaCy compatibility must be verified; whisper.cpp vs WhisperKit evaluation needed; LangChain vs custom orchestration decision. Recommend research spike in Phase 2 to de-risk.

Phases with standard patterns (skip research-phase):
- **Phase 1 (Tauri + React + SQLCipher):** Well-documented stack with established patterns. Tauri 2.x has explicit sidecar support documentation.
- **Phase 1 (RBAC + auth):** Standard implementation patterns for role-based access with Argon2 hashing and TOTP MFA.
- **Phase 4 (AWS infrastructure):** Standard cloud patterns with well-documented services (RDS, Lambda, Cognito, S3).

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | MEDIUM | Core technologies (React, FastAPI, SQLCipher, FAISS) are well-established. MedSpaCy compatibility is LOW confidence. All version numbers need verification (web search was unavailable). |
| Features | MEDIUM-HIGH | Table stakes and differentiators are well-defined from PRD + competitor analysis. HIPAA requirements are stable regulation (HIGH). Competitor-specific pricing may have changed (LOW). |
| Architecture | MEDIUM | Rust-for-CRUD + Python-for-AI-only pattern is well-reasoned and internally consistent. Tauri 2.x sidecar specifics and rusqlite + SQLCipher integration need verification against current docs. |
| Pitfalls | MEDIUM-HIGH | Regulatory pitfalls (HIPAA, DEA, X12) are based on stable standards (HIGH). AI accuracy statistics need validation against current benchmarks (MEDIUM). Performance characteristics are based on documented specs (MEDIUM). |

**Overall confidence:** MEDIUM

### Gaps to Address

- **MedSpaCy Python 3.11/3.12 compatibility:** Highest-risk dependency. Test in week 1 of Phase 1. If incompatible, fall back to spaCy + SciSpaCy + custom clinical rules (covers 90% of value).
- **SQLCipher Python bindings + SQLAlchemy 2.0:** Verify pysqlcipher3/sqlcipher3 maintenance status. Moot for Phase 1-3 (Rust owns DB), but needed for Phase 4 cloud migration planning.
- **Tauri 2.x sidecar IPC specifics:** Verify current Tauri 2.x documentation for sidecar lifecycle management, health checking, and process restart patterns.
- **PyInstaller + spaCy/FAISS/torch bundling:** Non-trivial packaging challenge. Budget 3-5 days for a packaging spike in early Phase 3. Nuitka is the fallback.
- **@medplum/react and fhir-react maturity:** Niche FHIR UI libraries with uncertain maintenance status. Evaluate early; may need custom FHIR components.
- **LangChain vs custom orchestration:** LangChain adds ~50 transitive dependencies and has frequent breaking changes. Evaluate whether a lighter custom pipeline is sufficient during Phase 3 research.
- **Whisper hallucination rate at current versions:** The ~1% figure is from older research. Validate against current whisper.cpp/WhisperKit benchmarks before Phase 3 implementation.

## Sources

### Primary (HIGH confidence)
- HIPAA Security Rule (45 CFR 164.312) -- stable regulation, well-documented requirements
- DEA 21 CFR Part 1311 -- EPCS requirements, established regulation
- X12 837P 5010 / NCPDP SCRIPT -- stable healthcare IT standards
- FHIR R4 specification -- stable, well-documented resource structures

### Secondary (MEDIUM confidence)
- MedArc Day0.md PRD -- comprehensive requirements document with cited statistics and competitor analysis
- Tauri 2.x architecture and capabilities -- training data (May 2025 cutoff), needs version verification
- SQLCipher documentation -- encryption characteristics, performance overhead
- OpenEMR v8.0.0 feature baseline -- from PRD analysis, aligns with known capabilities

### Tertiary (LOW confidence)
- MedSpaCy maintenance status and Python compatibility -- last major release 2023, uncertain
- @medplum/react and fhir-react library maturity -- niche libraries, unverified
- AI accuracy benchmarks (Whisper hallucination, LLaMA NEJM scores, GPT-4 ICD-10 accuracy) -- cited in PRD from published research but not independently verified against original papers
- Competitor-specific pricing (SimplePractice, DrChrono, Tebra) -- may have changed since PRD research

---
*Research completed: 2026-03-10*
*Ready for roadmap: yes*

# Architecture Research

**Domain:** AI-powered desktop EMR (macOS, local-first)
**Researched:** 2026-03-10
**Confidence:** MEDIUM (training data + PRD analysis; web verification unavailable)

## Standard Architecture

### System Overview

```
+===================================================================+
|                  macOS Application (.app / DMG)                    |
|                                                                   |
|  +-------------------------------------------------------------+ |
|  |               Tauri 2.x Shell (Rust Core)                    | |
|  |                                                               | |
|  |  - Process lifecycle management (spawn/kill sidecar)          | |
|  |  - SQLCipher CRUD via rusqlite (DB owner)                     | |
|  |  - macOS Keychain integration (Secure Enclave)                | |
|  |  - File system operations (documents, backups)                | |
|  |  - Audit log writer (append-only, hash-chained)               | |
|  |  - Auto-updater (Ed25519 signed, tauri-plugin-updater)        | |
|  |  - Permission system (Tauri capability-based ACL)             | |
|  +------------------+------------------------------------------+ |
|         |  IPC       |            |  Sidecar mgmt               |
|         | (invoke)   |            |  (spawn/kill)                |
|         v            |            v                               |
|  +----------------+  |  +--------------------------------------+ |
|  | WKWebView      |  |  | FastAPI Sidecar (PyInstaller bin)    | |
|  | React 18+ / TS |  |  |                                      | |
|  |                |  |  |  - Ollama client (LLM inference)      | |
|  | - UI rendering |  |  |  - whisper.cpp bindings (STT)         | |
|  | - Form state   |  |  |  - MedSpaCy/SciSpaCy (NER)           | |
|  | - Client route |  |  |  - FAISS (vector search)              | |
|  | - Zustand store|  |  |  - LangChain/LangGraph (orchestrate)  | |
|  |                |  |  |  - fhir.resources (Pydantic models)   | |
|  +-------+--------+  |  +------------------+-------------------+ |
|          |            |                     |                     |
|          |  HTTP      |                     | HTTP                |
|          | localhost  |                     | localhost            |
|          | :8321      |                     | (Ollama :11434)      |
|          v            v                     v                     |
|  +--------------------------------------+  +-----------------+   |
|  |          Rust Backend Layer           |  |   Ollama Server  |  |
|  |  - rusqlite + SQLCipher driver        |  |   (separate      |  |
|  |  - Repository pattern (traits)        |  |    process)      |  |
|  |  - Unit of Work (transactions)        |  +-----------------+  |
|  |  - FHIR JSON serialization            |                       |
|  |  - Alembic migration runner           |  +-----------------+  |
|  |  - Backup/restore                     |  | RxNav-in-a-Box  |  |
|  +------------------+-------------------+  | (Docker)         |  |
|                     |                       +-----------------+  |
|                     v                                            |
|  +-------------------------------------------------------------+ |
|  |            SQLCipher Database (AES-256)                       | |
|  |                                                               | |
|  |  - FHIR R4 resources as JSON columns                          | |
|  |  - Indexed lookup tables (patient, encounter, etc.)           | |
|  |  - Audit log table (hash-chained)                             | |
|  |  - Per-page HMAC tamper detection                             | |
|  +-------------------------------------------------------------+ |
+===================================================================+
  Keys: macOS Keychain (Secure Enclave on Apple Silicon)
  External: AWS Bedrock (BAA, 5% of AI calls) | Weno Exchange (eRx)
```

### Component Responsibilities

| Component | Responsibility | Typical Implementation |
|-----------|----------------|------------------------|
| **Tauri Rust Core** | Process orchestrator, DB owner, security boundary, IPC hub | Tauri 2.x commands, rusqlite, keychain-services crate |
| **React Frontend** | UI rendering, form management, client-side validation, routing | React 18+, TypeScript, MUI, Zustand, React Hook Form, TanStack Table |
| **FastAPI Sidecar** | AI inference only: STT, NLP, LLM, vector search, FHIR validation | PyInstaller binary, FastAPI, uvicorn, pydantic/fhir.resources |
| **Ollama Server** | LLM model hosting and inference API | Ollama (separate managed process), LLaMA 3.1 8B Q4_K_M |
| **SQLCipher Database** | Encrypted persistent storage, FHIR resource store | SQLCipher 4.x, FHIR JSON columns, indexed lookup tables |
| **RxNav-in-a-Box** | Drug interaction checking, RxNorm API | Docker container from NLM, localhost REST API |
| **macOS Keychain** | Encryption key storage, Touch ID, credential management | Security.framework via Tauri plugin or Rust keychain crate |

## Validation: Rust-for-CRUD + Python-for-AI-Only

### Why This Split is Correct

The PRD's proposed architecture -- Rust handles all database CRUD, Python handles AI inference exclusively -- is the right call for four reasons:

1. **Memory pressure.** Running LLaMA 3.1 8B (6 GB) + Whisper large-v3-turbo + MedSpaCy simultaneously already pushes 16 GB Macs. Keeping the CRUD path in Rust (zero-overhead, no Python runtime loaded) means the Python sidecar can be cold-started only when AI is needed, reclaiming 200-400 MB when idle.

2. **Startup time.** PyInstaller binaries take 3-8 seconds to cold-start. If Python owned CRUD, every app launch would block on this. With Rust owning CRUD, the app is usable instantly; the Python sidecar boots in the background and is ready by the time the user navigates to an AI feature.

3. **Security boundary.** Rust code runs inside the Tauri process with direct Keychain access and SQLCipher key material. The Python sidecar runs as a separate process with no direct database access and no encryption keys. This enforces the HIPAA minimum-necessary principle architecturally -- the AI layer only sees the data explicitly sent to it per request.

4. **Failure isolation.** If the Python sidecar crashes (OOM from a large model, segfault in a native library), the core EMR keeps working. Doctors can still view patients, document encounters manually, and manage schedules. AI features degrade gracefully.

### Where This Split Gets Tricky

| Challenge | Mitigation |
|-----------|------------|
| **FHIR validation in two places** | Define FHIR schemas once in TypeScript (frontend validation) and Python (fhir.resources for AI output). Rust does not need FHIR validation -- it stores/retrieves JSON blobs opaquely. |
| **Python needs patient context for AI** | Frontend sends patient context in each AI request body via HTTP. Python never queries the DB directly. This adds request payload size but preserves the security boundary. |
| **SQLAlchemy lives in Python but Rust owns the DB** | Do NOT use SQLAlchemy in Phase 1. Rust owns the DB exclusively via rusqlite. SQLAlchemy enters only in Phase 4 (cloud migration) when PostgreSQL is introduced and the data access layer needs dialect abstraction. In Phase 1-3, Alembic migrations can run via a separate Python CLI tool (not the sidecar). |
| **Schema migrations** | Use a Rust migration tool (refinery or sqlx-migrate) for Phase 1-3. Switch to Alembic only when PostgreSQL enters the picture. This avoids the awkward "Python manages schema, Rust reads data" split. |

### Revised Data Access Architecture

The PRD mentions SQLAlchemy as the ORM from day one. This is premature for Phase 1-3 and creates unnecessary complexity. Here is the recommended phased approach:

**Phase 1-3 (SQLite/SQLCipher only):**
```
React Frontend
    |
    | invoke("get_patient", { id })
    v
Tauri Rust Commands
    |
    | rusqlite queries with prepared statements
    v
SQLCipher Database
```

- Rust `src-tauri/src/db/` module owns all SQL
- Repository pattern implemented as Rust traits
- FHIR JSON stored/retrieved as TEXT columns
- Migration files in SQL, applied by Rust at startup

**Phase 4 (Cloud migration):**
```
React Frontend
    |
    | invoke("get_patient", { id })       HTTP (cloud mode)
    v                                      v
Tauri Rust Commands  ----OR---->  Cloud API (FastAPI on Lambda)
    |                                      |
    | rusqlite (local)                     | SQLAlchemy + asyncpg
    v                                      v
SQLCipher                           RDS PostgreSQL
    ^                                      ^
    +--------  PowerSync  -----------------+
```

SQLAlchemy is introduced only in the cloud API layer, not in the desktop app. The desktop Rust layer continues using rusqlite for local operations. PowerSync handles bidirectional sync between them.

## Recommended Project Structure

```
medarc/
├── src-tauri/                    # Rust backend (Tauri core)
│   ├── src/
│   │   ├── main.rs               # Tauri app entry point
│   │   ├── commands/              # IPC command handlers
│   │   │   ├── mod.rs
│   │   │   ├── patient.rs         # Patient CRUD commands
│   │   │   ├── encounter.rs       # Encounter CRUD commands
│   │   │   ├── schedule.rs        # Scheduling commands
│   │   │   ├── medication.rs      # Medication list commands
│   │   │   ├── billing.rs         # Billing/claims commands
│   │   │   ├── document.rs        # Document management commands
│   │   │   ├── auth.rs            # Authentication commands
│   │   │   └── sidecar.rs         # Sidecar lifecycle management
│   │   ├── db/                    # Database layer
│   │   │   ├── mod.rs
│   │   │   ├── connection.rs      # SQLCipher connection pool
│   │   │   ├── migrations/        # SQL migration files
│   │   │   ├── repository.rs      # Repository trait definitions
│   │   │   └── models/            # Rust structs for DB rows
│   │   ├── fhir/                  # FHIR JSON helpers
│   │   │   ├── mod.rs
│   │   │   └── resources.rs       # Serde structs for FHIR resources
│   │   ├── security/              # Security layer
│   │   │   ├── mod.rs
│   │   │   ├── keychain.rs        # macOS Keychain integration
│   │   │   ├── audit.rs           # Hash-chained audit log
│   │   │   ├── rbac.rs            # Role-based access control
│   │   │   └── encryption.rs      # Field-level encryption
│   │   └── backup/                # Backup/restore logic
│   ├── Cargo.toml
│   ├── tauri.conf.json            # Tauri configuration
│   └── capabilities/             # Tauri 2.x permission definitions
│
├── src/                           # React frontend
│   ├── App.tsx
│   ├── main.tsx
│   ├── routes/                    # Page-level route components
│   │   ├── patients/
│   │   ├── scheduling/
│   │   ├── encounters/
│   │   ├── billing/
│   │   ├── prescriptions/
│   │   ├── labs/
│   │   ├── reports/
│   │   └── settings/
│   ├── components/                # Shared UI components
│   │   ├── clinical/              # SOAP forms, vitals, ROS
│   │   ├── fhir/                  # FHIR resource renderers
│   │   ├── layout/                # App shell, navigation
│   │   └── common/                # Buttons, tables, modals
│   ├── hooks/                     # Custom React hooks
│   │   ├── useTauriCommand.ts     # Typed IPC wrapper
│   │   ├── useAI.ts               # AI sidecar communication
│   │   └── useAuth.ts             # Auth state management
│   ├── stores/                    # Zustand stores
│   │   ├── patientStore.ts
│   │   ├── encounterStore.ts
│   │   ├── authStore.ts
│   │   └── aiStore.ts
│   ├── services/                  # API/IPC service layer
│   │   ├── tauri.ts               # Tauri invoke wrappers
│   │   └── ai.ts                  # HTTP calls to FastAPI sidecar
│   ├── types/                     # TypeScript type definitions
│   │   ├── fhir.ts                # FHIR R4 resource types
│   │   ├── clinical.ts
│   │   └── billing.ts
│   └── utils/                     # Shared utilities
│
├── sidecar/                       # Python AI sidecar
│   ├── medarc_ai/
│   │   ├── __init__.py
│   │   ├── main.py                # FastAPI app entry point
│   │   ├── api/                   # API route handlers
│   │   │   ├── transcribe.py      # Voice-to-text endpoint
│   │   │   ├── extract.py         # NLP entity extraction
│   │   │   ├── generate.py        # SOAP note generation
│   │   │   ├── code.py            # ICD-10/CPT coding
│   │   │   ├── diagnose.py        # Differential diagnosis
│   │   │   └── schedule.py        # No-show prediction
│   │   ├── pipelines/             # AI pipeline orchestration
│   │   │   ├── soap_pipeline.py   # Full voice-to-SOAP chain
│   │   │   └── coding_pipeline.py # Encounter-to-codes chain
│   │   ├── models/                # Model loading/management
│   │   │   ├── whisper.py         # whisper.cpp wrapper
│   │   │   ├── nlp.py             # MedSpaCy/SciSpaCy loader
│   │   │   ├── llm.py             # Ollama client wrapper
│   │   │   └── vector.py          # FAISS index management
│   │   ├── fhir/                  # FHIR resource generation
│   │   │   └── builders.py        # fhir.resources constructors
│   │   └── config.py              # Sidecar configuration
│   ├── pyproject.toml
│   ├── build.py                   # PyInstaller build script
│   └── tests/
│
├── shared/                        # Shared schemas (source of truth)
│   ├── fhir-schemas/              # JSON Schema for FHIR R4 resources
│   └── api-contracts/             # OpenAPI specs for sidecar API
│
├── migrations/                    # SQL migration files (Phase 1-3)
│   ├── 001_initial_schema.sql
│   └── ...
│
├── package.json                   # Frontend dependencies
├── tsconfig.json
├── vite.config.ts
└── .github/                       # CI/CD
```

### Structure Rationale

- **src-tauri/src/commands/:** One module per clinical domain. Each command is a thin handler that validates input, calls the repository, and returns results. This maps cleanly to Tauri's `#[tauri::command]` attribute system.
- **src-tauri/src/db/:** Repository pattern with Rust traits. Each repository (PatientRepository, EncounterRepository) defines the interface; the SQLCipher implementation fulfills it. In Phase 4, a second implementation could target PostgreSQL directly if needed.
- **src/ (React):** Route-based organization matching clinical workflows. Doctors think in terms of "patient chart" and "today's schedule," not technical modules.
- **sidecar/:** Completely standalone Python project with its own dependency management. Builds to a single binary via PyInstaller. Has zero knowledge of the database -- receives data via HTTP, returns structured results.
- **shared/:** JSON Schemas and OpenAPI specs that both TypeScript and Python code can generate types from. This is the contract layer that prevents Rust-Python-TypeScript type drift.

## Architectural Patterns

### Pattern 1: Command-Query Separation in Rust

**What:** Every Tauri command is either a command (mutates state, returns success/error) or a query (reads state, returns data). Never both.
**When to use:** All IPC handlers.
**Trade-offs:** Slightly more functions, but much easier to audit for HIPAA (all writes are explicit), test, and cache.

**Example:**
```rust
// Query: returns data, no side effects
#[tauri::command]
async fn get_patient(db: State<'_, DbPool>, id: String) -> Result<PatientRecord, AppError> {
    let repo = PatientRepository::new(&db);
    let patient = repo.find_by_id(&id).await?;
    Ok(patient)
}

// Command: mutates state, returns confirmation
#[tauri::command]
async fn update_patient(
    db: State<'_, DbPool>,
    audit: State<'_, AuditLogger>,
    user: AuthenticatedUser,
    id: String,
    updates: PatientUpdate,
) -> Result<(), AppError> {
    let repo = PatientRepository::new(&db);
    repo.update(&id, &updates).await?;
    audit.log(user.id, AuditAction::Update, "Patient", &id).await?;
    Ok(())
}
```

### Pattern 2: Sidecar as Stateless Service

**What:** The Python sidecar holds no state between requests. Every AI request includes all context needed to produce a result. The sidecar loads ML models into memory at startup but maintains no patient data, no session, no database connection.
**When to use:** All AI interactions.
**Trade-offs:** Larger request payloads (must include patient context each time), but dramatically simpler failure recovery (restart the sidecar and lose nothing) and stronger security (no PHI cached in the AI layer).

**Example:**
```python
# Sidecar endpoint: receives all context, returns structured output
@app.post("/api/v1/generate-soap")
async def generate_soap(request: SOAPRequest) -> SOAPResponse:
    # request.transcript: raw voice transcript
    # request.patient_context: relevant history, allergies, meds
    # request.template: SOAP template preferences

    entities = await nlp_pipeline.extract(request.transcript)
    soap_draft = await llm_client.generate_soap(
        transcript=request.transcript,
        entities=entities,
        context=request.patient_context,
    )
    codes = await vector_search.suggest_codes(soap_draft)

    return SOAPResponse(
        soap=soap_draft,
        entities=entities,
        suggested_codes=codes,
        confidence=soap_draft.confidence_score,
    )
```

### Pattern 3: FHIR-as-JSON-Column with Indexed Projections

**What:** Store the full FHIR R4 resource as a JSON TEXT column, but extract frequently-queried fields into indexed columns on the same row. This gives you both standards-compliant data and fast SQL queries.
**When to use:** All clinical data storage.
**Trade-offs:** Some data duplication (indexed fields exist in both the JSON and the column), but SQLite's JSON functions are too slow for list screens with hundreds of patients. The indexed columns are the "read optimization" while the JSON column is the "source of truth."

**Example schema:**
```sql
CREATE TABLE patient (
    id TEXT PRIMARY KEY,           -- FHIR Resource.id (UUID)
    mrn TEXT NOT NULL UNIQUE,      -- indexed projection
    family_name TEXT NOT NULL,     -- indexed projection
    given_name TEXT NOT NULL,      -- indexed projection
    birth_date TEXT NOT NULL,      -- indexed projection (ISO 8601)
    gender TEXT NOT NULL,          -- indexed projection
    active INTEGER DEFAULT 1,     -- indexed projection
    fhir_resource TEXT NOT NULL,   -- Full FHIR Patient JSON
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX idx_patient_name ON patient(family_name, given_name);
CREATE INDEX idx_patient_mrn ON patient(mrn);
CREATE INDEX idx_patient_dob ON patient(birth_date);
```

### Pattern 4: Hash-Chained Audit Log

**What:** Every audit entry includes a SHA-256 hash of the previous entry, creating a tamper-evident chain. If any entry is modified or deleted, the chain breaks and verification fails.
**When to use:** All PHI access logging (HIPAA requirement).
**Trade-offs:** Sequential write dependency (each entry depends on the previous hash), but audit logs are append-only and writes are infrequent relative to reads.

```rust
struct AuditEntry {
    id: i64,
    timestamp: String,        // UTC ISO 8601
    user_id: String,
    action: String,           // CREATE, READ, UPDATE, DELETE
    resource_type: String,    // Patient, Encounter, etc.
    resource_id: String,
    device_id: String,
    success: bool,
    previous_hash: String,    // SHA-256 of previous entry
    entry_hash: String,       // SHA-256 of this entry (all fields above)
}
```

## Data Flow

### Core CRUD Flow (No AI)

```
User Action (e.g., save patient)
    |
    v
React Component
    |
    | form validation (React Hook Form + Zod)
    v
Zustand Store (optimistic update)
    |
    | invoke("save_patient", { data })
    v
Tauri IPC Bridge
    |
    v
Rust Command Handler
    |
    | RBAC check (does user have permission?)
    | Input sanitization
    v
Repository (Rust trait impl)
    |
    | Prepare FHIR JSON + indexed columns
    | Begin transaction
    v
SQLCipher (encrypted write)
    |
    | Commit transaction
    v
Audit Logger
    |
    | Append hash-chained log entry
    v
Return Result to Frontend
    |
    v
Zustand Store (confirm or rollback optimistic update)
    |
    v
UI Update
```

### AI-Assisted Documentation Flow

```
Doctor speaks during encounter
    |
    v
React: Audio capture (MediaRecorder API in WKWebView)
    |
    | Audio chunks streamed
    v
React: POST /api/v1/transcribe (to FastAPI sidecar at localhost:8321)
    |
    v
FastAPI Sidecar:
    |
    | 1. whisper.cpp transcription (CoreML accelerated)
    | 2. PyAnnote speaker diarization (doctor vs patient)
    v
Raw transcript returned to frontend
    |
    v
React: Display transcript, allow editing
    |
    | User clicks "Generate SOAP Note"
    | POST /api/v1/generate-soap { transcript, patient_context }
    v
FastAPI Sidecar:
    |
    | 1. MedSpaCy section detection
    | 2. SciSpaCy entity extraction (meds, conditions, etc.)
    | 3. Ollama LLM call (LLaMA 3.1 8B) with clinical prompt
    |    - If complex: route to AWS Bedrock (de-identified)
    | 4. FAISS vector search for ICD-10/CPT suggestions
    v
SOAP draft + entities + codes returned to frontend
    |
    v
React: Display SOAP draft in editable form
    |
    | Doctor reviews, edits, approves
    | invoke("save_encounter", { soap, codes })
    v
Tauri Rust: Save encounter to SQLCipher
    |
    v
Audit log: Record AI-assisted documentation event
```

### Key Data Flows

1. **Patient lookup:** React -> Tauri invoke -> Rust query (SQLCipher) -> JSON response -> React render. No sidecar involvement. Sub-50ms expected.

2. **Voice transcription:** React captures audio -> HTTP POST to sidecar -> whisper.cpp processes -> streaming text response. Latency: 3-10 seconds for 1 minute of audio on M1+.

3. **SOAP generation:** React sends transcript + patient context -> sidecar runs NLP + LLM pipeline -> returns structured SOAP. Latency: 5-15 seconds for a typical encounter note.

4. **Drug interaction check:** React sends medication list -> HTTP to RxNav-in-a-Box (Docker, localhost) -> returns interaction alerts. Latency: <1 second.

5. **Backup:** Rust copies SQLCipher file -> encrypts with separate backup key -> writes to external storage. No frontend involvement.

## Integration Points

### External Services

| Service | Integration Pattern | Notes |
|---------|---------------------|-------|
| **Ollama** | HTTP REST API on localhost:11434 | Separate managed process. Sidecar calls it. Must be installed separately or bundled. |
| **RxNav-in-a-Box** | HTTP REST API via Docker | NLM Docker composition. Requires Docker Desktop or colima. ~2 GB disk. |
| **AWS Bedrock** | HTTPS REST API (Claude) | BAA-covered. Only for complex cases. De-identify data before sending. |
| **Weno Exchange** | HTTPS API for e-prescribing | Phase 2. Requires $300 activation. EPCS certification needed for controlled substances. |
| **Apple Notarization** | xcrun notarytool | Build-time only. Required for macOS distribution. |
| **tauri-plugin-updater** | HTTPS to update manifest | Checks for updates on launch. Ed25519 signature verification. |

### Internal Boundaries

| Boundary | Communication | Notes |
|----------|---------------|-------|
| React <-> Rust | Tauri IPC (invoke/events) | Type-safe via tauri-specta or manual type alignment. All CRUD goes here. |
| React <-> Python sidecar | HTTP localhost:8321 | OpenAPI contract. All AI goes here. Frontend chooses which path based on operation type. |
| Rust <-> SQLCipher | rusqlite (in-process) | Direct library call. No network. Fastest possible path. |
| Python sidecar <-> Ollama | HTTP localhost:11434 | Standard Ollama REST API. Sidecar is the only Ollama client. |
| Python sidecar <-> RxNav | HTTP localhost:4000 | Standard RxNav REST API. Could also be called from frontend directly. |
| Rust -> Audit log | In-process write | Same SQLCipher database, separate table. Write-after-every-mutation pattern. |

## Anti-Patterns

### Anti-Pattern 1: Python Sidecar Accessing Database Directly

**What people do:** Give the Python sidecar its own SQLCipher connection for convenience ("it needs patient data for AI context").
**Why it's wrong:** Two processes with open connections to the same SQLite file causes WAL contention, potential corruption on concurrent writes, and breaks the security boundary (Python process now has the encryption key).
**Do this instead:** Frontend includes all necessary patient context in the HTTP request body to the sidecar. Rust is the sole DB accessor.

### Anti-Pattern 2: SQLAlchemy in the Desktop App (Phase 1)

**What people do:** Use SQLAlchemy from day one "for future PostgreSQL migration."
**Why it's wrong:** SQLAlchemy adds 200+ MB to the PyInstaller bundle, requires the Python sidecar to be running for all DB operations (defeating the Rust-for-CRUD architecture), and introduces an ORM impedance mismatch with Rust.
**Do this instead:** Use rusqlite with raw SQL and Repository pattern in Rust for Phase 1-3. Introduce SQLAlchemy only in Phase 4 when a cloud API layer (running on Lambda/ECS) needs PostgreSQL access.

### Anti-Pattern 3: Single Monolithic Sidecar Binary with All Models Pre-loaded

**What people do:** Load whisper.cpp + MedSpaCy + SciSpaCy + FAISS indices all at sidecar startup.
**Why it's wrong:** On a 16 GB Mac, loading everything at once leaves insufficient memory for LLaMA 3.1 8B. Startup time balloons to 15-30 seconds.
**Do this instead:** Lazy-load models on first use. Keep only the NLP models (small, ~100 MB) resident. Load Whisper when voice capture starts. Load FAISS indices when coding is requested. Provide a "pre-warm" endpoint the frontend can call when navigating to encounter documentation.

### Anti-Pattern 4: Storing FHIR Resources Only (No Indexed Projections)

**What people do:** Store only the FHIR JSON and use JSON path queries for everything.
**Why it's wrong:** SQLite's JSON functions (json_extract) cannot use indexes. A patient list query across 10,000 records takes seconds instead of milliseconds.
**Do this instead:** Dual storage -- indexed columns for query fields, JSON column for the full resource. Keep them in sync via the Rust repository layer (single write path).

### Anti-Pattern 5: Trusting AI Output Without Human Review Gate

**What people do:** Auto-populate clinical forms with AI-generated content.
**Why it's wrong:** Whisper has ~1% hallucination rate. LLaMA achieves 64% on clinical cases. ICD-10 exact match is only 34%. Auto-populating without review is a malpractice risk.
**Do this instead:** AI output always lands in a "draft" state with visual differentiation (color, badge). Doctor must explicitly review and approve. Audit log records that the note was AI-assisted.

## Scaling Considerations

This is a desktop app for 1-5 providers, not a cloud SaaS. "Scaling" means handling growing data volume per clinic and eventually supporting multi-site deployment.

| Scale | Architecture Adjustments |
|-------|--------------------------|
| **1 provider, Year 1** (~2,000 encounters) | Single SQLCipher file. All models fit on 16 GB Mac. No scaling needed. |
| **5 providers, Year 2-3** (~20,000 encounters) | SQLCipher still fine (SQLite handles millions of rows). Consider WAL mode for read concurrency if multiple users share one Mac. |
| **Multi-site, Year 3+** (~100,000+ encounters) | Phase 4 cloud migration. PostgreSQL on RDS. PowerSync for offline-first sync. Each Mac keeps local replica. |
| **AI model growth** | Monitor Apple Silicon releases. M4 Ultra (192 GB) can run 70B parameter models. Architecture accommodates model swaps via Ollama config. |

### Scaling Priorities

1. **First bottleneck: AI memory pressure.** On 16 GB Macs, the LLM + Whisper + NLP pipeline will compete for memory. Mitigation: lazy model loading, model unloading after idle timeout, and supporting the smaller LLaMA 3.2 3B as a fallback.
2. **Second bottleneck: SQLCipher file size.** After 5+ years of clinical data with documents, the database could reach 5-10 GB. Mitigation: archive old encounters to a separate SQLCipher file; store documents on the filesystem (not in the DB) with only metadata in SQLCipher.

## Build Order (Dependencies)

The build order is dictated by what depends on what. Each layer must be stable before the layer above it can be built meaningfully.

### Phase 1: Foundation (Months 1-2)

Build order within Phase 1:

```
1. SQLCipher schema + Rust DB layer       (everything depends on this)
     |
2. Tauri shell + IPC scaffolding          (needs DB layer to wire commands)
     |
3. Authentication + RBAC                  (needs DB + IPC)
     |
4. Audit logging                          (needs auth context)
     |
5. React app shell + routing              (can parallel with 2-4)
     |
6. Patient CRUD (first clinical feature)  (needs all of 1-5)
```

**Rationale:** The database schema is the hardest thing to change later, especially with FHIR JSON columns. Get the data model right first. Authentication and audit logging are HIPAA requirements that must be baked in from the first line of code, not bolted on later.

### Phase 1: Clinical Features (Months 3-6)

```
7. Scheduling module                       (needs patient records)
     |
8. Clinical documentation (SOAP forms)    (needs patient + encounter schema)
     |
9. Medication list + allergy tracking     (needs patient records)
     |
10. Lab results viewer (manual entry)     (needs encounter schema)
     |
11. Document upload/scanning              (needs file system + patient records)
     |
12. Encrypted backups                     (needs all data in place to test)
```

### Phase 2: Feature Parity (Months 7-10)

```
13. Billing module (CPT/ICD coding, claims)   (needs encounters + patient)
     |
14. E-prescribing (Weno Exchange)             (needs medications + patient)
     |
15. RxNav-in-a-Box integration               (needs medications)
     |
16. HL7 v2 lab interface                     (needs lab schema)
     |
17. Reporting engine                         (needs all clinical data)
```

### Phase 3: AI Enhancement (Months 11-15)

```
18. FastAPI sidecar scaffold + build pipeline  (independent of clinical features)
     |
19. whisper.cpp integration                    (needs sidecar running)
     |
20. MedSpaCy/SciSpaCy NLP pipeline           (needs sidecar + transcript input)
     |
21. Ollama LLM integration                   (needs sidecar + NLP output)
     |
22. SOAP note generation pipeline            (needs 19+20+21 combined)
     |
23. FAISS vector search (ICD-10/CPT)         (needs encounter data for context)
     |
24. Smart scheduling (XGBoost/LightGBM)      (needs historical appointment data)
     |
25. AWS Bedrock fallback                     (needs 22 working locally first)
```

### Phase 4: Cloud Migration (Months 16-18)

```
26. AWS infrastructure (RDS, S3, Cognito)    (independent provisioning)
     |
27. Cloud API layer (FastAPI + SQLAlchemy)   (needs RDS)
     |
28. PowerSync integration                   (needs both local + cloud DB)
     |
29. Dual-write mode                         (needs PowerSync)
     |
30. Migration tooling + validation          (needs dual-write working)
     |
31. Cloud cutover                           (needs validated migration)
```

## Key Architectural Decisions Summary

| Decision | Recommendation | Confidence |
|----------|---------------|------------|
| Rust owns all DB CRUD | Correct. Enforces security boundary, eliminates Python startup dependency for core EMR. | HIGH |
| Python sidecar for AI only | Correct. Isolates failure, allows lazy loading, keeps core EMR fast. | HIGH |
| SQLAlchemy in Phase 1 | Do NOT use. Use rusqlite directly. Introduce SQLAlchemy only in Phase 4 cloud API. | HIGH |
| FHIR JSON + indexed columns | Correct hybrid. Standards-compliant storage with query performance. | HIGH |
| Sidecar as stateless service | Correct. No DB access, no PHI caching, simple restart recovery. | HIGH |
| Lazy model loading in sidecar | Correct for 16 GB Macs. Pre-warm on navigation to AI features. | MEDIUM |
| Ollama as separate process | Correct. Well-maintained, handles model management, GPU scheduling. | MEDIUM |
| Hash-chained audit log | Correct for HIPAA. Standard pattern for tamper-evident logging. | HIGH |

## Sources

- PRD analysis (Day0.md) -- primary source for architecture decisions and constraints
- Tauri 2.x architecture (training data, MEDIUM confidence) -- sidecar support, IPC model, WKWebView on macOS
- SQLCipher documentation (training data, MEDIUM confidence) -- AES-256-CBC, PBKDF2, per-page HMAC
- FHIR R4 specification (training data, HIGH confidence) -- resource structure is stable and well-documented
- rusqlite crate (training data, MEDIUM confidence) -- SQLite bindings for Rust, widely used
- PyInstaller bundling patterns (training data, MEDIUM confidence) -- single-binary Python distribution
- HIPAA Security Rule (training data, HIGH confidence) -- technical safeguard requirements are stable regulation

**Note:** WebSearch and WebFetch were unavailable during this research session. Confidence on Tauri 2.x-specific sidecar IPC details, rusqlite + SQLCipher integration specifics, and current Ollama API surface should be verified against official documentation before implementation begins.

---
*Architecture research for: MedArc AI-Powered Desktop EMR*
*Researched: 2026-03-10*

# Stack Research

**Domain:** AI-powered desktop EMR (macOS, local-first, HIPAA-compliant)
**Researched:** 2026-03-10
**Confidence:** MEDIUM (web verification tools unavailable; versions based on training data through May 2025 and must be validated against current releases)

> **NOTE:** WebSearch, WebFetch, and Brave API were all unavailable during this research session. All version numbers and release claims are based on training data with a May 2025 cutoff. Every version marked with [VERIFY] should be confirmed against official release pages before locking the stack.

---

## PRD Stack Verdict

The PRD's stack choices are **well-considered and defensible**. No major changes recommended. Refinements below address version specifics, alternative considerations, and risk areas.

| PRD Choice | Verdict | Notes |
|------------|---------|-------|
| Tauri 2.x | **STRONG AGREE** | Correct over Electron for memory-constrained AI workloads |
| React 18+ TypeScript | **AGREE** | Consider React 19 if stable by project start |
| FastAPI sidecar | **AGREE with caveat** | PyInstaller bundling is the hard part; consider Nuitka as fallback |
| SQLCipher | **STRONG AGREE** | Perfect for local-first HIPAA; SQLAlchemy abstraction is key |
| Ollama | **AGREE** | Consider also MLX for Apple Silicon native; Ollama wraps llama.cpp |
| whisper.cpp | **AGREE** | WhisperKit (Swift/CoreML) is a stronger alternative on macOS |
| MedSpaCy | **AGREE with caution** | Verify Python 3.11+ compatibility; last major release was 2023 |
| FAISS | **STRONG AGREE** | Standard for local vector search at this scale |

---

## Recommended Stack

### Desktop Shell

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| Tauri | 2.x (latest stable) [VERIFY] | Desktop application shell | 30-50 MB idle RAM vs Electron's 150-300 MB. When running 6-35 GB AI models simultaneously, this headroom is not optional. Rust backend enforces explicit API surface (HIPAA minimum-necessary). Built-in sidecar support for PyInstaller binaries. WKWebView on macOS is fine for controlled-deployment medical software. Ed25519-signed auto-updates via tauri-plugin-updater. |
| Rust | 1.77+ [VERIFY] | Tauri backend, DB CRUD, file I/O | Tauri commands handle all non-AI operations in Rust for zero sidecar overhead on routine tasks. Memory safety without GC aligns with medical-grade reliability requirements. |

### Frontend

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| React | 18.3+ [VERIFY] | UI framework | Largest medical UI component ecosystem. React 19 may be stable by project start -- evaluate but do not adopt pre-release. Server Components are irrelevant for desktop; stick with client-side React. |
| TypeScript | 5.5+ [VERIFY] | Type safety | Non-negotiable for medical software. Catches data model errors at compile time. FHIR type definitions available via @types/fhir. |
| Vite | 5.x or 6.x [VERIFY] | Build tooling | Tauri's default bundler. Fast HMR for development. Do not use Create React App (deprecated) or webpack (unnecessary complexity). |

### Frontend Libraries

| Library | Version | Purpose | Why Recommended |
|---------|---------|---------|-----------------|
| Material UI (MUI) | 6.x [VERIFY] | Component library | WCAG 2.1 AA out of the box. Dense medical UIs need data tables, form controls, and dialogs -- MUI covers all. Theming system supports clinical color schemes. |
| React Hook Form | 7.x | Clinical form management | Uncontrolled form model avoids re-renders on large clinical forms (50+ fields in a SOAP note). Zod integration for runtime validation. |
| Zod | 3.x | Schema validation | Runtime validation of FHIR resources. Shared schemas between frontend validation and API contracts. |
| TanStack Table | 8.x | Data tables | Patient lists, lab results, medication lists, billing line items -- every EMR view is a table. Virtualization for large datasets. |
| TanStack Query | 5.x | Server state | Cache management for Tauri IPC and FastAPI calls. Handles loading/error states. Offline mutation queue for future cloud sync. |
| Zustand | 4.x or 5.x [VERIFY] | Client state | Lightweight global state for auth session, active patient, UI preferences. No boilerplate. |
| @medplum/react | latest [VERIFY] | FHIR components | SmartText clinical concept detection, SNOMED/ICD coding widgets, FHIR resource forms. Evaluate maturity -- may need custom wrappers. |
| fhir-react | latest [VERIFY] | FHIR rendering | FHIR resource display for DSTU2/STU3/R4. Useful for patient summary views. |
| date-fns | 3.x | Date handling | Lightweight date manipulation. Medical scheduling requires extensive date math. Do NOT use Moment.js (deprecated, bloated). |
| react-big-calendar | latest | Scheduling UI | Multi-provider calendar views. Needs customization for medical appointment types but saves months vs building from scratch. |

### Backend (Python Sidecar)

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| Python | 3.11 or 3.12 [VERIFY] | Sidecar runtime | 3.11 for best library compatibility (MedSpaCy concern). 3.12 is faster but verify all medical NLP libraries support it. Do NOT use 3.13+ until ecosystem catches up. |
| FastAPI | 0.115+ [VERIFY] | API framework | Async support for non-blocking AI inference. Pydantic models align with fhir.resources. Auto-generated OpenAPI docs for Tauri IPC contract documentation. |
| Pydantic | 2.x | Data validation | V2 is 5-50x faster than V1. FHIR resource validation via fhir.resources (Pydantic-based). |
| SQLAlchemy | 2.0+ | ORM / DB abstraction | Dialect swap from sqlite to postgresql with zero business logic changes. This is the keystone of the cloud migration strategy. |
| Alembic | 1.13+ [VERIFY] | Schema migrations | `render_as_batch=True` for SQLite ALTER TABLE limitations. Same migration files work on both SQLite and PostgreSQL. |
| PyInstaller | 6.x [VERIFY] | Binary packaging | Compiles Python + dependencies into single macOS executable for Tauri sidecar. Notarization-compatible with `--osx-bundle-identifier`. |
| uvicorn | 0.30+ [VERIFY] | ASGI server | Runs FastAPI in the sidecar. Use `--host 127.0.0.1` to bind only to localhost (HIPAA: no external network exposure). |

### Database

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| SQLite | 3.45+ (bundled) | Local database engine | Zero-config, serverless, single-file. Perfect for single-machine EMR. JSON1 extension for FHIR resource columns. |
| SQLCipher | 4.6+ [VERIFY] | Encryption layer | AES-256-CBC with PBKDF2-HMAC-SHA512 (256K iterations). Per-page HMAC for tamper detection. 5-15% overhead. Encryption key in macOS Keychain (Secure Enclave on Apple Silicon). HIPAA breach safe harbor: encrypted = "secured" = no breach notification. |
| sqlcipher3 (Python) | latest [VERIFY] | Python SQLCipher binding | SQLAlchemy dialect for SQLCipher. Verify compatibility with SQLAlchemy 2.0. Alternative: pysqlcipher3. |

### AI / ML Pipeline

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| Ollama | latest [VERIFY] | Local LLM runtime | Wraps llama.cpp with model management. Runs LLaMA 3.1 8B Q4_K_M (~6 GB) at 15-28 tok/sec on M1-M4. REST API on localhost. Simple model pulling and versioning. |
| LLaMA 3.1 8B | Q4_K_M quantization | SOAP note generation, coding assist | Best accuracy-to-size ratio for medical text generation. RAG-augmented outperforms fine-tuned biomedical models (64% vs 30% on NEJM). 6 GB fits in 16 GB unified memory alongside other models. |
| whisper.cpp | latest [VERIFY] | Voice transcription | CoreML acceleration on Apple Silicon for ~3x real-time. large-v3-turbo (1.5B params) for 95%+ accuracy. Critical: ~1% hallucination rate requires human review. |
| WhisperKit | latest [VERIFY] | Alternative transcription | Swift-native CoreML implementation. Tighter macOS integration than whisper.cpp. Evaluate both; WhisperKit may have better Apple Silicon optimization. |
| MedSpaCy | 1.x [VERIFY] | Clinical NLP | Section detection, negation/uncertainty via ConText algorithm. CRITICAL: verify Python 3.11/3.12 compatibility -- last major release was 2023. If incompatible, use spaCy directly with custom clinical rules. |
| SciSpaCy | 0.5.x [VERIFY] | Medical entity extraction | en_core_med7_lg (drugs, dosages, durations, routes, forms, frequencies), en_ner_bc5cdr_md (diseases, chemicals). All local, no cloud dependency. |
| spaCy | 3.7+ [VERIFY] | NLP framework | Foundation for MedSpaCy and SciSpaCy. Ensure version compatibility across all three. |
| FAISS | 1.8+ [VERIFY] | Vector search | Local similarity search for ICD-10/CPT code matching against 70K+ codes. faiss-cpu is sufficient; faiss-gpu not needed on Apple Silicon. |
| LangChain | 0.3+ [VERIFY] | AI orchestration | Pipeline orchestration for voice -> NLP -> LLM -> coding chain. LangGraph for conditional routing (local vs cloud). Evaluate whether lighter custom orchestration is sufficient -- LangChain adds significant dependency weight. |
| fhir.resources | 7.x [VERIFY] | FHIR data models | Pydantic-based FHIR R4 resource models. Validates FHIR JSON. Integrates naturally with FastAPI request/response models. |
| pyannote.audio | 3.x [VERIFY] | Speaker diarization | Separates physician vs patient speech in ambient recording. Requires HuggingFace token for model download. Runs locally after initial download. |

### Drug Safety

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| RxNav-in-a-Box | latest [VERIFY] | Drug interaction checking | Docker composition from NLM. Complete RxNorm API stack locally. DrugBank-sourced drug-drug interactions with severity ratings. No BAA required -- fully offline. Most privacy-preserving approach. |
| Docker Desktop | latest | Container runtime | Required for RxNav-in-a-Box. Evaluate Colima as lighter macOS alternative to Docker Desktop (avoids Docker Desktop licensing for commercial use). |

### Scheduling ML

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| XGBoost | 2.x [VERIFY] | No-show prediction | Gradient-boosted model on appointment history. Published AUC 0.75-0.85. Lightweight, trains on local data only. |
| scikit-learn | 1.4+ [VERIFY] | ML utilities | Feature engineering, model evaluation, preprocessing for scheduling ML. |

### Cloud AI (Fallback)

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| AWS Bedrock | N/A (managed) | Cloud LLM gateway | Hosts Claude and LLaMA with standard BAA. Near-instant approval. Zero data retention. Use for complex cases only (~5% of tasks). |
| boto3 | 1.34+ [VERIFY] | AWS SDK | Bedrock API access. Also used for future S3 backup and RDS connectivity in Phase 4. |

### Testing

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| Vitest | 2.x [VERIFY] | Frontend unit tests | Vite-native, faster than Jest. Compatible with React Testing Library. |
| React Testing Library | 16.x [VERIFY] | Component testing | Accessibility-first testing approach. Tests user behavior, not implementation. |
| Playwright | 1.x [VERIFY] | E2E testing | Cross-browser E2E. Tauri has Playwright integration via WebDriver. Test clinical workflows end-to-end. |
| pytest | 8.x [VERIFY] | Python testing | FastAPI test client, SQLAlchemy test fixtures, AI pipeline integration tests. |
| pytest-asyncio | latest | Async test support | FastAPI routes are async; tests must be too. |

### Development Tools

| Tool | Purpose | Notes |
|------|---------|-------|
| Biome | Linting + formatting (replaces ESLint + Prettier) | Faster, single tool. Tauri projects commonly use it. |
| Ruff | Python linting + formatting | Replaces flake8 + black + isort. 10-100x faster. |
| pre-commit | Git hook management | Enforce lint/format/type-check before commits. Critical for multi-developer consistency. |
| cargo-tauri | Tauri CLI | Build, dev server, sidecar management. |

---

## Alternatives Considered

| Category | Recommended | Alternative | When to Use Alternative |
|----------|-------------|-------------|-------------------------|
| Desktop shell | Tauri 2.x | Electron | Only if you need Chromium-specific features (e.g., Chrome DevTools Protocol for advanced debugging). Not recommended for this project. |
| Desktop shell | Tauri 2.x | Swift/AppKit native | If abandoning cross-platform entirely and going pure macOS. Better performance but 3-5x development cost and no web tech reuse. |
| Frontend | React 18+ | SolidJS | If starting fresh with no medical component ecosystem needs. Smaller bundle, finer reactivity. But medical UI libraries (medplum, fhir-react) are React-only. |
| State management | Zustand | Redux Toolkit | If team has strong Redux experience. But RTK adds boilerplate unnecessary for this app's state complexity. |
| ORM | SQLAlchemy 2.0 | Prisma (via TypeScript) | If the sidecar were Node.js instead of Python. Not applicable here since Python is required for AI/NLP. |
| Vector search | FAISS | ChromaDB | If you want a higher-level API with built-in persistence. FAISS is lower-level but faster and more battle-tested for medical code search at 70K+ vectors. |
| Vector search | FAISS | Qdrant (local mode) | If you need filtering + vector search combined. FAISS handles this use case fine with metadata pre-filtering. |
| LLM runtime | Ollama | MLX (Apple) | If targeting Apple Silicon exclusively and wanting tighter Metal integration. MLX is Python-native and potentially faster on M-series. Consider running both: Ollama for ease, MLX for performance-critical paths. |
| LLM runtime | Ollama | llama.cpp directly | If Ollama's HTTP overhead is measurable. Ollama wraps llama.cpp; direct integration via Python bindings (llama-cpp-python) removes one layer. |
| Transcription | whisper.cpp | WhisperKit (Swift) | For tighter CoreML/macOS integration. Evaluate both early. WhisperKit may have better Apple Silicon optimization but smaller community. |
| Transcription | whisper.cpp | Deepgram/AssemblyAI | Cloud transcription with BAAs available. Only if local transcription quality is insufficient after medical vocabulary tuning. Adds latency and cloud dependency. |
| AI orchestration | LangChain | Custom pipeline | If LangChain's dependency weight (~50+ transitive deps) is concerning. A custom orchestration layer with simple function chaining may be sufficient and more maintainable. Recommend: start with LangChain, extract to custom if it becomes a liability. |
| Python packaging | PyInstaller | Nuitka | If PyInstaller produces overly large binaries or has notarization issues. Nuitka compiles Python to C and can produce smaller, faster binaries. More complex setup. |
| Python packaging | PyInstaller | cx_Freeze | Fallback if PyInstaller fails. Less commonly used but viable. |
| Docker runtime | Docker Desktop | Colima | If Docker Desktop licensing is a concern (commercial use >250 employees). Colima is free, lighter, CLI-only. Fine for RxNav-in-a-Box. |

---

## What NOT to Use

| Avoid | Why | Use Instead |
|-------|-----|-------------|
| Electron | 150-300 MB idle RAM leaves insufficient headroom for 6-35 GB AI models. Node.js full API access requires extensive security hardening for HIPAA. 100-150 MB bundle. | Tauri 2.x |
| Create React App | Deprecated, unmaintained. Slow builds. | Vite (Tauri default) |
| Moment.js | Deprecated, 300KB+ bundle. Mutable API causes bugs. | date-fns |
| Redux (classic) | Excessive boilerplate for this app's state complexity. | Zustand |
| MongoDB | No encryption-at-rest equivalent to SQLCipher. No SQLAlchemy dialect for seamless cloud migration. Schema-less is wrong for regulated medical data. | SQLCipher (SQLite) -> PostgreSQL |
| Firebase | Google Cloud without standard BAA path for small teams. No local-first mode. Vendor lock-in. | SQLCipher + PowerSync (Phase 4) |
| OpenAI API (direct) | No BAA on consumer/API tiers without Azure deployment. Data retention policies unclear. | AWS Bedrock (standard BAA, zero retention) |
| Fine-tuned biomedical LLMs | OpenBioLLM-8B scored 30% vs LLaMA-3-8B + RAG at 64% on NEJM cases. Fine-tuning is expensive and underperforms RAG for clinical tasks. | RAG pipeline with general-purpose LLM |
| ESLint + Prettier (separate) | Two tools, complex config, slow. | Biome (single tool, faster) |
| flake8 + black + isort (separate) | Three tools for what Ruff does alone, 100x slower. | Ruff |
| Jest | Slower than Vitest, requires extra config with Vite projects. | Vitest |

---

## Stack Patterns by Variant

**If 16 GB Mac (minimum hardware):**
- Use LLaMA 3.2 3B instead of 3.1 8B (~3 GB vs ~6 GB)
- Use Whisper base or small instead of large-v3-turbo
- Skip pyannote speaker diarization (memory-intensive)
- Total AI footprint: ~4-5 GB, leaving headroom for app + OS

**If 32-64 GB Mac (optimal hardware):**
- Use full pipeline: LLaMA 3.1 8B + Whisper large-v3-turbo + MedSpaCy + pyannote
- Total AI footprint: ~10-15 GB with room for FAISS indices
- Consider MLX for faster Apple Silicon inference

**If MedSpaCy has Python compatibility issues:**
- Use spaCy 3.7+ directly with custom clinical pipeline components
- Implement ConText-like negation detection via spacy-negex or custom rules
- Use SciSpaCy models (these are maintained more actively)
- This fallback still covers 90% of MedSpaCy's value

**If PyInstaller notarization fails:**
- Switch to Nuitka for Python compilation
- Alternative: embed Python via PyO3 (Rust-Python bridge) directly in Tauri, eliminating the sidecar entirely. Higher complexity but better integration.

---

## Version Compatibility Matrix

| Package A | Must Be Compatible With | Risk Level | Notes |
|-----------|------------------------|------------|-------|
| MedSpaCy | Python 3.11/3.12, spaCy 3.7+ | HIGH | Last major release ~2023. Most likely compatibility issue in the stack. Test early. |
| SciSpaCy | spaCy 3.7+, MedSpaCy | MEDIUM | Usually tracks spaCy releases. Verify model compatibility. |
| sqlcipher3/pysqlcipher3 | SQLAlchemy 2.0, Python 3.11+ | MEDIUM | SQLAlchemy 2.0 changed dialect API. Verify binding works. |
| fhir.resources | Pydantic 2.x, FastAPI 0.100+ | LOW | Both Pydantic-based, should align. But verify FHIR R4 model generation. |
| PyInstaller | All Python deps, macOS notarization | MEDIUM | Hidden imports from spaCy/torch/transformers commonly cause issues. Budget 2-3 days for packaging. |
| Tauri sidecar | PyInstaller binary, localhost HTTP | LOW | Well-documented pattern. Tauri 2.x has explicit sidecar support. |
| React 18/19 | @medplum/react, fhir-react | MEDIUM | If upgrading to React 19, verify medical component libraries support it. |
| LangChain | Ollama, FAISS, Pydantic 2.x | MEDIUM | LangChain's rapid release cycle causes breaking changes. Pin versions carefully. |

---

## Installation (Phase 1 Bootstrap)

```bash
# Prerequisites
brew install rust node python@3.11

# Tauri CLI
cargo install tauri-cli

# Frontend
npm create tauri-app@latest medarc -- --template react-ts
cd medarc
npm install @mui/material @mui/icons-material @emotion/react @emotion/styled
npm install react-hook-form @hookform/resolvers zod
npm install @tanstack/react-table @tanstack/react-query
npm install zustand date-fns react-big-calendar
npm install @medplum/react fhir-react

# Dev dependencies
npm install -D vitest @testing-library/react @testing-library/jest-dom
npm install -D @biomejs/biome
npm install -D playwright @playwright/test

# Python sidecar (use venv)
python3.11 -m venv sidecar/.venv
source sidecar/.venv/bin/activate
pip install fastapi uvicorn[standard] pydantic
pip install sqlalchemy alembic
pip install pysqlcipher3  # or sqlcipher3 -- verify which is maintained
pip install fhir.resources
pip install spacy medspacy scispacy
python -m spacy download en_core_web_sm
pip install https://s3-us-west-2.amazonaws.com/ai2-s2-scispacy/releases/v0.5.4/en_core_med7_lg-0.5.4.tar.gz
pip install faiss-cpu langchain langchain-community
pip install ollama  # Python client for Ollama API
pip install pyinstaller
pip install pytest pytest-asyncio httpx  # httpx for FastAPI test client
pip install ruff

# Ollama (separate install)
brew install ollama
ollama pull llama3.1:8b-instruct-q4_K_M

# whisper.cpp (build from source for CoreML)
git clone https://github.com/ggerganov/whisper.cpp.git
cd whisper.cpp && make clean && WHISPER_COREML=1 make -j

# RxNav-in-a-Box (Phase 2)
# docker pull ghcr.io/nicktobey/rxnav-in-a-box  # verify current image location
```

---

## Key Risk Areas

1. **MedSpaCy compatibility (HIGH RISK):** Last major release was ~2023. If it does not support Python 3.11+/spaCy 3.7+, fall back to spaCy + SciSpaCy + custom clinical rules. Test this in week 1.

2. **PyInstaller + AI dependencies (MEDIUM RISK):** Packaging spaCy models, torch, transformers, and FAISS into a single PyInstaller binary is non-trivial. Hidden imports, large binary size (potentially 2-5 GB), and macOS notarization issues are common. Budget 3-5 days for packaging spikes.

3. **SQLCipher Python bindings (MEDIUM RISK):** pysqlcipher3 and sqlcipher3 have had maintenance gaps. Verify SQLAlchemy 2.0 dialect support. Alternative: use Rust-side SQLCipher via rusqlite with Tauri commands for all DB operations, removing Python DB dependency entirely.

4. **LangChain version churn (MEDIUM RISK):** LangChain releases break APIs frequently. Pin exact versions. Consider extracting to custom orchestration if LangChain becomes a maintenance burden.

5. **Whisper hallucination (INHERENT RISK):** ~1% hallucination rate is a known limitation. Human-in-the-loop review is mandatory, not optional. UI must make review frictionless.

---

## Sources

All findings in this document are based on training data with a **May 2025 cutoff**. Web verification tools (WebSearch, WebFetch, Brave API) were unavailable during this research session.

**Confidence breakdown:**
- Tauri 2.x architecture and capabilities: MEDIUM (training data aligns with PRD claims; version specifics need verification)
- React/TypeScript ecosystem: HIGH (stable, well-known ecosystem)
- FastAPI/SQLAlchemy/Alembic: HIGH (mature Python stack, unlikely to have changed significantly)
- SQLCipher: MEDIUM (stable project but verify latest version and Python binding status)
- Ollama/whisper.cpp: MEDIUM (rapidly evolving; versions likely newer than training data)
- MedSpaCy: LOW (maintenance status uncertain; highest risk item in stack)
- FAISS: HIGH (stable Meta project, well-established)
- LangChain: MEDIUM (frequent breaking changes; verify current API patterns)
- Medical UI libraries (@medplum/react, fhir-react): LOW (niche libraries; verify maturity and maintenance)

**Action required:** Before locking the stack, verify all [VERIFY] tagged versions against official GitHub releases and PyPI/npm. Prioritize MedSpaCy and SQLCipher Python binding verification as these are the highest-risk items.

---
*Stack research for: MedArc AI-Powered Desktop EMR*
*Researched: 2026-03-10*

# Feature Research

**Domain:** Small-practice EMR/EHR (1-5 providers), desktop-native, AI-powered
**Researched:** 2026-03-10
**Confidence:** MEDIUM-HIGH
**Baseline:** OpenEMR v8.0.0 (ONC-certified, Feb 2026) + competitor analysis (Practice Fusion, DrChrono, Tebra)

## Feature Landscape

### Table Stakes (Users Expect These)

Features physicians and staff assume exist. Missing any of these means the product is not a viable EMR -- practices will not switch from their current system.

#### Patient Management

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| Patient demographics CRUD | Foundation of every EMR; regulatory requirement | MEDIUM | Name, DOB, sex/gender, contact, insurance (primary/secondary/tertiary), employer, clinical identifiers, patient photo. Must support FHIR Patient resource. |
| Patient search (name, MRN, DOB, provider) | Staff search patients 50-100x/day; sub-second results required | LOW | Full-text + indexed field search. Must handle partial matches, phonetic similarity. |
| Insurance management (primary/secondary/tertiary) | Billing cannot function without it; every encounter ties to a payer | MEDIUM | Eligibility verification is Phase 2; basic insurance capture is Phase 1. |
| Related Persons / Care Team | Required for pediatrics, geriatrics, guardianship; OpenEMR baseline | LOW | Care Team Widget with role assignments (PCP, specialist, caregiver). |
| Allergy tracking | Patient safety -- drug interaction checks depend on it; malpractice risk if missing | LOW | Drug, food, environmental allergies with severity and reaction type. FHIR AllergyIntolerance resource. |
| Problem list / Active diagnoses | Core clinical record; required for coding, decision support, continuity of care | LOW | ICD-10 coded, date-stamped, active/inactive/resolved status. |
| Medication list | Patient safety; e-prescribing depends on it; reconciliation at every visit | LOW | Active, discontinued, historical. Links to RxNorm codes. |
| Immunization history | Regulatory reporting requirement; pediatric practices cannot function without it | LOW | CVX codes, lot numbers, administration dates, VIS documentation. |

#### Scheduling

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| Multi-provider calendar (day/week/month views) | Every practice has multiple providers; single-provider calendars are useless at 2+ providers | MEDIUM | Color-coded appointment categories, configurable slot durations (5-60 min). |
| Patient Flow Board | Real-time clinic tracking (checked in, roomed, with provider, checkout); OpenEMR baseline | MEDIUM | This is how front desk and nursing staff coordinate. Without it, workflow breaks down. |
| Recurring appointments | Chronic disease management requires follow-up scheduling; therapy/mental health require weekly slots | LOW | Weekly, biweekly, monthly recurrence patterns. |
| Appointment reminders (SMS/email) | 20-30% no-show rates without reminders; every competitor offers this | MEDIUM | Requires integration with SMS gateway (Twilio) and email service. Template-based. |
| Appointment search by open slots | Staff need to find next available appointment quickly when patient is on the phone | LOW | Filter by provider, appointment type, date range. |
| Waitlist management | Practices need to fill cancelled slots; OpenEMR baseline feature | LOW | Auto-notify patients when preferred slot opens. |
| Recall Board | Patient follow-up scheduling (annual physicals, chronic disease check-ins) | LOW | Overdue patient lists with outreach tracking. |

#### Clinical Documentation

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| SOAP note entry (structured) | The universal clinical documentation format; every EMR has this | HIGH | Subjective, Objective, Assessment, Plan with structured sub-fields. Must support free-text AND structured data entry. This is the single most-used feature in any EMR. |
| Vitals tracking with flowsheets | Nurses record vitals at every visit; trending over time is expected | MEDIUM | BP, HR, RR, Temp, SpO2, Weight, Height, BMI auto-calc, pain scale. Growth charts for pediatrics. |
| Review of Systems (ROS) forms | Standard intake documentation; required for E/M coding levels | MEDIUM | 14 organ systems, positive/negative/not reviewed, template-driven. |
| Physical exam templates | Structured PE documentation; required for E/M coding | MEDIUM | System-based templates (HEENT, CV, Pulm, etc.) with normal/abnormal findings. |
| Template library (clinical forms) | OpenEMR ships 60+ form types; physicians expect specialty-specific templates | HIGH | Custom form builder is Phase 2. Ship with 10-15 common templates (general, cardio, peds, OB/GYN, psych) for Phase 1. |
| Multi-provider encounter co-signing | Required for NP/PA supervision; legal documentation requirement | LOW | Supervising physician signs off on mid-level provider notes. |
| Clinical Decision Rules | Drug-allergy alerts, duplicate therapy, care gap reminders | MEDIUM | Passive alerts (info) vs active alerts (blocks workflow). Alert fatigue is a real risk -- be judicious. |
| Document management (upload/scan) | Practices receive faxes, outside records, consent forms; must store with patient | MEDIUM | PDF, image upload with categorization. SHA-1 integrity validation. Up to 64 MB per document. |

#### E-Prescribing

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| Medication search (drug database) | Providers must find medications by name, class, or indication | MEDIUM | Requires RxNorm drug database. Local-first for offline support. |
| E-prescribing transmission | Mandatory in many states; practices will not adopt an EMR without it | HIGH | Weno Exchange integration ($300 activation). SureScripts network connectivity. This is a hard external dependency. |
| EPCS (controlled substances) | DEA-required for Schedule II-V prescribing; growing state mandates | HIGH | Requires identity proofing, two-factor authentication, DEA-compliant audit trail. Weno supports this. |
| Drug interaction checks | Patient safety; malpractice liability without it | MEDIUM | RxNav-in-a-Box provides this locally via Docker. Severity ratings essential. |
| Formulary awareness | Reduces pharmacy callbacks; improves patient cost transparency | MEDIUM | Requires payer formulary data feeds -- complex to maintain. Phase 2 feature. |
| Medication reconciliation workflow | Required at transitions of care; meaningful use requirement | LOW | Side-by-side comparison of reported vs documented medications. |

#### Lab Integration

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| Lab results viewing and management | Providers review labs daily; results must flow into patient chart | MEDIUM | Manual entry for Phase 1. Electronic results in Phase 2. |
| Laboratory procedure ordering | Structured order entry with provider signature | MEDIUM | Order catalogue configuration, LOINC code mapping. |
| HL7 v2 message exchange | Standard lab interface; Quest, LabCorp, hospital labs all use HL7 v2 | HIGH | ORU^R01 (results), ORM^O01 (orders). Requires message parsing, acknowledgment, error handling. Phase 2 feature. |
| Results workflow (review, sign, notify) | Providers must review, acknowledge, and act on results; medicolegal requirement | MEDIUM | Abnormal flagging, provider notification, patient notification workflow. |

#### Billing and Revenue Cycle

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| CPT/ICD-10 code entry per encounter | Every encounter must be coded for billing; this is how practices get paid | MEDIUM | Fee sheet interface with code search. Must support CPT, HCPCS, ICD-10, SNOMED. |
| Fee schedule management | Practices have contracted rates per payer; need to track what they charge | LOW | Multiple fee schedules, modifier support (25, 59, etc.). |
| Claim generation (X12 837P) | Electronic claims are how 95%+ of billing happens; paper claims are nearly dead | HIGH | ANSI X12 5010 standard. Must validate before submission. Clearinghouse integration (Office Ally, ZirMED, Availity). |
| ERA/EOB processing (835) | Automated payment posting from insurance remittances | HIGH | Parsing 835 files, auto-matching to claims, posting payments, identifying denials. |
| Accounts Receivable tracking | Practices must track outstanding claims, aging, and collections | MEDIUM | AR aging reports (30/60/90/120 days), denial management, patient balance tracking. |
| Patient statements and collections | Patients owe increasing amounts due to high-deductible plans | LOW | Statement generation, payment plan tracking. |
| Insurance eligibility verification | Front desk verifies coverage before appointments | MEDIUM | Real-time eligibility via X12 270/271. Clearinghouse-dependent. Phase 2 feature. |

#### Reporting

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| Clinical reports (patient lists, encounters, prescriptions) | Practice management requires population views; regulatory reporting | MEDIUM | Filterable, exportable. Common: patients by diagnosis, encounter volume, prescription history. |
| Financial reports (collections, revenue, payer mix) | Practice owners need to understand financial health | MEDIUM | Daily/weekly/monthly revenue, collections rate, payer distribution, provider productivity. |
| CQM/eCQM measures | Required for MIPS reporting; penalty for non-participation | HIGH | Clinical Quality Measures calculation, submission formatting. Can defer to Phase 2 but architecture must support. |

#### Security and Compliance

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| RBAC (role-based access control) | HIPAA requires minimum-necessary access; every EMR has this | MEDIUM | 5 roles minimum: Admin, Provider, Nurse/MA, Billing, Front Desk. Field-level access control. |
| Audit logging (tamper-proof) | HIPAA requirement; medicolegal necessity | MEDIUM | Every ePHI access logged: who, what, when, from where. Hash-chain integrity. 6-year retention. |
| AES-256 encryption at rest | HIPAA technical safeguard; breach notification safe harbor | MEDIUM | SQLCipher handles this. Key management via macOS Keychain/Secure Enclave. |
| TLS 1.3 in transit | HIPAA transmission security requirement | LOW | macOS ATS enforces by default. Certificate pinning for API endpoints. |
| Unique user IDs + strong authentication | HIPAA requires no shared accounts; MFA increasingly expected | MEDIUM | Bcrypt/Argon2 hashing, TOTP MFA, Touch ID, auto-logoff (10-15 min). |
| Encrypted backups | HIPAA contingency plan requirement; data loss = practice closure | MEDIUM | 3-2-1 backup rule. Automated daily encrypted backups. Restore testing. |

### Differentiators (Competitive Advantage)

Features that set MedArc apart from Practice Fusion, DrChrono, Tebra, and OpenEMR. These align with the project's core value proposition.

#### AI-Powered Clinical Workflow (Primary Differentiator)

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| Ambient voice-to-SOAP note generation | Eliminates 30-41% of documentation time; the #1 physician complaint about EMRs. 42% of medical groups already using ambient AI -- this is rapidly becoming table stakes for new EMR adoption. | HIGH | whisper.cpp + MedSpaCy/SciSpaCy + LLaMA 3.1 8B pipeline. Human-in-the-loop mandatory (1% Whisper hallucination rate). This is the product's reason for existing. |
| AI-assisted ICD-10/CPT coding | Reduces claim denial rates from 8-12% to below 3%; delivers 3-7% revenue increase per practice. Concrete, measurable financial ROI. | HIGH | LLM entity extraction + FAISS vector search. GPT-4 only gets 33.9% exact match alone -- vector search architecture is required. Always human-reviewed, never auto-submitted. |
| AI diagnostic decision support | Differential diagnosis suggestions grounded in clinical evidence via RAG. Reduces cognitive load, catches missed diagnoses. | HIGH | LLaMA 3.1 8B + RAG (StatPearls, clinical guidelines) + FAISS. Local-first, no BAA needed. LLaMA-3-8B-Instruct: 64% on NEJM cases (vs 30% fine-tuned). |
| AI pre-charting (pre-visit context assembly) | Automatically assembles relevant history, pending results, due screenings before patient arrives. Saves 3-5 min per encounter setup. | MEDIUM | Pulls from problem list, recent encounters, pending orders, care gaps. Generates briefing for provider. |
| Smart scheduling (no-show prediction) | Reduces revenue loss from no-shows (average 18-20% of appointments); optimizes provider utilization. | MEDIUM | XGBoost/LightGBM on historical data. AUC 0.75-0.85 published. Overbooking suggestions, targeted reminder escalation. |

#### Local-First Architecture (Secondary Differentiator)

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| Zero monthly SaaS fees | Competitors charge $49-349/mo per provider with annual increases (SimplePractice +63% in 2025). One-time license eliminates recurring cost; ROI within 12-18 months for 3-provider practice. | LOW (business model, not technical) | Cloud hosting optional at $65-110/mo per clinic when practice chooses to migrate. |
| PHI never leaves device (routine operations) | Average healthcare breach costs $9.77M. Local encryption = HIPAA breach notification safe harbor. Eliminates trust dependency on third-party cloud providers. | MEDIUM | 95% of AI operations local. Cloud fallback only for complex cases with de-identified data. |
| Offline-first operation | Works without internet; rural clinics, unreliable connections, internet outages don't halt patient care. Cloud-only competitors go completely down during outages. | MEDIUM | SQLCipher local storage. PowerSync for cloud sync when connected. |
| macOS-native experience | Leverages CoreML, Secure Enclave, Touch ID, Keychain. Feels like a native app, not a web page. 30-50 MB idle (vs Electron 150-300 MB or browser tabs). | MEDIUM | Tauri 2.x + WKWebView. Apple Silicon optimization for AI models. |

#### Workflow Intelligence (Tertiary Differentiator)

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| FHIR-first data model | Future-proofs for ONC certification, interoperability, health information exchange without retrofitting. Every data point is standards-compliant from day one. | MEDIUM | FHIR R4 resources as JSON columns. Enables C-CDA generation, USCDI compliance. |
| Intelligent clinical alerts (low fatigue) | Alert fatigue causes providers to ignore 49-96% of alerts in typical EMRs. Tiered alert system with severity and suppression logic preserves attention for critical safety alerts. | MEDIUM | Passive (info bar) vs active (modal block) vs critical (requires override reason). Track override rates to tune. |
| Track Anything (arbitrary clinical data graphing) | Patients with rare conditions, custom metrics, or research needs can track any numeric value over time. OpenEMR differentiator worth replicating. | LOW | Generic form: name, value, date, optional units. Line chart visualization. |
| CAMOS (Computer-Aided Medical Ordering) | Structured clinical decision trees for common presentations. Reduces variation, improves consistency. | MEDIUM | Decision tree builder with branching logic. Phase 2 feature. |

### Anti-Features (Commonly Requested, Often Problematic)

Features that seem valuable but create problems disproportionate to their benefit. Deliberately avoid these.

| Feature | Why Requested | Why Problematic | Alternative |
|---------|---------------|-----------------|-------------|
| Patient portal (Phase 1) | Patients want online access to records, messaging, appointments | Massive surface area: authentication, authorization, web hosting, HIPAA for public-facing app, mobile responsiveness, patient identity verification. Doubles the security attack surface. Not what makes physicians switch EMRs. | Defer to Phase 2+. Physicians adopt based on clinical workflow, not patient portal. Secure messaging via existing channels (phone, email with consent) for Phase 1. |
| Full ONC certification (Phase 1) | Needed for MIPS incentive payments; seems like a requirement | ONC certification process costs $50-100K+, takes 6-12 months, requires specific testing. Technically voluntary. Architecture should support it, but pursuing certification before product-market fit burns runway. | Build FHIR-first, USCDI-compliant data model from day one. Pursue certification when revenue supports it (Phase 3-4). |
| Real-time multi-user collaboration (Google Docs-style) | Multiple providers editing same note simultaneously | Operational conflict resolution in clinical notes is a patient safety hazard. CRDTs for medical records are an unsolved problem. Small practices rarely have concurrent edits on same patient. | Encounter locking (one editor at a time) + co-signing workflow. Simpler, safer, sufficient for 1-5 provider practices. |
| Custom form builder (Phase 1) | Every specialty wants custom forms; OpenEMR has this | Form builders are deceptively complex: validation logic, conditional fields, data extraction for reporting, FHIR mapping, migration. Ship with 10-15 pre-built templates, add builder in Phase 2. | Pre-built specialty templates (general, cardio, peds, OB/GYN, ortho, psych, derm). Community template sharing in Phase 3. |
| Integrated fax server | Practices still receive faxes; seems necessary | Fax integration requires HIPAA-compliant fax service, phone line management, OCR for incoming, formatting for outgoing. Third-party fax services (eFax, SRFax) handle this better. | Integrate with cloud fax API (Phase 2). For Phase 1, manual document upload covers the need. |
| Built-in telemedicine/video | Post-COVID expectation; competitors offer it | Video infrastructure is complex (WebRTC, TURN servers, recording, consent). HIPAA-compliant video solutions exist (Doxy.me, Zoom for Healthcare). Building your own is a distraction. | Integration with existing telehealth platform. Launch link from appointment, attach visit note to encounter. Phase 3. |
| Mobile companion app (Phase 1) | Providers want to check schedules, review results on phone | Doubles the development surface. Tauri 2.x supports iOS/Android but mobile EMR UX is fundamentally different from desktop. Small practices manage fine with desktop-only initially. | Phase 4+ after core desktop is solid. Mobile web view as stopgap if demand is high. |
| Automated claim submission (no human review) | Speeds up billing workflow | Medical billing errors have financial and legal consequences. AI coding has 33.9% exact match rate (GPT-4). Auto-submitting claims without review guarantees denials and potential fraud liability. | AI suggests codes; human reviews and approves. "One-click submit" after review, not "zero-click auto-submit." |
| Windows/Linux support (Phase 1) | Larger addressable market | Triples testing surface, loses macOS-specific advantages (CoreML, Secure Enclave, Keychain, Touch ID). Tauri supports cross-platform but optimization is macOS-specific. | macOS-first. Tauri enables future cross-platform. Revisit after product-market fit. |
| Natural language query of patient data | "Show me all diabetic patients with A1c > 9" | NL-to-SQL is unreliable for clinical data queries. Wrong results have patient safety implications. Requires extensive guardrails. | Structured report builder with predefined filters. Saved report templates. Consider NL query as Phase 3 AI feature with heavy validation. |

## Feature Dependencies

```
[Patient Demographics CRUD]
    |
    +--requires--> [Allergy Tracking]
    |                  |
    |                  +--enables--> [Drug Interaction Checks]
    |                                    |
    |                                    +--enables--> [E-Prescribing]
    |                                                      |
    |                                                      +--requires--> [Weno Exchange Integration]
    |
    +--requires--> [Problem List / Active Diagnoses]
    |                  |
    |                  +--enables--> [CPT/ICD-10 Coding]
    |                  |                 |
    |                  |                 +--enables--> [AI Coding Suggestions]
    |                  |                 |
    |                  |                 +--enables--> [Claim Generation (837P)]
    |                  |                                   |
    |                  |                                   +--enables--> [ERA Processing (835)]
    |                  |                                   |
    |                  |                                   +--enables--> [AR Tracking]
    |                  |
    |                  +--enables--> [AI Diagnostic Support]
    |
    +--requires--> [Medication List]
    |                  |
    |                  +--enables--> [Medication Reconciliation]
    |                  +--enables--> [E-Prescribing]
    |
    +--requires--> [Insurance Management]
                       |
                       +--enables--> [Claim Generation]
                       +--enables--> [Eligibility Verification]

[Scheduling / Calendar]
    |
    +--requires--> [Patient Demographics] (appointment must link to patient)
    |
    +--enables--> [Patient Flow Board]
    |
    +--enables--> [Recall Board]
    |
    +--enables--> [Appointment Reminders] --requires--> [SMS/Email Gateway]
    |
    +--enables--> [AI Smart Scheduling] --requires--> [Historical Appointment Data]

[SOAP Note Entry]
    |
    +--requires--> [Patient Demographics] + [Encounter Context]
    |
    +--enables--> [Vitals Tracking] (recorded within encounter)
    |
    +--enables--> [AI Voice-to-SOAP] --requires--> [whisper.cpp] + [NLP Pipeline] + [Local LLM]
    |
    +--enables--> [Clinical Decision Rules] --requires--> [Allergy List] + [Medication List] + [Problem List]
    |
    +--enables--> [Multi-provider Co-signing]

[RBAC + Authentication]
    |
    +--required-by--> [EVERYTHING] (no feature works without user identity and access control)

[Audit Logging]
    |
    +--required-by--> [EVERYTHING that touches ePHI] (HIPAA mandate)

[Encrypted Database (SQLCipher)]
    |
    +--required-by--> [All Data Storage] (HIPAA encryption requirement)
```

### Dependency Notes

- **RBAC + Auth + Audit + Encryption are foundation layers:** These must exist before any clinical feature. They are not features users interact with directly, but without them, no feature is HIPAA-compliant.
- **Patient Demographics is the data backbone:** Every clinical, billing, and scheduling feature links back to a patient record. Build this first and build it right.
- **Allergy + Medication + Problem List form the "safety triad":** Drug interaction checks, clinical decision rules, and AI diagnostic support all depend on accurate, coded clinical data. These must be populated before AI features add value.
- **Billing depends on clinical documentation:** You cannot code an encounter that has not been documented. SOAP notes must exist before CPT/ICD-10 coding, which must exist before claim generation.
- **AI features are enhancement layers, not foundations:** Every AI feature enhances an underlying manual workflow. The manual workflow must work perfectly before AI is layered on top. This is why AI is Phase 3, not Phase 1.
- **E-prescribing has hard external dependencies:** Weno Exchange integration requires activation ($300), SureScripts network enrollment, and identity proofing for EPCS. These have lead times measured in weeks. Start the process early even if the feature ships in Phase 2.

## MVP Definition

### Launch With (v1 -- Phase 1, Months 1-6)

The minimum viable EMR that a solo practitioner could use for daily patient care without AI features.

- [ ] **RBAC + Authentication + Audit Logging** -- HIPAA foundation; everything depends on this
- [ ] **SQLCipher encrypted database with FHIR data model** -- Data layer must be right from day one; retrofitting FHIR later is a rewrite
- [ ] **Patient demographics CRUD with search** -- Cannot do anything without patient records
- [ ] **Allergy, medication, and problem list management** -- The clinical safety triad; required for any meaningful clinical documentation
- [ ] **Appointment scheduling (multi-provider calendar, flow board)** -- How patients get seen; front desk cannot function without this
- [ ] **SOAP note entry (structured, template-based)** -- The core clinical workflow; 10-15 pre-built specialty templates
- [ ] **Vitals tracking with flowsheets** -- Nurses record at every visit; required for clinical documentation
- [ ] **ROS and physical exam forms** -- Required for E/M coding compliance
- [ ] **Lab results viewer (manual entry)** -- Providers must review and document lab results
- [ ] **Document upload and management** -- Outside records, consent forms, faxed documents
- [ ] **Encrypted backups** -- HIPAA contingency plan; cannot lose patient data
- [ ] **macOS code-signed, notarized DMG with auto-updates** -- Distribution mechanism

### Add After Validation (v1.x -- Phase 2, Months 7-10)

Features to add once core clinical workflow is validated with real users.

- [ ] **Billing module (fee sheets, 837P claims, 835 ERA processing, AR tracking)** -- Add when practices need to bill through MedArc instead of a separate billing system
- [ ] **E-prescribing via Weno Exchange (including EPCS)** -- Add when practices want to prescribe from within the EMR; start Weno enrollment in Phase 1
- [ ] **Drug interaction checking via RxNav-in-a-Box** -- Ships with e-prescribing; requires Docker runtime on clinic machine
- [ ] **HL7 v2 lab interface** -- Add when practices want electronic lab ordering/results instead of manual entry
- [ ] **Insurance eligibility verification (270/271)** -- Add when billing module is live and practices want real-time eligibility
- [ ] **CQM/eCQM reporting** -- Add when practices need MIPS reporting; architecture must support from Phase 1
- [ ] **Custom form builder** -- Add when 10-15 pre-built templates are insufficient for user needs
- [ ] **Referral management** -- Add when practices need structured referral tracking beyond fax/phone
- [ ] **Financial and clinical reports (full suite)** -- Basic reports in Phase 1; full report builder in Phase 2

### Future Consideration (v2+ -- Phase 3-4, Months 11-18)

Features to defer until product-market fit is established and core EMR is stable.

- [ ] **AI voice-to-SOAP generation** -- Phase 3; the flagship differentiator, but the manual SOAP workflow must be solid first
- [ ] **AI coding suggestions (ICD-10/CPT)** -- Phase 3; enhances billing workflow that must already work manually
- [ ] **AI diagnostic decision support** -- Phase 3; RAG pipeline requires stable clinical data to query against
- [ ] **AI smart scheduling** -- Phase 3; requires historical data that only exists after months of scheduling use
- [ ] **AI pre-charting** -- Phase 3; requires encounter history and clinical data to assemble
- [ ] **Cloud migration (PowerSync + AWS RDS)** -- Phase 4; only when practices need multi-device or multi-location
- [ ] **Patient portal** -- Phase 4+; patient-facing features after clinical workflow is proven
- [ ] **Mobile companion** -- Phase 4+; after desktop is feature-complete
- [ ] **Telemedicine integration** -- Phase 3+; integrate with existing platforms, do not build video infrastructure
- [ ] **ONC certification pursuit** -- Phase 4+; when revenue supports $50-100K+ certification cost

## Feature Prioritization Matrix

| Feature | User Value | Implementation Cost | Priority | Phase |
|---------|------------|---------------------|----------|-------|
| RBAC + Auth + Audit | HIGH (compliance gate) | MEDIUM | P1 | 1 |
| SQLCipher + FHIR data model | HIGH (foundation) | MEDIUM | P1 | 1 |
| Patient demographics CRUD | HIGH | LOW | P1 | 1 |
| Patient search | HIGH | LOW | P1 | 1 |
| Allergy/Medication/Problem lists | HIGH (safety) | LOW | P1 | 1 |
| Appointment scheduling + calendar | HIGH | MEDIUM | P1 | 1 |
| Patient Flow Board | HIGH | MEDIUM | P1 | 1 |
| SOAP note entry (structured) | HIGH | HIGH | P1 | 1 |
| Vitals tracking | HIGH | LOW | P1 | 1 |
| ROS + Physical exam forms | MEDIUM | MEDIUM | P1 | 1 |
| Lab results (manual entry) | MEDIUM | LOW | P1 | 1 |
| Document management | MEDIUM | MEDIUM | P1 | 1 |
| Encrypted backups | HIGH (compliance) | MEDIUM | P1 | 1 |
| macOS distribution (DMG + updates) | HIGH (delivery) | MEDIUM | P1 | 1 |
| Billing (fee sheets, 837P, 835) | HIGH | HIGH | P2 | 2 |
| E-prescribing (Weno) | HIGH | HIGH | P2 | 2 |
| Drug interaction checks | HIGH (safety) | MEDIUM | P2 | 2 |
| HL7 v2 lab interface | MEDIUM | HIGH | P2 | 2 |
| Insurance eligibility | MEDIUM | MEDIUM | P2 | 2 |
| CQM/eCQM reporting | MEDIUM | HIGH | P2 | 2 |
| Recurring appointments | MEDIUM | LOW | P2 | 1-2 |
| Appointment reminders (SMS/email) | MEDIUM | MEDIUM | P2 | 2 |
| Recall Board | LOW | LOW | P2 | 2 |
| AI voice-to-SOAP | HIGH (differentiator) | HIGH | P2 | 3 |
| AI coding suggestions | HIGH (financial ROI) | HIGH | P2 | 3 |
| AI diagnostic support | MEDIUM | HIGH | P3 | 3 |
| AI smart scheduling | LOW | MEDIUM | P3 | 3 |
| AI pre-charting | MEDIUM | MEDIUM | P3 | 3 |
| Cloud migration | MEDIUM | HIGH | P3 | 4 |
| Patient portal | LOW (for physician adoption) | HIGH | P3 | 4+ |
| Mobile companion | LOW | HIGH | P3 | 4+ |

**Priority key:**
- P1: Must have for launch (Phase 1 MVP)
- P2: Should have, add when possible (Phase 2-3)
- P3: Nice to have, future consideration (Phase 3-4+)

## Competitor Feature Analysis

| Feature | OpenEMR v8 | Practice Fusion | DrChrono | Tebra | MedArc Approach |
|---------|-----------|-----------------|----------|-------|-----------------|
| Patient Management | Full CRUD, care team, photo, SDOH | Full CRUD, basic | Full CRUD, iPad-native | Full CRUD, integrated PM | Full CRUD + FHIR-native + care team |
| Scheduling | Flow board, recall, multi-provider | Basic calendar | Calendar + check-in kiosk | Calendar + online booking | Multi-provider + flow board + AI smart scheduling (Phase 3) |
| SOAP Notes | Structured + CAMOS + 60+ forms | Template-based | iPad dictation + templates | Template-based | Structured + AI voice-to-SOAP (Phase 3); 10-15 templates Phase 1 |
| E-Prescribing | Weno Exchange, EPCS | SureScripts, EPCS | SureScripts, EPCS | SureScripts, EPCS | Weno Exchange, EPCS (Phase 2) |
| Drug Interactions | Basic checking | Basic | Basic | Basic | RxNav-in-a-Box with severity ratings (Phase 2) |
| Lab Integration | HL7 v2, manual entry | HL7, Quest/LabCorp | HL7, built-in lab ordering | HL7, limited | HL7 v2 (Phase 2), manual entry (Phase 1) |
| Billing | 837P/835, fee sheets, AR | 837P, basic billing | 837P/835, RCM services | Full RCM suite | 837P/835, AR, AI-assisted coding (Phase 2-3) |
| Reporting | CQM, clinical, financial | Basic, CQM | Basic, CQM | Full analytics | CQM + clinical + financial (Phase 2) |
| AI Documentation | None (no AI) | None | Voice dictation (basic) | None | Ambient AI with SOAP generation -- primary differentiator |
| AI Coding | None | None | None | None | FAISS vector search + LLM entity extraction -- unique |
| AI Diagnostics | None | None | None | None | RAG-powered differential diagnosis -- unique |
| Pricing | Free (open source) | Free (ad-supported) then $149+/mo | $199-399/mo per provider | $125-349/mo per provider | One-time license, zero monthly fees |
| Deployment | Self-hosted (cloud/local) | Cloud-only | Cloud-only | Cloud-only | Local-first macOS desktop, optional cloud |
| Data Privacy | Self-controlled | Practice Fusion controls data | DrChrono controls data | Tebra controls data | PHI never leaves device; patient owns data |
| Offline Support | Yes (self-hosted) | No | No | No | Yes, full offline operation |
| Support Quality | Community (variable) | Poor (95% cite issues) | Poor-moderate | Poor-moderate | Self-contained app reduces support dependency |

## Sources

- **Primary:** MedArc Day0.md requirements document (comprehensive PRD with cited statistics and competitor analysis) -- MEDIUM-HIGH confidence
- **OpenEMR feature baseline:** Derived from Day0.md analysis of OpenEMR v8.0.0 features -- MEDIUM confidence (could not verify against OpenEMR wiki directly due to tool restrictions; features align with known OpenEMR capabilities from training data)
- **Competitor pricing and features:** Day0.md cites Practice Fusion, DrChrono, Tebra specifics -- MEDIUM confidence (training data corroborates general competitive landscape; specific pricing may have changed)
- **AI accuracy statistics:** Day0.md cites specific published research (Whisper hallucination rates, GPT-4 ICD-10 accuracy, LLaMA NEJM scores, RxNav-in-a-Box capabilities) -- MEDIUM confidence (statistics are plausible and internally consistent but not independently verified against original papers)
- **HIPAA requirements:** Well-established regulatory framework; HIGH confidence from training data
- **Alert fatigue statistic (49-96% override rate):** Widely cited in clinical informatics literature -- MEDIUM confidence
- **No-show prediction AUC (0.75-0.85):** Consistent with published ML literature on appointment no-show prediction -- MEDIUM confidence
- **SimplePractice 63% price increase:** Cited in Day0.md -- LOW confidence (single source, not independently verified)

---
*Feature research for: Small-practice EMR/EHR (1-5 providers)*
*Researched: 2026-03-10*

# Pitfalls Research

**Domain:** AI-powered desktop EMR with local-first architecture
**Researched:** 2026-03-10
**Confidence:** MEDIUM (training data only -- web search unavailable; however, HIPAA regulations, HL7/FHIR standards, X12 claims processing, and e-prescribing requirements are well-established and stable domains where training data is reliable)

---

## Critical Pitfalls

### Pitfall 1: Treating HIPAA Compliance as a Checkbox Instead of an Architecture

**What goes wrong:**
Teams implement encryption and access controls in the final sprint before launch. They discover that audit logging was bolted on after the fact and misses actions, that RBAC was added to route handlers but not to database queries (allowing direct SQL access to bypass controls), and that the encryption key management was improvised. The result is a system that "has encryption" but fails a real audit because the security architecture was not designed in from day one.

**Why it happens:**
HIPAA's Security Rule reads like a list of checkboxes (encryption: check, audit logs: check, access controls: check). Developers treat each as an independent feature rather than an integrated architecture. The 45 CFR 164.312 requirements are deceptively simple to read but interconnected in implementation.

**How to avoid:**
- Build the audit log table and logging middleware in the very first sprint, before any PHI-touching code exists. Every database operation must flow through a layer that logs automatically -- not opt-in per endpoint.
- Implement RBAC at the repository/data-access layer, not the route/controller layer. A query for patient records must enforce role-based filtering regardless of which code path invokes it.
- Store the SQLCipher encryption key in macOS Keychain from day one. Never use a hardcoded key "temporarily" -- there is no temporary in healthcare software.
- Implement field-level encryption for 42 CFR Part 2 data (substance abuse), psychotherapy notes, and HIV status from the first schema migration. These have stricter access rules than general PHI.

**Warning signs:**
- Audit log tests are deferred or marked as "will add later"
- Any PHI-containing endpoint lacks corresponding audit log assertions in tests
- Encryption key appears in source code, environment variables, or configuration files
- RBAC checks exist only in frontend code or route middleware but not in the data layer

**Phase to address:** Phase 1 (MVP) -- this is not Phase 2 work. The entire data access layer must be audit-logged and role-filtered from the first line of database code.

---

### Pitfall 2: AI Hallucination in Clinical Documentation Without Adequate Safety Rails

**What goes wrong:**
Whisper generates plausible but fabricated medical terms ("hyperactivated antibiotics," phantom medication names, invented dosages). The LLM-generated SOAP note includes clinically reasonable but factually incorrect assessments. The provider, fatigued and trusting the AI, approves the note without catching the error. The hallucinated content becomes part of the legal medical record and drives downstream clinical decisions, prescriptions, or billing codes.

**Why it happens:**
Whisper's ~1% hallucination rate means roughly 1 in 100 transcription segments contains fabricated content. LLaMA 3.1 8B with RAG achieves ~94% accuracy but still hallucinates 4-6% of the time. These rates sound low but in a medical context with 20+ encounters per day, a provider will encounter multiple hallucinations daily. The danger compounds because AI-generated text is fluent and confident -- hallucinations look identical to correct output.

**How to avoid:**
- Implement a mandatory human-in-the-loop review workflow where AI-generated content is visually distinct (different background color, "AI-generated" watermark) and requires explicit provider confirmation before becoming part of the medical record.
- Build a confidence scoring system: Whisper outputs token-level confidence scores. Flag any segment below 0.85 confidence with visual highlighting for mandatory review.
- Cross-validate critical entities: if the AI transcribes a medication name, verify it exists in RxNorm. If it generates a diagnosis, verify the ICD-10 code exists. If a dosage is mentioned, check it against known ranges in RxNav.
- Never auto-populate prescription fields from AI output. Prescriptions must always be entered through the structured e-prescribing workflow, not copied from AI-generated notes.
- Log every AI-generated field separately from human-entered data with provenance tracking. The audit trail must distinguish "AI-suggested, provider-confirmed" from "provider-entered."

**Warning signs:**
- UI mockups show AI output in the same visual style as human-entered data
- No entity validation pipeline between AI output and medical record storage
- Provider review workflow allows bulk-approve of multiple AI-generated notes
- No confidence score display or threshold-based flagging

**Phase to address:** Phase 3 (AI Enhancement) -- but the data model supporting provenance tracking ("source: ai | human") must be designed in Phase 1 schema.

---

### Pitfall 3: FHIR Data Model Impedance Mismatch with SQLite/Relational Storage

**What goes wrong:**
FHIR resources are deeply nested JSON documents with polymorphic references (a Reference can point to Patient, Practitioner, Organization, etc.). Teams store the full JSON blob in a column and then discover they cannot efficiently query across resources ("find all medications for patients with diabetes"), cannot enforce referential integrity between resources, and face O(n) scans for any cross-resource query. Alternatively, teams fully normalize FHIR into relational tables and end up with 200+ tables with complex joins that make simple operations slow and the codebase unmanageable.

**Why it happens:**
FHIR was designed for REST API interchange, not relational storage. The spec has 145+ resource types with deeply nested structures. Neither pure JSON storage nor full normalization works well. Teams pick one extreme and discover the problems 6 months in.

**How to avoid:**
- Use a hybrid approach: store the canonical FHIR JSON in a column for standards compliance and interchange, but maintain indexed lookup tables for the 15-20 fields you actually query (patient MRN, encounter date, medication name, diagnosis code, provider ID). This is already in the PROJECT.md -- enforce it rigorously.
- Define a strict subset of FHIR resources you support (Patient, Encounter, Observation, MedicationRequest, Condition, Procedure, DiagnosticReport, AllergyIntolerance, Claim). Do not attempt to support all 145+ resource types.
- Build a FHIR validation layer using fhir.resources (Pydantic-based) that validates every resource before storage. Invalid FHIR in, garbage out -- enforce this at the repository boundary.
- Create materialized views or denormalized tables for the specific query patterns your UI needs (patient summary, encounter timeline, medication list). Update these on write.
- Test with realistic data volumes: a 10-year patient with chronic conditions can have 5,000+ Observations. Query performance against 50,000+ resources per patient must be validated.

**Warning signs:**
- Schema has either one "resources" table or 100+ tables
- Queries for common UI views require more than 2 joins
- No FHIR validation at the data access boundary
- JSON blob queries using SQLite json_extract in WHERE clauses on large tables

**Phase to address:** Phase 1 (MVP) -- the data model is the foundation. Getting this wrong means a rewrite.

---

### Pitfall 4: E-Prescribing Integration Treated as a Simple API Call

**What goes wrong:**
Teams assume e-prescribing is "send a prescription to a pharmacy via API." In reality, EPCS (Electronic Prescribing of Controlled Substances) requires DEA-mandated two-factor authentication using a CLIA-certified identity proofing process, a third-party auditor (Drummond Group, Leidos) must certify the EPCS module, and the prescriber's DEA certificate must be validated in real-time. Weno Exchange integration requires passing their certification process, which takes 4-8 weeks minimum. NCPDP SCRIPT 2017071 is the required message format -- not a simple REST API.

**Why it happens:**
E-prescribing looks like a solved problem from the outside. But it is one of the most regulated integrations in healthcare IT, governed by DEA regulations (21 CFR Part 1311), state pharmacy board rules (which vary by state), and NCPDP standards. The Surescripts network (which routes most e-prescriptions) has its own certification requirements.

**How to avoid:**
- Budget 3-4 months for e-prescribing integration, not 2-3 weeks. Start the Weno Exchange certification process at the beginning of Phase 2, not the end.
- EPCS is a separate, higher-bar certification from basic e-prescribing. Plan it as a distinct deliverable. Many EMRs launch without EPCS and add it later.
- Implement the full NCPDP SCRIPT message lifecycle: NewRx, RxChangeRequest, RxChangeResponse, CancelRx, RxRenewalRequest, RxRenewalResponse, RxFill. Each has its own workflow and error handling.
- Drug interaction checking must happen BEFORE the prescription is transmitted, not after. RxNav-in-a-Box handles this locally, but the UI must block transmission until interactions are reviewed.
- Formulary checking (is this drug covered by the patient's insurance?) requires connectivity to pharmacy benefit managers. Defer this to Phase 3+ if budget is tight.

**Warning signs:**
- E-prescribing is scoped as a single 2-week sprint
- No distinction between basic prescribing and EPCS in the roadmap
- Weno Exchange certification timeline not accounted for in project schedule
- Drug interaction checking is planned as a "nice to have" rather than a safety gate

**Phase to address:** Phase 2 (Feature Parity) -- but certification processes should be initiated at the start of Phase 2, not when the code is ready.

---

### Pitfall 5: X12 837P/835 Claims Processing Brittleness

**What goes wrong:**
The X12 EDI format is a positional, segment-based format from the 1980s with thousands of situational rules. Teams build a happy-path claims generator that produces valid-looking X12 but gets rejected by clearinghouses at a 30-40% rate because: segment qualifiers are wrong for the specific payer, required loops are missing for the claim type (professional vs. institutional), NPI/taxonomy code combinations are invalid, or the rendering/billing/referring provider hierarchy is incorrect. Each payer has different adjudication rules on top of the X12 standard.

**Why it happens:**
The X12 837P Implementation Guide is 900+ pages. The "standard" has hundreds of situational rules ("include this segment IF the claim involves anesthesia AND the payer is Medicare"). Clearinghouses like Office Ally and Availity add their own validation layers. Most rejected claims fail on data quality issues (wrong subscriber ID format, missing secondary insurance info) rather than structural X12 errors.

**How to avoid:**
- Use an existing X12 library (pyx12, or a commercial library) rather than hand-rolling segment generation. The positional format with segment terminators, element separators, and sub-element separators is deceptively complex.
- Implement a claim scrubbing/validation step BEFORE transmission that checks: all required segments present for the claim type, NPI numbers validate against NPPES, ICD-10 codes are valid and not truncated, CPT/HCPCS modifiers are appropriate for the code, patient subscriber ID matches payer format requirements.
- Build a clearinghouse abstraction layer. Office Ally, Availity, and Change Healthcare all have different submission APIs and different rejection code formats. Do not couple your claims logic to a single clearinghouse.
- ERA (835) processing is equally complex: payment amounts must reconcile against claim amounts, adjustment reason codes (CARCs/RARCs) must be mapped to human-readable explanations, and denied claims must flow into a rework queue.
- Start with a single clearinghouse (Office Ally is free for claim submission) and validate end-to-end with real test claims before adding alternatives.

**Warning signs:**
- Claims module generates X12 from string concatenation rather than a structured library
- No claim validation/scrubbing step before transmission
- Testing uses only synthetic data without real clearinghouse submission tests
- ERA processing only handles paid claims, not partial payments, denials, or adjustments

**Phase to address:** Phase 2 (Feature Parity) -- claims processing needs real-world testing with a clearinghouse sandbox. Budget 6-8 weeks, not 2-3.

---

### Pitfall 6: SQLCipher Performance Cliff with Clinical Data Volumes

**What goes wrong:**
SQLCipher adds 5-15% overhead on simple operations, but this compounds with complex queries. A practice with 5 providers seeing 25 patients/day accumulates 30,000+ encounters per year with 150,000+ observations. When the UI needs to render a patient's full history with medications, allergies, labs, and vitals, the query hits multiple JSON column extractions across tens of thousands of rows. The provider waits 3-5 seconds for a patient chart to load -- unacceptable in a clinical workflow where they switch patients every 15 minutes.

**Why it happens:**
SQLite is single-writer. SQLCipher adds encryption overhead to every page read. JSON column queries (json_extract) cannot use indexes efficiently. The FHIR JSON storage pattern means even simple lookups require JSON parsing. These issues are invisible with 100 test patients but catastrophic with 50,000+ real records.

**How to avoid:**
- Create covering indexes on the lookup/denormalized tables for every common query pattern. The patient chart view, encounter list, medication list, and lab results should each have a purpose-built indexed query, not a generic FHIR resource scan.
- Implement pagination everywhere. Never load a patient's full encounter history -- load the most recent 20 with a "load more" pattern.
- Use SQLite WAL mode (Write-Ahead Logging) for concurrent read performance. This is compatible with SQLCipher.
- Pre-compute and cache the patient summary (active medications, active problems, allergies, recent vitals) in a denormalized table that updates on write. The chart-open operation reads one row, not 50 tables.
- Load test with realistic data: generate 5 years of synthetic but realistic clinical data (use Synthea) and validate that chart-open, patient-search, and encounter-list all complete in under 500ms.
- Set a hard performance budget: any user-facing query must complete in <200ms. Add automated performance regression tests.

**Warning signs:**
- No load testing with more than 100 patients
- Patient chart view queries the FHIR JSON directly rather than indexed lookup tables
- No pagination on list views
- No performance regression tests in CI

**Phase to address:** Phase 1 (MVP) -- performance architecture must be designed up front. Retrofitting indexes and denormalized tables into a live database with patient data is operationally risky.

---

### Pitfall 7: Tauri + Python Sidecar Process Lifecycle Management

**What goes wrong:**
The FastAPI Python sidecar (compiled via PyInstaller) crashes, hangs, or fails to start. The main Tauri app has no visibility into the sidecar's health. Providers click "Generate SOAP Note" and nothing happens -- no error message, no timeout, just a spinner. Or worse: the sidecar process becomes a zombie, consuming memory and GPU resources, and the only fix is force-quitting the entire application, losing unsaved clinical data.

**Why it happens:**
Tauri's sidecar support launches an external process but does not inherently manage its health. PyInstaller binaries can fail on macOS due to Gatekeeper issues, missing runtime dependencies, or code signing problems. The Python process running Ollama inference can hang on large inputs, run out of memory when loading models, or deadlock on concurrent requests. These failure modes are invisible to the Rust host process.

**How to avoid:**
- Implement a health check endpoint in the FastAPI sidecar (/health) that the Tauri app polls every 5 seconds. If 3 consecutive health checks fail, automatically restart the sidecar.
- Add request timeouts on every sidecar call: 30 seconds for transcription, 60 seconds for SOAP generation, 10 seconds for entity extraction. Display a meaningful error to the provider on timeout, not a spinner.
- Design the core EMR to function without the sidecar. Patient lookup, scheduling, manual note entry, prescribing, and billing must all work when the AI sidecar is down. The sidecar is an enhancement, not a dependency for core workflows.
- Implement graceful degradation: if Ollama is not available or the model is not loaded, show "AI features unavailable" in the UI and disable AI buttons. Do not show errors or crash.
- PyInstaller binary must be code-signed and include the hardened runtime entitlements. Test the compiled binary on a clean macOS install (not a developer machine) before every release.
- Monitor sidecar memory usage. LLaMA 3.1 8B Q4 uses ~6GB. If memory exceeds 80% of available unified memory, refuse to load additional models and alert the user.

**Warning signs:**
- No health check endpoint on the sidecar
- Core EMR features (charting, scheduling) make calls to the Python sidecar
- No timeout handling on AI inference calls
- Sidecar tested only on developer machines, never on clean installs

**Phase to address:** Phase 1 (MVP) for sidecar lifecycle management; Phase 3 (AI) for AI-specific resilience.

---

## Technical Debt Patterns

| Shortcut | Immediate Benefit | Long-term Cost | When Acceptable |
|----------|-------------------|----------------|-----------------|
| Skipping field-level encryption for sensitive PHI | Faster Phase 1 delivery | Fails HIPAA audit for 42 CFR Part 2 data; requires full database migration to add later | Never -- substance abuse and psychotherapy notes have stricter rules than general PHI |
| Hardcoding a single clearinghouse (Office Ally) | Faster billing integration | Provider cannot switch clearinghouses without code changes; some payers only work with specific clearinghouses | Phase 2 MVP only -- must abstract by Phase 2 completion |
| Using json_extract in WHERE clauses instead of indexed lookup tables | Simpler data layer | O(n) full-table scans on every query; performance degrades linearly with data growth | Never for production queries; acceptable for ad-hoc admin/reporting queries |
| Storing audit logs in the same SQLCipher database as clinical data | Simpler architecture | Audit logs can be tampered with by anyone with database access; fails the "tamper-proof" HIPAA requirement | Phase 1 only -- must move to append-only storage (separate file or hash chain) before launch |
| Skipping FHIR validation on write | Faster development | Invalid FHIR resources accumulate; interoperability breaks; cloud migration fails on data quality | Never -- validate at the repository boundary from day one |
| Single SQLCipher database file for all data | Simpler backup/restore | 2GB+ database files become slow to back up, slow to encrypt, and risky for corruption | Phase 1 only -- partition by year or data type before reaching 1GB |
| Not implementing MFA in Phase 1 | Faster auth implementation | HIPAA requires "addressable" MFA implementation; auditors will flag its absence | Phase 1 with documented risk acceptance, must ship in Phase 2 |

## Integration Gotchas

| Integration | Common Mistake | Correct Approach |
|-------------|----------------|------------------|
| Weno Exchange (e-prescribing) | Assuming REST API -- it uses NCPDP SCRIPT XML messages over HTTPS with mutual TLS | Study the Weno integration guide first; budget for their certification process; implement NCPDP SCRIPT message builder |
| HL7 v2 (lab results) | Parsing HL7 as plain text with string splitting on pipe characters | Use a proper HL7 parser (python-hl7 or hl7apy); HL7 v2 has escape sequences, repeating fields, sub-components, and segment groups that string splitting will corrupt |
| RxNav-in-a-Box | Running the full Docker composition (4 containers, 8GB+ RAM) alongside the EMR and AI models | Use only the RxNorm API container (RxNav) and the drug interaction container; skip the full NLM stack; consider pre-loading interaction data into SQLite for offline use |
| Office Ally (clearinghouse) | Submitting claims via their web portal manually during development | Use their SFTP batch submission interface from the start; the web portal workflow does not translate to automated submission |
| macOS Keychain | Storing the SQLCipher key as a generic password | Use kSecAttrAccessControl with biometric protection (Touch ID) and kSecAttrAccessibleWhenUnlockedThisDeviceOnly; the key should not sync to iCloud Keychain |
| Ollama (local LLM) | Assuming Ollama is always running and the model is loaded | Ollama may not be installed, may be on a different version, or may not have the required model pulled. Check for Ollama availability at app startup, validate model availability, and provide a guided setup flow for first-time users |
| FAISS (vector search) | Loading the full ICD-10 (70K+ codes) and CPT (10K+ codes) indexes into memory at startup | Lazy-load indexes on first use; use memory-mapped indexes (faiss.read_index with IO_FLAG_MMAP); consider HNSW index type for better memory/accuracy tradeoff |

## Performance Traps

| Trap | Symptoms | Prevention | When It Breaks |
|------|----------|------------|----------------|
| Unindexed JSON queries on FHIR resources | Patient chart takes 2-5 seconds to load | Maintain denormalized lookup tables with proper indexes; update on write | 10,000+ encounters in the database (~1 year of 5-provider clinic) |
| Loading full encounter history in patient timeline | UI freezes when opening a chronic patient's chart | Paginate with cursor-based pagination; load most recent 20 encounters | Patient with 500+ encounters (~2 years of regular visits) |
| Whisper transcription blocking the UI thread | Application appears frozen during voice transcription | Run transcription in background thread; stream partial results; show progress indicator | Any transcription over 30 seconds |
| SQLCipher PBKDF2 key derivation on every database open | App takes 2-3 seconds to start | Cache the derived key in memory for the session duration; SQLCipher's sqlcipher_export can create an in-memory copy | Every app launch, especially on older M1 hardware |
| Loading all AI models at startup | App uses 15-20GB RAM immediately, even before any AI feature is used | Lazy-load models on first use; unload after 5 minutes of inactivity; only load Whisper when voice recording starts | Any machine with 16GB unified memory (the stated minimum) |
| Unbounded audit log table growth | Database file grows 100MB+/year from audit logs alone | Archive audit logs older than 1 year to a separate encrypted file; keep 90 days hot; maintain hash chain across archives | After 2-3 years of operation with 5 active users |

## Security Mistakes

| Mistake | Risk | Prevention |
|---------|------|------------|
| Storing PHI in Tauri IPC messages without clearing | PHI persists in IPC buffers/memory after the UI navigates away; memory dumps expose data | Clear sensitive data from frontend state on navigation; use Rust's zeroize crate for memory that held PHI; minimize PHI in IPC -- send record IDs, not full records |
| Logging PHI in application logs or error messages | Stack traces contain patient names, MRNs, or diagnoses; logs are less protected than the database | Implement a PHI scrubber on all log output; never log patient identifiers in error messages; use correlation IDs that map to audit logs, not patient data |
| Auto-logoff that loses unsaved work | Provider loses 10 minutes of documentation; stops using auto-logoff; disables the security feature | Save a draft (encrypted) before auto-logoff triggers; restore the draft on re-authentication; use a 2-minute warning countdown before logoff |
| SQLCipher key derivation with insufficient iterations | Brute-force attack on a stolen database file succeeds | Use SQLCipher v4 defaults (256,000 PBKDF2-HMAC-SHA512 iterations); never reduce iterations for performance; the 5-15% overhead is the cost of compliance |
| Backup files stored unencrypted | HIPAA breach from a lost USB drive or cloud storage misconfiguration | Encrypt backups with a separate key before writing to any storage; verify encryption by attempting to read backup without key as a test; never use the same key for database and backups |
| Emergency "break-glass" access without proper scoping | Admin uses break-glass to access celebrity patient records; no audit trail distinguishes emergency from routine access | Break-glass access must be time-limited (4 hours max), require a reason code, generate an immediate notification to the privacy officer, and appear distinctly in audit logs |
| Python sidecar exposing FastAPI on all interfaces | Another process on the machine (or the network) can access the AI API and extract PHI from requests/responses | Bind FastAPI to 127.0.0.1 only; use a per-session random token in request headers that the Tauri app generates at sidecar launch; reject requests without the token |

## UX Pitfalls

| Pitfall | User Impact | Better Approach |
|---------|-------------|-----------------|
| Requiring too many clicks to complete common workflows | Providers average 15 minutes per encounter; adding 3 clicks per encounter costs 20+ minutes per day across 20 patients | Map the top 5 clinical workflows (new encounter, refill prescription, review labs, sign note, check schedule) and ensure each completes in 3 clicks or fewer |
| Alert fatigue from excessive drug interaction warnings | Providers override 90%+ of alerts because most are clinically insignificant; they miss the truly dangerous one | Classify interactions by severity (critical/serious/moderate/minor); only interrupt workflow for critical and serious; show moderate/minor as non-blocking indicators |
| AI-generated content that cannot be easily edited | Provider must delete the entire AI SOAP note and retype rather than editing specific sections | Generate SOAP notes as structured sections (S, O, A, P) that can be independently edited, accepted, or regenerated; support inline editing, not just accept/reject |
| Clinical forms that do not match paper workflow | Provider's mental model follows the paper chart they used for 20 years; digital form in a different order slows them down | Mirror the traditional SOAP note order; allow providers to customize form layouts; observe actual clinical workflows before designing forms |
| Scheduling UI that does not show patient context | Front desk books a 15-minute slot for a complex patient who needs 45 minutes; appointment runs over, cascading delays | Show visit history and complexity indicators on the scheduling view; suggest appointment durations based on visit type and patient history |
| Voice recording that requires holding a button | Provider cannot use hands during physical exam while dictating | Support hands-free recording with a keyboard shortcut or voice activation; auto-detect silence to pause/resume; allow recording to continue across UI navigation |

## "Looks Done But Isn't" Checklist

- [ ] **Audit logging:** Often missing failed access attempts, schema changes, and export/print operations -- verify that EVERY PHI access path (including direct database queries via admin tools) generates an audit entry
- [ ] **RBAC:** Often enforced only in the UI layer -- verify that a direct API call to the FastAPI sidecar or Tauri command with a lower-privilege token is properly rejected
- [ ] **Encryption at rest:** Often covers the database but not backup files, exported reports, printed documents, or temporary files created during PDF generation -- verify all PHI-containing files are encrypted or securely deleted
- [ ] **Patient search:** Often works for exact matches but fails on partial names, hyphenated names, name changes, and non-ASCII characters -- verify with real-world name patterns (O'Brien, Al-Rashid, maiden name changes)
- [ ] **E-prescribing:** Often handles NewRx but not refill requests (RxRenewalRequest), change requests (RxChangeRequest), or cancellations (CancelRx) -- verify the complete prescription lifecycle
- [ ] **Claims processing:** Often generates valid X12 for simple office visits but fails for claims with modifiers, multiple procedures, anesthesia, or secondary insurance -- verify with the 10 most common claim scenarios for the practice specialty
- [ ] **Lab results:** Often displays results but does not handle abnormal value flagging, critical result alerting, or result trending over time -- verify the complete results workflow including acknowledge/sign-off
- [ ] **Auto-logoff:** Often logs the user out but does not save the current work state -- verify that unsaved encounters, prescriptions, and notes are preserved across logoff/logon
- [ ] **AI transcription:** Often works for clear dictation but fails with accented speech, background noise, medical abbreviations, and multiple speakers -- verify with realistic clinical audio including exam room noise
- [ ] **Backup/restore:** Often creates backups but restore has never been tested -- perform a full restore to a clean machine and verify all data, encryption, and application state are intact

## Recovery Strategies

| Pitfall | Recovery Cost | Recovery Steps |
|---------|---------------|----------------|
| FHIR data model wrong (too normalized or too denormalized) | HIGH | Must migrate all existing data to new schema; requires downtime; risk of data loss during migration; budget 4-6 weeks |
| Missing audit log entries discovered in compliance review | HIGH | Cannot retroactively create audit logs; must implement comprehensive logging and document the gap; may need to report to compliance officer; 6-year retention means this gap persists |
| AI hallucination in signed medical record | MEDIUM | Provider must create an addendum (never delete the original -- legal medical record); review all AI-generated notes for the affected time period; consider disabling AI features until the root cause is identified |
| Claims rejected at 30%+ rate | MEDIUM | Implement claim scrubbing; analyze rejection codes to identify systematic issues; resubmit corrected claims within timely filing limits (typically 90-365 days depending on payer); 2-4 weeks to stabilize |
| SQLCipher performance degradation | MEDIUM | Add indexes and denormalized tables; may require rebuilding the database file (VACUUM) which requires downtime; 1-2 weeks to diagnose and resolve |
| Sidecar process instability | LOW | Add health checks and auto-restart; ensure core EMR works without sidecar; 1-2 days to implement proper process management |
| E-prescribing certification failure | HIGH | Must fix all deficiencies and re-test; Weno certification cycle is 4-8 weeks per attempt; may delay the entire Phase 2 launch |

## Pitfall-to-Phase Mapping

| Pitfall | Prevention Phase | Verification |
|---------|------------------|--------------|
| HIPAA as checkbox, not architecture | Phase 1 | Run HIPAA Security Rule gap analysis against the codebase before Phase 1 launch; every 164.312 requirement has a corresponding test |
| AI hallucination safety | Phase 1 (data model) + Phase 3 (implementation) | Clinical validation study: 100 AI-generated notes reviewed by 2 providers; hallucination rate below 2% for transcription, 5% for SOAP generation |
| FHIR data model impedance mismatch | Phase 1 | Load test with 50,000 synthetic FHIR resources (Synthea); patient chart opens in <500ms; all common queries complete in <200ms |
| E-prescribing underestimated | Phase 2 | Weno certification initiated in Phase 2 month 1; EPCS scoped as separate Phase 2/3 deliverable; end-to-end prescription lifecycle tested |
| X12 claims brittleness | Phase 2 | Submit 50 test claims to Office Ally sandbox; achieve >95% acceptance rate before production launch |
| SQLCipher performance cliff | Phase 1 | Automated performance regression tests in CI with 50K+ record dataset; chart-open <500ms, patient-search <200ms |
| Tauri/Python sidecar lifecycle | Phase 1 (lifecycle) + Phase 3 (AI resilience) | Core EMR functions without sidecar running; sidecar auto-restarts within 10 seconds of crash; all AI calls have timeouts |
| Alert fatigue from drug interactions | Phase 2 | Implement severity-based filtering from day one of e-prescribing; only critical/serious interactions interrupt workflow |
| Backup/restore never tested | Phase 1 | Monthly automated restore test to a clean environment in CI; restore time <15 minutes for 1GB database |
| PHI in application logs | Phase 1 | Automated scan of all log output for PHI patterns (SSN, MRN, names) in CI; zero PHI in logs |

## Sources

- HIPAA Security Rule (45 CFR 164.312) -- established regulation, stable requirements (HIGH confidence)
- DEA 21 CFR Part 1311 -- EPCS requirements (HIGH confidence)
- X12 837P 5010 Implementation Guide -- claims processing standard (HIGH confidence)
- NCPDP SCRIPT standard -- e-prescribing message format (HIGH confidence)
- Whisper hallucination rate (~1%) -- cited in project requirements from published research (MEDIUM confidence)
- LLaMA 3.1 8B RAG accuracy (94%, 4% hallucination) -- cited in project requirements (MEDIUM confidence)
- GPT-4 ICD-10 exact match (33.9%) -- cited in project requirements (MEDIUM confidence)
- SQLCipher performance characteristics (5-15% overhead) -- from SQLCipher documentation (HIGH confidence)
- OpenEMR v8.0.0 feature baseline -- from project requirements document (MEDIUM confidence)
- Note: Web search was unavailable during this research. All findings are based on training data knowledge of healthcare IT regulations, standards, and common implementation patterns. The regulatory and standards content (HIPAA, HL7, X12, NCPDP, DEA) is well-established and unlikely to have materially changed. AI-specific pitfalls (Whisper hallucination rates, LLM accuracy) should be validated against current benchmarks during Phase 3 research.

---
*Pitfalls research for: AI-powered desktop EMR with local-first architecture*
*Researched: 2026-03-10*
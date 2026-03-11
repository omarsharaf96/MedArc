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

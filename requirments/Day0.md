# AI-powered desktop EMR: product requirements and implementation plan

**An AI-native electronic medical records application built for solo practitioners and small clinics (1–5 providers), delivered as a self-contained macOS desktop application with local-first data storage and a clear cloud migration path.** This PRD defines the complete feature set, AI capabilities, tech stack, security architecture, and phased implementation plan needed to build a production-grade EMR that eliminates the documentation burden plaguing small practices—where physicians spend twice as much time on EHR screens as with patients. The application fills critical gaps left by incumbents like Practice Fusion, DrChrono, and Tebra: ambient AI documentation, intelligent coding, offline-first privacy, and zero monthly SaaS fees. With **42% of medical groups already using ambient AI** and CMS estimating healthcare workers waste 45 minutes daily on inefficient workflows, the timing is optimal for a desktop-native, AI-first EMR.

---

## 1. Core feature requirements derived from OpenEMR analysis

OpenEMR v8.0.0 (ONC-certified, February 2026) serves as the feature baseline. The following capabilities must be replicated or improved upon, organized by clinical priority.

### Patient management and demographics

The patient record forms the data backbone. Required fields include name, DOB, sex/gender, contact information, insurance (primary/secondary/tertiary), employer data, social determinants of health, patient photo, and clinical identifiers (primary provider, HIPAA information). The system must support **Related Persons** for care team relationships, a **Care Team Widget** with role assignments, and a fully customizable layout via a form editor. Patient search must work across name, demographics, MRN, and procedure history. Address verification via USPS API is a nice-to-have.

### Scheduling and appointment system

The scheduling module must include a **Patient Flow Board** for real-time clinic tracking, multi-provider calendars with day/week/month views, color-coded appointment categories with configurable durations, recurring appointment support, appointment reminders via SMS and email, and waitlist management. A Recall Board for patient follow-up scheduling is essential. OpenEMR supports appointment searching by open slots and restricting appointments by type—both are required.

### Clinical documentation

SOAP notes are the core clinical workflow. The system must support structured SOAP entry (Subjective, Objective, Assessment, Plan), multi-provider co-signing, vitals tracking with flowsheets and growth charts, Review of Systems forms, physical exam templates, and a **template library with 60+ form types**. OpenEMR's CAMOS (Computer Aided Medical Ordering System) and Track Anything (graph-generating form for arbitrary data) are differentiating features worth replicating. A Clinical Decision Rules engine must provide physician reminders, drug-allergy interaction alerts, and clinical quality measure calculations.

### E-prescribing

Prescription management requires online drug search, e-prescribing transmission (via Weno Exchange or NewCrop integration at **$300 activation**), support for EPCS (electronic prescribing of controlled substances), drug interaction checks, duplicate therapy alerts, formulary awareness, and RxNorm/SNOMED medication coding. A local pharmacy dispensary module is optional for Phase 1.

### Lab integration

Lab orders and results viewing are essential. The system must support laboratory catalogue configuration, procedure ordering with results workflow, HL7 v2 message exchange (ORU^R01 for results, ORM^O01 for orders), manual result entry, and LOINC code mapping. Quest Hub-style API integration is a Phase 2 feature.

### Billing and revenue cycle

The billing module must support CPT, HCPCS, ICD-10, and SNOMED coding, with a fee sheet interface per encounter, multiple fee schedules, and modifier support. Claim generation in ANSI X12 837P format (5010 standard) is required for electronic submission to clearinghouses (Office Ally, ZirMED). ERA processing in 835 format with automated payment posting, insurance eligibility queries, and Accounts Receivable tracking complete the revenue cycle.

### Reporting, documents, and patient portal

Clinical reports (patient lists, encounters, prescriptions, immunizations), financial reports (sales, collections, insurance distributions), and Clinical Quality Measures (CQM/eCQM) are required. Document management must support upload/scanning of PDFs and images with categorization, SHA-1 integrity validation, and document sizes up to 64 MB. The patient portal must provide appointment scheduling, secure messaging, lab/medication access, and online payments. For Phase 1, the portal is deprioritized in favor of the desktop clinical workflow.

---

## 2. AI capabilities architecture: four pillars

Every AI capability must operate in a **hybrid local/cloud architecture**: local models handle routine tasks with PHI never leaving the device, while cloud APIs (accessed via BAA-covered services) handle complex cases with de-identified data when possible.

### Clinical note generation via NLP

This is the highest-impact AI feature. The pipeline chains voice capture → transcription → entity extraction → SOAP note generation → human review.

**Voice-to-text** uses **whisper.cpp with CoreML** acceleration on Apple Silicon, achieving ~3× faster-than-real-time transcription on M1+ Macs. The large-v3-turbo model (1.5B parameters) provides **95%+ general accuracy** but requires medical vocabulary fine-tuning—Medical Whisper models on HuggingFace address this. A critical caveat: Whisper exhibits **~1% hallucination rate** (fabricating phrases like "hyperactivated antibiotics"), making human-in-the-loop review mandatory. WhisperKit (Swift-native CoreML) is the alternative for tighter macOS integration.

**Entity extraction** combines MedSpaCy (section detection, negation/uncertainty via ConText algorithm) with SciSpaCy's pre-trained `en_core_med7_lg` model (drugs, dosages, durations, routes, forms, frequencies) and `en_ner_bc5cdr_md` (diseases, chemicals). All models run locally via Python/spaCy with no cloud dependency. UMLS entity linking maps extracted concepts to standardized terminologies.

**SOAP note generation** uses a local LLM via Ollama. **LLaMA 3.1 8B** (Q4_K_M quantized, ~6 GB) achieves **15–28 tokens/sec on M1–M4 Macs** and handles routine SOAP notes with strong clinical prompting. For complex cases, the system routes to **Claude via AWS Bedrock** (BAA-covered, zero data retention). Research shows general-purpose models with RAG pipelines outperform biomedically fine-tuned models: LLaMA-3-8B-Instruct scored 64% on NEJM cases versus 30% for the fine-tuned OpenBioLLM-8B. The recommended approach is **RAG grounded in clinical guidelines** (e.g., StatPearls) using FAISS for local vector search, achieving 94% accuracy with 4% hallucination rates in published pipelines.

**Minimum hardware**: Mac with **16 GB unified memory** runs LLaMA 3.2 3B + Whisper base + MedSpaCy comfortably. **Optimal**: 32–64 GB runs LLaMA 3.1 8B + Whisper large-v3-turbo + full NLP pipeline.

### Diagnostic decision support

Symptom analysis and differential diagnosis suggestions use the same local LLM (LLaMA 3.1 8B) with RAG retrieval from embedded clinical knowledge bases (UpToDate-style content, drug references). Drug interaction checking uses **RxNav-in-a-Box**, a Docker composition from NLM providing the complete RxNorm API stack locally, including DrugBank-sourced drug-drug interaction data with severity ratings. This runs entirely offline with no BAA required—**the most privacy-preserving approach for drug safety**. The system supplements with DailyMed bulk downloads for drug labeling and FDA safety data via OpenFDA API.

Decision support alerts include drug-allergy interactions, duplicate therapy detection, dosage range checking, and care gap identification based on clinical quality measures.

### Smart scheduling intelligence

No-show prediction uses a lightweight gradient-boosted model (XGBoost/LightGBM) trained on historical appointment data—patient demographics, appointment history, day/time patterns, weather, and visit type. Published models achieve **AUC 0.75–0.85** for no-show prediction. Smart scheduling optimizes provider time by suggesting appointment slots based on visit-type duration patterns, buffer time for complex cases, and provider preferences. Automated reminders via SMS/email integrate with the recall board.

### Medical coding and billing automation

LLMs alone perform poorly at exact code generation (GPT-4 achieves only **33.9% exact match on ICD-10-CM**). The winning architecture combines LLM entity extraction → vector embedding → similarity search against ICD-10/CPT ontology databases. A published pipeline using LangChain + text-embedding-3-large against vectorized 2025 CPT codes demonstrates this approach. The ICD-10 codeset contains **70,000+ codes** and CPT has **10,000+ codes**, updated annually—vector databases handle this scale efficiently with FAISS.

AI-assisted coding reduces denial rates from **8–12% to below 3%** and delivers a **3–7% revenue increase** per practice. The system always presents suggested codes for human review, never auto-submits claims without provider confirmation.

---

## 3. Technology stack: Tauri + React + FastAPI + SQLCipher

### Desktop shell: Tauri 2.x

**Tauri 2.x is the recommended application shell**, chosen over Electron for three decisive reasons in a medical context:

- **Memory efficiency**: Tauri uses ~30–50 MB idle (vs. Electron's 150–300 MB), critical when running local AI models consuming 6–35 GB of RAM simultaneously
- **Security model**: Rust backend with explicit API exposure and permission-based access aligns with HIPAA's minimum-necessary principle; Electron's full Node.js API access requires extensive hardening
- **Bundle size**: 3–10 MB for Tauri vs. 100–150 MB for Electron, plus Tauri's built-in sidecar support for bundling PyInstaller-compiled Python binaries

Tauri 2.x uses WKWebView (WebKit) on macOS. WebKit rendering differences from Chromium are manageable for internal medical software where the deployment platform is controlled. The built-in updater plugin (`tauri-plugin-updater`) provides cryptographic Ed25519 signature verification for secure medical software updates. Tauri 2.x also extends to iOS/Android, future-proofing for a mobile EMR companion.

### Frontend: React 18+ with TypeScript

React offers the largest ecosystem of medical UI components. Key libraries include **@medplum/react** (SmartText clinical concept detection, SNOMED/ICD coding, resource forms), **fhir-react** (FHIR resource rendering for DSTU2/STU3/R4), **Material UI v6** (WCAG 2.1 AA accessibility), **React Hook Form** (optimized for large clinical forms), and **TanStack Table** (patient lists, lab results). State management via Zustand keeps the architecture lightweight.

### Backend: Python FastAPI as Tauri sidecar

The Python backend compiles into a single executable via **PyInstaller** and runs as a Tauri sidecar process. FastAPI provides native async support (non-blocking AI inference), and its Pydantic models align perfectly with `fhir.resources` (a Pydantic-based FHIR library). Communication flows via HTTP on localhost between the WebView frontend and the FastAPI sidecar.

A hybrid Rust+Python approach optimizes performance: **Rust-native Tauri commands handle database CRUD and file system operations** (zero sidecar overhead for routine tasks), while the **Python sidecar handles AI inference exclusively**. This means the Python process doesn't need to run continuously for basic EMR operations.

```
┌──────────────────────────────────────────────────┐
│            macOS Application (.app)               │
│  ┌────────────────────────────────────────────┐  │
│  │  Tauri 2.x Shell (Rust Core)               │  │
│  │  • IPC, lifecycle, Keychain, auto-updater  │  │
│  ├────────────────────────────────────────────┤  │
│  │  WKWebView → React + TypeScript Frontend   │  │
│  │  • MUI + fhir-react + @medplum/react       │  │
│  ├──────────────┬─────────────────────────────┤  │
│  │              │ HTTP localhost               │  │
│  │  FastAPI Sidecar (PyInstaller binary)      │  │
│  │  • LLM inference (Ollama/MLX)              │  │
│  │  • NLP pipeline (MedSpaCy + SciSpaCy)      │  │
│  │  • whisper.cpp transcription               │  │
│  │  • FHIR processing (fhir.resources)        │  │
│  ├────────────────────────────────────────────┤  │
│  │  SQLite + SQLCipher (AES-256 encrypted)    │  │
│  │  • FHIR resources as JSON columns          │  │
│  │  • Indexed lookup tables for queries       │  │
│  └────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────┘
  Keys: macOS Keychain (Secure Enclave on Apple Silicon)
  Updates: tauri-plugin-updater (Ed25519 signed)
  Distribution: Code-signed + Notarized DMG
```

### Database: SQLite + SQLCipher for Phase 1

**SQLite + SQLCipher** provides zero-configuration, serverless, AES-256 encrypted storage with only **5–15% performance overhead**. SQLCipher v4 uses PBKDF2-HMAC-SHA512 with 256,000 iterations for key derivation and per-page HMAC for tamper detection. The encryption key is stored exclusively in the macOS Keychain (hardware-backed via Secure Enclave on Apple Silicon).

**SQLAlchemy 2.0** serves as the ORM, abstracting the database engine so that swapping `sqlite:///local.db` to `postgresql://rds-host/emr_db` requires zero changes to business logic. **Alembic** handles schema migrations with `render_as_batch=True` for SQLite's ALTER TABLE limitations—the same migration files work on both SQLite and PostgreSQL.

Data is modeled as **FHIR resources stored as JSON columns** from day one, with indexed lookup tables for frequently queried fields (patient MRN, encounter dates, medication names). This FHIR-first approach ensures interoperability without complex mapping layers during cloud migration.

### AI model orchestration pipeline

```
Voice Input → [whisper.cpp/CoreML] → Raw Transcript
  → [PyAnnote speaker diarization] → Speaker-labeled text
  → [MedSpaCy section detection + context] → Structured sections
  → [SciSpaCy en_core_med7 + bc5cdr] → Medical entities
  → [LLM via Ollama: LLaMA 3.1 8B] → SOAP Note draft
  → [FAISS vector search] → ICD-10/CPT code suggestions
  → [RxNav-in-a-Box] → Drug interaction alerts
  → Human Review → Final EHR Entry
```

LangChain/LangGraph orchestrates this pipeline with conditional routing: **95% of tasks run on the local LLM**, with the remaining 5% (complex differential diagnoses, unusual presentations) routed to Claude via AWS Bedrock with de-identified data. FAISS provides local vector search for ICD-10/CPT code matching against the full 70,000+ code database.

---

## 4. HIPAA compliance and security architecture

### Technical safeguards implementation

Every HIPAA technical safeguard maps to a specific application control:

**Access controls (§164.312(a))** require unique user IDs for every workforce member (no shared accounts), automatic logoff after **10–15 minutes** of inactivity on private workstations (2–5 minutes on shared stations), emergency "break-glass" access that is time-limited, tightly scoped, and fully logged, and AES-256 encryption at rest via SQLCipher.

**Audit controls (§164.312(b))** require logging of all ePHI access: files opened/closed, records created/read/edited/deleted, login attempts (success and failure), commands initiated, authentication attempts, and the source device. Each log entry captures timestamp (UTC), user ID, action type, patient/record identifier, device identifier, and success/failure status. Logs must be retained for **minimum 6 years** with tamper-proof storage using cryptographic hash chains (each entry includes hash of previous entry).

**Transmission security (§164.312(e))** requires TLS 1.3 for all network communications—AI API calls, cloud backups, and any external data exchange. macOS App Transport Security (ATS) enforces HTTPS by default. Certificate pinning should be implemented for known API endpoints.

**Integrity controls (§164.312(c))** use checksums, digital signatures, and database integrity checks to verify ePHI has not been altered. SQLCipher's per-page HMAC provides this at the database level.

### Role-based access control for small clinic

| Role | Clinical Records | Scheduling | Billing | Prescriptions | Audit Logs |
|------|-----------------|------------|---------|---------------|------------|
| System Admin | Full (troubleshooting) | Full | Full | None | Read all |
| Provider | Full CRUD | Read/Write own | Read | Full CRUD | Read own |
| Nurse/MA | Read + Update vitals | Read/Write | None | Read only | None |
| Billing Staff | Demographics + codes only | Read | Full CRUD | None | None |
| Front Desk | Demographics only | Full CRUD | Limited read | None | None |

### Encryption architecture: defense in depth

Three layers of encryption protect PHI:

1. **Full disk encryption**: macOS FileVault 2 (XTS-AES-128 with 256-bit key) on every workstation
2. **Database encryption**: SQLCipher AES-256-CBC with per-page HMAC tamper detection
3. **Field-level encryption**: Additional AES-256 encryption on high-sensitivity columns (SSN, psychiatric notes, HIV status) via pgcrypto-style functions

The encryption key hierarchy stores the master key in macOS Keychain (Secure Enclave-backed), which protects the database encryption key (DEK), which protects per-field keys. Annual key rotation uses SQLCipher's `PRAGMA rekey`. Keys are never hardcoded or stored alongside encrypted data.

### Business associate agreements for AI providers

**Any cloud AI API receiving PHI requires a BAA.** Current BAA availability:

| Provider | BAA Available | Best HIPAA Path | Notes |
|----------|--------------|-----------------|-------|
| OpenAI | Enterprise/API only | Azure OpenAI Service | Consumer tiers (Free/Plus/Team) have no BAA |
| Anthropic | API Enterprise | AWS Bedrock | Consumer Claude (Free/Pro) has no BAA |
| Google | Vertex AI | GCP Vertex AI | 1–2 business day approval |
| AWS Bedrock | Yes (standard) | Direct | Hosts Claude, LLaMA; near-instant BAA approval |

**AWS Bedrock is the recommended cloud AI gateway**—it hosts Claude and LLaMA models with standard BAA coverage and near-instant approval. The preferred strategy is **local models for all routine PHI processing** (no BAA needed) with cloud fallback using de-identified data where possible.

### Backup and disaster recovery

HIPAA's contingency plan (§164.308(a)(7)) requires retrievable exact copies of ePHI in at least two physical locations. The **3-2-1 backup rule** applies: 3 copies, 2 different media types, 1 offsite. Daily automated encrypted backups to external storage, with weekly backups to cloud (AWS S3 with BAA). All backups encrypted with AES-256 before leaving the machine. Restore procedures must be tested at least annually.

The **breach notification safe harbor** is critical: if PHI is encrypted per NIST standards, it is considered "secured" and breach notification is NOT required. This makes comprehensive encryption the single most valuable compliance investment.

### ONC certification considerations

ONC certification is **technically voluntary** but **required for Medicare/Medicaid incentive programs** (MIPS/Quality Payment Program). For Phase 1 targeting solo practitioners, ONC certification is not required for basic HIPAA compliance. However, the architecture should support future certification by implementing FHIR R4 APIs (§170.315(g)(10)), C-CDA document generation, clinical decision support, and USCDI v3 data elements from the start.

---

## 5. Local-to-cloud migration: designed for Phase 2

### Data access layer abstraction

The **Repository Pattern** with **Unit of Work** ensures business logic never directly touches database-specific code. SQLAlchemy's dialect system means changing `sqlite:///local.db` to `postgresql://rds.amazonaws.com/emr` requires only a configuration change—zero code modifications.

Schema design uses the common SQL subset: UUIDs as primary keys (TEXT in SQLite, UUID in PostgreSQL), ISO 8601 date strings, JSON columns (supported by both), and explicitly named constraints. Alembic migrations with `render_as_batch=True` handle SQLite's ALTER TABLE limitations while generating normal ALTER statements for PostgreSQL. Conditional logic handles DB-specific operations:

```python
if context.get_context().dialect.name == 'postgresql':
    # PostgreSQL-specific: add index concurrently, use JSONB, etc.
```

### AWS services for a small clinic

| Service | Configuration | Monthly Cost |
|---------|--------------|-------------|
| RDS PostgreSQL | db.t4g.small Multi-AZ, 20 GB gp3 | $49 |
| S3 | 25 GB Standard + lifecycle to Glacier | $2 |
| Cognito | 20 MAUs (Lite tier, free) | $0 |
| Lambda | ~50K invocations (free tier) | $0 |
| KMS | 3 customer-managed keys | $3 |
| CloudWatch | Basic monitoring + 2 GB logs | $6 |
| AWS Backup | 10 GB additional storage | $1 |
| Data Transfer | ~5 GB outbound | $1 |
| **Total** | **Minimal configuration** | **~$65–75/mo** |
| **Total** | **With NAT Gateway for VPC Lambda** | **~$95–110/mo** |

AWS HealthLake ($197/mo base) is **not recommended** for small clinics. FHIR-compatible APIs implemented in the application layer with PostgreSQL JSONB storage achieve the same interoperability at a fraction of the cost.

### Sync strategy: PowerSync for offline-first

**PowerSync** is the recommended sync engine, providing production-proven PostgreSQL ↔ SQLite synchronization with offline-first architecture. Its bucket system enables partial sync (each user receives only data they need), and it guarantees causal consistency. For medical data conflict resolution:

- **Non-clinical fields** (scheduling, admin): Last-Write-Wins with timestamps
- **Clinical text fields** (notes, assessments): Per-column merge or flag for provider review
- **Structured clinical data** (medications, allergies, vitals): Append-only—never overwrite, always add timestamped entries
- **Documents/images**: Sync metadata only through PowerSync; binary files upload directly to S3 via presigned URLs

The migration from local-only to cloud follows a **dual-write strategy**: the application writes to both local SQLite and cloud RDS simultaneously during a 3–4 week parallel-running period, with automated integrity verification before cutover. Local database is preserved as read-only backup for 30 days post-migration.

---

## 6. Implementation plan: four phases over 18 months

### Phase 1 — MVP (Months 1–6)

**Goal**: Core EMR functionality without AI, locally installed on macOS.

| Month | Deliverables |
|-------|-------------|
| 1–2 | Tauri + React scaffold, SQLCipher database schema, FHIR data models, RBAC authentication, macOS code signing/notarization pipeline |
| 3–4 | Patient demographics CRUD, appointment scheduling (calendar views, recurring appointments), basic clinical encounter forms with SOAP template |
| 5–6 | Medication list management, allergy tracking, lab results viewer (manual entry), document upload/scanning, audit logging, session management, encrypted backups |

**Team**: 2 full-stack developers, 1 UI/UX designer (part-time), 1 clinical advisor (part-time).

**Testing**: Unit tests (pytest + React Testing Library), integration tests against SQLCipher, manual HIPAA compliance checklist review.

### Phase 2 — Full feature parity (Months 7–10)

**Goal**: Feature parity with OpenEMR core modules.

| Month | Deliverables |
|-------|-------------|
| 7–8 | Billing module (fee sheets, CPT/ICD-10 entry, X12 837P claim generation, ERA 835 processing), insurance management, financial reporting |
| 9 | E-prescribing integration (Weno Exchange), drug interaction checking via RxNav-in-a-Box, patient portal (secure messaging, lab access) |
| 10 | HL7 v2 lab interface (ORU/ORM messages), referral management, clinical quality measures (CQM/eCQM), report builder |

**Testing**: End-to-end billing workflow tests, e-prescribing integration tests, HIPAA penetration testing by third-party auditor.

### Phase 3 — AI enhancement (Months 11–15)

**Goal**: Deploy all four AI pillars.

| Month | Deliverables |
|-------|-------------|
| 11–12 | whisper.cpp voice-to-text integration with CoreML, MedSpaCy + SciSpaCy NLP pipeline, LLM-powered SOAP note generation via Ollama (LLaMA 3.1 8B), human review workflow |
| 13 | AI coding assistant (FAISS vector search for ICD-10/CPT suggestions), drug interaction enhancement with severity ratings, diagnostic decision support with RAG pipeline |
| 14 | Smart scheduling (no-show prediction model, slot optimization), automated reminders, AI pre-charting (pre-visit context assembly) |
| 15 | Cloud AI fallback via AWS Bedrock (Claude) for complex cases, A/B testing of local vs. cloud accuracy, fine-tuning Whisper for medical vocabulary |

**Testing**: Clinical accuracy validation with practicing physicians (target: >90% SOAP note acceptance rate without major edits), AI hallucination monitoring, coding accuracy benchmarks against human coders.

### Phase 4 — Cloud migration (Months 16–18)

**Goal**: Production cloud deployment with offline-first sync.

| Month | Deliverables |
|-------|-------------|
| 16 | AWS infrastructure (VPC, RDS Multi-AZ, S3, Cognito, KMS), PowerSync integration, dual-write mode |
| 17 | Data migration tooling, parallel-running validation, document migration to S3, performance benchmarking |
| 18 | Cloud cutover, multi-device support, mobile companion scoping, post-migration monitoring |

**Testing**: Migration dry runs to staging RDS, failover testing, data integrity verification (row counts, checksums, sample record comparison), security audit of cloud configuration.

### Deployment and distribution

macOS distribution requires Apple Developer Program membership ($99/year), Developer ID Application certificate for code signing, Hardened Runtime enabled, and notarization via `xcrun notarytool`. Distribution as a **signed and notarized DMG** (drag-and-drop to /Applications). Auto-updates via **tauri-plugin-updater** with Ed25519 signature verification, served from S3 or a static JSON endpoint.

---

## 7. How this product wins against incumbents

The competitive landscape reveals consistent weaknesses across all major small-practice EMR vendors that this product directly addresses:

**Documentation burden elimination**. Practice Fusion, DrChrono, and Tebra all require extensive manual data entry with rigid templates. This product's ambient AI documentation—voice capture → automatic SOAP note generation—targets the **30–41% documentation time reduction** demonstrated by tools like Abridge and Nuance DAX, but built directly into the EMR rather than bolted on as a $35–99/month add-on.

**Zero recurring SaaS fees**. Every competitor charges $49–349/month per provider with frequent price increases (SimplePractice raised prices 63% in 2025). A locally installed application with a one-time license eliminates this recurring cost. AI inference runs on-device for free; cloud AI fallback is pay-per-use at $2–15 per million tokens.

**Data sovereignty and privacy**. All competitors are cloud-only, requiring clinics to trust third parties with PHI. This product stores all data locally with AES-256 encryption, with cloud migration only when the clinic chooses. PHI never leaves the device for routine AI operations.

**No customer support dependency**. The single most consistent complaint across competitors—**95% of Practice Fusion reviewers cite poor support**, DrChrono and Tebra show similar patterns—becomes less critical when the software runs locally, updates automatically, and doesn't depend on cloud availability for daily operations.

**Intelligent coding and billing**. Most small-practice EMRs offer basic code lookup. AI-assisted coding that reduces denial rates from 8–12% to below 3% and delivers 3–7% revenue increases represents a concrete, measurable financial benefit that justifies adoption.

---

## 8. Security architecture summary and compliance checklist

The security architecture implements defense-in-depth across seven layers:

1. **Physical**: macOS FileVault 2 full-disk encryption required on all workstations; Gatekeeper and SIP enforce application integrity
2. **Application**: Tauri's Rust security model with explicit API permissions; Hardened Runtime prevents code injection and dylib hijacking; App Sandbox restricts resource access
3. **Authentication**: Unique user IDs with bcrypt/Argon2 password hashing (minimum 12 characters), MFA support via TOTP, Touch ID integration on supported hardware, automatic logoff after 10–15 minutes
4. **Authorization**: RBAC with principle of least privilege; minimum-necessary data retrieval; field-level access masking based on role
5. **Data at rest**: SQLCipher AES-256 (database), field-level encryption for high-sensitivity PHI, macOS Keychain for key storage (Secure Enclave-backed)
6. **Data in transit**: TLS 1.3 for all network calls, certificate pinning for API endpoints, App Transport Security enforcement
7. **Audit and monitoring**: Append-only tamper-proof logs with hash chains, 6-year retention, automated de-identification support (Safe Harbor 18-identifier removal)

Encrypted PHI triggers HIPAA's **breach notification safe harbor**: if data is encrypted per NIST standards, it is considered "secured" and breach notification is not required even if the device is lost or stolen. This single architectural decision eliminates the most financially devastating compliance risk (average healthcare breach cost: **$9.77 million** per IBM 2024 data).

---

## Conclusion: a local-first, AI-native EMR for the next decade

Three architectural decisions define this product's long-term defensibility. First, **FHIR-first data modeling** from day one means every patient record, encounter, and observation is stored as a standards-compliant resource, eliminating the interoperability retrofitting that plagues legacy EMRs and enabling ONC certification when the practice needs it. Second, the **hybrid AI architecture**—local models for privacy-sensitive routine tasks, cloud APIs for complex edge cases—sidesteps the false binary of "local or cloud" that limits competitors. Third, the **Repository Pattern with SQLAlchemy abstraction** means the same application binary works offline on a MacBook in a rural clinic and connected to AWS RDS in a multi-site practice, with PowerSync bridging the two modes seamlessly.

The total estimated development cost for an 18-month build with a 3–4 person team is approximately **$500K–750K** to reach full-featured AI-enhanced status, with cloud hosting adding only **$65–110/month** per clinic. Against competitors charging $149–349/month per provider with annual increases, the economics favor adoption within 12–18 months for a typical 3-provider practice. The 42% of medical groups already using ambient AI—up from near-zero three years ago—signals that the market is ready for an EMR where AI is not an add-on but the foundation.
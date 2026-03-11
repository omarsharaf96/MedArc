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

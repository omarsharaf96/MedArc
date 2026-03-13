# Architecture Research

**Domain:** Tauri 2 + React EMR — M002 integration (clinical UI + AI pipeline + billing)
**Researched:** 2026-03-11
**Confidence:** HIGH for integration patterns; MEDIUM for whisper-rs CoreML build; LOW for x12-types 837P completeness

---

## Context: What M001 Delivered vs What M002 Adds

M001 (complete) delivered a fully working Rust backend with zero React clinical UI. The gap to close:

| Layer | M001 State | M002 Must Add |
|-------|-----------|---------------|
| Rust commands | All 50+ commands registered in lib.rs | No new commands for existing domains; new ones for AI (transcription, SOAP gen) and billing (claim gen, Weno) |
| React frontend | Auth screens only; App.tsx is a flat conditional | Router + clinical views for every domain |
| lib/tauri.ts | Only auth/FHIR/audit wrapped | Patient, scheduling, clinical, labs, documentation all need wrappers |
| State management | None (auth state in useAuth hook only) | Active patient context, UI state (active tab, sidebar), AI job queue |
| External processes | None | whisper-rs (Rust crate) + Ollama HTTP (port 11434) + Weno HTTPS (cloud REST) |

---

## System Overview

```
+-----------------------------------------------------------------------------+
|                         WKWebView (React 18 SPA)                            |
|                                                                             |
|  +------------------+  +------------------------------------------------+  |
|  |   Auth Shell     |  |              React Router SPA                   |  |
|  |   (existing)     |  |                                                  |  |
|  |   LoginForm      |  |  /patients    /patients/:id/:tab   /schedule    |  |
|  |   LockScreen     |  |  /encounters  /billing             /admin       |  |
|  |   MfaPrompt      |  |                                                  |  |
|  +------------------+  |  +-------------+  +--------------+  +--------+  |  |
|                        |  | PatientList |  | PatientChart  |  |Sched.  |  |  |
|                        |  | SearchBar   |  |   (tabbed)    |  |FlowBd. |  |  |
|                        |  +-------------+  |   Summary     |  +--------+  |  |
|                        |                  |   Appts       |              |  |
|                        |  +-------------+ |   Notes       |  +--------+  |  |
|                        |  |  AI Panel   | |   Labs        |  |Billing |  |  |
|                        |  |  Record btn | |   Rx          |  |Claims  |  |  |
|                        |  |  Draft SOAP | +--------------+   +--------+  |  |
|                        |  +-------------+                                 |  |
|                        +------------------------------------------------+  |
|                                                                             |
|  State: Zustand stores (auth | activePatient | ui | aiJobs)               |
|  IPC:   lib/tauri.ts -> invoke() for all data ops                         |
+-----------------------------------------------------------------------------+
                                    | Tauri IPC (invoke)
                                    v
+-----------------------------------------------------------------------------+
|                         Tauri 2 Rust Core (existing)                        |
|                                                                             |
|  invoke_handler (single, all 50+ commands registered)                      |
|                                                                             |
|  +----------+ +----------+ +----------+ +----------+ +------------------+  |
|  | patient  | |scheduling| | clinical | |  labs /  | |  NEW M002        |  |
|  |  (S04)   | |  (S06)   | |  (S05)   | |  docs    | |  transcription   |  |
|  |          | |          | |  docs    | |  (S08)   | |  soap_gen        |  |
|  +----------+ +----------+ |  (S07)   | +----------+ |  billing         |  |
|                            +----------+               |  eprescribe      |  |
|                                                       +------------------+  |
|                                                                             |
|  Shared state: Database (Arc<Mutex<Connection>>), SessionManager, DeviceId |
|                                                                             |
|  +------------------------------------------------------------------+      |
|  | SQLCipher — medarc.db (encrypted, FHIR JSON + indexed projections)|      |
|  +------------------------------------------------------------------+      |
|                                                                             |
|  +---------------------------+   +------------------------------------+     |
|  | whisper-rs (Rust crate)   |   | reqwest -> Ollama :11434 (HTTP)    |     |
|  | CoreML feature flag       |   | POST /api/generate (LLaMA 3.1 8B) |     |
|  | Audio -> text (local)     |   +------------------------------------+     |
|  +---------------------------+                                              |
|                                                                             |
|  +--------------------------------------------------------------------+     |
|  | reqwest -> Weno Exchange HTTPS API (TLS 1.2+, NCPDP SCRIPT cloud)  |     |
|  +--------------------------------------------------------------------+     |
+-----------------------------------------------------------------------------+
```

---

## Component Responsibilities

### Existing Components (M001 — Do Not Break)

| Component | Location | Responsibility |
|-----------|----------|----------------|
| `commands/auth.rs` | Rust | Login, register, logout, break-glass |
| `commands/session.rs` | Rust | Lock/unlock, session state, timeout |
| `commands/audit.rs` | Rust | Audit log query + chain verification |
| `commands/patient.rs` | Rust | Patient CRUD, search, care team |
| `commands/clinical.rs` | Rust | Allergies, problems, medications, immunizations |
| `commands/documentation.rs` | Rust | Encounters, SOAP, vitals, ROS, PE, templates, cosign |
| `commands/labs.rs` | Rust | Lab catalogue, orders, results, documents |
| `commands/scheduling.rs` | Rust | Appointments, flow board, waitlist, recalls |
| `commands/backup.rs` | Rust | Encrypted backup/restore |
| `lib/tauri.ts` | React | Type-safe invoke() wrappers (auth/FHIR/audit only) |
| `hooks/useAuth.ts` | React | Auth state, login/logout, MFA |
| `App.tsx` | React | Auth gate (login to lock to main content) |

### New Components Required for M002

| Component | Location | Responsibility | Status |
|-----------|----------|----------------|--------|
| `lib/tauri.ts` additions | React | Wrappers for patient, scheduling, clinical, documentation, labs | MODIFIED |
| Router setup | React | React Router v6 `createBrowserRouter` in `main.tsx` | NEW |
| `store/` (Zustand) | React | activePatient, ui state, aiJob queue | NEW |
| `components/layout/` | React | AppShell, Sidebar, TopNav | NEW |
| `components/patients/` | React | PatientList, PatientSearch, PatientForm, PatientChart (tabbed) | NEW |
| `components/scheduling/` | React | CalendarView, AppointmentForm, FlowBoard, WaitlistBoard | NEW |
| `components/clinical/` | React | AllergyList, ProblemList, MedicationList, ImmunizationList | NEW |
| `components/encounters/` | React | EncounterEditor, SOAPForm, VitalsForm, ROSForm, PEForm | NEW |
| `components/labs/` | React | LabOrderForm, LabResultsTable, DocumentUpload, DocumentList | NEW |
| `components/ai/` | React | RecordButton, TranscriptReview, DraftSOAPPanel | NEW |
| `components/billing/` | React | ClaimForm, ClaimList, FeeSchedule, ARDashboard | NEW |
| `components/eprescribe/` | React | PrescriptionForm, DrugSearch, WenoStatus | NEW |
| `commands/transcription.rs` | Rust | whisper-rs audio capture + transcription Tauri command | NEW |
| `commands/soap_gen.rs` | Rust | Ollama HTTP call for SOAP generation | NEW |
| `commands/billing.rs` | Rust | X12 837P claim assembly, fee schedule CRUD | NEW |
| `commands/eprescribe.rs` | Rust | Weno Exchange HTTPS API calls (NewRx, CancelRx) | NEW |
| `types/patient.ts` | React | TypeScript types matching Rust PatientInput/PatientResponse | NEW |
| `types/clinical.ts` | React | Types for allergies, problems, medications, immunizations | NEW |
| `types/scheduling.ts` | React | Types for appointments, flow status, waitlist | NEW |
| `types/encounters.ts` | React | Types for encounters, SOAP, vitals, ROS, PE | NEW |
| `types/billing.ts` | React | Types for claims, CPT/ICD-10 codes, fee schedule | NEW |

---

## Recommended Project Structure

```
src/
|-- main.tsx                    # Router mount point (replace current direct App mount)
|-- App.tsx                     # Auth gate only — routes render inside here when authenticated
|-- index.css
|
|-- router/
|   `-- index.tsx               # Route definitions, lazy-loaded views
|
|-- store/                      # Zustand global state
|   |-- auth.ts                 # Auth state (move from useAuth hook — optional)
|   |-- patient.ts              # Active patient context (patientId, active tab)
|   |-- ui.ts                   # Sidebar open/closed, active route
|   `-- ai.ts                   # AI job queue, transcription state, draft SOAP
|
|-- lib/
|   `-- tauri.ts                # EXTENDED: add patient/scheduling/clinical/labs wrappers
|
|-- hooks/
|   |-- useAuth.ts              # Existing (keep)
|   |-- useIdleTimer.ts         # Existing (keep)
|   |-- usePatient.ts           # Load + cache patient data by ID via TanStack Query
|   |-- useSchedule.ts          # Appointment queries for a date range / provider
|   `-- useAI.ts                # Transcription recording + SOAP generation state machine
|
|-- components/
|   |-- auth/                   # Existing (LoginForm, RegisterForm, LockScreen, MfaPrompt)
|   |-- layout/
|   |   |-- AppShell.tsx        # Sidebar + TopNav + main content area
|   |   |-- Sidebar.tsx         # Nav links (Patients, Schedule, Billing, Admin)
|   |   `-- TopNav.tsx          # Active patient breadcrumb, user menu, AI status
|   |
|   |-- patients/
|   |   |-- PatientList.tsx     # Search + paginated list
|   |   |-- PatientSearch.tsx   # Debounced search bar
|   |   |-- PatientForm.tsx     # Create/edit demographics
|   |   `-- PatientChart.tsx    # Tabbed view (Summary|Appointments|Notes|Labs|Rx)
|   |
|   |-- scheduling/
|   |   |-- CalendarView.tsx    # Multi-provider day/week/month
|   |   |-- AppointmentForm.tsx # Create/edit appointment
|   |   |-- FlowBoard.tsx       # Real-time clinic status
|   |   `-- WaitlistBoard.tsx   # Waitlist + recall management
|   |
|   |-- clinical/               # Patient chart clinical data lists
|   |   |-- AllergyList.tsx
|   |   |-- ProblemList.tsx
|   |   |-- MedicationList.tsx
|   |   `-- ImmunizationList.tsx
|   |
|   |-- encounters/
|   |   |-- EncounterList.tsx   # Past encounters for a patient
|   |   |-- EncounterEditor.tsx # Active SOAP note editor
|   |   |-- SOAPForm.tsx        # S/O/A/P sections with rich text
|   |   |-- VitalsForm.tsx      # BP, HR, RR, Temp, SpO2, Weight, Height, BMI
|   |   |-- ROSForm.tsx         # 14 organ systems review
|   |   `-- PEForm.tsx          # Physical exam with template system
|   |
|   |-- labs/
|   |   |-- LabOrderForm.tsx
|   |   |-- LabResultsTable.tsx # Abnormal value flagging
|   |   |-- DocumentUpload.tsx
|   |   `-- DocumentList.tsx
|   |
|   |-- ai/                     # AI panel — additive, not load-bearing
|   |   |-- RecordButton.tsx    # Start/stop audio recording
|   |   |-- TranscriptReview.tsx # Human-in-the-loop transcript edit
|   |   `-- DraftSOAPPanel.tsx  # AI-generated SOAP with "Draft — Review Required" banner
|   |
|   |-- billing/
|   |   |-- ClaimForm.tsx       # CPT/ICD-10 coding, provider/patient info
|   |   |-- ClaimList.tsx       # Submitted claims, status, ERA
|   |   `-- FeeSchedule.tsx     # Fee schedule management
|   |
|   |-- eprescribe/
|   |   |-- PrescriptionForm.tsx
|   |   `-- WenoStatus.tsx      # Weno connection health indicator
|   |
|   |-- AuditLog.tsx            # Existing (keep)
|   |-- DatabaseStatus.tsx      # Existing (keep)
|   `-- FhirExplorer.tsx        # Existing (keep, dev tool)
|
`-- types/
    |-- auth.ts                 # Existing
    |-- audit.ts                # Existing
    |-- fhir.ts                 # Existing
    |-- patient.ts              # NEW
    |-- clinical.ts             # NEW
    |-- scheduling.ts           # NEW
    |-- encounters.ts           # NEW
    |-- labs.ts                 # NEW
    `-- billing.ts              # NEW

src-tauri/src/
|-- lib.rs                      # MODIFIED: register new commands
|-- commands/
|   |-- mod.rs                  # MODIFIED: add pub mod for new modules
|   |-- patient.rs              # Existing (S04)
|   |-- clinical.rs             # Existing (S05)
|   |-- scheduling.rs           # Existing (S06)
|   |-- documentation.rs        # Existing (S07)
|   |-- labs.rs                 # Existing (S08)
|   |-- backup.rs               # Existing (S09)
|   |-- transcription.rs        # NEW: whisper-rs audio transcription
|   |-- soap_gen.rs             # NEW: Ollama HTTP SOAP generation
|   |-- billing.rs              # NEW: X12 837P + fee schedule
|   `-- eprescribe.rs           # NEW: Weno Exchange API
`-- Cargo.toml                  # MODIFIED: add whisper-rs, ollama-rs/reqwest
```

### Structure Rationale

- **`store/` separation:** Zustand owns client state only (which patient is open, which tab is active, AI recording state). TanStack Query owns all server data (patient FHIR resources, appointments, labs). These must never mix — putting FHIR data in Zustand creates two sources of truth.
- **`components/ai/` isolation:** AI is additive, not load-bearing. If Ollama is not running or whisper model is not downloaded, clinical UI must still function. Separating AI components makes graceful degradation straightforward.
- **`lib/tauri.ts` as single IPC surface:** All `invoke()` calls in one file. When a Rust command signature changes, one edit in `tauri.ts` fixes all callers.
- **New Rust commands in new files:** `transcription.rs`, `soap_gen.rs`, `billing.rs`, `eprescribe.rs` are new capabilities, not changes to M001 behavior. Keeping them in separate files prevents conflicts with tested M001 code.

---

## Architectural Patterns

### Pattern 1: Extend lib/tauri.ts (Not Bypass It)

**What:** All Tauri IPC goes through `src/lib/tauri.ts` as typed wrapper functions. M002 extends this file. `invoke()` is never called directly from a component.

**When to use:** Every Tauri command invocation, without exception.

**Trade-offs:** Single point of truth for the IPC contract. Type errors caught at compile time when Rust signatures change. Slightly more setup per command, but prevents the scattered `invoke("some_command")` anti-pattern.

**Example addition to lib/tauri.ts:**
```typescript
import type { PatientInput, PatientResponse, SearchPatientsInput } from "../types/patient";

// In the commands object:
createPatient: (input: PatientInput) =>
  invoke<PatientResponse>("create_patient", { input }),

searchPatients: (query: SearchPatientsInput) =>
  invoke<PatientResponse[]>("search_patients", { query }),

transcribeAudio: (audioPath: string) =>
  invoke<TranscriptResult>("transcribe_audio", { audioPath }),

generateSoap: (transcript: string, patientId: string, encounterId: string) =>
  invoke<DraftSoapResult>("generate_soap", { transcript, patientId, encounterId }),
```

### Pattern 2: Active Patient Context via Zustand (IDs Only)

**What:** A single Zustand store (`store/patient.ts`) holds `activePatientId: string | null` and `activeTab`. Route params (`/patients/:id`) drive the store. Components call `usePatient(id)` from TanStack Query to get actual data.

**When to use:** Any component that needs to know "which patient are we viewing."

**Trade-offs:** Zustand for IDs is trivially simple. TanStack Query handles data fetching, caching, and stale-time logic. Two concerns cleanly separated.

**Example:**
```typescript
// store/patient.ts
interface PatientStore {
  activePatientId: string | null;
  activeTab: "summary" | "appointments" | "notes" | "labs" | "rx";
  setActivePatient: (id: string) => void;
  setActiveTab: (tab: PatientStore["activeTab"]) => void;
  clearActivePatient: () => void;
}
```

### Pattern 3: TanStack Query for All Tauri IPC Data Fetching

**What:** Every Tauri command that reads data is called inside `useQuery`. Mutations use `useMutation` with `onSuccess: () => queryClient.invalidateQueries(...)`.

**When to use:** All server-state (patient demographics, appointments, labs, encounters).

**Trade-offs:** Automatic cache invalidation, loading/error states, background refetch. Query key design is the cache invalidation contract — keep it hierarchical.

**Example:**
```typescript
// hooks/usePatient.ts
export function usePatient(patientId: string) {
  return useQuery({
    queryKey: ["patient", patientId],
    queryFn: () => commands.getPatient(patientId),
    enabled: !!patientId,
    staleTime: 5 * 60 * 1000, // 5 min
  });
}

// Query key hierarchy: ["patient", id] -> ["patient", id, "allergies"]
// Invalidating ["patient", id] cascades to all sub-queries for that patient
```

### Pattern 4: whisper-rs as Rust Crate (Not Sidecar)

**What:** Audio transcription runs inside the Tauri Rust process via `whisper-rs` v0.15.1, not as a PyInstaller sidecar or subprocess. The crate compiles whisper.cpp directly into the binary.

**When to use:** All voice transcription in M002.

**Why not a sidecar:**
- whisper-rs is a native Rust FFI binding — no separate process, no IPC overhead
- `coreml` feature flag enables Apple Neural Engine acceleration (3x real-time vs CPU)
- `metal` feature flag for GPU fallback
- whisper.cpp model files (`.bin`) are app data, not a binary — no notarization concern

**Caveats:** Large model files (large-v3-turbo ~1.5 GB) must be downloaded or bundled. CoreML requires pre-generating CoreML model files with `generate_coreml_model.py` from the whisper.cpp repo. Compilation time increases significantly with this crate added.

**Cargo.toml change:**
```toml
whisper-rs = { version = "0.15.1", features = ["coreml"] }
```

**Critical: Run transcription on spawn_blocking** — whisper is synchronous and takes 2-15 seconds. Must not block the Tauri async runtime:
```rust
#[tauri::command]
pub async fn transcribe_audio(audio_path: String) -> Result<TranscriptResult, AppError> {
    tokio::task::spawn_blocking(move || {
        run_whisper_sync(&audio_path)
    }).await.map_err(|e| AppError::Internal(e.to_string()))?
}
```

### Pattern 5: Ollama via reqwest in Rust (PHI Never Leaves Rust Layer)

**What:** SOAP generation calls Ollama at `http://127.0.0.1:11434` using `ollama-rs` (v0.3.4, Tokio async) from inside a Rust Tauri command. The React frontend sends structured intent; Rust assembles the full prompt with PHI.

**When to use:** All LLM calls (SOAP generation, future coding assist, diagnostic support).

**Why Rust calls Ollama, not JavaScript:**
- Patient allergies, medications, and problem list are fetched from SQLCipher in Rust and assembled into the prompt — PHI never appears in WebView network requests
- HIPAA minimum-necessary: WebView only receives the structured result, not the raw clinical context
- Future: streaming response can be forwarded via Tauri events (`app.emit("soap_token", token)`) for real-time streaming UI

**Cargo.toml change:**
```toml
ollama-rs = { version = "0.3.4", features = ["stream"] }
tokio = { version = "1", features = ["full"] }
```

**Data flow (PHI boundary):**
```
React: invoke("generate_soap", { transcript, patientId, encounterId })
  Rust: fetch allergies + medications + problems from DB   [PHI stays in Rust]
  Rust: assemble LLaMA prompt with clinical context        [PHI stays in Rust]
  Rust: POST http://127.0.0.1:11434/api/generate
  Rust: validate + parse LLM response
  Rust: return { soap: DraftSoap, confidence, source: "ai" }
React: render DraftSOAPPanel with "AI Draft — Review Required" banner
```

### Pattern 6: Weno Exchange via Rust reqwest (Credentials in Keychain)

**What:** E-prescribing uses Weno's Switch API — NCPDP SCRIPT 20170715 messages over HTTPS REST. All calls originate from a Rust Tauri command using `reqwest`.

**Why Rust:**
- Weno API credentials stored in macOS Keychain via `keyring` crate (already in Cargo.toml)
- JavaScript fetch from WebView would require exposing credentials in the frontend
- TLS certificate validation in `reqwest` with `rustls-tls` feature

**Technical requirements:**
- TLS 1.2 minimum (Weno requirement)
- Weno Switch API supports NewRx and CancelRx at minimum; refill and pharmacy change require additional setup
- Developer registration + kickoff meeting required before credentials are issued (4-8 week lead time)
- No NCPDP SCRIPT raw socket connection — Weno wraps SCRIPT in their NONCE schema over HTTPS

**Start Weno enrollment at M002 kickoff, not at the sprint that implements it.**

### Pattern 7: X12 837P via Rust (Evaluate x12-types First)

**What:** Claims are generated as X12 837P transaction sets in Rust, written to `app_data_dir/claims/`, then submitted to a clearinghouse HTTPS endpoint.

**Crate evaluation:**
- `x12-types` v0.9.1 (updated 2025-07-09): provides ASC X12 bindings implemented on demand. LOW confidence that 837P loop/segment coverage is complete for professional claims. Must verify against actual crate docs before depending on it.
- Alternative: custom Rust struct with `Display` impl — 837P for a basic professional claim is ~30-50 segments, well-specified in X12N 005010X222A1. A custom serializer is 400-600 lines and has zero external dependency risk.

**Recommendation:** Spike `x12-types` against the CMS 837P companion guide in the first billing task. If 837P loop structures are present and correct, use it. If not, write the custom serializer — it is not a large implementation.

---

## Data Flow

### Clinical UI Request Flow

```
URL: /patients/:patientId/notes
  -> PatientChart.tsx (router component)
  -> reads activePatientId from Zustand store
  -> triggers usePatient(patientId) via TanStack Query
     -> cache miss: invoke("get_patient") -> Rust: RBAC + DB -> PatientResponse
     -> cache hit: return cached data immediately
  -> renders <NotesTab patientId={patientId} />
     -> useEncounters(patientId) -> invoke("list_encounters")
     -> click encounter -> EncounterEditor
        -> useEncounter(encounterId) -> invoke("get_encounter")
        -> usePatientAllergies -> invoke("list_allergies") [for CDS alerts]
```

### AI Pipeline Data Flow

```
[Record] button click
  -> MediaRecorder captures audio (Web Audio API)
[Stop] button click
  -> Audio blob -> temp file via Tauri fs write
  -> invoke("transcribe_audio", { audioPath })
     -> Rust: load whisper model (cached in WhisperState after first load)
     -> Rust: spawn_blocking(|| whisper_rs transcription)
     -> Returns { text, segments }
  -> TranscriptReview.tsx: provider reviews and edits transcript
[Generate SOAP] click
  -> invoke("generate_soap", { transcript, patientId, encounterId })
     -> Rust: fetch patient allergies + medications + active problems
     -> Rust: assemble LLaMA prompt (all PHI assembly in Rust)
     -> Rust: POST http://127.0.0.1:11434/api/generate (ollama-rs)
     -> Returns { soap: DraftSoap, confidence, source: "ai" }
  -> DraftSOAPPanel.tsx: shows draft with mandatory review banner
[Sign] click
  -> invoke("update_encounter", { ..., source: "human" })
  -> Audit log: provider signed, source transition ai -> human recorded
```

### Billing Data Flow

```
EncounterEditor: [Code Encounter] button
  -> ClaimForm.tsx: pre-populated from encounter data
  -> Provider adds CPT codes, modifiers
[Create Claim] click
  -> invoke("create_claim", { encounterId, cptCodes, icdCodes, ... })
     -> Rust: validate required 837P fields
     -> Rust: generate X12 837P segments (x12-types or custom)
     -> Rust: write .x12 file to app_data_dir/claims/{claimId}.x12
     -> Rust: audit log claim creation
     -> Returns { claimId, filePath, segmentCount }
[Submit] click
  -> invoke("submit_claim", { claimId })
     -> Rust: read X12 file
     -> Rust: POST to clearinghouse HTTPS endpoint
     -> Returns { acknowledgmentId, status }
```

---

## Integration Points

### New Components: Where They Plug Into Existing Architecture

| New Component | Integrates With | Integration Method |
|---------------|-----------------|-------------------|
| React Router | `main.tsx` + `App.tsx` | Wrap `<Outlet>` in auth gate; replace flat App content |
| Zustand stores | All clinical components | `usePatientStore()` hook |
| TanStack Query | All data-fetching hooks | `QueryClientProvider` at root; hooks wrap `commands.*` |
| `lib/tauri.ts` extensions | All clinical components | Append typed wrappers; import `commands` object |
| `whisper-rs` | `commands/transcription.rs` | Rust crate only; no React dependency |
| Ollama HTTP | `commands/soap_gen.rs` | `ollama-rs` crate in Rust; React only sees typed result |
| Weno HTTPS | `commands/eprescribe.rs` | `reqwest` in Rust; credentials via `keyring` crate |
| X12 generator | `commands/billing.rs` | Pure Rust; writes files to `app_data_dir` |

### Modified Files in M001 Foundation

| File | What Changes | Risk |
|------|-------------|------|
| `src/App.tsx` | Add `<QueryClientProvider>` wrapper; auth gate logic unchanged | LOW |
| `src/main.tsx` | Mount providers at root; add router | LOW |
| `src/lib/tauri.ts` | Append new wrappers (additive only) | LOW |
| `src-tauri/src/lib.rs` | Register new commands (additive only) | LOW |
| `src-tauri/src/commands/mod.rs` | Add `pub mod` entries for new modules | LOW |
| `src-tauri/Cargo.toml` | Add `whisper-rs`, `ollama-rs`, `reqwest` | MEDIUM — whisper-rs coreml feature significantly increases build time and binary size |
| `src-tauri/tauri.conf.json` | Add `NSMicrophoneUsageDescription` to Info.plist section for audio recording permission | LOW |

---

## Scaling Considerations

This is a single-user desktop app. Scaling means data volume, not concurrent users.

| Scale | Architecture Adjustments |
|-------|--------------------------|
| 1-500 patients | Current approach. No changes needed. |
| 500-5000 patients | Add React virtualization (TanStack Virtual) to PatientList and LabResultsTable. Ensure `list_encounters` uses cursor pagination, not full load. |
| AI model memory | whisper-rs: lazy-load model on first transcription, cache in `WhisperState` managed resource, unload after 10 min idle. Ollama manages LLaMA lifecycle independently. |
| Claims volume | X12 files in `app_data_dir/claims/` — no size concern for a solo practice. ERA processing is sequential file reading — acceptable. |

### Scaling Priorities

1. **First bottleneck:** Patient list rendering at 1000+ records. Fix: `search_patients` with cursor-based pagination already exists in Rust backend; wire TanStack Virtual to paginate in React.
2. **Second bottleneck:** Whisper model cold-start (2-4 seconds). Fix: load model in background on app startup via a Tauri command called after auth succeeds, not on first Record click.

---

## Anti-Patterns

### Anti-Pattern 1: Direct invoke() Calls from Components

**What people do:** `const result = await invoke("get_patient", { patientId })` inside a component.

**Why it's wrong:** Bypasses TypeScript typing in lib/tauri.ts. String-based command names drift from Rust signatures silently. Current codebase establishes the correct pattern — M002 must follow it.

**Do this instead:** Add a typed wrapper in `lib/tauri.ts`, import `commands` in components.

### Anti-Pattern 2: PHI Assembly in React for AI or Prescriptions

**What people do:** Build the LLaMA prompt in TypeScript with patient data, call `fetch("http://127.0.0.1:11434/api/generate")` from React. Or pass Weno API key to the frontend.

**Why it's wrong:** PHI appears in WebView network traffic (visible in devtools, requires CSP relaxation). HIPAA minimum-necessary is architecturally violated. Weno credentials become accessible from JavaScript.

**Do this instead:** React invokes a Rust command with structured intent. Rust fetches clinical context from DB and assembles all outbound calls. Frontend only receives the typed result.

### Anti-Pattern 3: Putting FHIR Data in Zustand

**What people do:** `usePatientStore.setState({ patient: fullPatientObject })` to share patient data between tabs.

**Why it's wrong:** Zustand is client state, not a data cache. Stale data when mutations occur. Fights TanStack Query's cache invalidation. Creates two sources of truth for the same data.

**Do this instead:** Zustand stores `activePatientId: string` only. Components call `usePatient(id)` which goes through TanStack Query. TanStack Query owns all FHIR resource data.

### Anti-Pattern 4: Blocking the Tauri Main Thread with Whisper

**What people do:** Call `whisper_rs::transcribe(...)` synchronously inside a `#[tauri::command]` function (not async, or async without spawn_blocking).

**Why it's wrong:** Whisper on large-v3-turbo takes 2-15 seconds. Blocking the Tauri command handler freezes all IPC — the app appears completely unresponsive.

**Do this instead:** Mark command `async` and use `tokio::task::spawn_blocking`:
```rust
#[tauri::command]
pub async fn transcribe_audio(audio_path: String) -> Result<TranscriptResult, AppError> {
    tokio::task::spawn_blocking(move || {
        run_whisper_sync(&audio_path)
    }).await.map_err(|e| AppError::Internal(e.to_string()))?
}
```

### Anti-Pattern 5: Hash-Based Routing

**What people do:** Use `HashRouter` or `createHashRouter` because of a misbelief that Tauri requires hash routing.

**Why it's wrong:** Tauri 2 serves from its custom protocol and supports browser history routing correctly. Multiple official Tauri templates use React Router v6 with standard routing. Hash mode is unnecessary and produces uglier route strings.

**Do this instead:** Use `createBrowserRouter` (React Router v6). No hash mode needed.

---

## Build Order (Dependency Graph for M002 Slices)

```
S01: lib/tauri.ts extensions + TypeScript types
  Prerequisite for all React clinical components
  Add patient/scheduling/clinical/docs/labs/billing wrappers
  No Rust changes needed (all commands already exist in M001)
       |
       v
S02: State management foundation (Zustand + TanStack Query setup)
  Depends on: S01 (types needed for store shapes)
  Install zustand, @tanstack/react-query
  Set up QueryClientProvider, define store shapes
       |
       v
S03: Router + AppShell layout
  Depends on: S02 (auth store needed for router auth guard)
  Install react-router-dom v6
  Create route tree, AppShell with Sidebar + TopNav
  Replace flat App.tsx rendering with router outlet
       |
       v
S04: Patient List + Patient Chart shell
  Depends on: S03 (router required for /patients/:id routes)
  PatientList, PatientSearch, PatientChart skeleton with tab navigation
  usePatient and usePatientSearch hooks
       |
       +---> S05: Clinical data tabs (parallel with S06)
       |       AllergyList, ProblemList, MedicationList, ImmunizationList
       |       Fills Summary and clinical tabs in PatientChart
       |
       +---> S06: Scheduling (parallel with S05)
               CalendarView, FlowBoard, WaitlistBoard
               Depends on patient context from S04
               |
               v
             S07: Encounter documentation
               Depends on: S04 + S05 (CDS alerts need allergy + medication data)
               EncounterEditor, SOAPForm, VitalsForm, ROSForm, PEForm
               |
               v
             S08: AI pipeline
               Depends on: S07 (encounter editor must exist to receive SOAP output)
               NEW Rust: transcription.rs (whisper-rs), soap_gen.rs (ollama-rs)
               NEW React: RecordButton, TranscriptReview, DraftSOAPPanel
               Cargo.toml: add whisper-rs (coreml), ollama-rs
               CRITICAL: whisper model download must be part of setup flow
               |
               v
             S09: Billing
               Depends on: S07 (encounters to bill) + S05 (diagnoses)
               NEW Rust: billing.rs (X12 837P generation)
               NEW React: ClaimForm, ClaimList, FeeSchedule
               |
               v
             S10: E-prescribing (Weno)
               Depends on: S05 (medications list) + S07 (encounter context)
               NEW Rust: eprescribe.rs (Weno HTTPS)
               NEW React: PrescriptionForm, WenoStatus
               START Weno developer enrollment at M002 kickoff (4-8 weeks)
```

**Parallelization opportunities:**
- S05 and S06 build in parallel after S04 (patient chart shell)
- Weno enrollment runs independently from day 1 — no coding dependency
- whisper-rs CoreML model generation (Python script, runs once) can spike during S07 while the encounter editor is being built
- x12-types evaluation spike can happen during S07 to validate 837P coverage before S09 starts

---

## Sources

- Tauri 2 sidecar documentation (official): [https://v2.tauri.app/develop/sidecar/](https://v2.tauri.app/develop/sidecar/) — HIGH confidence
- `whisper-rs` v0.15.1, coreml + metal feature flags confirmed, Cargo.toml verified via GitHub raw: [https://crates.io/crates/whisper-rs](https://crates.io/crates/whisper-rs) — HIGH confidence
- `ollama-rs` v0.3.4 (updated 2026-02-12), async Tokio, streaming: [https://github.com/pepperoni21/ollama-rs](https://github.com/pepperoni21/ollama-rs) — HIGH confidence
- `x12-types` v0.9.1 (updated 2025-07-09): [https://crates.io/crates/x12-types](https://crates.io/crates/x12-types) — LOW confidence on 837P completeness; verify before committing
- Weno Exchange Switch API (NCPDP SCRIPT 20170715 over HTTPS): [https://wenoexchange.com/api-learn-more/](https://wenoexchange.com/api-learn-more/) — MEDIUM confidence (full technical docs require dashboard registration)
- Weno Switch API July 2025 PDF: [https://wenoexchange.com/wp-content/uploads/2025/07/Switch_API_Documentation_07-14-2025.pdf](https://wenoexchange.com/wp-content/uploads/2025/07/Switch_API_Documentation_07-14-2025.pdf) — HIGH confidence
- React Router in Tauri 2 (no hash mode needed): [https://github.com/tauri-apps/tauri/discussions/7899](https://github.com/tauri-apps/tauri/discussions/7899) — MEDIUM confidence
- TanStack Router + Tauri 2 template: [https://github.com/thecodingmontana/tauri-tanstarter](https://github.com/thecodingmontana/tauri-tanstarter) — MEDIUM confidence
- MedArc M001 codebase (directly verified): 50+ Rust commands in lib.rs, lib/tauri.ts covers auth/FHIR/audit only, package.json has no router or state management deps — HIGH confidence

---

*Architecture research for: MedArc M002 — Clinical UI + AI Pipeline + Billing*
*Researched: 2026-03-11*

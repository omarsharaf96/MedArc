# Project Research Summary

**Project:** MedArc — AI-Powered Desktop EMR (M002)
**Domain:** Solo-practice desktop EMR — clinical UI, AI voice pipeline, billing, e-prescribing additions
**Researched:** 2026-03-11
**Confidence:** MEDIUM-HIGH

## Executive Summary

MedArc M002 is almost entirely a React frontend build on top of a complete, tested Rust backend. M001 delivered 50+ Tauri commands covering every clinical domain (patient CRUD, scheduling, clinical data, encounters, labs, documents, audit), but shipped zero clinical UI — App.tsx is an auth gate pointing to no views. The core work of M002 is wiring those commands to a tabbed clinical UI shell, which then hosts SOAP note entry, vitals/ROS/PE forms, billing, and e-prescribing. The AI voice pipeline (whisper-rs + Ollama/LLaMA 3.1 8B) is the primary product differentiator and belongs in a post-baseline phase, after the manual clinical workflow is validated with real users.

The recommended approach is a strictly dependency-ordered build: IPC type bindings and Zustand/TanStack Query foundations first, then the router and app shell, then patient chart UI, then clinical data tabs and scheduling in parallel, then encounter documentation, then AI pipeline on top of a working encounter editor, then billing and e-prescribing as separate parallel tracks. This order mirrors the architecture dependency graph confirmed by all four research files. Weno Exchange enrollment must begin at M002 kickoff — it has a 2-4 week external lead time that blocks the e-prescribing phase if not initiated on day one.

The top four risks are: (1) TypeScript types drifting silently from Rust structs if not generated from source, creating runtime serialization failures that are hard to diagnose; (2) macOS App Sandbox blocking Ollama and Weno API calls in notarized production builds even though dev mode works; (3) AI-generated SOAP notes and billing codes presented without mandatory human review, creating malpractice and CMS audit exposure; and (4) the x12-types crate for 837P generation being incomplete for real-world claim scenarios. Each has a clear mitigation, but all four must be addressed before the relevant phase starts — none can be retrofitted cleanly.

---

## Key Findings

### Recommended Stack

The M001 stack (Tauri 2.x + React 18 + TypeScript + Vite 5 + TailwindCSS 3 + SQLCipher) is unchanged. M002 adds navigation, state management, UI components, and AI/billing infrastructure on top.

**Core technologies (new for M002):**
- **react-router 7.13.1**: Client-side routing for patient chart tabs — use SPA mode (`ssr: false`), import from `react-router` only (react-router-dom is merged in v7)
- **zustand 5.0.11**: Global client state for active patient ID, auth session, and AI job queue — stores IDs only, never FHIR resource data
- **whisper-rs 0.15.1**: Rust-native whisper.cpp bindings with `coreml` feature for Apple Neural Engine acceleration — runs in-process via `spawn_blocking`, no Python sidecar
- **ollama-rs 0.3.4**: Rust async Tokio client for Ollama REST API — SOAP generation calls originate from Rust so PHI never reaches the WebView layer
- **shadcn/ui (CLI)**: Component system for chart tabs, cards, command palette — no runtime dependency, Radix UI primitives, Tauri-native template available
- **@tanstack/react-query 5.90.21**: Async state for all Tauri IPC data fetching — set `staleTime: Infinity` and `retry: false` for local IPC
- **@tanstack/react-table 8.21.3**: Headless table for lab results, billing line items, medication lists — add TanStack Virtual for large datasets
- **react-hook-form 7.x + zod 4.3.6**: Form management and schema validation — do NOT use react-hook-form v8 (beta breaks useFieldArray); zod v4 is 14x faster than v3
- **x12-types 0.9.x (LOW confidence)**: Rust X12 segment bindings for 837P claim generation — spike against CMS 837P companion guide before depending on it; custom serializer is the documented fallback (400-600 lines)

**What NOT to use:** react-router-dom as a separate package, Python sidecar for audio (adds 2 GB binary), cloud STT/LLM (PHI leaves device), react-hook-form v8 beta, OpenAI direct API (no BAA), "Accept All" UI for AI code suggestions, Redux Toolkit (excessive boilerplate for this complexity).

See `/STACK.md` for full rationale and alternatives.

### Expected Features

**Must have for M002 launch (P1 — clinical UI that makes the M001 backend usable for daily care):**
- Patient banner + tab navigation (Summary, Encounters, Meds, Problems, Allergies, Labs, Documents) — prevents wrong-patient errors, matches Epic/DrChrono conventions physicians expect
- Patient facesheet with active problems, medications, allergies, last vitals, upcoming appointments
- Structured SOAP note entry with ICD-10 linking, draft auto-save every 30s, note status workflow (draft → signed), 10-15 specialty templates
- Vitals entry (BP/HR/RR/Temp/SpO2/Weight/Height/BMI) with BMI auto-calc and abnormal value flagging
- 14-system Review of Systems form with real-time E/M level indicator
- System-based Physical Exam templates with "all normal" macro and specialty variants
- Billing fee sheet with CPT/ICD-10 search, modifiers, charge capture, and superbill
- X12 837P claim generation and Office Ally clearinghouse submission
- E-prescribing via Weno Exchange (non-EPCS) with RxNorm drug search, pharmacy routing, allergy conflict alerts

**Should have — M002.x (add once P1 workflow is validated with real users):**
- AI voice-to-SOAP pipeline (whisper-rs transcription + Ollama/LLaMA 3.1 8B generation) — primary product differentiator
- AI CPT/ICD-10 coding suggestions via FAISS vector search — unique in the solo-practice market segment
- ERA/835 payment posting and accounts receivable tracking
- EPCS for controlled substances via Weno ONLINE API — DEA enrollment is a parallel track
- Drug interaction checking via RxNav-in-a-Box (Docker, local, free with UMLS license)
- E/M level real-time calculator based on documented ROS + PE + MDM complexity
- Refill request inbox with one-click approval (high daily time savings for solo physician)

**Defer to Phase 3+ (future consideration):**
- Formulary tier display (requires PBM data feeds)
- Prior authorization detection (requires formulary data)
- Denial pattern detection (requires 6+ months of claims data)
- Pediatric growth charts (pediatric specialty, not general practice)
- Ambient continuous capture mode (significantly more complex than push-to-record MVP)

**Anti-features — do not build:**
- "Accept All" for AI coding suggestions (CMS audit exposure; individual code review is a compliance requirement)
- Auto-sign generated note without physician review (malpractice liability, CMS scrutiny of AI scribes)
- Copy-forward entire prior note (documented cause of medical record errors and HIPAA audit findings)
- Infinite scroll within chart sections (breaks clinical data landmarks)
- Raw audio long-term storage (ePHI, consent complexity, 50-100 MB per encounter storage burden)
- Direct SureScripts connection (not designed for small EMR vendors; Weno exists for this exact use case)

See `/FEATURES.md` for full feature dependency graph and prioritization matrix.

### Architecture Approach

M002 extends the existing Tauri 2 + SQLCipher architecture by adding a React Router SPA with four Zustand stores (auth, activePatient, ui, aiJobs) and TanStack Query wrapping all Tauri IPC calls. The critical architectural boundary: Zustand holds IDs only — TanStack Query owns all FHIR resource data. All PHI assembly for AI prompts and Weno API calls happens in Rust, never in the WebView. The single `src/lib/tauri.ts` file remains the only surface where `invoke()` is called; TypeScript bindings must be generated from Rust structs (ts-rs or tauri-specta) before any component is written.

**Major components:**
1. **React Router + AppShell** — Route tree (`/patients/:id/:tab`, `/schedule`, `/billing`, `/admin`) with auth guard; Sidebar + TopNav layout
2. **PatientChart (tabbed)** — Drives all clinical views; reads activePatientId from Zustand, fetches FHIR data via TanStack Query, renders tab-based clinical content
3. **Encounter Editor + SOAP forms** — SOAPForm, VitalsForm, ROSForm, PEForm; draft auto-save every 30s; note status workflow; required before billing or AI can be tested end-to-end
4. **AI Panel (additive, isolated)** — RecordButton, TranscriptReview, DraftSOAPPanel in `components/ai/`; designed to degrade gracefully when Ollama is unavailable; audit trail extended for all AI operations
5. **Billing module** — ClaimForm + X12 837P generation in Rust `commands/billing.rs`; Office Ally clearinghouse submission via reqwest; ERA 835 parsing
6. **E-prescribing module** — PrescriptionForm + WenoStatus; Rust `commands/eprescribe.rs` via reqwest with Weno credentials in macOS Keychain; NCPDP SCRIPT message lifecycle

**Key architectural patterns to follow:**
- Extend `lib/tauri.ts`, never bypass it with raw `invoke()` calls from components
- Generate TypeScript bindings from Rust structs — never hand-write IPC types
- Zustand for IDs only; TanStack Query for all server state with `staleTime: Infinity` and `retry: false` for local IPC
- Whisper runs on `spawn_blocking` in Rust — not on the Tauri async runtime — to prevent UI freeze during 2-15s transcription
- PHI assembly for LLM prompts and Weno calls stays in Rust; React receives only the typed result

See `/ARCHITECTURE.md` for full component tree, project structure, data flow diagrams, and anti-patterns.

### Critical Pitfalls

1. **TypeScript types hand-written instead of generated from Rust structs** — serde snake_case vs TS camelCase drift, optional field mismatches, and enum shape differences cause silent runtime corruption. Use ts-rs or tauri-specta before the first UI component. Cannot be retrofitted after 20+ components exist. (Phase 1)

2. **App Sandbox blocking Ollama and Weno API calls in production builds** — `entitlements.plist` must include `com.apple.security.network.client`; OLLAMA_ORIGINS must be set via LaunchAgent plist (not .zshrc); test every network call in a notarized build, not just `tauri dev`. Confirmed via Tauri GitHub issue #13878. (Phase 1 validation)

3. **AI pipeline bypasses the HIPAA audit chain** — Raw audio transcription and LLM inference are ePHI operations. Every AI operation (transcription_started, transcription_completed, soap_generated, soap_accepted/rejected) must flow through the existing Rust audit system. Cannot be retrofitted after AI features are in use. (Phase 4)

4. **Weno Exchange certification not initiated early enough** — Prescriber identity proofing takes 2-4 weeks; EPCS DEA audit takes longer. The $300 activation and onboarding must happen at M002 kickoff, not when code is ready. NCPDP SCRIPT 2017071 is retired January 1, 2028 — build the message layer version-switchable from day one. (Start at M002 kickoff)

5. **X12 837P claim builder fails on real-world scenarios** — The 837P spec is 900+ pages; a simple office visit covers roughly 10% of real claims. Test against a 10-scenario set (modifiers -25/-59, secondary insurance, Medicare rendering provider, mental health taxonomy, telehealth -95, prior auth number, denial resubmission) via Office Ally sandbox before declaring billing complete. (Phase 5)

6. **AI coding suggestions presented as authoritative** — RAG-enhanced LLM coding achieves ~69% exact match (Nature 2025). "Accept All" is never acceptable. Show confidence scores, run CCI edit validation before surfacing suggestions, require individual code confirmation. Log every accepted AI suggestion in the audit trail. (Phase 5)

7. **PHI leaking into URL parameters, browser console, and TanStack Query cache** — Use numeric record IDs in URLs only (never patient names), call `queryClient.clear()` on session lock, disable React DevTools and Safari Web Inspector in production builds. Establish this as a code review gate in Phase 1. (Phase 1)

See `/PITFALLS.md` for 10 critical pitfalls, technical debt patterns, integration gotchas, and performance traps.

---

## Implications for Roadmap

Based on combined research, the architecture dependency graph maps to a 5-phase roadmap with one parallel subtraction (billing + e-prescribing run in parallel in Phase 5). Weno enrollment is a day-one background task, not a phase.

### Phase 1: IPC Foundation + App Shell
**Rationale:** No UI component can be built or tested until TypeScript bindings match Rust structs and the router/layout shell exists. This is the enabling infrastructure for everything else. Doing this phase correctly prevents the most expensive pitfall (type drift). App Sandbox validation must also happen here — all subsequent phases assume networking works in production builds.
**Delivers:** Generated TypeScript bindings for all 50+ Rust commands; Zustand stores (auth, activePatient, ui, aiJobs); TanStack Query with Tauri-optimized configuration; React Router SPA with auth guard; AppShell with Sidebar and TopNav; RBAC resource entries for all M002 resource types; App Sandbox networking validated in a notarized build
**Addresses:** Foundation for all patient chart features; offline and PHI handling patterns established
**Avoids:** Pitfalls 1 (type mismatch), 2 (App Sandbox), 7 (offline state model), 8 (PHI in cache/URLs), 10 (RBAC not extended)
**Research flag:** Standard patterns — well-documented Tauri + React Router integration

### Phase 2: Patient Chart + Clinical Data Views
**Rationale:** The patient chart UI is the delivery vehicle for every other M002 feature. Clinical data tabs must exist before encounter documentation because the encounter editor pulls allergy/medication/problem data into CDS alerts. Scheduling can be built in parallel with clinical data tabs since both depend only on the Phase 1 router and patient chart shell.
**Delivers:** PatientList with debounced search and pagination; PatientChart tabbed shell with patient banner; facesheet summary view; AllergyList, ProblemList, MedicationList, ImmunizationList; LabResultsTable with abnormal flagging; scheduling CalendarView, FlowBoard, WaitlistBoard
**Addresses:** All table-stakes patient chart UI features; clinical data display; scheduling workflow
**Avoids:** Pitfall 10 (RBAC for new resources extended as each component is added); TanStack Virtual added preemptively for clinical lists
**Research flag:** Standard patterns — no additional research needed

### Phase 3: Encounter Documentation
**Rationale:** Encounter documentation is the highest-frequency physician workflow (15-25 encounters/day). It must be built and validated manually before the AI pipeline is added on top. Billing also depends on signed, coded encounters — so this phase is the prerequisite for both Phase 4 and Phase 5.
**Delivers:** EncounterEditor + SOAPForm (structured S/O/A/P, ICD-10 linking, draft auto-save, sign workflow, addendum-on-signed pattern); VitalsForm with BMI auto-calc and abnormal value flagging; 14-system ROSForm with E/M level indicator; specialty-based PEForm with "all normal" macro; 10-15 encounter templates; encounter list with status; PDF export of signed note
**Addresses:** All SOAP note entry, vitals, ROS, and PE table-stakes features from FEATURES.md
**Avoids:** Copy-forward anti-feature; auto-sign anti-feature; mandatory rich text anti-feature; note status workflow correctly gated by RBAC
**Research flag:** Standard patterns — E/M coding guidelines are regulatory standards (2021 CMS); SOAP form patterns well understood

### Phase 4: AI Voice Pipeline
**Rationale:** The AI pipeline is additive on top of a working encounter editor. It must be isolated in `components/ai/` so clinical workflow continues when Ollama is unavailable. Adding AI before the manual workflow is validated creates confounding variables — neither the physician nor the team can assess AI quality if the base workflow is also unfamiliar. Whisper-rs CoreML model generation must be validated in a production build before UI integration.
**Delivers:** RecordButton + TranscriptReview + DraftSOAPPanel with mandatory "AI Draft — Review Required" banner; whisper-rs Rust command with CoreML acceleration and `spawn_blocking`; ollama-rs SOAP generation from Rust with PHI boundary enforced; audit log extensions for all AI operations (transcription_started, transcription_completed, soap_generated, soap_accepted/rejected); Ollama health check with graceful degradation; patient context injection into LLM prompt (allergies, medications, active problems)
**Addresses:** AI SOAP generation table stakes and differentiators; ambient capture deferred to Phase 3+
**Avoids:** Pitfalls 3 (AI bypassing audit chain), 9 (CoreML model not in production bundle); anti-features: auto-submit, streaming during encounter, raw audio retention
**Research flag:** NEEDS RESEARCH — whisper-rs CoreML build requirements on target Xcode version; Ollama CORS configuration for sandboxed notarized Tauri app; LLaMA 3.1 8B prompt engineering for SOAP structure; CoreML .mlmodelc production bundle inclusion

### Phase 5: Billing + E-Prescribing (Parallel Tracks)
**Rationale:** Billing and e-prescribing are independent tracks that can be built in parallel after the encounter documentation foundation exists. Both depend on signed notes and the clinical data in the patient chart, but not on each other and not on the AI pipeline. Weno enrollment (initiated at M002 kickoff) should be complete by the time this phase starts.
**Delivers (Billing track):** ClaimForm with CPT/ICD-10 search and modifier support; X12 837P generation in Rust (spike x12-types first; fallback to custom serializer); Office Ally clearinghouse submission; claim status tracking; ERA/835 payment posting with CARC/RARC interpretation; AR dashboard; AI CPT/ICD-10 suggestions with per-code review UX, confidence scores, and CCI edit pre-validation
**Delivers (E-prescribing track):** Drug search via RxNorm local database (RxNav-in-a-Box or bundled SQLite snapshot); prescription creation form; Weno Exchange NewRx/CancelRx/RxRenewalRequest/RxRenewalResponse (full lifecycle, not just NewRx); pharmacy directory routing; drug interaction checking; allergy conflict hard stops; EPCS via Weno ONLINE API (if enrollment complete); refill request inbox
**Addresses:** All billing and e-prescribing features from FEATURES.md
**Avoids:** Pitfalls 4 (Weno enrollment), 5 (837P claim scenarios), 6 (AI coding as authoritative); NCPDP SCRIPT version abstracted for 2028 migration
**Research flag:** NEEDS RESEARCH (Billing) — x12-types v0.9.x 837P loop coverage verification; Office Ally sandbox submission and 10-scenario test suite; ERA 835 CARC/RARC mapping. NEEDS RESEARCH (E-Rx) — Weno Switch API full message lifecycle (blocked until developer account); NCPDP SCRIPT 2023011 vs 2017071 feature delta for migration planning; EPCS DEA Part 1311 biometric/hardware token requirements

### Phase Ordering Rationale

- Phases 1 → 2 → 3 are strictly ordered by dependency: IPC types must exist before components; patient chart must exist before encounters
- Phase 4 (AI) is gated on Phase 3 because the AI panel outputs to the encounter editor and the audit trail must be extended before AI code runs
- Phase 5 billing and e-prescribing are gated on Phase 3 (signed encounters to bill/prescribe from) but are independent of Phase 4 (AI) — these three phases can sequence as 3 → 4 and 3 → 5 in parallel
- Weno enrollment must be initiated on Day 1 of M002 regardless of Phase 5 timing — 2-4 week external lead time is the binding constraint
- The EPCS architecture decision (own DEA audit vs Weno ONLINE API) must be made before any Phase 5 architecture work begins

### Research Flags

Phases needing deeper research during planning:
- **Phase 4 (AI Pipeline):** whisper-rs CoreML build requirements; Ollama CORS in sandboxed notarized app; LLaMA 3.1 8B SOAP prompt engineering; CoreML .mlmodelc production bundle inclusion
- **Phase 5-Billing:** x12-types v0.9.x 837P loop coverage (spike required); Office Ally sandbox API and test claim suite; ERA 835 CARC/RARC standard code mapping; FAISS ICD-10/CPT index build pipeline and codebook licensing (AMA license for CPT)
- **Phase 5-E-Rx:** Weno Switch API full lifecycle (requires developer account); NCPDP SCRIPT 2023011 migration delta; DEA Part 1311 EPCS biometric requirements

Phases with standard patterns (skip research-phase):
- **Phase 1 (IPC Foundation):** React Router v7 + Tauri SPA mode is well-documented; Zustand + TanStack Query patterns are established
- **Phase 2 (Patient Chart):** Standard React component patterns over existing Tauri commands; no novel integrations
- **Phase 3 (Encounter Docs):** E/M coding guidelines are regulatory standards; SOAP form patterns are well understood

---

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | MEDIUM-HIGH | All npm versions verified against current registry (March 2026); whisper-rs CoreML confirmed active (Sep 2025); ollama-rs confirmed active (Feb 2026); x12-types is LOW confidence specifically for 837P completeness — spike before committing |
| Features | HIGH | Requirements backed by regulatory standards (DEA 1311, CMS E/M guidelines, NCPDP SCRIPT), peer-reviewed AI coding accuracy studies (NEJM AI, Nature 2025), and direct competitor analysis (OpenEMR v8, DrChrono, Practice Fusion) |
| Architecture | HIGH (integration patterns); MEDIUM (whisper-rs CoreML build); LOW (x12-types 837P) | M001 codebase directly verified — 50+ commands in lib.rs; lib/tauri.ts covers auth/FHIR/audit only; Tauri integration patterns confirmed via official docs and GitHub discussions |
| Pitfalls | MEDIUM-HIGH | Healthcare regulations HIGH confidence (stable regulatory standards); Tauri App Sandbox issue confirmed via GitHub #13878; AI hallucination rates from peer-reviewed sources; Weno certification timeline from official Weno documentation |

**Overall confidence:** MEDIUM-HIGH

### Gaps to Address

- **x12-types 837P coverage:** Verify during Phase 5 billing spike (1-2 days) before committing to the crate. Custom serializer fallback is 400-600 lines and has zero external dependency risk.
- **Weno Switch API technical documentation:** Full message lifecycle requires a Weno developer account. Initiate registration at M002 kickoff. Abstract the NCPDP SCRIPT version at module boundary from day one — mandatory 2023011 migration by January 1, 2028 per CMS.
- **whisper-rs CoreML production bundle:** CoreML `.mlmodelc` files must be pre-generated via whisper.cpp's `generate_coreml_model.py` and verified resolvable inside the signed Tauri app bundle. Validate in a notarized production build before declaring Phase 4 complete.
- **EPCS path decision (business decision):** Own DEA audit (4-8 weeks) vs Weno ONLINE API (Weno handles DEA compliance). This decision must be made at M002 kickoff — it affects Phase 5 architecture before any code is written.
- **FAISS ICD-10/CPT index:** AI billing coding suggestions require a FAISS index of ICD-10-CM (NLM, free) and CPT (AMA license, ~$200/year). Index build pipeline not covered in current research. Address during Phase 5 AI coding planning.
- **Generated TypeScript bindings tooling:** Choose between ts-rs (simpler, generates types from proc macro) and tauri-specta (generates types + validates command signatures). Decision must be made in Phase 1 before any IPC wrappers are extended.

---

## Sources

### Primary (HIGH confidence)
- MedArc M001 codebase (directly verified) — 50+ Rust commands in lib.rs; lib/tauri.ts covers auth/FHIR/audit only; package.json has no router or state management dependencies
- CMS.gov — X12 837P 5010A1 implementation guide; E/M coding 2021 guidelines; Medicare Part D EPCS mandate
- DEA Diversion Control Division — 21 CFR Part 1311 EPCS requirements; biometric/hardware token two-factor mandates
- NLM/LHNCBC — RxNav-in-a-Box official documentation; UMLS license requirements; ICD-10-CM codebook
- Weno Exchange — Official Switch API documentation; EPCS DEA 1311.120 compliance; OpenEMR wiki integration pattern
- NEJM AI / Nature 2025 — AI medical coding accuracy: GPT-4 33.9% ICD-10 exact match without RAG; ~69% with RAG+FAISS
- Tauri 2 official docs — sidecar, CSP, entitlements, App Sandbox networking requirements
- whisper-rs crates.io — v0.15.1 (released 2025-09-10), coreml + metal feature flags confirmed
- ollama-rs GitHub — v0.3.4 (updated 2026-02-12), async Tokio, streaming confirmed

### Secondary (MEDIUM confidence)
- react-router npm — v7.13.1 current (March 2026); SPA mode documented at reactrouter.com
- zustand npm — v5.0.11 current; useSyncExternalStore native in v5
- @tanstack/react-query npm — v5.90.21 current
- @tanstack/react-table npm — v8.21.3 current; v9 in development but not stable
- shadcn vs MUI comparison (makersden.io 2025) — bundle size and Tauri integration notes
- agmmnn/tauri-ui template — shadcn + Tauri 2 integration confirmed working
- Tauri GitHub issue #13878 — App Sandbox blocking outbound HTTP confirmed in production builds
- AMA 2025 / NEJM Catalyst — AI scribe documentation workflow and physician burnout research
- OpenEMR v8, DrChrono, Practice Fusion — competitor feature analysis (patient chart UI, e-Rx, billing)
- NCPDP SCRIPT 2017071 retirement — January 1, 2028 deadline per CMS

### Tertiary (LOW confidence)
- x12-types crates.io v0.9.1 (updated 2025-07-09) — 837P loop coverage unverified; spike required before committing
- WhisperKit (argmaxinc, ICML 2025) — Apple Neural Engine alternative to whisper.cpp; noted for M003 evaluation only, not recommended for M002
- node-x12 npm — JavaScript X12 fallback option; not the recommended path for data that already lives in Rust

---
*Research completed: 2026-03-11*
*Ready for roadmap: yes*

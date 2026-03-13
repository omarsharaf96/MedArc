# Pitfalls Research

**Domain:** Adding clinical UI, AI voice pipeline, X12 billing, and Weno e-prescribing to an existing Tauri 2 + React 18 desktop EMR (M002 focus)
**Researched:** 2026-03-11
**Confidence:** MEDIUM-HIGH (healthcare regulations HIGH confidence; AI-specific claims MEDIUM; Tauri-specific integration issues confirmed via GitHub issues)

> **Scope note:** This document focuses on M002-specific pitfalls — mistakes that occur when adding a full React UI layer, AI transcription pipeline, X12 billing, and Weno e-prescribing to an existing Rust backend that already has a complete data model. The existing M001 PITFALLS.md covers greenfield architecture pitfalls (FHIR storage, HIPAA-as-checkbox, SQLCipher performance). This document covers integration, retrofit, and extension pitfalls.

---

## Critical Pitfalls

### Pitfall 1: React UI Assumes Backend Contract That Doesn't Exist

**What goes wrong:**
The Rust backend (M001) exposes ~50 Tauri commands with well-defined Rust structs. When building the React UI, developers write TypeScript interfaces that look plausible but diverge from the actual Rust types — field names differ (snake_case in Rust vs camelCase in TS), enums are stringly-typed on one side but structured on the other, optional fields are non-nullable in TS, and date formats differ (ISO 8601 string vs Unix timestamp). The UI compiles with no errors but silently corrupts data at runtime, or Tauri IPC calls fail with cryptic serialization errors that don't point to the type mismatch.

**Why it happens:**
Tauri IPC uses JSON serialization (serde) on the Rust side and TypeScript on the frontend — two independent type systems with no shared schema. Developers write TS types from memory or by reading Rust structs, and the two drift over time. The Rust backend was built in M001 without generated TypeScript bindings.

**How to avoid:**
- Generate TypeScript types from Rust structs using `tauri-specta` or `ts-rs` as the first action of M002, before writing any UI component. Every Tauri command must have a generated TypeScript binding — no hand-written IPC types.
- Run `cargo test` as part of the frontend build pipeline so type changes on the Rust side break the build before UI code diverges.
- Define a strict naming convention: Rust uses snake_case, serde renames to camelCase via `#[serde(rename_all = "camelCase")]`. Enforce this in all new commands.
- Add integration tests that call every Tauri command from TypeScript and assert response shape, not just that the call succeeds.

**Warning signs:**
- TypeScript interfaces for Tauri responses are written by hand, not generated
- Any field in a TS interface has a different name than the Rust struct field
- IPC call errors are caught globally with "unknown error" rather than typed
- `JSON.parse` appears anywhere in frontend Tauri response handling

**Phase to address:** M002 Phase 1 (Clinical UI) — generate bindings before the first UI component is built. Retrofitting types after 20+ components are built is a multi-day refactor.

---

### Pitfall 2: App Sandbox Blocks Ollama, FastAPI Sidecar, and Outbound HTTP

**What goes wrong:**
MedArc's `entitlements.plist` (added in S09) enables App Sandbox. When the AI pipeline is added in M002, the app cannot reach the FastAPI sidecar on `127.0.0.1:8000`, cannot connect to Ollama on `127.0.0.1:11434`, and the auto-updater's outbound HTTPS calls also fail. All three fail silently — the Rust `reqwest` calls return connection refused, the React UI shows a spinner forever, and no error surfaces because the sandbox kill is at the OS level.

**Why it happens:**
macOS App Sandbox blocks all outbound network connections by default. The `com.apple.security.network.client` entitlement must be explicitly declared. Tauri's GitHub issue #13878 documents this exact failure: all outgoing HTTP requests from the Rust core are blocked in production sandboxed builds. This is a known, documented issue that only manifests in production notarized builds — not in development.

**How to avoid:**
- Verify `entitlements.plist` includes `com.apple.security.network.client` (Boolean true) before building any AI feature. MedArc's S09 `entitlements.plist` must be audited against this requirement immediately.
- Test every network call (Ollama, FastAPI, updater, Weno Exchange API) in a notarized test build on a clean Mac before any M002 slice is considered complete.
- Ollama requires its CORS origins to include the Tauri app's origin. On macOS, Ollama runs as a GUI app and does not inherit shell environment variables. The `OLLAMA_ORIGINS` variable must be set via a LaunchAgent plist, not `.zshrc`. Add setup documentation for clinic deployment.
- The FastAPI sidecar must bind to `127.0.0.1` only (already planned). The sandbox allows loopback connections when `com.apple.security.network.client` is set.
- For the Tauri sidecar notarization issue (GitHub #11992): when signing with a Developer ID Application certificate, use `--keychain $HOME/Library/Keychains/login.keychain-db` explicitly to prevent "nested code is modified or invalid" notarization failures.

**Warning signs:**
- AI features tested only in `npm run tauri dev` (development mode — no sandbox)
- `entitlements.plist` does not include `com.apple.security.network.client`
- Ollama setup instructions say to export `OLLAMA_ORIGINS` in `.zshrc`
- No notarized test build before M002 AI slice is declared complete

**Phase to address:** M002 Phase 1 (Clinical UI baseline) — validate sandbox networking immediately. All subsequent AI and API integration work depends on this being confirmed working.

---

### Pitfall 3: AI Transcription Output Bypasses the Existing HIPAA Audit Chain

**What goes wrong:**
M001 built a tamper-proof audit log with hash chaining that covers every PHI access via Tauri commands. When the AI pipeline is added, it runs in a Python FastAPI sidecar. The sidecar reads audio, calls Ollama, and returns a generated SOAP note — but none of this flows through the Rust audit layer. The AI inference on patient audio is never logged. The resulting SOAP note is written to the database through the existing `documentation` Tauri commands (which ARE audited), but the AI generation step itself — which processes the most sensitive data (raw voice with PHI) — has zero audit trail.

**Why it happens:**
The sidecar is a separate process with no access to the Rust audit system. Developers correctly wire the final "save note" action through the existing Tauri command, but treat the AI generation step as a pure computation with no PHI implications. In reality, the raw audio and transcription are ePHI that must appear in the audit log.

**How to avoid:**
- Add a Tauri command `log_ai_operation` that the sidecar calls (via its loopback HTTP call back to the Tauri app, or via a dedicated audit endpoint) to record: patient_id, encounter_id, operation_type (transcription | soap_generation | coding_suggestion), model_used, duration_ms, and whether the output was accepted or rejected by the provider.
- Never pass patient audio directly to the sidecar without first logging the intent. The audit entry must precede the AI operation, not follow it.
- Store AI-generated content with provenance metadata: the existing encounter schema must include `source` (human | ai) and `ai_model` fields. This was noted as needed in M001 but the schema must be verified to actually include these columns before M002 AI work begins.
- Define data retention policy for raw audio files: HIPAA requires a minimum retention period for records. Audio files are ePHI. Decide at M002 start: delete after transcription (audit log shows it existed), or retain encrypted. Never leave audio files in unencrypted temp directories.

**Warning signs:**
- Sidecar processes audio without a preceding audit log entry
- AI-generated content and human-entered content are stored in the same fields without provenance tracking
- Temporary audio files are created in `/tmp` rather than the encrypted app data directory
- No audit entries for "ai_transcription_started," "ai_transcription_completed," "soap_generated," "soap_accepted"

**Phase to address:** M002 Phase 2 (AI Pipeline) — define audit schema extensions before any AI code is written. The audit chain cannot be retrofitted cleanly after AI features are in use.

---

### Pitfall 4: Weno Exchange Certification Timeline Not Initiated Early Enough

**What goes wrong:**
The Weno Exchange integration is scoped as a development task within the M002 billing/prescribing phase. Developers write the NCPDP SCRIPT message builder, complete unit tests, and then initiate the Weno certification process — only to discover that: (1) prescriber ID proofing takes 2-4 weeks per provider and blocks all testing, (2) pharmacy registration for test pharmacies must be configured separately and cannot be done retroactively, (3) Weno's always-on sandbox is self-serve but real certification requires Weno staff involvement with scheduling lead times, and (4) pharmacy-side "selective interoperability" means some test pharmacies reject valid ePrescriptions from smaller intermediaries citing "corporate policy." The M002 milestone cannot close because e-prescribing end-to-end is blocked on external parties.

**Why it happens:**
E-prescribing looks like an API integration problem (write code, call API, done). It is actually a certification and enrollment problem that happens to have an API. Weno's self-testing sandbox creates a false sense of progress — the sandbox works, but the sandbox is not the certification.

**How to avoid:**
- Initiate Weno Exchange API access and prescriber enrollment at the START of M002, not when the code is ready. The $300 activation fee and onboarding should happen in week 1.
- EPCS (controlled substances) requires a DEA audit. Use Weno's "DEA Fast Lane Program" or "WENO ONLINE API" (EPCS without your own audit) to avoid the 4-8 week DEA audit process. Decide which path at M002 kickoff — this decision affects the entire architecture.
- Implement all NCPDP SCRIPT message types from the start: NewRx, CancelRx, RxRenewalRequest, RxRenewalResponse, RxChangeRequest, RxChangeResponse. Weno certification requires the complete lifecycle, not just NewRx. Building only NewRx and deferring the rest means the certification fails and the code must be extended under deadline pressure.
- **Critical 2028 deadline:** NCPDP SCRIPT 2017071 (current Weno standard) is retired January 1, 2028. Medicare Part D requires SCRIPT 2023011 by that date. MedArc must plan for SCRIPT 2023 migration before any production e-prescribing goes live — build the message layer to be version-switchable from day one, not version-hardcoded.
- Validate prescriber data EXACTLY matches Weno's records: prescriber name, NPI, DEA number, and practice address must match character-for-character or messages are rejected without explanation.

**Warning signs:**
- Weno Exchange activation not initiated in M002 sprint 1
- EPCS path (own DEA audit vs Weno ONLINE API) not decided before architecture work begins
- NCPDP SCRIPT message builder handles only NewRx
- No test with a real pharmacy (even sandbox) before calling e-prescribing "done"
- SCRIPT version hardcoded rather than configurable

**Phase to address:** M002 Phase 3 (E-prescribing) — but enrollment initiated at M002 kickoff. EPCS decision made before any architecture work.

---

### Pitfall 5: X12 837P Claim Generation Breaks for Non-Standard Claim Scenarios

**What goes wrong:**
The X12 837P claim builder is developed and tested against simple office visit claims (one procedure, one diagnosis, standard payer). When tested against real scenarios, it fails for: claims with modifiers (-25 for same-day E/M + procedure, -59 for distinct procedural services), claims with secondary insurance (loop 2330 for other subscriber), mental health claims requiring special NPI/taxonomy combinations, and Medicare claims requiring the rendering provider loop (2310B) separate from the billing provider (2010BB). The clearinghouse (Office Ally) rejects 30-40% of claims with rejection codes that map to obscure X12 segment requirements that weren't in the happy-path spec.

**Why it happens:**
The X12 837P 5010 implementation guide is 900+ pages. Developers implement the 50-page "core" and assume the rest is edge cases. In reality, modifier rules, CCI edits, and payer-specific loops are daily occurrences for any practice seeing Medicare patients or performing procedures alongside E/M visits. 2025-2026 CMS guidance has added stricter edits on modifier 59/XU/XP/XS/XE for distinct procedural services, and new HCPCS drug wastage modifiers (JW/JZ) that existing X12 libraries may not handle correctly.

**How to avoid:**
- Use an existing X12 library (pyx12 for Python, or a Rust crate) rather than hand-rolling segment generation. The positional format with segment terminators is deceptively complex and error-prone.
- Define the "mandatory test scenario set" before writing a line of claims code: (1) simple office visit, (2) office visit + procedure same day with -25, (3) multiple procedures with -59, (4) secondary insurance, (5) Medicare with separate rendering/billing providers, (6) mental health with behavioral health taxonomy, (7) drug administration with JW/JZ modifiers, (8) telehealth with -95 modifier, (9) claim with prior authorization number (loop 2300 REF*G1), (10) claim denial resubmission. Do not declare billing "done" until all 10 scenarios pass clearinghouse acceptance.
- Implement payer-specific rule validation before submission. Medicare has different requirements than commercial payers. The clearinghouse abstraction layer must accommodate payer-specific pre-submission edits.
- ERA 835 processing must handle adjustment reason codes (CARCs) and remark codes (RARCs) — map all standard codes to human-readable explanations. Do not store raw adjustment codes without interpretation.
- Submit test claims to the Office Ally sandbox (not just synthetic validation) before M002 milestone close. Aim for >95% first-pass acceptance on the 10-scenario test set.

**Warning signs:**
- Claims test suite has fewer than 5 distinct claim scenarios
- X12 generation uses string concatenation rather than a structured library
- No test submissions to a real clearinghouse sandbox
- ERA processing only posts paid claims, not denials and adjustments
- CCI edits not implemented (bundling rules between CPT codes)

**Phase to address:** M002 Phase 4 (Billing) — budget 6-8 weeks minimum including clearinghouse testing. The 10-scenario test set is the acceptance criterion.

---

### Pitfall 6: AI Coding Suggestions Presented as Authoritative, Not Advisory

**What goes wrong:**
The AI coding suggestion feature (LLM entity extraction + FAISS vector search for ICD-10/CPT) surfaces suggested codes in the billing workflow. The UI presents these as "Suggested Codes" with a one-click "Accept All" button. Providers and billers, under time pressure, accept all suggestions without review. Billing staff later discover that accepted codes include: specificity errors (non-specific ICD-10 codes where a more specific code was required), bundling violations (two CPT codes that cannot be billed together per CCI edits), codes that don't match the documented clinical encounter, and in rare cases, fabricated codes (ICD-10 or CPT codes that technically exist but were applied to the wrong body part or service). Claim denial rates increase. In a worst case, a pattern of incorrect coding attracts payer audits.

**Why it happens:**
Even the best LLM coding approach (RAG + FAISS) achieves ~69% exact match on clinical notes (Nature 2025 study). The 31% error rate, spread across hundreds of claims per week, produces multiple incorrect codes daily. The "Accept All" pattern is the failure mode — it turns a suggestion into an automated submission with human liability for an AI error.

**How to avoid:**
- Never implement "Accept All" for AI-suggested billing codes. Each code must be individually confirmed by a billing staff member. The UI should show one code at a time with Accept/Reject/Modify options.
- Display confidence scores alongside every suggestion. FAISS similarity scores and LLM probability estimates must be visible to the reviewer. Any suggestion below 0.80 confidence should display a visual warning.
- Run CCI edit validation on AI-suggested code combinations BEFORE they are presented to the user. Do not show unbundleable code pairs as valid suggestions.
- Validate ICD-10 specificity: if the AI suggests an unspecified code (ending in 9 or generic subcategory) but the clinical note contains laterality, body part, or encounter type information, flag the suggestion for more specific coding.
- Maintain a feedback loop: track which AI suggestions are rejected and why. Use this data to improve FAISS index recall and LLM prompt quality over time.
- Log every accepted AI code suggestion in the audit trail with model version, confidence score, and who approved it. This is essential for CMS audit defense.

**Warning signs:**
- "Accept All" button exists in the billing UI
- Confidence scores are computed but not displayed to users
- CCI edit validation happens after code acceptance, not before suggestion display
- No audit trail distinguishes AI-suggested codes from human-entered codes

**Phase to address:** M002 Phase 4 (Billing) — the advisory-only model must be designed into the UI before the coding feature is built, not added as a constraint afterward.

---

### Pitfall 7: Clinical UI Built Without a Working Offline State Model

**What goes wrong:**
The React clinical UI is built with TanStack Query for server state. Developers configure it to fetch from Tauri commands (which call the local SQLite database). This works perfectly in development. In production, a clinic's Mac unexpectedly restarts mid-shift, Ollama crashes and the sidecar becomes unavailable, or the app is opened on a Mac that hasn't been used in 2 weeks. The UI shows loading spinners indefinitely, shows cached stale data from a previous session without indicating staleness, or shows error states in the middle of a patient encounter. Providers cannot document a visit because the UI is stuck waiting for a state it cannot reach.

**Why it happens:**
TanStack Query is designed for client-server architectures where the "server" is a remote API. When adapted for local Tauri IPC, developers reuse the same patterns (staleTime, refetch intervals, error retry) without accounting for the fact that the "server" is always local and always available — unless it isn't. The boundary conditions (sidecar crash, DB locked by backup, session expired mid-encounter) are not tested.

**How to avoid:**
- Design explicit offline and degraded states for every UI view: (1) Core EMR views (patient chart, encounter, scheduling) must work even when the AI sidecar is down — show "AI unavailable" badge, disable AI buttons, do not block clinical workflow. (2) If a Tauri command fails, show a specific error with recovery action (not a generic spinner). (3) Session timeout during an active encounter must save a draft before showing the lock screen — never lose unsaved clinical work.
- TanStack Query configuration for Tauri IPC should set `staleTime: Infinity` (local data does not become stale like remote data) and `retry: false` (failed local IPC calls should surface immediately, not retry silently).
- Test the following failure scenarios explicitly before any M002 phase closes: (1) Kill the Python sidecar mid-transcription and verify the UI surfaces an error and core functions continue. (2) Lock the session while a SOAP note is being edited and verify the draft is saved. (3) Start a backup while viewing a patient chart and verify the chart remains accessible.
- The "active patient context" (which patient is currently open) must be managed in Zustand global state, not only in URL/route state. If React Router re-renders or the component tree remounts, the patient context must survive.

**Warning signs:**
- TanStack Query `staleTime` is set to default (0) for Tauri IPC calls
- No "AI unavailable" UI state — AI features show error modals that block clinical workflow
- Session timeout during an open encounter does not save a draft
- Active patient context is lost on browser navigation (React Router)

**Phase to address:** M002 Phase 1 (Clinical UI) — offline state model must be defined before any data-fetching component is written.

---

### Pitfall 8: PHI Leaks Into React Component State, URL Parameters, and Browser DevTools

**What goes wrong:**
Developers build the patient chart as a React component that fetches the full patient record and stores it in component state and TanStack Query cache. Patient names appear in browser history via URL parameters (`/patients/12345/chart`). The full FHIR Patient resource (including SSN, DOB, address) is stored in Zustand global state and accessible via React DevTools in development. Console.log statements added during debugging emit patient data. After a developer shares a screen recording to report a bug, they notice patient names are visible in the DevTools panel in the background. The recording was made from a real patient's record used for testing.

**Why it happens:**
Frontend developers treat the local app like a web application where DevTools and memory inspection are safe. In a HIPAA-regulated app, any PHI in browser-accessible memory, URL parameters, or console output is a risk surface. The WKWebView in Tauri is subject to the same security concerns as a browser — any tool that can inspect the WebView (including Safari Web Inspector on connected devices) can read state and console output.

**How to avoid:**
- Never put PHI in URL parameters. Use numeric record IDs in URLs (`/encounters/123`) and fetch the associated patient data via Tauri command. The patient name must not appear in the URL, history, or browser navigation state.
- Disable React DevTools in production builds. Add `process.env.NODE_ENV === 'production' && (window.__REACT_DEVTOOLS_GLOBAL_HOOK__ = { isDisabled: true })` to the app bootstrap, or use the `react-devtools` `enableConsolePausing` config.
- Implement a PHI redaction layer for console output. Any `console.log`, `console.error`, or `console.warn` that includes patient data structures must be wrapped in a logger that strips PHI fields before output in production. In development, show PHI with a `[DEV ONLY - PHI]` prefix to make it visually distinct.
- The TanStack Query cache holds PHI. Implement cache clearing on session lock/logout: `queryClient.clear()` on session lock, not just on logout.
- Zustand stores that hold PHI (active patient, active encounter) must be cleared on session lock and not persisted to localStorage or sessionStorage.
- Safari Web Inspector can connect to WKWebView in developer-mode Tauri builds on any connected Mac. Disable WKWebView remote inspection in production builds via Tauri's `devtools: false` configuration.

**Warning signs:**
- Patient name appears in URL bar
- `console.log(patient)` anywhere in production code
- `persist` middleware on Zustand stores that contain patient data
- TanStack Query cache not cleared on session lock
- No `devtools: false` in production `tauri.conf.json`

**Phase to address:** M002 Phase 1 (Clinical UI) — PHI handling patterns must be established in the first component and enforced as a code review gate throughout.

---

### Pitfall 9: whisper.cpp CoreML Model Not Available in Production App Bundle

**What goes wrong:**
whisper.cpp with CoreML acceleration (WHISPER_COREML=1) is built and tested on the developer's Mac. It works beautifully — 3x real-time transcription speed. The PyInstaller sidecar is built without the CoreML model files (`.mlmodelc` directory) because they are not Python files and PyInstaller's `--collect-data` hook doesn't know to include them. The production app ships without CoreML support and silently falls back to CPU inference — 3-8x slower, potentially too slow for real-time clinical use on 16GB machines running other models.

**Why it happens:**
PyInstaller bundles Python code and data files explicitly listed. The whisper.cpp CoreML models are binary `.mlmodelc` bundles that live outside the Python package structure. They are not automatically discovered by PyInstaller's dependency analysis. The same issue affects FAISS index files and SciSpaCy model data.

**How to avoid:**
- Audit every non-Python binary asset the sidecar requires: CoreML model bundles (`.mlmodelc`), FAISS index files (`.faiss`, `.index`), SciSpaCy/MedSpaCy model directories, and RxNorm SQLite cache (if pre-loaded). Each must be explicitly included in the PyInstaller spec file via `datas` or `binaries`.
- Build the production PyInstaller bundle on a clean macOS environment (not a developer machine with dependencies already installed globally) and run the complete AI pipeline against it. If CoreML falls back to CPU, the performance difference is immediately apparent.
- Add a startup health check in the FastAPI sidecar that verifies: CoreML models are accessible, FAISS indexes are loadable, spaCy models are valid. Return model-specific status in the `/health` endpoint so Tauri can report meaningful diagnostics to the user.
- Test the final sidecar binary size and startup time on the minimum hardware (16GB M1). LLaMA 3.1 8B Q4 + Whisper large-v3-turbo + MedSpaCy models together require 8-12GB of RAM. On a 16GB machine, this leaves 4-8GB for the OS and Tauri app. Validate this before declaring the AI pipeline complete.

**Warning signs:**
- PyInstaller spec file does not explicitly include model files
- Sidecar health check does not report CoreML vs CPU inference mode
- Sidecar tested only by running `python app.py`, never via the bundled binary
- No startup latency measurement for the bundled sidecar (model loading time can be 30-60 seconds on first use)

**Phase to address:** M002 Phase 2 (AI Pipeline) — validate the production bundle before integrating with the React UI. Discovering missing model files after UI integration doubles the debugging time.

---

### Pitfall 10: Existing Rust RBAC Not Extended to New M002 Resources

**What goes wrong:**
M001 built a complete RBAC system in `rbac/roles.rs` with a resource-permission matrix. M002 adds new resources: AI transcription, billing claims, e-prescribing, EPCS (controlled substances). Developers add the new Tauri commands without adding corresponding RBAC resource variants. The new commands call `check_permission()` but against a resource that doesn't exist in the matrix, defaulting to the fallback behavior (which may be `deny_all` or — worse — `allow_all` if the fallback is permissive for unknown resources). EPCS commands accessible to Billing staff who should not prescribe controlled substances is a DEA compliance violation.

**Why it happens:**
The RBAC matrix was built for M001 resources. Adding new commands in M002 requires parallel work in `roles.rs` that is easy to forget when focused on the feature itself. The Rust type system does not enforce that every command has a corresponding RBAC resource — the check is runtime, not compile-time.

**How to avoid:**
- Add a compile-time test that enumerates every Tauri command handler registered in `lib.rs` and verifies it has a corresponding RBAC resource. This is achievable via a proc macro or a test that matches command names against the resource enum.
- For M002, define RBAC resources before implementing commands: `BillingClaim` (Admin, Provider, Billing read/write; others no access), `Prescription` (Provider only for write; Nurse/MA read; others no access), `ControlledSubstancePrescription` (Provider with EPCS credential only; separate from non-controlled), `AiTranscription` (Provider only), `AiCodingSuggestion` (Provider, Billing read).
- EPCS-specific authorization must check not just the role but also the provider's DEA registration status stored in the `users` table. A provider without DEA registration must be blocked from EPCS commands even if their role is Provider.
- Every new M002 Tauri command must have an RBAC test in the same file, following the M001 pattern established in `rbac/roles.rs`.

**Warning signs:**
- New Tauri commands added in M002 without corresponding RBAC resource entries
- `ControlledSubstancePrescription` treated as the same resource as `Prescription`
- No test that every registered Tauri command has a corresponding RBAC check
- DEA registration status checked only in the UI, not in the Rust command handler

**Phase to address:** M002 Phase 3 (E-prescribing) for EPCS specifically; Phase 1 (Clinical UI) for establishing the M002 RBAC extension pattern.

---

## Technical Debt Patterns

| Shortcut | Immediate Benefit | Long-term Cost | When Acceptable |
|----------|-------------------|----------------|-----------------|
| Hand-written TypeScript types for Tauri IPC | Faster initial UI development | Types drift from Rust structs; runtime serialization errors that are hard to debug; every Rust struct change requires manual TS update | Never — use `ts-rs` or `tauri-specta` from day 1 of M002 |
| Single "Accept" button for all AI coding suggestions | Simpler billing UX | CMS/payer audit exposure; incorrect code patterns become habitual; difficult to retroactively identify AI-sourced errors | Never — individual code review is a compliance requirement |
| Testing AI pipeline only in `npm run tauri dev` | No need for a signing certificate during development | CoreML model paths, App Sandbox entitlements, and sidecar notarization issues are invisible until production build | Development only — every sprint must include at least one production build test |
| Hardcoding NCPDP SCRIPT version 2017071 | Simpler initial implementation | Mandatory migration to SCRIPT 2023011 by January 2028; retrofit requires touching every message builder function | Phase 1 of e-prescribing only — abstract version at module boundary before M002 closes |
| Storing raw audio files in the OS temp directory | Simplest implementation | `/tmp` is unencrypted; audio files are ePHI; OS may not delete them promptly; HIPAA breach risk | Never — use the app's encrypted data directory exclusively |
| Using the same AES key for database, backups, and AI model encryption | One key to manage | Key rotation affects all assets simultaneously; compromised key exposes everything; FIPS guidance recommends key separation by purpose | Never for production — separate keys with different Keychain entries per asset type |
| `retry: 3` on TanStack Query for Tauri IPC calls | Looks more robust | Local IPC failures are usually permanent (command not found, session expired) not transient — retries delay error surfacing and create duplicate state mutations | Never for mutation commands (create, update, delete); acceptable for read-only queries with `retry: 1` |

---

## Integration Gotchas

| Integration | Common Mistake | Correct Approach |
|-------------|----------------|------------------|
| Ollama from sandboxed Tauri app | Assuming `127.0.0.1` localhost access works inside App Sandbox | Add `com.apple.security.network.client` entitlement; set `OLLAMA_ORIGINS` to include `tauri://localhost` via LaunchAgent plist; test in a notarized build |
| Weno Exchange NCPDP SCRIPT | Building only NewRx and treating the rest as future work | Implement full lifecycle: NewRx, CancelRx, RxRenewalRequest/Response, RxChangeRequest/Response — Weno certification requires all; budget 3-4 months not 3-4 weeks |
| Office Ally clearinghouse | Validating X12 only with a local parser/validator | Submit real test claims to Office Ally sandbox and measure first-pass acceptance rate; local validation misses payer-specific rules |
| whisper.cpp CoreML in PyInstaller | Assuming Python dependencies are all that PyInstaller bundles | Explicitly include `.mlmodelc` directories in PyInstaller `datas`; verify in production build that CoreML path is found |
| FAISS indexes in PyInstaller sidecar | Loading index from development path | Bundle index files with the sidecar via `datas`; use relative paths from the sidecar executable's location, not absolute paths |
| Tauri sidecar notarization | Using system keychain default for code signing | Explicitly pass `--keychain $HOME/Library/Keychains/login.keychain-db` to avoid "nested code is modified or invalid" notarization failure |
| RxNav-in-a-Box Docker containers | Running all 4 containers (8GB+ RAM) alongside AI models | Use only the RxNorm API container and the drug interaction endpoint; pre-load frequently needed interaction data into a local SQLite cache |
| EPCS two-factor authentication | Implementing TOTP as the only second factor | DEA 21 CFR Part 1311 requires a biometric OR hardware token as one of the two factors — TOTP alone does not satisfy EPCS requirements; Weno's ONLINE API handles this complexity |
| ICD-10 code validation in AI suggestions | Checking only that the code format is valid (3-7 alphanumeric) | Validate against the current fiscal year's ICD-10-CM code table (updated October 1 annually); codes valid in FY2024 may be deleted in FY2025 |
| ERA 835 auto-posting | Posting all payment amounts as final | ERA payments must be matched against the original claim; post to the claim, not to the patient balance, until the claim is fully adjudicated; track CARCs/RARCs for denial workflows |

---

## Performance Traps

| Trap | Symptoms | Prevention | When It Breaks |
|------|----------|------------|----------------|
| Rendering full patient medication/problem/allergy lists without virtualization | Patients with 20+ years of records cause noticeable scroll jank | Use TanStack Table with row virtualization for all clinical list views; paginate encounter history | Patients with 200+ medications or 500+ problems (chronic complex patients) |
| Loading AI models synchronously at sidecar startup | FastAPI sidecar takes 30-60 seconds to start; Tauri app appears hung | Lazy-load models on first use; return HTTP 503 (not 500) while model is loading; show "AI initializing" state in UI | Every cold start, especially on 16GB machines |
| Running Whisper transcription in the same event loop as FastAPI requests | Transcription blocks all other sidecar API calls for 30-60 seconds | Run Whisper in a separate thread or process (asyncio subprocess); never block the FastAPI event loop | Any transcription longer than 10 seconds |
| Re-rendering the full patient chart on every Tauri command response | Chart flickers on any update; providers lose scroll position | Use React.memo on chart sections; update only the section that changed (SOAP note update should not re-render the medication list) | Any clinical UI with 5+ data sections visible simultaneously |
| FAISS similarity search across all 70K+ ICD-10 codes for every coding suggestion | Coding suggestions take 3-5 seconds per code | Pre-filter by specialty/specialty code set before FAISS search; use HNSW index (faster than flat L2 for large indexes); cache top-20 results per encounter type | Any machine; especially noticeable on 16GB machines where RAM is shared with AI models |
| SQLite write lock contention during backup while encounter is being saved | "database is locked" errors during backup if a backup starts mid-encounter | Run backups only when no active write transactions are open; use SQLite WAL mode (already configured in M001) to allow concurrent reads during backup | Any backup initiated while a provider is actively saving a SOAP note |

---

## Security Mistakes

| Mistake | Risk | Prevention |
|---------|------|------------|
| EPCS prescriptions signed with TOTP alone | DEA 21 CFR Part 1311 violation — EPCS requires biometric OR hardware token as one factor; TOTP is not compliant as the sole second factor | Implement Touch ID (biometric) as the EPCS second factor; TOTP satisfies the knowledge factor but Touch ID must be the possession/biometric factor |
| AI-generated prescriptions auto-populated into e-prescribing fields | Patient safety and legal liability — AI hallucinated medication names or dosages go directly to pharmacy | AI output is NEVER auto-populated into prescription fields; all prescription fields must be manually entered through the structured e-prescribing UI |
| Ollama accessible on non-loopback interfaces | PHI from clinical prompts accessible to any process or device on the network | Verify Ollama binds only to `127.0.0.1`; the App Sandbox provides additional isolation but do not rely on it as the only control |
| FastAPI sidecar session token not validated | Any local process can call the AI API and extract PHI from responses | Generate a per-session random token at sidecar launch (in Rust), pass it to the sidecar via environment variable, and validate it on every request |
| Weno API credentials stored in app configuration file | E-prescribing credentials in plaintext in `~/Library/Application Support/medarc/` | Store Weno API key and organization credentials in macOS Keychain alongside the database key; never in configuration files or environment variables |
| Clearinghouse credentials (Office Ally SFTP) stored in settings database | PHI exposure through credential theft | Store all external service credentials in Keychain; credentials must not be queryable via any Tauri command (not even to display the last 4 characters) |
| X12 837P claims containing full patient SSN in NM1 subscriber segment | HIPAA minimum necessary — SSN is rarely required; when required it must be transmitted over TLS 1.3 | Use the insurance subscriber ID (not SSN) in NM1; the Office Ally clearinghouse connection must use TLS 1.3 and be validated in production |

---

## UX Pitfalls

| Pitfall | User Impact | Better Approach |
|---------|-------------|-----------------|
| SOAP note sections all visible simultaneously on load | Cognitive overload; providers don't know where to start; critical findings buried | Use accordion sections with Subjective auto-expanded on load; auto-advance to Objective after Subjective is saved; keep Assessment and Plan collapsed until Objective is complete |
| Voice recording button requires clicking to start and clicking to stop | During physical exam, provider cannot use hands; interrupts clinical flow | Implement keyboard shortcut (Space to start/stop) and auto-pause on 5 seconds of silence; do not require mouse interaction for core recording flow |
| Drug interaction alerts shown as modal dialogs | Modal blocks all workflow; providers habitually click dismiss; alert fatigue within 2 weeks | Critical interactions (contraindicated): modal with override reason required. Serious: inline warning banner with "review before prescribing" indicator. Moderate/minor: small badge on the medication field, visible but non-blocking |
| Billing code search showing all 70K+ ICD-10 codes unfiltered | Provider search returns too many results; searching "diabetes" returns 400+ codes with no obvious priority | Pre-filter by specialty and encounter type; surface codes used in the last 10 encounters by that provider at the top; show clinical description, not just code number |
| E-prescribing workflow requiring 8+ clicks per prescription | Provider spends 3-5 minutes per prescription; reverts to fax | Map the prescription workflow to: (1) patient med list → (2) new prescription → (3) drug search with RxNorm autocomplete → (4) sig builder → (5) pharmacy selector (last-used default) → (6) interaction check → (7) sign; maximum 6 screens |
| AI transcription showing "generating..." without partial results | Provider doesn't know if the system is working; may speak again, creating duplicate audio | Stream partial Whisper output to the UI as segments complete; show real-time transcription text while SOAP generation is in progress |

---

## "Looks Done But Isn't" Checklist

- [ ] **Tauri IPC Type Safety:** TypeScript types match Rust structs — verify by calling every command from TS and asserting on field names, not just that the call succeeded
- [ ] **App Sandbox Network Access:** `entitlements.plist` includes `com.apple.security.network.client` — verify in a notarized production build, not just in `tauri dev`
- [ ] **Ollama CORS for Tauri:** `OLLAMA_ORIGINS` includes `tauri://localhost` — verify via LaunchAgent plist, not `.zshrc` (GUI apps don't inherit shell env vars)
- [ ] **EPCS Two-Factor:** Touch ID biometric is implemented as one of the two EPCS factors — TOTP alone does not satisfy DEA 21 CFR Part 1311
- [ ] **AI Audit Trail:** Every AI operation (transcription, SOAP generation, coding suggestion) has a corresponding entry in the HIPAA audit log — verify with a filter on `action LIKE 'ai_%'`
- [ ] **X12 837P Modifier Handling:** Claims with modifier -25 (same-day E/M + procedure) generate correct loop structure — test by submitting to Office Ally sandbox
- [ ] **NCPDP SCRIPT Completeness:** All six message types implemented (NewRx, CancelRx, RxRenewalRequest, RxRenewalResponse, RxChangeRequest, RxChangeResponse) — verify Weno sandbox shows all six as accepted
- [ ] **CoreML Models in Production Bundle:** PyInstaller sidecar includes `.mlmodelc` files — verify by running the bundled binary on a clean Mac and checking the `/health` endpoint's `inference_mode` field
- [ ] **PHI Not in URLs:** Navigate to a patient chart and check the URL bar — patient name must not appear; record ID is acceptable
- [ ] **TanStack Query Cache Cleared on Lock:** Lock the session and inspect DevTools cache — patient data must not be readable from the cache after lock
- [ ] **RBAC Extended for New Resources:** Every M002 Tauri command has a corresponding RBAC resource entry — verify with the `cargo test` RBAC coverage test
- [ ] **Weno Credentials in Keychain:** Weno API key is in macOS Keychain — verify that `SELECT * FROM app_settings WHERE key LIKE '%weno%'` returns no credentials
- [ ] **Backup Still Works with New Schema:** After M002 adds new tables, `create_backup` and `restore_backup` still function — run a full backup/restore cycle after every migration

---

## Recovery Strategies

| Pitfall | Recovery Cost | Recovery Steps |
|---------|---------------|----------------|
| TypeScript types diverged from Rust structs across 20+ components | HIGH | Introduce `ts-rs` or `tauri-specta`; generate canonical types; create a TypeScript-side compatibility shim; audit every IPC call against generated types; 2-4 weeks |
| App Sandbox blocking discovered after AI features built | MEDIUM | Add `com.apple.security.network.client` to entitlements; rebuild and re-notarize; 1-2 days if the entitlement is the only issue; longer if hardened runtime + sidecar signing also needs fixing |
| Weno certification failed after code complete | HIGH | Implement missing message types; fix prescriber data mismatches; re-certify (Weno staff scheduling); 4-8 weeks per certification attempt |
| X12 claims rejected at 30%+ rate | MEDIUM | Analyze rejection codes by category; implement targeted fixes (modifier rules, secondary insurance loops, etc.); resubmit corrected claims within timely filing window; 3-6 weeks to stabilize |
| AI billing codes accepted without review discovered in audit | HIGH | Issue voluntary disclosure to payer; correct and resubmit affected claims; disable "Accept All" immediately; implement individual review workflow; potential overpayment recoupment; 4-8 weeks |
| PHI discovered in URL history or console logs | MEDIUM | Patch and release immediately; assess whether PHI was accessible to unauthorized individuals; document for HIPAA breach assessment; 1-2 weeks |
| CoreML models missing from production sidecar | MEDIUM | Update PyInstaller spec; rebuild sidecar; re-notarize; deploy update via tauri-plugin-updater; 3-5 days but requires a full release cycle |
| RBAC gap on new M002 resources discovered | HIGH for EPCS | For EPCS: treat as security incident; audit who accessed what; patch and release same day; for non-critical resources: patch in next sprint |

---

## Pitfall-to-Phase Mapping

| Pitfall | Prevention Phase | Verification |
|---------|------------------|--------------|
| React UI type mismatch with Rust IPC | M002 Phase 1 (Clinical UI) | `ts-rs` or `tauri-specta` generates TS bindings; no hand-written IPC types in codebase |
| App Sandbox blocking Ollama/sidecar | M002 Phase 1 (Clinical UI) | Notarized test build passes health check against sidecar and Ollama before any AI code written |
| AI transcription bypassing audit chain | M002 Phase 2 (AI Pipeline) | `SELECT * FROM audit_logs WHERE action LIKE 'ai_%'` shows entries for every transcription and SOAP generation |
| Weno certification timeline not started | M002 kickoff (before any e-prescribing code) | Weno account activated and prescriber enrolled within week 1; EPCS path decided within week 2 |
| X12 837P non-standard claim failures | M002 Phase 4 (Billing) | 10-scenario test set achieves >95% first-pass acceptance on Office Ally sandbox |
| AI coding suggestions presented as authoritative | M002 Phase 4 (Billing) | No "Accept All" button exists in the billing UI; each code individually reviewed |
| Clinical UI missing offline/degraded state model | M002 Phase 1 (Clinical UI) | Kill sidecar during an open encounter and verify clinical documentation continues without errors |
| PHI in React state and URLs | M002 Phase 1 (Clinical UI) | Automated test scans URLs during navigation and asserts no PHI in URL bar; cache inspection after session lock shows empty patient data |
| whisper.cpp CoreML models missing from bundle | M002 Phase 2 (AI Pipeline) | Sidecar `/health` endpoint reports `inference_mode: "coreml"` in production build on clean Mac |
| RBAC not extended for new M002 resources | M002 Phase 1–4 (each phase) | RBAC coverage test in CI; EPCS commands fail when called with Billing role token |

---

## Sources

- HIPAA Security Rule 45 CFR 164.312 and 45 CFR 164.308 — regulatory (HIGH confidence)
- DEA 21 CFR Part 1311 — EPCS requirements (HIGH confidence)
- Weno Exchange API documentation (March 2026): [wenoexchange.com/api-learn-more](https://wenoexchange.com/api-learn-more/) (HIGH confidence)
- NCPDP SCRIPT 2023 deadline: CMS Federal Register June 2024, [federalregister.gov](https://www.federalregister.gov/documents/2024/06/17/2024-12842/medicare-program-medicare-prescription-drug-benefit-program-health-information-technology-standards) — January 2028 deadline confirmed (HIGH confidence)
- Drummond Group: "Preparing for the New Script Standard" (MEDIUM confidence — article content not fully accessible)
- Tauri GitHub issue #13878: App Sandbox blocks all outgoing HTTP in production builds (HIGH confidence — confirmed issue)
- Tauri GitHub issue #11992: Sidecar notarization "nested code is modified or invalid" with fix via `--keychain` flag (HIGH confidence — confirmed issue with documented workaround)
- Tauri Ollama CORS issue #10507: Windows Tauri origin not in default OLLAMA_ORIGINS (HIGH confidence — macOS `tauri://` support confirmed added)
- NEJM AI: "Large Language Models Are Poor Medical Coders" — GPT-4 ICD-10-CM exact match 33.9% (HIGH confidence)
- Nature npj Health Systems (2025): Fine-tuned LLM ICD-10 coding achieving 69.20% exact match on clinical notes (MEDIUM confidence)
- CMS clarification on HCPCS drug wastage modifiers JW/JZ — mid-2025 (MEDIUM confidence)
- PyInstaller hardened runtime crash issue #4629 — CoreML model bundling (MEDIUM confidence — issue is from 2020 but pattern persists)
- M001 S09-SUMMARY.md: AES-256-GCM backup implementation, App Sandbox entitlements status, RBAC extension patterns — direct codebase evidence (HIGH confidence)

---
*Pitfalls research for: M002 — Clinical UI, AI Voice Pipeline, Billing, and E-prescribing on existing Tauri 2 EMR*
*Researched: 2026-03-11*

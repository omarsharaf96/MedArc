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

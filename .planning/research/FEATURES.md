# Feature Research

**Domain:** Solo-practice desktop EMR — M002 clinical UI, AI transcription, billing, e-prescribing
**Researched:** 2026-03-11
**Confidence:** MEDIUM-HIGH
**Scope:** M002 adds clinical workflow UI on top of M001's complete Rust backend. M001 already built: auth, RBAC, audit logging, patient CRUD, scheduling, clinical docs, labs, documents. This file covers what M002 must deliver and how each feature category should work for a solo physician.

---

## Feature Landscape by Category

### Category 1: Patient Chart View UI Shell

The clinical UI shell is the container that makes the M001 backend usable. Every other M002 feature lives inside it. Without it, the backend is invisible.

#### Table Stakes

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| Patient banner / header bar | Every EMR pins patient identity at top of chart to prevent "wrong-patient" errors — a documented patient safety event category | LOW | Name, DOB, age, MRN, primary provider, active allergies badge (count + severity). Never scrolls away. |
| Tab navigation (Summary, Encounters, Meds, Problems, Allergies, Labs, Documents) | Physicians trained on Epic/DrChrono expect tab-based chart organization; context-switching must be instant | MEDIUM | Tabs render content within the same patient context. Active tab highlighted. Unsaved changes warn before tab switch. |
| Patient facesheet / summary view | The at-a-glance patient overview that opens when chart is accessed; physicians see this 30-50x per day | MEDIUM | Active problems, current medications, allergies, last vitals, upcoming appointments, recent encounters. All from M001 backend data. |
| Encounter list with status | Providers need to see all encounters (open, signed, co-sign pending) for a patient and open prior notes | LOW | Date, encounter type, provider, status (draft, pending co-sign, signed). Click to open note. |
| Keyboard navigation and shortcuts | Power users (physicians) document faster with keyboard; clicking through tabs is too slow for clinical pace | LOW | Ctrl/Cmd+[number] for tabs, Escape to dismiss modals, Enter to confirm. Standard desktop app behavior. |
| "Back to patient list" breadcrumb | Users drill into a chart from a search; must be able to return without losing context | LOW | Breadcrumb trail: Patient List > [Patient Name] > [Section]. |

#### Differentiators

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| Collapsible sidebar with patient mini-summary | Keeps critical patient data visible while working in any section; reduces cognitive load, eliminates need to switch tabs to check allergies while writing a note | MEDIUM | Sticky panel: allergies, active meds, active problems. Collapsible to maximize note entry space. OpenEMR's "Dashboard" pattern but inline. |
| Activity feed / timeline view | Solo physician sees the complete patient story (encounters, labs, prescriptions, documents) in chronological order — a mental model that matches how physicians think | MEDIUM | Reverse-chronological timeline across all event types. Filterable by type. Requires joining across M001 backend tables. |
| "Quick access" pinned sections | Physician pins their most-used chart sections (e.g., vitals flowsheet, medication list) to top of facesheet | LOW | Draggable card layout on facesheet. Preference stored per-provider. |

#### Anti-Features

| Feature | Why Requested | Why Problematic | Alternative |
|---------|---------------|-----------------|-------------|
| Infinite scroll within chart sections | Seems modern and fluid | Clinical data has natural section boundaries (encounters, labs, meds); infinite scroll destroys visual landmarks and makes it impossible to find the "current encounter" vs prior ones | Tab + paginated list with clear date headers |
| Dashboard-first navigation (chart as drill-down from dashboard) | Analytics dashboards are fashionable | Solo physician workflow is patient-centric, not population-centric. Opening a dashboard first adds a click before every chart access. Desktop EMR should open to schedule or patient search, then directly into chart. | Schedule view or patient search as home; chart opens directly |
| Floating/draggable panels | Seems productive and multi-monitor friendly | Draggable panels create state management complexity, lose position between sessions, and break keyboard navigation. Not worth the complexity for solo practice. | Collapsible fixed panels with persistent state |

---

### Category 2: SOAP Note Entry

The SOAP note editor is the single highest-frequency feature in the EMR. A solo physician documents 15-25 encounters per day. If note entry is slow, the product fails.

#### Table Stakes

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| Structured SOAP sections (S/O/A/P as distinct fields) | SOAP is the universal clinical documentation standard; every EMR structures notes this way. Free-text blob is not acceptable for billing or medicolegal use. | HIGH | Subjective: CC + HPI + review of symptoms. Objective: vitals + ROS + PE + labs. Assessment: diagnosis list with ICD-10. Plan: orders, prescriptions, follow-up, instructions. |
| Auto-populated patient context | Note editor must pre-populate patient demographics, prior diagnoses, current meds, allergies into the note context — typing this from scratch each time is why physicians hate EMRs | MEDIUM | Pull from M001 patient/clinical commands. Name, DOB, provider, date auto-fill. Problem list pre-populates Assessment section. |
| Draft auto-save (local) | Physicians are interrupted constantly; losing a partially written note is a critical failure | MEDIUM | Auto-save to SQLCipher every 30 seconds. Unsaved draft indicator. Restore from draft on re-open. |
| Note status workflow (draft → pending co-sign → signed) | Medicolegal requirement; NP/PA notes require physician co-signature; signed notes must be locked | LOW | Status machine with role-gated transitions. Provider signs their own notes; supervisor co-signs mid-level notes. Signed notes append-only (addendum pattern). |
| Encounter templates (pre-built by specialty) | Physicians configure their preferred note structure once; every specialty has a different standard format | HIGH | 10-15 templates minimum: Annual Physical, Sick Visit, Follow-up, New Patient, Cardiology, Pediatric WCC, OB/GYN, Psychiatry, Orthopedic Eval, Dermatology. Loaded from template library, customizable per encounter. |
| Chief complaint (CC) and HPI structured fields | The opening of every SOAP note; physicians expect a consistent location for CC and narrative HPI | LOW | CC: short text (1-2 lines). HPI: free text with prompts (OLDCARTS: Onset, Location, Duration, Characteristics, Aggravating, Relieving, Treatment, Severity). |
| ICD-10 diagnosis linking in Assessment section | Cannot bill without ICD-10 codes; Assessment section must link coded diagnoses to the encounter | MEDIUM | Type-ahead search against ICD-10 CM database. Multiple diagnoses per encounter. Primary diagnosis flagged. Links to M001 problem list (adds to problem list or selects existing). |
| Plan section structured sub-fields | The Plan section drives all downstream actions; must be structured to capture orders, prescriptions, referrals, follow-up | MEDIUM | Sub-fields: Medications/Rx (links to Rx module), Orders (labs, imaging), Referrals (free text for Phase 1), Follow-up (date/timeframe), Patient instructions (free text). |
| Print / PDF export of note | Practices need paper records for referrals, patient copies, faxed records | LOW | Clean print layout suppressing UI chrome. PDF export of signed note. M001 document storage to save PDF with patient record. |

#### Differentiators

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| Encounter-level allergy and drug alert sidebar | During note writing, physician sees current allergy list and receives passive alerts if Plan section Rx conflicts — before signing, not after | MEDIUM | Read from M001 AllergyIntolerance and MedicationRequest data. Passive (info bar) for moderate interactions; active modal for severe. Avoids alert fatigue by only triggering on clinical relevance. |
| Previous encounter quick-reference panel | Solo physician often needs to see what was done at the last visit while writing the current note — without opening a second window | LOW | Collapsible right-rail showing last encounter's Assessment + Plan. Single API call to M001 clinical commands. |
| Free-text to structured data extraction hints | As physician types in HPI, system suggests matching ICD-10 codes in the Assessment field — reducing the mental context switch of "now I have to go code this" | HIGH | Requires local NLP (MedSpaCy or keyword heuristics for Phase 1). LLM-powered in Phase 2/3. Show as non-blocking suggestions, never auto-apply. |

#### Anti-Features

| Feature | Why Requested | Why Problematic | Alternative |
|---------|---------------|-----------------|-------------|
| Rich text formatting (bold, tables, bullets) in note body | Seems to improve readability | Clinical notes with heavy formatting look polished but are harder to parse by downstream systems, break FHIR data extraction, and increase file size. Excessive formatting also encourages padding ("note bloat"). | Limited formatting: bold for emphasis, bulleted lists for plan items. No custom fonts, colors, or tables in SOAP body. |
| Auto-sign on save | Saves a click | Signed notes have medicolegal finality. Physicians must consciously attest that a note is complete. Auto-signing on save would cause accidental finalization of draft notes. | Explicit "Sign Note" button that is visually distinct and requires confirmation. Draft state is the default. |
| Copy-forward entire prior note | Seems to save time | "Note cloning" is one of the top cited causes of medical record errors and HIPAA audit findings. Copying a prior note without updating it creates false attestation. | Allow copy-forward of specific sections (problem list, medications) not the entire note. Flag copied sections visually. Show warning on first use. |
| Mandatory structured fields for all note elements | Seems to improve data quality | Forcing every field to be structured increases documentation time by 40-60% (a primary driver of physician burnout). The goal is to capture key structured data while allowing free text for narrative. | Structured for: diagnoses (ICD-10), vitals, medications (RxNorm). Free text for: HPI, PE narrative, assessment narrative, plan details. |

---

### Category 3: Vitals, Review of Systems, Physical Exam Forms

These three data types — vitals, ROS, and PE — are the Objective section of every SOAP note. They have distinct UI patterns and are used by different staff (nurse records vitals; physician completes ROS/PE).

#### Table Stakes (Vitals)

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| Vitals entry form (BP, HR, RR, Temp, SpO2, Weight, Height, BMI) | Nurses record vitals at every visit; this is a non-negotiable core workflow | LOW | BP: systolic/diastolic with arm/position. Temp: value + route (oral/rectal/axillary/tympanic). SpO2: % + on-room-air flag. Weight: lbs or kg. BMI: auto-calculated from weight + height. Pain scale: 0-10. |
| BMI auto-calculation | Nurses expect this; manual calculation is error-prone | LOW | BMI = weight(kg) / height(m)^2. Show calculated value immediately on entry, highlighted if outside normal range. |
| Vitals flowsheet (trend view over time) | Physicians need to see BP trends, weight changes, SpO2 trends — not just the current value | MEDIUM | Date-ordered table of all vitals entries for the patient. Line graph for numeric values (BP, HR, Weight). Highlight abnormal values (BP >140/90, SpO2 <94%, etc.). |
| Abnormal value flagging | Out-of-range vitals must be flagged immediately; patient safety issue | LOW | Color-coded: green (normal), yellow (borderline), red (abnormal). Reference ranges are age/sex adjusted. |
| Pediatric growth charts | Pediatricians cannot use an EMR without growth charts; it is a core clinical tool for all well-child visits | MEDIUM | WHO/CDC growth charts for weight-for-age, height-for-age, BMI-for-age, head circumference. Plot current and historical vitals automatically. Percentile calculation. Birth to 20 years. |

#### Table Stakes (Review of Systems)

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| 14-system ROS form | The standard ROS covers 14 organ systems; E/M coding levels depend on number of systems reviewed | MEDIUM | Systems: Constitutional, Eyes, ENT, CV, Respiratory, GI, GU, MSK, Skin, Neuro, Psychiatric, Endocrine, Hematologic, Immunologic. Each system: positive / negative / not reviewed toggle. |
| ROS free-text per system | Positive findings need elaboration; "positive for chest pain" requires specifics | LOW | Optional free-text note per system, shown only when system is marked positive. |
| ROS carry-forward (from intake) | Nurses often complete ROS during rooming; physician should be able to review and attest, not re-enter | LOW | Mark entire ROS as "reviewed and attested by [provider]" with timestamp. Individual modifications allowed. |
| E/M coding level indicator | Physicians need to know if their documentation supports the billing level they intend to code | MEDIUM | Count of ROS systems reviewed, PE systems examined, diagnoses, data complexity. Show E/M level calculation (99202-99215 range) in real time as documentation is completed. This is a significant workflow value-add that directly impacts revenue. |

#### Table Stakes (Physical Exam)

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| System-based PE templates | Physicians document PE by organ system (HEENT, Cardiovascular, Pulmonary, Abdomen, MSK, Neuro, Skin, etc.); each system has standard normal findings | MEDIUM | Toggle-based: Normal / Abnormal / Not Examined per system. Abnormal triggers free-text field. Pre-populated normal text for one-click documentation of unremarkable exam. |
| "All systems normal" macro | The majority of follow-up visits have normal exams; single-click documentation is expected | LOW | One-click sets all examined systems to "within normal limits." Physician then modifies only abnormal findings. |
| Specialty-specific PE templates | A cardiologist's PE focuses on cardiac/pulmonary; a dermatologist's focuses on skin/lesion description; templates must match specialty | HIGH | Load template based on provider specialty or note template type. 10-15 templates covering general, cardiology, pediatrics, OB/GYN, psychiatry, orthopedics, dermatology, ophthalmology. |

#### Differentiators

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| E/M level real-time calculator | Shows the achievable billing level (99202-99215) based on documented ROS, PE, diagnoses, and medical decision complexity — catches under-documentation before the note is signed | MEDIUM | Rule-based calculation per CMS 2021 E/M guidelines. Display as "supports Level 4 (99214)" with indication of what is missing for Level 5. Solo physician without a billing team especially benefits from this. |
| Vitals trend mini-sparklines on facesheet | Shows directional trend (BP trending up over 3 visits) at a glance without opening the flowsheet | LOW | SVG sparklines (last 5 data points) for BP, weight, A1c. Click to expand to full flowsheet. React-based, rendered from M001 clinical data. |

#### Anti-Features

| Feature | Why Requested | Why Problematic | Alternative |
|---------|---------------|-----------------|-------------|
| Fully customizable ROS systems (per-provider system ordering) | "I always do it my way" | Custom ordering breaks the E/M coding calculator (which counts standard systems), makes data extraction for reporting unreliable, and creates support complexity. | Fixed 14-system ROS order per clinical standard. Providers can mark only relevant systems; unused systems show as "not reviewed." |
| Wearable device auto-import (Apple Watch, Fitbit) | Patients wear these; seems convenient | Consumer wearable data is unvalidated for clinical use, requires user consent management, HIPAA business associate agreements with Apple/Fitbit, and creates clinician liability questions about ignoring imported values. | Manual vitals entry by clinical staff. Phase 3+ for validated medical device integration (FDA Class II, Bluetooth Medical Profile). |

---

### Category 4: AI-Assisted SOAP Generation (Whisper + Ollama Pipeline)

This is MedArc's primary differentiator. The pipeline is: audio capture → whisper.cpp transcription → NLP structuring → LLaMA 3.1 8B SOAP generation → physician review and edit → sign.

#### Table Stakes (for AI pipeline to be usable)

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| Audio recording with visual feedback | Physician must know the system is listening; visual indicator (waveform or pulsing dot) during capture is a patient safety requirement — prevents unintended recording | LOW | macOS AVFoundation for mic capture. Waveform visualization via Web Audio API. Start/Stop/Pause controls. Elapsed time counter. |
| Transcription display (editable text) | Physician must see what was transcribed before it is processed into a note — catching Whisper errors (1% hallucination rate) is the human-in-the-loop checkpoint | LOW | Show verbatim transcript in scrollable panel. Physician can edit transcript before proceeding. Clear visual distinction from generated SOAP. |
| Generated SOAP preview with diff from template | Show the AI-generated SOAP alongside what was in the template; physician reviews changes, accepts or rejects each section | MEDIUM | Section-by-section review: "AI generated this for Assessment: [text]." Accept / Modify / Discard per section. Never auto-apply to note without explicit physician acceptance. |
| "Edit before accepting" interaction model | Physician must review, not rubber-stamp | LOW | Generated text loads into note fields as editable. No "auto-sign" path. Must explicitly review each section before signing. |
| Processing status and failure handling | Transcription and LLM inference can take 30-120 seconds; physician must see progress and be able to abort | LOW | Progress steps: "Transcribing... Analyzing... Generating..." with elapsed time. Cancel button active throughout. Graceful failure with raw transcript preserved. |
| Model availability check (Ollama health check) | If Ollama is not running or the model is not pulled, the feature must fail gracefully with a clear explanation | LOW | Check Ollama HTTP endpoint (localhost:11434) on app launch. If unavailable, AI features show "AI model not available — manual entry mode." Instructions to start Ollama. |

#### Differentiators

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| Ambient (continuous) capture mode vs manual record | True ambient capture captures the entire encounter from start to finish without physician managing recording; reduces workflow interruption from "remember to press record" | HIGH | Continuous audio buffer, patient consent prompt at session start, auto-segmentation by silence detection. Significantly more complex than push-to-record. Consider push-to-record for Phase 1 AI MVP, ambient for Phase 2. |
| Patient context injection into LLM prompt | Feeding the LLM the patient's active problems, medications, and allergies before generating the SOAP produces dramatically better output than unconditioned generation | MEDIUM | System prompt includes: patient age/sex, active diagnoses, current medications, known allergies, chief complaint. Structured JSON context from M001 backend. Improves specificity and clinical relevance of generated note. |
| Multi-language transcription support | Practices with Spanish-speaking patients can document encounters in Spanish; Whisper supports 99 languages | LOW | Whisper auto-detects language. Generated note still in English (LLaMA prompt in English). Transcript shown in source language. |
| Confidence scoring / uncertainty flagging | Whisper-generated text with low confidence scores (unusual drug names, rare diagnoses, proper nouns) should be flagged for physician attention | MEDIUM | Whisper word-level confidence scores available via whisper.cpp C API. Highlight low-confidence tokens in transcript view. Critical for medications (dosing errors risk). |

#### Anti-Features

| Feature | Why Requested | Why Problematic | Alternative |
|---------|---------------|-----------------|-------------|
| Auto-submit generated note without physician review | "It saves another click" | The generated note is an AI output. Signing without review is the physician attesting that they personally documented these findings. This creates malpractice liability and violates documentation integrity requirements. Clinical AI scribes that disable review are being actively scrutinized by CMS and malpractice insurers. | Mandatory review UI before "Sign Note" is enabled. Consider a checklist (key diagnoses confirmed, medications verified) before sign button is active. |
| Real-time streaming note generation during encounter | Impressive demo feature | Streaming generation during the live encounter creates distraction (physician managing the note while talking to patient), premature note-writing (before history is complete), and requires constant rewriting as conversation evolves. | Post-encounter generation: record/transcribe during encounter, generate after patient leaves, physician reviews before next patient. 2-3 minute review window is acceptable. |
| Storing raw audio recordings long-term | Patients may want the recording as documentation | Audio recordings of medical encounters are highly sensitive PHI, are much larger than notes (50-100 MB per encounter vs 5-10 KB), create additional storage and retention obligations, and introduce consent complexity (two-party consent in some states). | Process audio → transcript → delete audio within session. Store only the transcript (PHI but manageable). Explicit consent for any audio retention. |

---

### Category 5: Billing — CPT/ICD-10 Coding, AI Suggestions, X12 837P Claims

Medical billing is how the practice gets paid. For a solo physician without a billing team, clarity and accuracy in the billing UI directly impacts revenue. Billing errors cause claim denials (industry average 8-12% denial rate); AI coding can reduce this to below 3%.

#### Table Stakes

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| Encounter-level fee sheet | After a note is signed, coder (or physician) selects CPT procedure codes and ICD-10 diagnosis codes for that encounter on a structured fee sheet | MEDIUM | CPT code search (by code or description). ICD-10 search (by code or description). Multiple CPT codes per encounter (E/M + procedures). Primary and secondary ICD-10 diagnoses. Modifiers (25, 59, -57, etc.). Units and charges. |
| Fee schedule / charge master | Practice configures their standard charges per CPT code; fee sheet auto-populates charge based on CPT selected | LOW | Per-payer fee schedules (self-pay, Medicare, commercial). Allowed amounts per payer. Physician can override charge on encounter. |
| Superbill generation | The internal document summarizing all charges for an encounter; the source of truth before claim generation | LOW | Print/PDF of encounter charges: patient demographics, date, provider, diagnoses, procedures, charges. |
| X12 837P claim generation | The electronic claim format required by all insurers; paper claims (CMS-1500) are nearly dead for commercial payers | HIGH | ANSI X12 5010A1 format. Loops and segments per CMS implementation guide. Required fields: provider NPI, patient demographics, dates of service, place of service code, diagnosis codes (up to 12), procedure codes with modifiers and charges. Clearinghouse transmission via SFTP or API (Office Ally for Phase 1). |
| Claim scrubbing / pre-submission validation | Claims submitted with errors are rejected, creating cash flow gaps; pre-validation catches common errors before transmission | MEDIUM | Check: NPI present and valid format, required fields populated, diagnosis-to-procedure linkage (diagnosis pointer), modifier compatibility, date of service within payer timely filing limits. Show error list with specific correction guidance. |
| Claim status tracking | After submission, practice must track which claims were accepted, rejected, or pending by payer | MEDIUM | Status: Submitted / Accepted / Rejected / Paid / Denied / Appealed. Payer-returned rejection codes with plain-English explanation. Aging: 0-30, 31-60, 61-90, 91-120, 120+ days. |
| ERA/835 payment posting | Insurers return Electronic Remittance Advice files that contain payment amounts, adjustments, and denial codes; these must be posted to outstanding claims | HIGH | Parse X12 835 EDI file. Auto-match payments to claims by claim number / date / patient. Post paid amount, contractual adjustment, patient responsibility. Flag denials with reason codes (CO-45, PR-1, etc.). |
| Patient balance tracking | High-deductible plans have shifted significant cost to patients; practice must track patient-owed balances | MEDIUM | Patient ledger showing charges, insurance payments, adjustments, patient payments, balance. Statement generation for patient billing. |

#### Differentiators

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| AI CPT/ICD-10 code suggestions from note content | Solo physician without a billing team manually codes every encounter; AI suggestions reduce coding time from 5-7 min to under 1 min per encounter, and reduce denial rate | HIGH | NLP entity extraction from signed SOAP note (diagnoses, procedures, symptoms). FAISS vector search against CPT/ICD-10 codebook. Show top 3-5 suggestions with confidence. Physician selects; AI never auto-selects. Critical constraint: GPT-4 alone achieves only 33.9% exact match on ICD-10 — pure LLM without vector search is insufficient. |
| E/M level code suggestion | The most common CPT codes (99202-99215) are determined by documentation complexity; AI reading the note can calculate the appropriate E/M level and pre-populate it | MEDIUM | Rule-based E/M calculator per 2021 CMS guidelines (MDM or time-based). Cross-validate with note content. Show as suggestion with explanation ("Level 4 supported by 2 chronic problems with exacerbation and prescription drug management"). |
| Denial pattern detection | When the same claim type is denied repeatedly, the system should alert: "Payer X consistently denies modifier 25 on same-day E/M + procedure — add documentation" | MEDIUM | Aggregate denial reasons by payer + CPT combination. Surface pattern after 3+ denials. Actionable guidance per denial reason code. Solo physician has no billing team to spot these patterns manually. |

#### Anti-Features

| Feature | Why Requested | Why Problematic | Alternative |
|---------|---------------|-----------------|-------------|
| Automated claim submission without human review | Faster cash flow | AI coding achieves 33.9% exact match for ICD-10 without vector search; even with vector search, complex encounters require clinical judgment. Auto-submitting incorrect codes creates denial cascades, recoupment demands, and potential fraud exposure. | One-click submit after physician/biller review. Show full claim preview before transmission. AI suggestions are read-only recommendations. |
| In-house clearinghouse (direct payer EDI connections) | Eliminate clearinghouse fees | Direct payer EDI connections require individual enrollment with every payer (100+), separate connection maintenance, payer-specific companion guide compliance, and 835 reconciliation per payer. Clearinghouse abstracts all of this. Office Ally is free for solo practices. | Use Office Ally (free) or Availity (low-cost) clearinghouse. Abstract clearinghouse behind a standard interface so it can be swapped. |
| Real-time eligibility during claim generation | Seems useful | Eligibility (270/271) should happen at scheduling and check-in, not at claim generation. By the time a claim is being generated, the encounter already happened. Real-time eligibility at claim time adds latency and is too late to affect the service delivered. | Real-time eligibility at scheduling (Phase 2). At claim generation, show the last known eligibility status with date. |
| Full RCM (Revenue Cycle Management) outsourcing integration | "Just outsource the billing" | RCM outsourcing is a business service, not an EMR feature. Building an API for an RCM company adds scope without core user value for the solo physician who wants to control their billing. | Export claims and payment data in standard formats (837P, 835, CSV). RCM companies can work from these exports without a special integration. Phase 3 if a specific RCM partner relationship develops. |

---

### Category 6: E-Prescribing via Weno Exchange

E-prescribing is table stakes for EMR adoption. Many states now mandate it. EPCS (controlled substances) is federally mandated for Medicare Part D. Without e-prescribing, the EMR is not a viable tool for primary care.

#### Table Stakes

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| Drug search (RxNorm-coded) | Physicians must find medications by brand, generic, or drug class; searching a database of 100K+ entries must be instant | MEDIUM | RxNorm local database (NLM, free). Type-ahead search by name, NDC, or RxCUI. Show: brand name, generic name, dosage forms, strengths. Select specific strength + formulation to generate structured prescription. |
| Prescription creation form | Standard prescription fields: drug, dose, route, frequency, quantity, days supply, refills, instructions (sig), pharmacy | LOW | Pre-populated sig phrases (e.g., "take 1 tablet by mouth twice daily"). Days supply auto-calculated from quantity + frequency. Refill limits enforced for Schedule II (0 refills). |
| Pharmacy directory and routing | Physician selects patient's preferred pharmacy; prescription routes electronically to that pharmacy | MEDIUM | Weno Exchange provides pharmacy directory (40K+ pharmacies in SureScripts network). Patient-linked preferred pharmacy. One-click re-route to alternate pharmacy. |
| Weno Exchange API integration | The transmission layer; all non-EPCS prescriptions route through Weno to the SureScripts network | HIGH | REST API or SFTP-based. Weno charges ~$300 activation + per-transaction fees. Integration requires enrollment, provider NPI verification, test environment validation before going live. NewRx, CancelRx, RenewResponse message types. Monthly drug database update (required per Weno terms). |
| Prescription history per patient | Physicians need to see all prescriptions sent for a patient — what was prescribed, when, to which pharmacy | LOW | List from M001 MedicationRequest resources. Status: sent, dispensed, cancelled, expired. Links back to the encounter where prescribed. |
| Drug interaction checking | Prescribing a drug that interacts with the patient's current medications is a patient safety and malpractice liability issue | MEDIUM | RxNav-in-a-Box (NLM Docker service, local, free with UMLS license). Checks new prescription against all current medications. Severity levels: contraindicated / major / moderate / minor. Show interaction before finalizing prescription. |
| EPCS for Schedule II-V controlled substances | DEA and CMS mandate for Medicare Part D; growing state mandates (42 states have EPCS requirements as of 2025) | HIGH | DEA Part 1311 compliance: identity proofing of prescriber, two-factor authentication per prescription (TOTP + password or biometric), DEA-compliant audit trail for every controlled prescription. Weno supports EPCS with DEA 1311.120 compliance. Requires provider EPCS enrollment (distinct from standard e-Rx enrollment). |
| Allergy-to-drug conflict alert | Prescribing a medication the patient is allergic to is an immediate patient safety event | LOW | Check new prescription drug class against M001 AllergyIntolerance resources. Hard stop for documented drug allergies; warn for drug-class cross-reactivity. |

#### Differentiators

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| Refill request inbox and one-click approval | Pharmacies send refill requests electronically; physician receives them in-app with patient context and current med list visible; approves with one click | MEDIUM | Weno RenewRequest message handling. Inbox view sorted by patient/urgency. Click to open patient chart alongside request. Send RenewResponse (approved/denied with reason). Reduces phone tag between pharmacy and office — high daily time savings for solo physician. |
| Formulary indication on drug search | When physician selects a drug, show patient's insurance formulary tier (preferred/non-preferred/not covered) and estimated copay | HIGH | Requires formulary data feeds from PBM. Complex to maintain. Show as Phase 2 enhancement once basic e-Rx is working. |
| Prior authorization detection | For drugs requiring prior authorization, alert physician before sending Rx and provide the PA form/requirements | MEDIUM | PBM-supplied prior authorization list. Alert: "This medication requires prior authorization for [payer]. Would you like to initiate PA?" Phase 2 — requires formulary data. |

#### Anti-Features

| Feature | Why Requested | Why Problematic | Alternative |
|---------|---------------|-----------------|-------------|
| SureScripts direct connection (bypassing Weno) | Lower transaction cost, direct integration | SureScripts requires its own enrollment, liability agreements, technical certification, and per-transaction commercial agreements that are not designed for small EMR vendors. Weno Exchange exists specifically as a certified SureScripts network aggregator for healthcare IT vendors. | Weno Exchange is the right integration point for a new EMR vendor. Revisit direct SureScripts only at Phase 4+ scale (thousands of providers). |
| In-app prescription printing (bypass e-Rx) | Paper Rx as fallback | Paper prescriptions for controlled substances require state-specific tamper-resistant paper, are not trackable, and are increasingly not accepted by pharmacies. Paper Rx for non-controlled substances is a workflow regression that defeats the purpose of EMR integration. | E-Rx for all prescriptions. EPCS for controlled substances. Paper Rx as literal last-resort fallback (prescriber network outage), not a standard workflow option. |
| Automated refills (schedule without physician action) | Convenience for chronic medications | Automating refills without physician review creates liability exposure and is not standard of care. Chronic medication management requires periodic physician assessment. | Auto-remind physician when refill request arrives (inbox notification). One-click approval with patient context visible — fast but requires human attestation. |

---

## Feature Dependencies

```
[M001 Backend (complete)]
    |
    +--required-by--> [Patient Chart UI Shell]
    |                      |
    |                      +--required-by--> [SOAP Note Entry]
    |                      |                     |
    |                      |                     +--required-by--> [AI SOAP Generation]
    |                      |                     |                     |
    |                      |                     |                     +--requires--> [whisper.cpp sidecar]
    |                      |                     |                     +--requires--> [Ollama (local, localhost:11434)]
    |                      |                     |                     +--requires--> [LLaMA 3.1 8B model pulled]
    |                      |                     |
    |                      |                     +--required-by--> [Billing Fee Sheet]
    |                      |                                           |
    |                      |                                           +--required-by--> [837P Claim Generation]
    |                      |                                           |                     |
    |                      |                                           |                     +--required-by--> [ERA 835 Processing]
    |                      |                                           |                     +--required-by--> [AR Tracking]
    |                      |                                           |
    |                      |                                           +--enhanced-by--> [AI CPT/ICD-10 Suggestions]
    |                      |                                                                 |
    |                      |                                                                 +--requires--> [FAISS vector index]
    |                      |                                                                 +--requires--> [ICD-10/CPT codebook data]
    |                      |
    |                      +--required-by--> [Vitals / ROS / PE Forms]
    |                      |                     |
    |                      |                     +--feeds-into--> [SOAP Objective Section]
    |                      |                     +--feeds-into--> [E/M Level Calculator]
    |                      |
    |                      +--required-by--> [E-Prescribing UI]
    |                                           |
    |                                           +--requires--> [Weno Exchange account + API credentials]
    |                                           +--requires--> [RxNav-in-a-Box (Docker, localhost)]
    |                                           +--requires--> [RxNorm local database]
    |                                           +--requires--> [EPCS enrollment (DEA 1311) -- if EPCS in scope]
    |
    +--feeds-data-into--> [Patient Chart Facesheet]
    |                          (problems, meds, allergies, vitals, encounters from M001 commands)
    |
    +--feeds-data-into--> [AI SOAP prompt context]
    |                          (patient demographics, active diagnoses, current medications, allergies)
    |
    +--feeds-data-into--> [Drug interaction check]
                               (current MedicationRequest list + AllergyIntolerance list → RxNav-in-a-Box)

[Ollama/LLaMA] ──independent-of──> [Weno Exchange]
    (AI pipeline and e-prescribing are parallel tracks; neither blocks the other)

[Vitals/ROS/PE] ──feeds-into──> [E/M Level Calculator] ──feeds-into──> [AI CPT Suggestion]
    (documentation quality directly affects achievable billing level)
```

### Dependency Notes

- **Patient chart UI is the delivery vehicle for everything else:** All M002 features are tabs or panels within the chart view. The chart shell must exist before any other feature can be tested end-to-end.
- **M001 backend commands are already complete:** All Rust commands (clinical, patient, scheduling, labs, documentation) are built. M002 is almost entirely a React frontend task, with new Python sidecars for AI and external integrations for Weno/RxNav.
- **AI pipeline requires local infrastructure that the physician installs separately:** Ollama must be running and LLaMA 3.1 8B must be pulled before AI features work. This is a setup/onboarding requirement, not a build requirement.
- **Weno Exchange has weeks-long enrollment lead time:** Enrollment (especially EPCS) requires DEA credential verification and identity proofing that takes 2-4 weeks. Start enrollment process before e-prescribing development begins.
- **RxNav-in-a-Box requires a UMLS license and Docker:** The NLM requires a free UMLS license agreement to download RxNav-in-a-Box. Docker Desktop must be installed on the clinic Mac. These are operational prerequisites.
- **Billing depends on signed notes:** Claim generation requires a complete, signed encounter note with ICD-10 codes in the Assessment section. SOAP note entry must be complete before billing can be built and tested.
- **AI coding suggestions depend on signed note content:** The AI reads the signed SOAP note to suggest CPT/ICD-10 codes. This means AI coding is tested after the full note → sign → fee sheet workflow exists.

---

## MVP Definition for M002

### Launch With (M002 Core — Clinical UI + Forms)

The clinical UI that makes M001's backend usable for daily patient care.

- [ ] **Patient chart UI shell** — Patient banner, tab navigation, facesheet with active problems/meds/allergies/vitals; without this, no other M002 feature is accessible
- [ ] **SOAP note entry** — Structured S/O/A/P, ICD-10 linking in Assessment, draft auto-save, status workflow (draft → signed), 10-15 specialty templates; this is the core daily workflow
- [ ] **Vitals entry + flowsheet** — BP/HR/RR/Temp/SpO2/Weight/Height/BMI with auto-calc, abnormal flagging, trend table; nurses record vitals before every encounter
- [ ] **Review of Systems form** — 14-system toggle form with E/M indicator; required for correct E/M coding
- [ ] **Physical exam templates** — System-based PE with "all normal" macro; specialty templates ship with note templates
- [ ] **Billing fee sheet + CPT/ICD-10 coding** — Per-encounter charge capture with CPT search, ICD-10 search, modifiers; prerequisite for claim generation
- [ ] **X12 837P claim generation + clearinghouse submission** — The billing module is only useful if it can actually submit claims; Office Ally SFTP integration
- [ ] **E-prescribing via Weno Exchange (non-EPCS)** — Drug search, prescription form, Weno transmission, pharmacy routing; required for any prescribing workflow

### Add After Core Is Stable (M002.x)

- [ ] **AI voice-to-SOAP pipeline** — whisper.cpp transcription + Ollama/LLaMA generation; add after manual SOAP workflow is validated with real users
- [ ] **AI CPT/ICD-10 coding suggestions** — FAISS vector search from signed note; add after billing workflow is validated
- [ ] **ERA/835 payment posting** — Required for accounts receivable, but billing workflow can start with manual payment posting
- [ ] **EPCS (controlled substances)** — DEA 1311 compliance, identity proofing; add after standard e-Rx is working; start Weno EPCS enrollment in parallel
- [ ] **Drug interaction checking (RxNav-in-a-Box)** — Add with or shortly after e-Rx goes live; Docker prerequisite
- [ ] **E/M level real-time calculator** — High value but requires ROS + PE + MDM complexity calculation; add as enhancement once forms are stable
- [ ] **Refill request inbox** — High daily value for solo physician; add as Weno inbox feature once base e-Rx is working

### Future Consideration

- [ ] **Formulary tier display** — Requires PBM data feeds; complex to maintain; Phase 3
- [ ] **Prior authorization detection** — Requires formulary data; Phase 3
- [ ] **Denial pattern detection / analytics** — Requires 6+ months of claims data to surface patterns; Phase 3
- [ ] **Pediatric growth charts** — Pediatric-specific; add when a pediatrics user is onboarded
- [ ] **Ambient continuous capture mode** — More complex than push-to-record; Phase 3 AI enhancement

---

## Feature Prioritization Matrix

| Feature | User Value | Implementation Cost | Priority | Category |
|---------|------------|---------------------|----------|----------|
| Patient chart UI shell + facesheet | HIGH | MEDIUM | P1 | UI Shell |
| SOAP note entry (structured + templates) | HIGH | HIGH | P1 | Clinical |
| Vitals entry + flowsheet | HIGH | LOW | P1 | Clinical |
| ROS 14-system form | MEDIUM | MEDIUM | P1 | Clinical |
| PE specialty templates | MEDIUM | MEDIUM | P1 | Clinical |
| Billing fee sheet (CPT/ICD-10) | HIGH | MEDIUM | P1 | Billing |
| X12 837P claim + clearinghouse | HIGH | HIGH | P1 | Billing |
| E-Rx via Weno (non-EPCS) | HIGH | HIGH | P1 | E-Rx |
| Drug interaction check (RxNav) | HIGH (safety) | MEDIUM | P1 | E-Rx |
| AI voice-to-SOAP (whisper + LLaMA) | HIGH (differentiator) | HIGH | P2 | AI |
| AI CPT/ICD-10 suggestions (FAISS) | HIGH (financial ROI) | HIGH | P2 | AI/Billing |
| ERA/835 payment posting | HIGH | HIGH | P2 | Billing |
| EPCS for controlled substances | MEDIUM-HIGH | HIGH | P2 | E-Rx |
| E/M level real-time calculator | MEDIUM | MEDIUM | P2 | Billing/Clinical |
| Refill request inbox | MEDIUM | MEDIUM | P2 | E-Rx |
| Patient chart activity timeline | MEDIUM | MEDIUM | P2 | UI Shell |
| Denial pattern detection | MEDIUM | MEDIUM | P3 | Billing |
| Ambient continuous capture | MEDIUM | HIGH | P3 | AI |
| Formulary tier display | LOW | HIGH | P3 | E-Rx |
| Pediatric growth charts | MEDIUM (specialty) | MEDIUM | P3 | Clinical |

**Priority key:**
- P1: Must have for M002 launch — practice cannot function without it
- P2: Add in M002.x — high value, add once P1 is validated
- P3: Phase 3+ — defer until product-market fit is established

---

## Competitor Feature Analysis

| Feature | OpenEMR v8 | DrChrono | Practice Fusion | MedArc M002 Approach |
|---------|-----------|----------|-----------------|----------------------|
| Patient chart UI | Multi-tab portal, complex | iPad-native sidebar | Web-based facesheet | Desktop-native tabs + sticky patient banner |
| SOAP note entry | CAMOS decision trees, 60+ form types | iPad-optimized templates | Template-based | Structured S/O/A/P with 10-15 specialty templates; AI generation in P2 |
| Vitals flowsheet | Full flowsheet + growth charts | Flowsheet with sparklines | Basic flowsheet | Flowsheet + BMI auto-calc + abnormal flagging; growth charts in P3 |
| ROS/PE forms | 14-system ROS + exam templates | iOS-native ROS | Basic templates | 14-system ROS + system-based PE with E/M indicator |
| E-prescribing | Weno Exchange, EPCS | SureScripts, EPCS | SureScripts, EPCS | Weno Exchange (same as OpenEMR); EPCS P2 |
| Drug interactions | Basic (RxNav) | Basic | Basic | RxNav-in-a-Box (local Docker, severity-rated) |
| Billing/coding | Full 837P/835, CMS-1500 | 837P/835 | 837P, basic | 837P/835 + AI code suggestions (FAISS) — unique |
| AI documentation | None | Basic voice dictation | None | Ambient AI: whisper.cpp + LLaMA 3.1 8B — primary differentiator |
| AI billing coding | None | None | None | LLM entity extraction + FAISS vector search — unique in this market segment |
| Offline capability | Yes (self-hosted) | No | No | Yes (SQLCipher local, offline-first) |

---

## Sources

- **Solo practice EMR clinical workflow:** WebSearch synthesis of clinikehr.com, myzhealth.io, docvilla.com (2025-2026 content) — MEDIUM confidence
- **AI scribe documentation workflow:** NEJM Catalyst, JMIR AI, AMA (2025), PMC clinical trials — HIGH confidence; published peer-reviewed sources
- **AI medical coding accuracy (GPT-4 33.9% ICD-10):** NEJM AI published research, AWS Industries blog, Ventus AI (2025) — HIGH confidence; consistent across multiple sources
- **X12 837P claim format:** CMS.gov official documentation, MediBill RCM — HIGH confidence; regulatory standard
- **Weno Exchange API and EPCS:** wenoexchange.com official API documentation, OpenEMR wiki — HIGH confidence; official vendor documentation
- **RxNav-in-a-Box:** NLM/LHNCBC official documentation (lhncbc.nlm.nih.gov), UMLS license requirement confirmed — HIGH confidence
- **EPCS requirements (DEA 1311, CMS mandate):** DEA Diversion Control Division, CMS.gov, RXNT — HIGH confidence; regulatory sources
- **Office Ally clearinghouse:** cms.officeally.com, Kalix EMR documentation — MEDIUM confidence; vendor documentation
- **E/M coding 2021 CMS guidelines:** Established regulatory standard — HIGH confidence
- **Patient chart UI patterns:** DrChrono support documentation, Tebra help center, Epic documentation — MEDIUM confidence
- **Whisper hallucination rate (1%):** Cited in PROJECT.md from validated sources — MEDIUM confidence
- **LLaMA 3.1 8B 64% NEJM performance:** Cited in PROJECT.md from published research — MEDIUM confidence

---
*Feature research for: Solo-practice desktop EMR — M002 clinical UI, AI transcription, billing, e-prescribing*
*Researched: 2026-03-11*

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

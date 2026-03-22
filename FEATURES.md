# PanaceaEMR — Feature List

> AI-powered, local-first Electronic Medical Records for Physical Therapy

---

## Authentication & Security

- **User registration** with 5-role system (SystemAdmin, Provider, NurseMa, BillingStaff, FrontDesk)
- **Argon2id password hashing** with 12+ character minimum
- **TOTP MFA** — QR code enrollment, 6-digit verification
- **Touch ID / biometric login** via macOS LAContext
- **Session management** — auto-lock on idle, password unlock, configurable timeout
- **SQLCipher encryption** — AES-256-CBC, key stored in macOS Keychain (Secure Enclave on Apple Silicon)
- **RBAC matrix** — resource-level + action-level + field-level access control
- **HIPAA audit log** — SHA-256 hash-chained, immutable, every ePHI operation logged
- **Break-glass access** — emergency elevated permissions with time-limited scope
- **Dev bypass** — auto-login in development builds (compiled out of production)

---

## Patient Management

- Create/update patients with full demographics, insurance, and SDOH data
- MRN generation and assignment
- Primary/secondary/tertiary insurance coverage tracking
- Care team management with provider roles
- Related persons (emergency contacts, next-of-kin, guarantors)
- Allergy list with severity and reactions (FHIR AllergyIntolerance)
- Problem list with ICD-10 codes and clinical status
- Medication list with RxNorm coding
- Immunization history with CVX codes
- Fast indexed search by name, MRN, or DOB
- Patient delete with cascade cleanup

---

## Scheduling & Appointments

- **Multi-provider calendar** — day/week/month views with provider filtering
- **Appointment management** — create, update, cancel, drag-to-reschedule
- **Color-coded categories** — new patient, follow-up, PT treatment, initial eval, telehealth
- **Configurable duration** — 5 to 60 minutes
- **Recurring appointments** — weekly, biweekly, monthly with end date
- **Privacy mode** — converts patient names to initials on the calendar
- **Patient Flow Board** — real-time status (checked-in, roomed, with provider, checkout)
- **Waitlist management** — auto-fill cancelled slots, priority tracking
- **Recall Board** — overdue patient follow-up tracking
- **Open slot search** — find available times by provider, type, and date range
- **Appointment reminders** — SMS (Twilio) and email (SendGrid) with 24hr/2hr intervals
- **No-show follow-up** — automatic messaging for missed appointments
- **Custom reminder templates** — per channel and reminder type

---

## Clinical Documentation

- **Encounter management** — SOAP notes with types (office visit, telehealth, urgent, preventive, procedure)
- **Vitals recording** — BP, HR, RR, temp, SpO2, weight, height, BMI (auto-calc), pain scale
- **Review of Systems** — 14 organ systems (positive/negative/not reviewed)
- **Physical exam** — system-based findings
- **10+ clinical templates** — general, cardiology, orthopedic, and more
- **Co-signature workflow** — NP/PA notes require supervising physician approval
- **Drug-allergy CDS alerts** — passive clinical decision support
- **Encounter-appointment linking** — link notes to scheduled visits
- **Amendment workflow** — edit finalized encounters with audit trail

---

## Objective Measures & Outcomes

- **ROM recording** — by joint and motion
- **MMT grading** — by muscle group (0-5 scale)
- **Special tests** — Lachman, Neer's, and more
- **Standardized outcome scores:**
  - LEFS (Lower Extremity Functional Scale)
  - DASH (Disabilities of Arm, Shoulder, Hand)
  - NDI (Neck Disability Index)
  - Oswestry Disability Index
  - PSFS (Patient Specific Functional Scale)
  - FABQ (Fear-Avoidance Beliefs Questionnaire)
- **Severity auto-classification** — mild/moderate/severe
- **MCID tracking** — Minimal Clinically Important Difference achievement
- **Score trending** — inline SVG charts, earliest-to-latest comparison

---

## AI Assistant (NEW)

- **Conversational interface** — slide-out chat panel accessible from any page
- **Keyboard shortcut** — Cmd+K to toggle
- **Natural language commands:**
  - "Schedule John Smith for PT treatment every Tuesday and Thursday at 10am for the next month"
  - "What appointments do I have tomorrow?"
  - "Show me patients who haven't been seen in 30 days"
  - "Search for patient Jane Doe"
- **Action confirmation cards** — review proposed actions before execution
- **Conversation history** — persistent across sessions, browsable and deletable
- **Multi-turn context** — remembers conversation history for follow-up questions
- **LLM-powered** — deepseek-r1:14b (local via Ollama) with AWS Bedrock fallback
- **RBAC-aware** — actions respect user permissions
- **Audit logged** — all interactions tracked (message content excluded for HIPAA)

---

## AI Voice-to-Note

- **Microphone recording** — 16kHz mono WAV, real-time level visualization
- **Local transcription** — whisper.cpp (small.en model by default, 4 sizes available)
- **AI note generation** — SOAP-PT format from session transcript
  - Progress notes and initial evaluations
  - Field-level confidence scoring (high/medium/low)
- **CPT code suggestions** — AI extraction from clinical narrative
- **Objective data extraction** — ROM, pain scores, and MMT from transcript
- **Privacy-first** — audio deleted after transcription, transcripts never logged
- **Configurable LLM** — deepseek-r1:14b (default), llama3.1:8b (fallback), AWS Bedrock Claude Haiku (cloud fallback)

---

## Home Exercise Program (HEP)

- **Exercise library** — ~50 built-in PT exercises
- **Filtering** — by body region (10 regions) and category (ROM, strengthening, stretching, balance, functional, cardio)
- **Difficulty levels** — beginner, intermediate, advanced
- **Two-panel builder** — library browser + program composer
- **Per-exercise prescription** — sets, reps, duration, hold time, frequency, resistance, pain threshold
- **Reusable templates** — save and load program templates
- **Patient linking** — HEP programs linked to encounters

---

## Billing & Claims

- **CPT code library** — evaluation, timed, and untimed PT codes
- **8-minute rule calculator** — Medicare and AMA/commercial methods
- **Fee schedule management** — per-payer CPT pricing
- **Encounter billing** — line items with units, charges, and diagnosis pointers
- **Therapy cap monitoring** — Medicare $2,480 PT/SLP combined cap
- **KX modifier** — auto-application when cap threshold reached
- **PTA CQ modifier** — 15% reduction for Physical Therapist Assistants
- **ABN workflow** — Advance Beneficiary Notice generation with patient choice recording

### Electronic Claims (837P EDI)

- **Payer configuration** — EDI payer ID, clearinghouse, billing rules
- **837P generation** — full EDI transaction set (ISA/GS/ST headers through trailers)
- **Claim lifecycle** — draft, validated, submitted, accepted, paid, denied, appealed
- **Claim validation** — NPI, Tax ID, payer EDI ID, member ID, diagnosis/CPT checks

### Remittance (ERA/835) Processing

- **835 parser** — extract payments, adjustments, and CARC codes
- **Auto-posting** — match ERA payments to claims by control number
- **Denial management** — denial queue with CARC descriptions and appeal tracking
- **A/R aging** — 0-30, 31-60, 61-90, 91-120, 120+ day buckets
- **Patient balance** — outstanding balance after insurance payments

---

## Document Center

- **Document upload** — file picker with category selection (up to 64 MB)
- **8 categories** — referral/Rx, imaging, consent, intake, insurance, legal, HEP, other
- **Inline preview** — PDF and image viewing
- **SHA-256 integrity** — document checksums for tamper detection
- **Intake survey builder** — custom forms with text, number, yes/no, pain scale, date fields
- **Kiosk mode** — patient-facing tablet interface with large UI, one field per screen
- **Referral tracking** — create, update, list referrals with status

---

## Fax Integration (Phaxio)

- **Send fax** — PDFs and documents with cover page
- **Receive fax** — poll inbox, link to patient
- **Fax contacts directory** — reusable contacts by type (insurance, referring MD, attorney)
- **Fax log** — delivery status, retry failed faxes
- **Encounter note faxing** — generate PDF and fax in one step

---

## PDF Export & Reports

- **Single encounter note** — formatted clinical note
- **Progress report** — date range summary with visit counts and improvements
- **Insurance narrative** — utilization review for payers
- **Legal/IME report** — formatted for attorney review
- **Full chart export** — all notes in a single PDF
- **Configurable letterhead** — practice name, address, logo, signature image
- **Export log** — track all generated exports

---

## Analytics & Outcomes Dashboard

- **Operational KPIs** — visits/month, cancellation rate, units/visit, new patients
- **Financial KPIs** — revenue/visit, net collection rate, days in A/R
- **Clinical outcomes** — MCID rates, average improvement, discharge rate
- **Payer mix** — revenue % by payer, visit counts, average reimbursement
- **Inline SVG charts** — donut charts, bar charts, KPI cards (zero npm dependencies)

---

## MIPS Quality Measures

- **Measure #182** — Functional Outcome Assessment
- **Measures #217-222** — Upper/Lower Extremity and Spine Functional Status
- **Measure #134** — Depression Screening (PHQ-2/PHQ-9)
- **Measure #155** — Falls Risk Screening (65+)
- **Measure #128** — BMI Documentation
- **MIPS dashboard** — performance year view, color-coded tiers, composite score projection

---

## Workers' Compensation

- **Case management** — employer, injury details, claim number, state jurisdiction
- **Case status** — open, closed, settled, disputed
- **WC contacts** — adjuster, attorney, nurse case manager, employer rep
- **FROI generation** — First Report of Injury document
- **State fee schedules** — max allowable rates by state/CPT (CA, TX, FL, NY, WA seeded)
- **Impairment ratings** — AMA Guides (3rd-6th edition), whole person percentage
- **Communication log** — phone, email, fax, letter, in-person tracking

---

## Backup & Recovery

- **Encrypted backups** — AES-256-GCM with SHA-256 integrity digest
- **Backup to external storage** — folder picker, timestamped files
- **Restore workflow** — decrypt, verify integrity, replace database
- **Backup history** — log with size, status, and digest

---

## Technical Highlights

| Layer | Technology |
|-------|-----------|
| Desktop | Tauri 2.x (Rust + WKWebView) |
| Frontend | React 18 + TypeScript 5.5 + Tailwind CSS 3.4 |
| Database | SQLCipher (AES-256 encrypted SQLite) |
| Auth | Argon2id + TOTP + Touch ID |
| AI (local) | whisper.cpp + Ollama (deepseek-r1:14b) |
| AI (cloud) | AWS Bedrock Claude Haiku |
| PDF | printpdf (Rust) |
| Fax | Phaxio API |
| SMS | Twilio |
| Email | SendGrid |
| Data model | FHIR R4 JSON + denormalized indexes |

- **32 database migrations** — append-only, validated on startup
- **521+ Rust unit tests** — full backend test coverage
- **200+ Tauri commands** — type-safe frontend-backend bridge
- **Local-first** — no cloud dependency, all PHI stays on device
- **macOS native** — Hardened Runtime, App Sandbox, universal binary (ARM + Intel)

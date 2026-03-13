# M003: PT Practice

**Vision:** Specialise MedArc for Physical Therapy solo practice. Build on the complete M001/M002 foundation to deliver PT-native note types, structured objective measures, AI voice-to-note, a categorised patient document vault, PDF export, Phaxio fax integration, and insurance authorisation tracking — making MedArc a viable day-one practice management tool for a solo PT.

## Success Criteria

- Provider can complete a full PT episode: Initial Eval → Daily Progress Notes → Discharge Summary, all co-signed and locked, without leaving the app
- Touch ID authenticates on the lock screen (real LAContext prompt, not stub)
- AI voice recording → signed note in under 3 minutes of provider review time on Apple Silicon
- All outcome measures scored automatically with no manual calculation; initial vs discharge comparison appears in Discharge Summary
- Any note or document can be faxed in under 30 seconds
- Auth warnings fire correctly with zero missed exhausted-auth encounters
- PDF exports are correctly formatted with letterhead and accepted by major PT payers
- All ePHI access audit-logged via existing hash-chain system
- `cargo test --lib` passes with new test coverage for all new Slice data models
- `tsc --noEmit` exits 0 after all slices

## Key Risks / Unknowns

- **whisper-rs build complexity** — whisper.cpp requires llvm/clang toolchain and model download; may fail on clean machine — retire in S03 T01
- **objc2-local-authentication async callback bridging** — LAContext completion callback bridged to Tauri synchronous command model — retire in S01 T01
- **printpdf layout engine** — line wrapping, page breaks, embedded fonts for letterhead PDFs — retire in S05 T01
- **Phaxio multipart upload** — reqwest multipart with correct headers — retire in S06 T01
- **App Sandbox + microphone entitlement** — silent failure if entitlement missing — added in S03

## Proof Strategy

- LAContext async FFI → retire in S01 T01 by triggering a real Touch ID prompt on hardware
- whisper-rs build + accuracy → retire in S03 T01 by transcribing a test audio clip to text
- printpdf layout → retire in S05 T01 by generating a real PDF that opens correctly in Preview
- Phaxio API → retire in S06 T01 by sending a real test fax to a Phaxio test number

## Verification Classes

- Contract verification: `cargo test --lib` for all new Tauri command data models; `tsc --noEmit` for all new React components
- Integration verification: Touch ID prompt fires on real hardware; Whisper transcribes real 20-min audio; Phaxio sends and receives real faxes; PDF opens in Preview with correct formatting
- Operational verification: Ollama startup check fires on app launch; Phaxio background polling task runs every 5 min; auth banner fires on note co-sign
- UAT / human verification: Provider completes full IE → progress note → discharge episode; re-auth letter faxes successfully

## Milestone Definition of Done

This milestone is complete only when all are true:

- All 7 slices are marked `[x]` with verified summaries
- Touch ID unlock works on real hardware (not just cargo test)
- Full PT episode (IE → daily notes → discharge) completes end-to-end with co-sign locking
- Outcome measure comparison populates in Discharge Summary from S02 data
- AI voice draft generates from real microphone audio via whisper.cpp + Ollama
- PDF exports are letterhead-formatted and open in Preview without errors
- Phaxio send confirmed with a delivered status from the API
- Auth visit counter increments correctly on note co-sign; all four banner conditions fire
- `cargo test --lib` passes (all existing 265 tests + new M003 tests)
- `tsc --noEmit` exits 0
- `.gsd/REQUIREMENTS.md` AUTH-04-FIX + all PT-* requirements marked validated

## Requirement Coverage

- Covers: AUTH-04-FIX, PT-DOC-01, PT-DOC-02, PT-DOC-03, PT-DOC-04, PT-OBJ-01, PT-OBJ-02, PT-OBJ-03, PT-OBJ-04, PT-AI-01, PT-AI-02, PT-AI-03, PT-DOC-CTR-01, PT-DOC-CTR-02, PT-DOC-CTR-03, PT-EXP-01, PT-EXP-02, PT-FAX-01, PT-FAX-02, PT-FAX-03, PT-AUTH-01, PT-AUTH-02, PT-AUTH-03
- Partially covers: none
- Leaves for later: AUDT-03, CLIN-08, BKUP-04, SCHD-08, SCHD-09
- Orphan risks: none

## Slices

- [ ] **S01: Touch ID Fix + PT Note Templates** `risk:high` `depends:[]`
  > After this: Touch ID works on the lock screen; provider can create, co-sign, and lock all three PT note types (IE, Daily Progress Note, Discharge Summary) with PT-specific fields — proven by cargo test --lib for data models and tsc --noEmit for UI.

- [ ] **S02: Objective Measures & Outcome Scores** `risk:medium` `depends:[S01]`
  > After this: Provider can record ROM/MMT/special ortho tests via body diagram UI and run auto-scored LEFS/DASH/NDI/Oswestry/PSFS/FABQ with longitudinal trend graph — proven by cargo test --lib for scoring logic and tsc --noEmit for UI.

- [ ] **S03: AI Voice-to-Note (Whisper + Ollama)** `risk:high` `depends:[S01]`
  > After this: Provider presses Record, speaks for up to 20 minutes, and gets a SOAP-PT draft pre-filling the note form in under 3 minutes — proven by end-to-end test with real microphone audio on Apple Silicon.

- [ ] **S04: Patient Document Centre** `risk:medium` `depends:[S01]`
  > After this: Patient document vault shows 8 PT categories with inline preview; intake survey builder produces a kiosk-fillable form that auto-generates a PDF saved to the chart; referral tracking visible in patient header — proven by tsc --noEmit and manual kiosk flow test.

- [ ] **S05: Export & PDF Generation** `risk:medium` `depends:[S01,S02]`
  > After this: Any PT note or report exports as a letterhead-formatted PDF; full chart exports as a merged bundle with TOC — proven by generated PDFs opening correctly in Preview with all required fields populated.

- [ ] **S06: Fax Integration (Phaxio)** `risk:medium` `depends:[S05]`
  > After this: Provider clicks Fax on any PDF, confirms recipient, and the document is transmitted via Phaxio; inbound faxes appear in Fax Inbox with badge — proven by real fax sent and received via Phaxio test account.

- [ ] **S07: Authorization & Visit Tracking** `risk:low` `depends:[S01,S06]`
  > After this: Auth record tracks visits used/remaining; amber/red banners fire correctly at 2 remaining and 0; re-auth request letter generates from latest note data and queues for fax — proven by cargo test --lib for visit counter logic and manual UI banner verification.

---

## Boundary Map

### S01 → S02
Produces:
- `commands/pt_notes.rs` → `create_pt_note(note_type, patient_id, encounter_id, ...)`, `get_pt_note(id)`, `list_pt_notes(patient_id, note_type?)`, `update_pt_note(id, ...)`, `cosign_pt_note(id)`, `lock_pt_note(id)`
- `PTNoteRecord` type with `note_type: "initial_eval" | "progress_note" | "discharge_summary"`, `status: "draft" | "signed" | "locked"`, full IE/SOAP-PT/Discharge fields
- Migration 15: `pt_note_index` table (patient_id, encounter_id, note_type, status, created_at, provider_id)
- `src/types/pt.ts` — TypeScript types for all PT note shapes
- `src/lib/tauri.ts` additions — all pt_notes command wrappers
- `src-tauri/src/auth/biometric.rs` — real `check_biometric_available()` via LAContext.canEvaluatePolicy
- New Tauri command `biometric_authenticate` — returns Ok(()) on Touch ID success

Consumes: nothing (first slice)

### S01 → S03
Produces:
- PT note fields (SOAP-PT structure) that the AI draft will populate
- `PTNoteInput` TypeScript type used by the note form (AI fills this shape)

Consumes: nothing (first slice)

### S01 → S04
Produces:
- `encounter_id` foreign key pattern used by Document Center for linking documents to encounters

Consumes: nothing (first slice)

### S01 → S05
Produces:
- `get_pt_note(id)` and `list_pt_notes(patient_id)` — PDF generation reads these to populate report content
- `PTNoteRecord` with all IE/SOAP-PT/Discharge fields

Consumes: nothing (first slice)

### S01 → S07
Produces:
- `cosign_pt_note(id)` — S07 hooks into this to increment `visits_used` on successful co-sign

Consumes: nothing (first slice)

### S02 → S01 (reverse dependency note)
S01 Discharge Summary references outcome measure scores; S02 provides these. S01 ships with a placeholder for outcome comparison; S02 fills it in.

### S02 → S05
Produces:
- `get_outcome_scores(patient_id, measure_type?, start_date?, end_date?)` — returns scored assessments with timestamps
- `OutcomeScoreRecord` type with measure_type, score, severity_class, recorded_at
- Migration 16: `outcome_score_index` (patient_id, measure_type, score, recorded_at, encounter_id)

Consumes from S01:
- `encounter_id` FK for linking scores to encounters

### S03 → S01
Produces:
- `transcribe_audio(wav_bytes)` → raw transcript text
- `generate_note_draft(transcript, note_type, patient_context)` → `PTNoteDraftResult` with field-level uncertainty flags
- No new DB tables (audio not stored; transcript stored alongside note via `update_pt_note`)

Consumes from S01:
- `PTNoteInput` shape — AI populates this shape for the note form
- `update_pt_note` command — saves AI-populated draft

### S04 → S06
Produces:
- Document Center category enum used by fax log (`"referral_rx" | "imaging" | "consent" | ...`)
- `IntakeSurveyRecord` with structured responses linked to patient_id
- `ReferralRecord` with referring provider NPI, authorized_visits, document_id FK

Consumes from S01:
- Existing `labs::upload_document` pattern for document storage (S04 upgrades categories, doesn't replace the storage mechanism)

### S05 → S06
Produces:
- `generate_pdf(note_id | report_type, patient_id, date_range?)` → file path to generated PDF in app temp directory
- `ExportedPdfRecord` with file_path, report_type, patient_id, generated_at

Consumes from S01:
- `get_pt_note`, `list_pt_notes` — note content for PDF body

Consumes from S02:
- `get_outcome_scores` — score comparison tables in progress reports

### S06 → S07
Produces:
- `send_fax(file_path, recipient_fax, recipient_name, patient_id?)` → `FaxRecord`
- `FaxRecord` type with fax_id, direction, status, patient_id, pages
- Migration 17: `fax_log_index` (fax_id, direction, patient_id, status, sent_at)
- Fax Contacts directory (name, org, fax, phone, contact_type)
- Fax Inbox component with badge count

Consumes from S05:
- `ExportedPdfRecord.file_path` — path to PDF to attach to fax

### S07 → S06
Produces:
- `generate_reauth_letter(auth_id, patient_id)` → file path to re-auth PDF
- Fax button in re-auth UI calls S06's `send_fax`

Consumes from S01:
- `cosign_pt_note` — visit counter increments on this event
- `list_pt_notes` — latest note data for re-auth letter content

Consumes from S06:
- `send_fax` — re-auth letter delivery

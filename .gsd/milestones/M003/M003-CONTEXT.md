# M003: PT Practice — Context

**Gathered:** 2026-03-13
**Status:** Ready for planning

## Project Description

M003 specialises MedArc for Physical Therapy practice. It builds on the complete M001 backend (88 Tauri commands, FHIR R4, SQLCipher, RBAC, audit chain) and M002 React frontend to deliver a PT-complete product: structured PT note types, objective measures and outcome scores, AI voice-to-note, a categorised patient document vault, PDF export, Phaxio fax integration, and insurance authorisation tracking.

The first task of S01 fixes the Touch ID stub that has existed since M001 — every user with a Touch ID Mac has been getting password-only unlock despite AUTH-04 being marked validated.

## Why This Milestone

Solo PT practitioners face the same documentation burden as physicians but with additional PT-specific workflow requirements: ROM/MMT measures, standardised outcome scores, auth tracking per insurer, and high-volume faxing to referring physicians and payers. The generic SOAP notes from M001/S07 don't map to PT practice. M003 turns MedArc into a viable practice management tool for a PT solo practice from day one.

## User-Visible Outcome

### When this milestone is complete, the user can:

- Unlock the app with Touch ID (real biometric prompt, not just password)
- Complete a full PT episode: Initial Eval → Daily Progress Notes → Discharge Summary, all co-signed and locked
- Record a treatment session by voice and get a SOAP-PT draft in under 3 minutes
- Tap a body diagram to record ROM/MMT; run an orthopedic test battery; see outcome scores graphed over the episode
- Store, preview, and search all patient documents in 8 PT-specific categories
- Export any note as a formatted letterhead PDF and fax it to a referring physician in under 30 seconds
- Track insurance authorisations with amber/red banners that fire before auth is exhausted
- Generate a re-auth request letter from the latest note data and fax it directly

### Entry point / environment

- Entry point: MedArc macOS desktop app (Tauri 2.x, WKWebView)
- Environment: local macOS, SQLCipher encrypted database, no network required except for Phaxio fax and optional Bedrock fallback
- Live dependencies: Ollama at localhost:11434 (optional, checked on startup), Phaxio API (S06), AWS Bedrock (S03 fallback only)

## Completion Class

- Contract complete means: all Tauri commands exist with real implementations, `cargo test --lib` passes for all new data models, `tsc --noEmit` exits 0 for all new frontend code
- Integration complete means: Touch ID LAContext prompt fires on real hardware; Whisper transcribes real audio; Phaxio sends a real fax; PDF exports open correctly in Preview
- Operational complete means: Ollama availability check on startup; Phaxio polling background task runs correctly; auth banners fire on note co-sign

## Final Integrated Acceptance

To call this milestone complete, we must prove:

- Touch ID prompt fires natively on a Mac with Touch ID; unlock completes on success
- Provider records a voice session → transcript appears → SOAP-PT draft pre-fills note fields
- A full IE → 3 progress notes → discharge summary episode completes with outcome measure comparison in the discharge note
- A progress report PDF exports with correct letterhead and is faxed to a test number via Phaxio
- Auth record shows correct visit count; red banner blocks note creation at 0 remaining

## Risks and Unknowns

- **whisper-rs build complexity** — whisper.cpp requires llvm/clang and model download; first build on a clean machine may fail. Retire in S03 T01 by getting a test transcription working end-to-end before building the UI.
- **objc2-local-authentication FFI async** — LAContext.evaluatePolicy uses a completion callback (async); bridging to Tauri's synchronous command model requires a blocking wait or channel. Retire in S01 T01 by implementing and testing the actual prompt.
- **printpdf layout complexity** — Generating well-formatted letterhead PDFs with variable content is non-trivial; line wrapping and page breaks need care. Retire in S05 T01 by generating a real single-note PDF and verifying it opens in Preview.
- **Phaxio multipart upload** — reqwest multipart with PDF bytes; needs correct Content-Type and auth headers. Retire in S06 T01 by sending a real test fax.
- **App Sandbox + microphone entitlement** — cpal audio capture requires `com.apple.security.device.microphone` entitlement; without it the permission prompt never fires and recording silently fails. Must be added in S03.

## Existing Codebase / Prior Art

- `src-tauri/src/auth/biometric.rs` — Current stub; `check_biometric_available()` hardcoded to `false`. Replace with real LAContext call.
- `src-tauri/src/commands/mfa.rs` — `check_biometric`, `enable_touch_id`, `disable_touch_id` commands exist. Need to add `biometric_authenticate` command.
- `src-tauri/src/commands/session.rs` — `unlock_session` takes a password string. Biometric unlock needs a parallel path that bypasses password verification.
- `src-tauri/entitlements.plist` — Add `com.apple.security.device.biometric-access` (Touch ID) and `com.apple.security.device.microphone` (audio capture for S03).
- `src/components/auth/LockScreen.tsx` — Touch ID button exists but `handleTouchId` falls through to `onUnlock("")`. Wire to real `biometric_authenticate` command.
- `src-tauri/src/commands/documentation.rs` — 2,955 lines. PT note types extend this module or live in a new `commands/pt_notes.rs`. Follow the same FHIR-aligned JSON pattern with `fhir_resources` table + index tables.
- `src-tauri/src/db/migrations.rs` — 14 migrations complete. M003 adds migrations 15+ for PT-specific index tables (pt_note_index, outcome_score_index, auth_tracking_index, fax_log_index, etc.).
- `src-tauri/src/lib.rs` — invoke_handler macro currently at 88 commands. New commands appended following existing pattern.
- `src/lib/tauri.ts` — Flat `commands` object. New wrappers appended following existing naming convention.
- `src-tauri/Cargo.toml` — Add: `objc2-local-authentication`, `printpdf`, `whisper-rs`, `cpal`, `reqwest` (multipart feature).

> See `.gsd/DECISIONS.md` for all architectural and pattern decisions — it is an append-only register; read it during planning, append to it during execution.

## Relevant Requirements

- AUTH-04-FIX — Real Touch ID via LAContext, not stub (S01)
- PT-DOC-01 through PT-DOC-04 — PT note types (S01)
- PT-OBJ-01 through PT-OBJ-04 — Objective measures and outcome scores (S02)
- PT-AI-01 through PT-AI-03 — AI voice-to-note pipeline (S03)
- PT-DOC-CTR-01 through PT-DOC-CTR-03 — Document centre and referral tracking (S04)
- PT-EXP-01 through PT-EXP-02 — PDF export (S05)
- PT-FAX-01 through PT-FAX-03 — Phaxio fax integration (S06)
- PT-AUTH-01 through PT-AUTH-03 — Auth tracking (S07)

## Scope

### In Scope

- Touch ID real implementation via objc2-local-authentication
- PT note types: Initial Evaluation, Daily Progress Note (SOAP-PT), Discharge Summary
- PT note co-sign/lock/addendum workflow
- ROM, MMT, special orthopedic tests, standardised outcome measures (LEFS, DASH, NDI, Oswestry, PSFS, FABQ)
- AI voice-to-note: cpal audio capture → whisper.cpp transcription → Ollama LLaMA 3.1 8B note generation
- AWS Bedrock Claude Haiku fallback
- PT-specific document vault with 8 categories
- Intake survey builder with kiosk mode
- Referral tracking
- PDF export (5 report types) via printpdf
- Phaxio fax send/receive/log
- Insurance authorisation records and visit tracking with warning banners
- Re-auth request letter generation

### Out of Scope / Non-Goals

- Electronic claims (837P) — M004
- ERA/remittance — M004
- Patient portal — M005
- HEP builder — M004
- Group/telehealth visits — M004
- Medicare G-code reporting — M004

## Technical Constraints

- All new Tauri commands must follow existing pattern: `State<'_, Database>` + `State<'_, SessionManager>` + RBAC middleware check + audit log write on every ePHI-touching operation
- Migrations are append-only; never modify existing migrations
- `tsc --noEmit` must exit 0 after each slice
- `cargo test --lib` must pass after each slice
- All PHI-touching commands write audit rows (success and failure paths)
- Audio is never stored — deleted after transcription completes
- Phaxio API credentials stored in SQLCipher, never in env vars or plaintext

## Integration Points

- `objc2-local-authentication` — macOS LAContext for Touch ID (S01)
- `whisper.cpp` via `whisper-rs` — Local speech-to-text (S03); provider installs via setup wizard
- `Ollama` at `localhost:11434` — Local LLM for note generation (S03); provider installs separately
- `AWS Bedrock` — Claude Haiku fallback via `reqwest` HTTPS (S03)
- `cpal` — Cross-platform audio capture (S03); macOS microphone entitlement required
- `printpdf` — Programmatic PDF generation (S05)
- `Phaxio API` — `POST /v2/faxes` send, `GET /v2/faxes` poll (S06)
- Existing `fhir_resources` table — All PT clinical data stored as FHIR R4 JSON, same as M001
- Existing `audit_logs` table — All ePHI access logged via `write_audit_entry`

## Open Questions

- Ollama model selection UI — app should detect available models and suggest LLaMA 3.1 8B; if not present, offer download instructions. Decided: startup check shows banner if Ollama not running; model selection in Settings.
- Phaxio practice fax number provisioning — handled in Settings setup wizard; user enters their Phaxio-provisioned number. Decided: first-time config wizard in SettingsPage > Fax tab.
- whisper.cpp model download — model stored in app support directory; downloaded on first AI use with progress indicator. Decided: setup wizard in SettingsPage > AI tab.

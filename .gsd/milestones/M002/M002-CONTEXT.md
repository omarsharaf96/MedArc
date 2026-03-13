# M002: MedArc Phase 2 Frontend — Context

**Gathered:** 2026-03-11
**Status:** Ready for planning

## Project Description

MedArc is a HIPAA-compliant AI-native desktop EMR for solo practitioners and small clinics, built with Tauri 2.x (Rust backend) + React 18/TypeScript (frontend) + SQLCipher-encrypted local database.

M001 delivered a complete, tested Rust backend — 265 unit tests, 60+ Tauri commands covering patients, scheduling, clinical documentation, labs, documents, and backup. The auth/RBAC/session/audit infrastructure is fully wired. **No practitioner-facing React UI exists yet.** The app currently shows a DatabaseStatus card, a FHIR resource explorer, and an audit log — all internal developer scaffolding.

## Why This Milestone

M001 proved the backend works. M002 makes it usable. A practitioner opening MedArc today sees no patients, no schedule, no notes — nothing clinical. M002 builds the full React UI layer on top of the existing Tauri API surface, making MedArc a real working desktop EMR that a solo practitioner can use for daily patient care.

This milestone is also the gating step for any AI feature work: AI-generated SOAP notes, voice transcription, and ambient documentation all require a functional clinical UI to surface their output.

## User-Visible Outcome

### When this milestone is complete, the user can:

- Create a patient record with demographics, insurance, and care team — and search/find it instantly by name or MRN
- View and navigate a multi-provider appointment calendar (day/week/month), create appointments, and track the patient flow board in real time
- Open a patient's chart and write a structured SOAP note with vitals, Review of Systems, and physical exam using built-in templates
- View the patient's problem list, medication list, allergy list, and immunization history — and add/update items
- Enter or upload lab results, view flagged abnormals, and sign off on results
- Upload documents (PDFs, images) and browse them per patient
- Initiate an encrypted backup and view backup history from a Settings panel
- All UI enforces RBAC — front desk staff see scheduling, providers see clinical notes, billing staff see what they should

### Entry point / environment

- Entry point: `npm run tauri dev` (macOS desktop app, Tauri 2.x WebView)
- Environment: local macOS development, no internet required
- Live dependencies: SQLCipher database (existing), Tauri Rust commands (all wired from M001)

## Completion Class

- Contract complete means: TypeScript compiles with 0 errors (`tsc --noEmit`); all new React components render without console errors
- Integration complete means: each UI workflow exercises the real Tauri command (not mocked); data persists across navigation
- Operational complete means: practitioner can complete a full patient visit workflow end-to-end in the running desktop app (log in → find patient → create/update encounter note → record vitals → add medications → schedule follow-up)

## Final Integrated Acceptance

To call this milestone complete, we must prove:

- A new user can create an account, log in, create a patient, complete a clinical encounter (SOAP note + vitals), schedule a follow-up, and log out — all through the UI in the running Tauri app
- The Patient Flow Board shows appointments for today and status transitions work in real time
- RBAC is enforced in the UI: a FrontDesk user cannot access clinical notes or the chart; a Provider sees all clinical features
- TypeScript builds clean (`tsc --noEmit` exits 0); `cargo test --lib` still passes 265 tests

## Risks and Unknowns

- **Date/time handling in calendar UI** — M001's scheduling backend stores datetimes without timezone suffix; the calendar UI must respect this convention consistently or produce subtle bugs in slot display
- **Tauri file dialog for document upload** — requires `tauri-plugin-dialog` for a native file picker; App Sandbox entitlements already cover user-selected file access; plugin must be added
- **React component complexity** — SOAP note editor, ROS form (14 systems), and physical exam form (13 systems) are the most complex UI surfaces; they must be usable, not just functional
- **Duplicate `* 2.rs` files in commands/** — must be cleaned up before Phase 2 development begins to avoid import confusion

## Existing Codebase / Prior Art

- `src/App.tsx` — current authenticated shell; needs to be replaced with a nav-routed layout
- `src/lib/tauri.ts` — typed invoke wrappers for auth/audit/FHIR; M002 must add wrappers for all 60+ M001 commands (patient, clinical, scheduling, documentation, labs, backup)
- `src/types/auth.ts`, `src/types/fhir.ts`, `src/types/audit.ts` — existing type definitions; extend, don't replace
- `src/hooks/useAuth.ts` — auth lifecycle hook; reuse as-is
- `src/hooks/useIdleTimer.ts` — idle lock hook; reuse as-is
- `src/components/auth/` — LoginForm, RegisterForm, LockScreen, MfaSetup, MfaPrompt all exist and work
- `src-tauri/src/commands/patient.rs` — PatientInput, PatientRecord, PatientSummary Rust types define the API contract
- `src-tauri/src/commands/scheduling.rs` — AppointmentInput, AppointmentRecord, FlowBoardEntry types define scheduling API
- `src-tauri/src/commands/documentation.rs` — EncounterInput, VitalsInput, RosInput, PhysicalExamInput types define clinical doc API
- `src-tauri/src/commands/labs.rs` — LabOrderInput, LabResultInput, DocumentInput define lab/doc API
- `.gsd/DECISIONS.md` — all architectural decisions from M001; read before any structural decisions in M002

## Relevant Requirements

- UI-01 (new) — Patient list, search, and detail view
- UI-02 (new) — Appointment calendar and patient flow board
- UI-03 (new) — Clinical encounter workspace (SOAP + vitals + ROS + PE)
- UI-04 (new) — Clinical data sidebar (problems, medications, allergies, immunizations)
- UI-05 (new) — Lab results and document management views
- UI-06 (new) — Settings panel (backup, MFA, user account)
- UI-07 (new) — RBAC-enforced navigation and role-appropriate feature access

## Scope

### In Scope

- Full navigation shell (sidebar + routing) replacing current monolithic App.tsx
- Patient module: list, search, create, view, edit
- Scheduling module: calendar view, appointment creation, flow board, waitlist, recall board
- Clinical documentation: encounter workspace, SOAP note editor, vitals flowsheet, ROS form, physical exam form, template picker
- Clinical sidebar: problem list, medication list, allergy list, immunization history (view and add)
- Drug-allergy alert surface (CDS)
- Lab results: order list, result entry, abnormal flagging, document upload/browser
- Settings: backup panel, MFA setup, session/account info
- TypeScript types for all M001 Rust command inputs/outputs
- Tauri invoke wrappers for all 60+ M001 commands
- RBAC-gated navigation (role determines which nav items are visible and which views are accessible)

### Out of Scope / Non-Goals

- AI features (voice transcription, SOAP generation, ambient documentation) — M003
- E-prescribing, HL7 integration, billing — M004+
- Co-sign workflow UI (backend exists; UI deferred as low-priority for solo practitioners)
- MFA setup UI (exists from S02; no changes needed)
- Audit log UI (exists from S03; no changes needed)
- Mobile or Windows/Linux UI adaptation

## Technical Constraints

- React 18 + TypeScript strict mode — no `any`, no suppressed type errors
- Tailwind CSS only — no CSS modules, no styled-components (established in M001)
- `tsc --noEmit` must exit 0 (zero TypeScript errors)
- `cargo test --lib` must continue to pass 265 tests throughout M002
- No new Rust commands or DB schema changes — M002 is purely frontend; if a Rust change is needed it requires explicit justification
- Tauri invoke wrappers must use exact Rust parameter names (snake_case) as established in `src/lib/tauri.ts`
- All UI components must call real Tauri commands, not mock data
- `tauri-plugin-dialog` required for native file picker (document upload) — only dependency addition allowed

## Integration Points

- Tauri IPC — all data flows through `commands` object in `src/lib/tauri.ts`; no direct fetch calls
- SQLCipher database — read/write via Rust commands only; frontend never touches the DB directly
- macOS Keychain — transparent (via existing auth and backup commands)
- macOS file system — document upload via `tauri-plugin-dialog` native file picker

## Open Questions

- **Router choice** — React Router vs TanStack Router vs manual state routing. Given the app is a Tauri desktop app (not a browser), a simple state-based router (no URL bar) may be cleaner than react-router. Decision needed in S01.
- **Date library** — scheduling UI needs reliable date arithmetic for the calendar. The M001 Rust backend uses no-timezone-suffix strings. Should the frontend use `date-fns` (small, tree-shakeable) or the native `Temporal` API (zero deps, modern)? Decision needed in S02.
- **Rich text for SOAP notes** — plain `<textarea>` vs a lightweight rich-text editor (e.g. Tiptap). Given the MVP scope, `<textarea>` with section tabs is likely sufficient. Decision needed in S03.

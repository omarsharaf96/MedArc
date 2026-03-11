# M002: MedArc Phase 2 Frontend

**Vision:** Transform MedArc from a proven backend with developer scaffolding into a fully usable clinical desktop application — a practitioner can log in, manage patients, run a complete clinical encounter, schedule appointments, view labs, and manage documents entirely through a polished React UI, with RBAC enforced throughout.

## Success Criteria

- A practitioner can complete a full patient visit workflow end-to-end: log in → find/create patient → write a SOAP note with vitals → add medications/allergies → schedule a follow-up → log out
- The appointment calendar shows the day/week view with live appointments; the Patient Flow Board shows today's clinic status and allows real-time status transitions
- RBAC is enforced in the UI: FrontDesk users see scheduling but not clinical charts; Providers see everything; BillingStaff see read-only views
- `tsc --noEmit` exits 0 (zero TypeScript errors) and `cargo test --lib` continues to pass 265+ tests
- The app is navigable entirely by keyboard and mouse — no dead-end states, no blank screens after data operations

## Key Risks / Unknowns

- **Router / navigation architecture** — wrong choice here forces a rewrite in S02; must be decided and proven in S01
- **SOAP note UX** — four-section free-text notes with template pre-population are the highest-complexity UI surface; must be prototyped early to find usability problems before downstream slices depend on encounter IDs
- **Tauri file dialog integration** — `tauri-plugin-dialog` requires entitlements already in place (they are); plugin must be wired and tested with the existing App Sandbox config before document upload is claimed working
- **Duplicate `* 2.rs` command files** — must be cleaned up before any M002 Rust touch points to prevent import confusion

## Proof Strategy

- **Router/nav** → retire in S01 by building the full navigation shell with real auth-gated routing and verifying in the running Tauri app
- **SOAP note UX** → retire in S03 by building the encounter workspace with template pre-population and verifying a complete note can be written and saved
- **File dialog** → retire in S05 by wiring `tauri-plugin-dialog` for document upload and verifying file selection, upload, and list display in the running app

## Verification Classes

- Contract verification: `tsc --noEmit` exits 0 after each slice; `cargo test --lib` passes 265+ tests throughout
- Integration verification: each slice's primary workflow exercised in the running Tauri app (`npm run tauri dev`)
- Operational verification: RBAC-gated navigation verified with Provider and FrontDesk user accounts
- UAT / human verification: end-to-end patient visit walkthrough in the running app at milestone completion

## Milestone Definition of Done

This milestone is complete only when all are true:

- All 7 slices are `[x]` and all slice summaries exist
- `tsc --noEmit` exits 0 with zero TypeScript errors
- `cargo test --lib` passes 265+ tests (no regressions)
- The full patient visit workflow (log in → patient → encounter → schedule → log out) is demoable in the live Tauri app
- RBAC navigation enforcement verified: Provider sees clinical features; FrontDesk does not
- No blank screens, unhandled errors, or dead-end navigation states in the primary workflows
- The `* 2.rs` and `* 2.tsx` duplicate files have been cleaned up

## Requirement Coverage

- Covers: UI-01, UI-02, UI-03, UI-04, UI-05, UI-06, UI-07
- Partially covers: CLIN-08 (growth charts deferred — vitals display only), BKUP-04 (backup UI ships; automation deferred)
- Leaves for later: e-prescribing, HL7 lab integration, billing, AI features
- Orphan risks: none — all M001 backend APIs have corresponding UI slices in this milestone

## Slices

- [ ] **S01: Navigation Shell & Type System** `risk:high` `depends:[]`
  > After this: The app has a full navigation sidebar (Patients, Schedule, Settings), state-based routing, RBAC-gated nav items, and a complete TypeScript type + invoke wrapper layer for all 60+ M001 commands — verified by `tsc --noEmit` and confirmed working in the Tauri app.

- [ ] **S02: Patient Module** `risk:medium` `depends:[S01]`
  > After this: A practitioner can create a patient with demographics, insurance, and care team assignments; search by name or MRN; view a patient detail page with all demographics; and edit existing records — all through the UI, backed by the real Tauri commands.

- [ ] **S03: Clinical Encounter Workspace** `risk:high` `depends:[S02]`
  > After this: A provider can open a patient's chart, start a clinical encounter, write a structured SOAP note using a built-in template, record vitals (BP, HR, Temp, SpO2, Weight, Height, BMI auto-displayed, pain score), complete a 14-system ROS form, and save the encounter — all wired to the real Tauri commands.

- [ ] **S04: Clinical Data Sidebar** `risk:medium` `depends:[S02]`
  > After this: A provider can view and manage a patient's problem list (ICD-10 coded), medication list (RxNorm coded), allergy list, and immunization history from the patient chart sidebar — with add/update/status-change flows and passive drug-allergy alerts surfaced when medications and allergies overlap.

- [ ] **S05: Scheduling & Flow Board** `risk:medium` `depends:[S02]`
  > After this: A provider or front desk user can view the appointment calendar in day and week views, create and cancel appointments (including recurring series), search for open slots, and manage the real-time Patient Flow Board — all exercised in the running Tauri app.

- [ ] **S06: Labs, Documents & Physical Exam** `risk:low` `depends:[S03,S04]`
  > After this: A provider can enter lab orders and results (with abnormal flags highlighted), sign lab results, upload PDF/image documents with category assignment, browse a patient's document history, and access the 13-system physical exam form within the encounter workspace.

- [ ] **S07: Settings, Cleanup & End-to-End Verification** `risk:low` `depends:[S01,S02,S03,S04,S05,S06]`
  > After this: The Settings panel provides backup management (create/list/restore), MFA setup, and account info; all duplicate `* 2.rs`/`* 2.tsx` files are removed; `tsc --noEmit` exits 0; the complete patient visit workflow is verified end-to-end in the live Tauri app with Provider and FrontDesk accounts.

## Boundary Map

### S01 → S02

Produces:
- `AppShell` component with `<Sidebar>` + `<ContentArea>` layout
- State-based router with routes: `/patients`, `/patients/:id`, `/schedule`, `/settings`
- `useNav()` hook for programmatic navigation
- Complete TypeScript types in `src/types/` for all M001 Rust command outputs (PatientRecord, PatientSummary, AppointmentRecord, EncounterRecord, VitalsRecord, LabResultRecord, DocumentRecord, etc.)
- Complete `commands` object in `src/lib/tauri.ts` with invoke wrappers for all 60+ M001 commands
- RBAC-gated nav: FrontDesk → Schedule only; Provider → Patients + Schedule + Labs; SystemAdmin → all

Consumes:
- `useAuth()` hook (existing)
- Auth/session/audit Tauri wrappers (existing in `src/lib/tauri.ts`)

### S02 → S03 and S04

Produces:
- `PatientListPage` — searchable, paginated patient roster
- `PatientDetailPage` — demographics, insurance, care team; acts as the chart shell that S03/S04 hang off
- `PatientFormModal` — create/edit patient with all fields
- `usePatient(id)` hook — loads patient record, exposes update functions

Consumes:
- Navigation shell from S01
- `commands.createPatient`, `commands.getPatient`, `commands.updatePatient`, `commands.searchPatients`, `commands.deletePatient` invoke wrappers

### S03 → S06

Produces:
- `EncounterWorkspace` — tabbed SOAP editor with template picker, vitals panel, and ROS form
- `useEncounter(patientId)` hook — creates/loads encounter, saves sections
- Encounter ID available for S06 (lab orders, cosign) to reference

Consumes:
- `PatientDetailPage` shell from S02
- `commands.createEncounter`, `commands.recordVitals`, `commands.saveRos`, `commands.listTemplates`, `commands.getTemplate` invoke wrappers

### S04 → S06

Produces:
- `ClinicalSidebar` — tabbed panel (Problems | Medications | Allergies | Immunizations)
- `useClinicalData(patientId)` hook — loads all four lists, exposes add/update functions
- `DrugAllergyAlertBanner` — reads alerts from `commands.checkDrugAllergyAlerts`

Consumes:
- `PatientDetailPage` shell from S02
- All `commands.addProblem/addMedication/addAllergy/addImmunization` wrappers

### S05 → S07

Produces:
- `CalendarPage` — day/week calendar grid with `<AppointmentCard>` components
- `FlowBoardPage` — real-time clinic status board
- `AppointmentFormModal` — create/edit with recurrence options

Consumes:
- Navigation shell from S01
- `commands.listAppointments`, `commands.createAppointment`, `commands.updateFlowStatus`, `commands.getFlowBoard` wrappers

### S06 → S07

Produces:
- `LabResultsPanel` — order list + result entry + sign-off
- `DocumentBrowser` — upload + list + verify
- `PhysicalExamForm` — 13-system exam within EncounterWorkspace

Consumes:
- EncounterWorkspace from S03, ClinicalSidebar from S04
- All `commands.enterLabResult`, `commands.signLabResult`, `commands.uploadDocument`, `commands.listDocuments` wrappers
- `tauri-plugin-dialog` for native file picker

# S06: Labs, Documents & Physical Exam

**Goal:** Add three independent UI surfaces to the existing app: a Physical Exam tab in EncounterWorkspace, a Lab Results Panel on PatientDetailPage, and a Document Browser (with native file picker) on PatientDetailPage. All backend commands exist; this slice is purely additive frontend work plus one Rust plugin installation (tauri-plugin-dialog + tauri-plugin-fs).

**Demo:** A provider opens a patient chart → opens an encounter → switches to the "Exam" tab and fills in free-text findings for multiple body systems → saves. From the patient chart, the provider views the lab orders and results panel (abnormal rows highlighted in amber), enters a result, and signs it. The provider clicks "Upload Document", a native macOS file picker opens, they select a PDF, it appears in the document list with title and category.

## Must-Haves

- `PhysicalExamTab` is the fourth tab in `EncounterWorkspace` with all 13 body-system textareas + Additional Notes; finalized encounters show read-only text
- `useEncounter` hook exposes `physicalExamRecord: PhysicalExamRecord | null` and `savePhysicalExam(input)` (same pattern as `saveRos`)
- `extractPhysicalExamDisplay()`, `extractLabResultDisplay()`, `extractLabOrderDisplay()`, `extractDocumentDisplay()` added to `fhirExtract.ts`
- `LabResultsPanel` lists lab orders and results for a patient; abnormal rows use amber highlight; "Enter Result" modal; "Sign" button gated to Provider/SystemAdmin; panel hidden from FrontDesk
- `DocumentBrowser` lists documents with category filter; "Upload" button opens native NSOpenPanel via `@tauri-apps/plugin-dialog`; file bytes read via `@tauri-apps/plugin-fs`, base64-encoded in chunks, then `uploadDocument` called; all roles with PatientDocuments Read can see documents
- `tauri-plugin-dialog` and `tauri-plugin-fs` installed in Cargo.toml, package.json, and registered in lib.rs
- `tsc --noEmit` exits 0 after every task
- No `any` types; all optional fields `T | null`

## Proof Level

- This slice proves: integration (UI wired to real Tauri commands; file picker exercises native macOS NSOpenPanel)
- Real runtime required: yes — file picker path must be verified in the running Tauri dev app for T03
- Human/UAT required: no (T01 and T02 verified by `tsc --noEmit` + visual inspection in dev app; T03 additionally requires the running Tauri app for file picker)

## Verification

- `npx tsc --noEmit` exits 0 after every task (zero TypeScript errors)
- After T01: EncounterWorkspace renders four tabs (SOAP / Vitals / ROS / Exam); Physical Exam tab shows 13-system form in dev app
- After T02: PatientDetailPage shows Lab Results panel (for Provider role) with order list, result entry modal, sign button; `tsc --noEmit` exits 0
- After T03: `tsc --noEmit` exits 0; Document Browser renders in dev app; `npm run tauri dev` — Upload button triggers native NSOpenPanel, selected PDF appears in document list

## Observability / Diagnostics

- Runtime signals: `console.error("[useEncounter] fetchAll failed …")` already emitted by hook on fetch failure; `console.error("[LabResultsPanel] …")` and `console.error("[DocumentBrowser] …")` added to per-domain error states following the `ClinicalSidebar` pattern
- Inspection surfaces: React component error state rendered inline with a red error message (same pattern as `ClinicalSidebar`); TS compiler as primary static gate
- Failure visibility: per-domain error strings exposed in component state; hook logs the failed encounter ID and error message
- Redaction constraints: document content_base64 must never be logged; file paths from the picker are transient and should not be persisted or logged

## Integration Closure

- Upstream surfaces consumed:
  - `src/hooks/useEncounter.ts` — extended with `physicalExamRecord` + `savePhysicalExam`
  - `src/pages/EncounterWorkspace.tsx` — `ActiveTab` union extended; new tab + sub-component added
  - `src/pages/PatientDetailPage.tsx` — two new `<SectionCard>` entries added
  - `src/lib/fhirExtract.ts` — four new pure extraction helpers appended
  - `src/lib/tauri.ts` — already has all wrappers; no changes needed
  - `src-tauri/Cargo.toml`, `package.json`, `src-tauri/src/lib.rs` — plugin installation (T03)
- New wiring introduced in this slice:
  - Physical Exam tab wired to `useEncounter.savePhysicalExam` → `commands.savePhysicalExam`
  - LabResultsPanel wired to `commands.listLabOrders`, `commands.listLabResults`, `commands.enterLabResult`, `commands.signLabResult`
  - DocumentBrowser wired to `@tauri-apps/plugin-dialog` open() → `@tauri-apps/plugin-fs` readFile() → base64 encode → `commands.uploadDocument` + `commands.listDocuments`
- What remains before the milestone is truly usable end-to-end: S07 (Settings panel, backup UI, duplicate file cleanup, final E2E verification)

## Tasks

- [x] **T01: Add Physical Exam tab to EncounterWorkspace** `est:1h`
  - Why: CLIN-04 requirement — provider can document 13-system physical exam findings within the encounter workspace; closes the last missing EncounterWorkspace tab
  - Files: `src/hooks/useEncounter.ts`, `src/pages/EncounterWorkspace.tsx`, `src/lib/fhirExtract.ts`
  - Do: (1) Append `extractPhysicalExamDisplay()` to `fhirExtract.ts` — extracts 13 system fields from FHIR ClinicalImpression `finding[].itemCodeableConcept` by system code, returns all-null struct on null/undefined input, never throws. (2) Extend `UseEncounterReturn` in `useEncounter.ts`: add `physicalExamRecord: PhysicalExamRecord | null` and `savePhysicalExam: (input: PhysicalExamInput) => Promise<void>`. Add a `seededPhysicalExamId` state guard (same pattern as `soapSeededForId`). Add `commands.getPhysicalExam(encounterId, patientId)` to the `Promise.all` in `fetchAll`. Implement `savePhysicalExam`: call `commands.savePhysicalExam(input)` then `reload()`. (3) In `EncounterWorkspace.tsx`: extend `ActiveTab` to `"soap" | "vitals" | "ros" | "exam"`. Add the "Exam" tab button in the tab bar array. Add `PhysicalExamTab` inline sub-component: 13-system textarea grid (general, heent, neck, cardiovascular, pulmonary, abdomen, extremities, neurological, skin, psychiatric, musculoskeletal, genitourinary, rectal) + Additional Notes textarea + Save button; `isReadOnly = isFinalized` guard; saves via `savePhysicalExam`. Wire `physicalExamRecord` from `useEncounter` to seed initial form state with a `seededPhysicalExamId`-style guard.
  - Verify: `npx tsc --noEmit` exits 0; EncounterWorkspace renders four tabs in dev app
  - Done when: `tsc --noEmit` exits 0 and the "Exam" tab is visible and functional in the running dev app

- [x] **T02: Build Lab Results Panel on PatientDetailPage** `est:1h`
  - Why: LABS-01, LABS-03, LABS-04 requirements — provider can view lab orders and results, enter results, and sign them from the patient chart
  - Files: `src/lib/fhirExtract.ts`, `src/components/clinical/LabResultsPanel.tsx`, `src/pages/PatientDetailPage.tsx`
  - Do: (1) Append `extractLabOrderDisplay()` and `extractLabResultDisplay()` to `fhirExtract.ts` — pure functions, never throw, return all-null structs on null input. `extractLabResultDisplay` reads `loincCode`, `status`, `hasAbnormal`, `lastUpdated` from LabResultRecord. (2) Build `LabResultsPanel` component in `src/components/clinical/LabResultsPanel.tsx`: accept `patientId: string`, `userId: string`, `role: string` props; fetch `listLabOrders(patientId)` and `listLabResults(patientId)` in parallel on mount; show two sub-sections ("Orders" and "Results"); abnormal result rows get `bg-amber-50 border-amber-200` styling based on `record.hasAbnormal`; "Enter Result" button opens an inline modal with fields: loincCode, displayName, status (select: preliminary/final), observations (at least one: loincCode + displayName + valueString); calls `commands.enterLabResult(input)` with `observations: []` acceptable; reloads after submit. "Sign" button per result row: gated to `role === "Provider" || role === "SystemAdmin"`; calls `commands.signLabResult({ resultId, providerId: userId, comment: null })`; reloads. Per-domain error isolation: independent try/catch for orders fetch and results fetch; errors shown inline. (3) Add `LabResultsPanel` to `PatientDetailPage` as a new `<SectionCard title="Lab Results">` below "Clinical Data", rendered only when `role !== "FrontDesk"`.
  - Verify: `npx tsc --noEmit` exits 0; Lab Results section visible for Provider role in dev app (patient detail page)
  - Done when: `tsc --noEmit` exits 0; LabResultsPanel renders with orders and results sub-sections; sign button gated correctly by role

- [x] **T03: Install tauri-plugin-dialog/fs and build Document Browser** `est:1.5h`
  - Why: DOCS-01, DOCS-03 requirements — provider can upload PDF/image documents with native file picker and browse patient document history; also retires the tauri-plugin-dialog integration risk flagged in the M002 proof strategy
  - Files: `src-tauri/Cargo.toml`, `package.json`, `src-tauri/src/lib.rs`, `src/lib/fhirExtract.ts`, `src/components/clinical/DocumentBrowser.tsx`, `src/pages/PatientDetailPage.tsx`
  - Do: (1) Install plugins: run `cargo add tauri-plugin-dialog --manifest-path src-tauri/Cargo.toml` and `cargo add tauri-plugin-fs --manifest-path src-tauri/Cargo.toml`; run `npm install @tauri-apps/plugin-dialog @tauri-apps/plugin-fs`; add `.plugin(tauri_plugin_dialog::init()).plugin(tauri_plugin_fs::init())` to the builder chain in `lib.rs` before `.invoke_handler(...)`. Verify major version is "2" in both Cargo.toml and package.json. (2) Append `extractDocumentDisplay()` to `fhirExtract.ts` — extracts `title`, `category`, `contentType`, `fileSizeBytes`, `uploadedAt` from `DocumentRecord`; pure, never throws. (3) Build `DocumentBrowser` in `src/components/clinical/DocumentBrowser.tsx`: accept `patientId: string`, `userId: string` props; fetch `listDocuments(patientId)` on mount; render category filter `<select>` (options: all / clinical-note / imaging / lab-report / consent / referral / other); render document rows with title, category, content type, file size (human-readable), upload date. "Upload Document" button: call `open({ multiple: false, filters: [{ name: "Documents", extensions: ["pdf", "jpg", "jpeg", "png"] }] })` from `@tauri-apps/plugin-dialog`; if path returned, call `readFile(path)` from `@tauri-apps/plugin-fs` to get `Uint8Array`; check `bytes.length > 64 * 1024 * 1024` and show error if too large; base64-encode in 8 KB chunks using `btoa` (avoids stack overflow on large files); detect MIME type from file extension; open upload modal for title + category input; call `commands.uploadDocument({ patientId, title, category, contentType, contentBase64, fileSizeBytes: bytes.length, uploadedBy: userId })`; reload list. Inline error state for upload failures. (4) Add `DocumentBrowser` to `PatientDetailPage` as a new `<SectionCard title="Documents">` below "Lab Results", visible to all roles (Provider, NurseMa, SystemAdmin, BillingStaff, FrontDesk all have PatientDocuments Read). (5) Verify in running Tauri dev app (`npm run tauri dev`): file picker opens native NSOpenPanel, selected PDF uploads, appears in document list.
  - Verify: `npx tsc --noEmit` exits 0; in `npm run tauri dev` — Upload button opens native file picker, document appears in list after upload
  - Done when: `tsc --noEmit` exits 0; native file picker opens in running Tauri app; uploaded document appears in list with correct title/category

## Files Likely Touched

- `src/hooks/useEncounter.ts`
- `src/pages/EncounterWorkspace.tsx`
- `src/lib/fhirExtract.ts`
- `src/components/clinical/LabResultsPanel.tsx` (new)
- `src/components/clinical/DocumentBrowser.tsx` (new)
- `src/pages/PatientDetailPage.tsx`
- `src-tauri/Cargo.toml`
- `src-tauri/src/lib.rs`
- `package.json`

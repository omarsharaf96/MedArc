# S06: Labs, Documents & Physical Exam — Research

**Date:** 2026-03-12
**Milestone:** M002

## Summary

S06 adds three independent UI surfaces to the existing `EncounterWorkspace` and `PatientDetailPage` shells: a **Lab Results Panel** (orders + results + sign-off), a **Document Browser** (upload via native file picker + list), and a **Physical Exam Form** (13-system form as a new tab in `EncounterWorkspace`). All backend commands are fully implemented (M001/S08), all TypeScript wrappers exist in `src/lib/tauri.ts`, and all types exist in `src/types/labs.ts` and `src/types/documentation.ts`. This slice is purely additive frontend work.

The highest-complexity implementation concern is `tauri-plugin-dialog` for the native macOS file picker. The plugin is **not yet installed** — it is missing from both `Cargo.toml` (Rust) and `package.json` (JS). Adding it requires one `Cargo.toml` change, one `package.json` change, and one `lib.rs` plugin registration — all low-risk. The entitlement `com.apple.security.files.user-selected.read-write` is already in `entitlements.plist` from S09 (backup), so no entitlement work is needed.

The Physical Exam Form integrates into `EncounterWorkspace` as a fourth tab ("Exam") alongside SOAP, Vitals, and ROS. The FHIR extraction pattern (`extractSoapSections`, `extractAllergyDisplay`, etc.) established in `fhirExtract.ts` is the authoritative template. The lab results panel and document browser are patient-scoped panels that live on `PatientDetailPage` (parallel to `ClinicalSidebar`), not inside `EncounterWorkspace`.

`tsc --noEmit` currently exits 0 with zero errors. S06 must maintain this gate throughout.

## Recommendation

**Decompose into 3 tasks:**

- **T01 — Physical Exam tab in EncounterWorkspace** (self-contained; touches only `EncounterWorkspace.tsx` and `useEncounter.ts`; no new dependencies required)
- **T02 — Lab Results Panel on PatientDetailPage** (reads `listLabOrders`, `listLabResults`, `enterLabResult`, `signLabResult`; no new dependencies)
- **T03 — Document Browser with tauri-plugin-dialog** (installs plugin, implements `DocumentBrowser` component with native file picker, `uploadDocument`, `listDocuments`)

Order: T01 → T02 → T03. T01 and T02 have zero dependency on each other but T03 is ordered last because it adds the only new dependency (tauri-plugin-dialog) which slightly elevates integration risk.

**No new Rust commands needed.** The constraint "no new Rust commands or DB schema changes in M002" is satisfied — all 10 lab/document commands and the `save_physical_exam` / `get_physical_exam` commands already exist and are registered in `lib.rs`.

## Don't Hand-Roll

| Problem | Existing Solution | Why Use It |
|---------|------------------|------------|
| Physical exam FHIR extraction | Add `extractPhysicalExamDisplay()` to `fhirExtract.ts` | All other clinical FHIR extraction helpers live there; consistent pattern; pure functions easy to call |
| Lab result FHIR extraction | Add `extractLabResultDisplay()` / `extractDocumentDisplay()` to `fhirExtract.ts` | Same reason |
| Physical exam load/save | Extend `useEncounter` hook with `physicalExamRecord` + `savePhysicalExam` | Same pattern as `rosRecord` / `saveRos` added in S03/T04; hook already runs `Promise.all` on mount |
| File reading for upload | `@tauri-apps/plugin-dialog` `open()` + `@tauri-apps/plugin-fs` `readFile()` | Native picker → avoids `<input type="file">` limitations in WKWebView; already entitled |
| Base64 encoding for upload | `Array.from(bytes).map(b => String.fromCharCode(b)).join('')` → `btoa()` | Browser API; no dependency needed |
| Tab navigation (Physical Exam) | Extend existing `ActiveTab` union in `EncounterWorkspace.tsx` | Four-tab pattern already there; just add `"exam"` variant |
| Abnormal flag highlighting | Read `LabResultRecord.hasAbnormal` boolean | Already computed server-side; stored in `lab_result_index.has_abnormal`; no client parsing |

## Existing Code and Patterns

- `src/pages/EncounterWorkspace.tsx` — four-tab shell with `SOAP | Vitals | ROS` already working. Add `"exam"` as a fourth tab by extending the `ActiveTab` union and the tab-bar array. `PhysicalExamTab` follows the same `isReadOnly = isFinalized` guard as `VitalsTab` and `RosTab`.
- `src/hooks/useEncounter.ts` — loads encounter, vitals, templates, ROS in `Promise.all`. Add `getPhysicalExam(encounterId, patientId)` to this same parallel fetch; expose `physicalExamRecord: PhysicalExamRecord | null` and `savePhysicalExam(input)` — exact same shape as `rosRecord` / `saveRos`.
- `src/lib/fhirExtract.ts` — append extraction helpers for lab results, documents, and physical exam. Pure functions only, never throw, return all-null structs on null/undefined input. Pattern established by `extractAllergyDisplay`, `extractProblemDisplay`, etc.
- `src/types/documentation.ts` — `PhysicalExamInput`, `PhysicalExamRecord` fully typed. 13 system fields: `general`, `heent`, `neck`, `cardiovascular`, `pulmonary`, `abdomen`, `extremities`, `neurological`, `skin`, `psychiatric`, `musculoskeletal`, `genitourinary`, `rectal` + `additionalNotes`. All `string | null`.
- `src/types/labs.ts` — `LabOrderRecord`, `LabResultRecord`, `DocumentRecord`, `SignLabResultInput`, `DocumentUploadInput`, `IntegrityCheckResult` all fully typed. `LabResultRecord.hasAbnormal: boolean` is the flag for abnormal highlighting.
- `src/lib/tauri.ts` — all 10 lab/document commands (`listLabOrders`, `listLabResults`, `enterLabResult`, `signLabResult`, `uploadDocument`, `listDocuments`, `verifyDocumentIntegrity`, `addLabCatalogueEntry`, `listLabCatalogue`, `createLabOrder`) wired with correct Rust parameter names. Also `savePhysicalExam`, `getPhysicalExam` both wired.
- `src/pages/PatientDetailPage.tsx` — renders `ClinicalSidebar` inside a `<SectionCard>`. Lab results panel and document browser follow the same pattern: new `<SectionCard>` entries added below `ClinicalSidebar`, gated to `role !== "FrontDesk"` for labs, `true` for documents (FrontDesk can view documents per RBAC Read).
- `src/components/clinical/ClinicalSidebar.tsx` — reference for multi-tab component with per-domain error isolation, add/edit modals, and `useClinicalData` hook pattern.
- `src-tauri/src/commands/labs.rs` — `upload_document` takes `content_base64: String` (not a file path). The frontend must convert the file bytes → base64 string before calling the command. `file_size_bytes: i64` maps to `number` in TS.
- `src-tauri/src/lib.rs` — `tauri_plugin_updater` is already registered. The `tauri_plugin_dialog` registration will follow the same `.plugin(tauri_plugin_dialog::init())` pattern.
- `src-tauri/entitlements.plist` — `com.apple.security.files.user-selected.read-write: true` is already present (added for backup in S09). No entitlement change needed for file dialog.

## Constraints

- **No new Rust commands.** All 10 lab/document commands + `save_physical_exam` / `get_physical_exam` already exist. Rust touches are limited to `Cargo.toml` (add `tauri-plugin-dialog`) and `lib.rs` (register the plugin).
- **`tsc --noEmit` must exit 0 after every task.** Currently passes with zero errors; must not regress.
- **`cargo test --lib` must pass 265+ tests.** Not touched by S06 (no new Rust code).
- **No `any` types.** TypeScript strict mode enforced throughout.
- **Tailwind CSS only.** No CSS modules, no styled-components.
- **`T | null` for all optional fields, never `T | undefined`.** Backend `Option<T>` serializes as `T | null`.
- **`signLabResult` is Provider/SystemAdmin only** — backend enforces a two-layer check (RBAC Update + role-specific check). UI must gate the "Sign" button to `role === "Provider" || role === "SystemAdmin"`.
- **`upload_document` takes base64 content, not a file path.** The frontend file picker must: (1) open native dialog, (2) read the selected file as bytes, (3) base64-encode, (4) call `uploadDocument` with the encoded string.
- **`getPhysicalExam` requires BOTH `encounterId` AND `patientId`** — same constraint as `getRos`. The Rust handler takes two positional params. Never pass only `encounter_id`.
- **`listLabOrders` param key is `status_filter`, not `status`.** Already correct in `tauri.ts` — do not change it.
- **`listLabResults` param key is `status_filter`, not `status`.** Same.
- **Document upload MIME types:** backend accepts `application/pdf` and image types. UI should restrict the file picker to `*.pdf,image/*` and pass the correct MIME type.
- **64 MB file size limit.** Backend validates. UI should show a clear error if the user selects a file that is too large. Read file size before encoding to avoid wasteful base64 conversion of oversized files.
- **Lab result `hasAbnormal` boolean is server-computed.** Do not re-compute abnormal flags on the client. Read `LabResultRecord.hasAbnormal` directly for row highlighting.
- **Physical exam tab:** finalized encounters must be read-only (same guard as SOAP, Vitals, ROS: `isReadOnly = isFinalized`). NurseMa has CRU on ClinicalDocumentation, so NurseMa can fill the exam form.

## Common Pitfalls

- **`tauri-plugin-dialog` not yet installed** — both `Cargo.toml` and `package.json` are missing the dependency; `lib.rs` has no plugin registration. Installing requires: `cargo add tauri-plugin-dialog --manifest-path src-tauri/Cargo.toml`, `npm install @tauri-apps/plugin-dialog`, and adding `.plugin(tauri_plugin_dialog::init())` to `lib.rs` before `.invoke_handler(...)`.
- **`tauri-plugin-fs` also needed** — `@tauri-apps/plugin-dialog`'s `open()` returns a file *path*, not file *bytes*. Reading the bytes requires `@tauri-apps/plugin-fs` (`readFile()`). Without this, the upload pipeline cannot complete. Both plugins should be installed together. The `fs` plugin also needs entitlement `com.apple.security.files.user-selected.read-write` (already present).
- **Base64 encoding large files** — `btoa(String.fromCharCode(...bytes))` fails on files >~1MB because `String.fromCharCode.apply(null, bytes)` hits stack size limits. Use a chunked approach: split the `Uint8Array` into chunks, encode each, concatenate. Or use `Buffer.from(bytes).toString('base64')` (not available in WKWebView) → use the chunk approach with `btoa`.
- **`useEncounter` already has seeded-ID guards** — when adding `physicalExamRecord`, follow the same `seededPhysicalExamId` guard used for `soapSeededForId` and `seededRosId` / `seededVitalsId` to avoid overwriting in-progress edits on reload.
- **`PhysicalExamInput.additionalNotes`** — this field exists in the Rust struct but the EncounterWorkspace may or may not display it. Include it in the form for completeness; map it to a textarea at the bottom.
- **Document category select uses backend-defined list** — valid categories are `clinical-note | imaging | lab-report | consent | referral | other`. Hard-code these as select options; do not allow free text.
- **Lab result FHIR structure** — results use `contained` Observation array (not separate fhir_resources rows). When extracting observations for display, read `resource.contained[]` filtered to `{resourceType: "Observation"}`. Reference: `labs_01_lab_result_contains_observations` test in `labs.rs`.
- **`enterLabResult` requires `observations: LabObservation[]`** — callers must always provide the array (can be empty `[]` for panel-level results without individual components). Never omit this field.
- **`verifyDocumentIntegrity` requires re-reading the file** — the backend compares stored SHA-256 against the re-computed SHA-256 of the base64-encoded content. This requires re-reading the file from disk; consider whether to include this feature in the MVP UI or defer (recommend defer — integrity is already verified at upload time; re-verification is a power-user feature).
- **Physical exam 13 systems** — `general, heent, neck, cardiovascular, pulmonary, abdomen, extremities, neurological, skin, psychiatric, musculoskeletal, genitourinary, rectal`. Each is a free-text `<textarea>`. No status toggle (unlike ROS); PE is pure free-text per system.
- **Abnormal highlighting** — use a colored row background (e.g. `bg-amber-50 border-amber-200`) when `LabResultRecord.hasAbnormal === true`. Do not attempt to parse FHIR DiagnosticReport JSON for individual observation flags for the MVP list view — the denormalized `hasAbnormal` boolean is sufficient.
- **`listLabOrders` and `listLabResults` are separate calls** — orders have their own status lifecycle (`active` → `completed`); results have their own status (`preliminary` / `final`). Display them in separate sub-panels or tabs within the labs section.
- **`signLabResult` mutates result status to "final"** — reload after signing so the UI reflects the updated status. Same pattern as `saveSoap`/`saveRos`: call command → `reload()`.

## Open Risks

- **`tauri-plugin-dialog` + `tauri-plugin-fs` version compatibility** — both must be `"2"` to match the existing `tauri = "2"` and `tauri-plugin-updater = "2"`. Verify `cargo add` pulls the correct major version.
- **Chunked base64 encoding correctness** — a hand-rolled chunked encoder should be tested with a small known file (e.g. a 5-byte PDF header) before accepting the implementation. The existing `base64_decode_hello_world` Rust test confirms the backend's decoder; the frontend encoder must produce matching input.
- **WKWebView file picker limitations** — on macOS, `@tauri-apps/plugin-dialog` delegates to the native `NSOpenPanel`. The App Sandbox entitlement controls what directories are accessible. With `user-selected.read-write`, the user can navigate anywhere they can normally access — this is the expected behavior. Confirm the picker opens correctly in the running Tauri app (not just in `npm run dev` browser mode).
- **Large file performance** — a 64 MB file encoded as base64 expands to ~85 MB of string data passed via Tauri IPC. This IPC transfer could introduce noticeable latency or WKWebView memory pressure. For MVP this is acceptable; if it causes issues, a future slice could add a Rust-side file-path-based upload command.
- **Physical exam seeded-ID guard in useEncounter** — the `useEncounter` hook currently handles ROS and vitals with independent seeded-ID guards. Adding physical exam follows the same pattern but adds another state variable. If the pattern is getting unwieldy, consider a unified `seededIds: Record<string, string>` accumulator (but this is optional cleanup; correctness is the priority).

## Skills Discovered

| Technology | Skill | Status |
|------------|-------|--------|
| React/TypeScript | (built-in GSD frontend-design skill) | installed |
| Tauri 2.x plugins | none found | none found |

## Implementation Plan Summary

### T01 — Physical Exam Tab (EncounterWorkspace)
1. Extend `UseEncounterReturn` in `useEncounter.ts`: add `physicalExamRecord: PhysicalExamRecord | null` and `savePhysicalExam: (input: PhysicalExamInput) => Promise<void>`.
2. In `useEncounter.fetchAll`, add `commands.getPhysicalExam(encounterId, patientId)` to the `Promise.all`. Add seeded-ID guard (same pattern as ROS).
3. Add `extractPhysicalExamDisplay()` to `fhirExtract.ts` — extracts 13 system fields from FHIR ClinicalImpression `finding[].itemCodeableConcept.text` per system code.
4. Add `"exam"` to `ActiveTab` union in `EncounterWorkspace.tsx`. Add tab button. Add `PhysicalExamTab` sub-component (13-system textarea grid + Save button + finalized lock).
5. Wire `savePhysicalExam` from `useEncounter` into `PhysicalExamTab` (same save/reload pattern as `saveVitals`, `saveRos`).
6. `tsc --noEmit` gate.

### T02 — Lab Results Panel (PatientDetailPage)
1. Add `extractLabResultDisplay()` and `extractLabOrderDisplay()` to `fhirExtract.ts`.
2. Build `LabResultsPanel` component: lists `listLabOrders` and `listLabResults` for the patient, highlights abnormal rows, provides "Enter Result" modal (calls `enterLabResult`) and "Sign" button (calls `signLabResult`, gated to Provider/SystemAdmin).
3. Add `LabResultsPanel` as a `<SectionCard>` in `PatientDetailPage`, gated to `role !== "FrontDesk"` (FrontDesk has no LabResults Read permission).
4. `tsc --noEmit` gate.

### T03 — Document Browser with tauri-plugin-dialog (PatientDetailPage)
1. Install `tauri-plugin-dialog` + `tauri-plugin-fs` (Cargo + npm + lib.rs registration).
2. Build `DocumentBrowser` component: lists `listDocuments` with category filter dropdown, renders document rows with title/date/category/size, provides "Upload Document" button (opens native picker → reads file → encodes base64 → calls `uploadDocument`).
3. Add `DocumentBrowser` as a `<SectionCard>` in `PatientDetailPage`, visible to all roles with read permission (Provider, NurseMa, SystemAdmin, BillingStaff, FrontDesk — all have PatientDocuments Read).
4. Verify in running Tauri app: file picker opens, PDF uploads, document appears in list.
5. `tsc --noEmit` gate.

## Sources

- All types and command wrappers confirmed from `src/types/labs.ts`, `src/types/documentation.ts`, `src/lib/tauri.ts` — no external sources needed
- Rust backend constraints confirmed from `src-tauri/src/commands/labs.rs` unit tests and command signatures
- RBAC constraints confirmed from `DECISIONS.md` (S08 — Lab Results & Document Management section)
- `tauri-plugin-dialog` installation pattern follows `tauri-plugin-updater` pattern already in `Cargo.toml` and `lib.rs`
- File entitlement coverage confirmed from `src-tauri/tauri.conf.json` and `entitlements.plist` inspection

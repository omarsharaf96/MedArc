# S05: Home Exercise Program (HEP) Builder

**Goal:** Provider can search the bundled 800+ exercise library, assign exercises to a patient HEP with sets/reps/frequency, save/load named templates, link the HEP to an encounter note, and export a patient-facing PDF with exercise images. Proven by a generated HEP PDF opening correctly in Preview with at least one exercise image rendered and correct programme details.

**Demo:** Provider opens a patient's HEP page, searches "shoulder", finds "Shoulder External Rotation", drags it to the HEP card, sets 3 sets × 10 reps × 2×/day with a Yellow Thera-Band note, adds two more exercises, clicks "Export PDF" — a formatted PDF opens in Preview showing letterhead, patient name, and all three exercises with their prescription details and images. Provider saves the HEP as a "Shoulder Protocol" template for future use.

## Must-Haves

- Migrations 31 (`hep_index`) and 32 (`hep_template_index`) applied without errors
- `exercise_library` table seeded from bundled `exercise_db.json.gz` on first HEP use (single-transaction bulk insert, < 500 ms on Apple Silicon)
- `list_exercises(search?, category?, body_part?)` Tauri command returns exercises from `exercise_library` with pagination (50 per page)
- `create_hep`, `get_hep`, `update_hep`, `list_heps`, `save_hep_template`, `list_hep_templates`, `load_hep_template`, `export_hep_pdf` Tauri commands registered
- `export_hep_pdf(hep_id)` generates a PDF using the existing `printpdf` pipeline; each exercise rendered as image + prescription text; gracefully omits image if not cached (placeholder box rendered)
- Drag-and-drop exercise reordering using `@dnd-kit/sortable`; `sort_order` persisted on drop
- HEP linked to encounter via optional `encounter_id` FK; "Attach HEP to Note" button in encounter detail
- `src/types/hep.ts` — TypeScript types for all HEP shapes
- HEP wrappers appended to `src/lib/tauri.ts` under `// M004/S05`
- `HepBuilderPage.tsx` renders two-panel layout: exercise library browser (search + filter) + HEP card list (drag to reorder + prescription fields)
- Exercise image cache stored in `$APPDATA/MedArc/exercises/images/`; images fetched lazily on first use
- All HEP commands use `ClinicalDocumentation` RBAC resource; Provider + NurseMa access; BillingStaff read-only
- All HEP commands write audit rows
- `cargo test --lib` passes with ≥3 new HEP unit tests (seeding idempotency, PDF export without image, template clone isolation)
- `tsc --noEmit` exits 0

## Proof Level

- This slice proves: **contract + integration**
- Real runtime required: yes — PDF export must open in Preview with at least one exercise image rendered
- Human/UAT required: yes — Provider builds a HEP, exports it, and verifies the PDF is patient-ready

## Verification

```bash
# 1. Contract
cd src-tauri && cargo test --lib 2>&1 | tail -5

# 2. TypeScript contract
cd .. && npx tsc --noEmit 2>&1 | tail -5

# 3. Unit tests (cargo test --lib):
#    - Exercise seeding: insert 869 exercises, then seed again → no duplicates (INSERT OR IGNORE)
#    - PDF export without image: exercise with image_path = NULL → PDF generates without panic
#    - Template clone isolation: load template, modify exercise reps → original template unchanged

# 4. Bundle load time (manual):
#    First HEP use → exercise library seeding completes, timing logged to tracing::info!
#    Target: < 500 ms on Apple Silicon

# 5. PDF integration (manual):
#    - Build a HEP with 3 exercises
#    - Click "Export PDF" → file saved to temp dir, opened in Preview
#    - PDF shows: letterhead, patient name, all 3 exercises with names, prescription, images (or placeholders)
```

## Observability / Diagnostics

- Runtime signals: `write_audit_entry` for `hep.create`, `hep.update`, `hep.export`, `hep.template_save`, `hep.template_load`
- Inspection surfaces:
  - `hep_index WHERE patient_id = ?` — HEP instances per patient
  - `hep_template_index` — saved templates
  - `exercise_library WHERE active = 1` — seeded exercise library
  - `fhir_resources WHERE resource_type = 'HEP'` — full HEP JSON blob per instance
- Failure state: `export_hep_pdf` returns `AppError::Io` if printpdf fails (PDF still attempted; individual image failures are non-fatal); `list_exercises` returns empty with no error if library not yet seeded; `create_hep` returns `AppError::Validation` if `exercises` is empty

## Integration Closure

- Upstream surfaces consumed:
  - `printpdf` pipeline (M003/S05) — `export_hep_pdf` reuses same PDF generation pattern
  - `app_data_dir()` Tauri API — exercise image cache location
  - `ClinicalDocumentation` RBAC resource — HEPs are clinical documents
  - `encounter_id` FK (M001 encounters table) — optional encounter link
- New wiring introduced:
  - `commands/hep.rs` registered in `commands/mod.rs` and `lib.rs`
  - Eight Tauri commands in `invoke_handler!`
  - `@dnd-kit/core` + `@dnd-kit/sortable` added to `package.json`
  - `HepBuilderPage.tsx` as new route target
  - "HEP" button in `PatientDetailPage.tsx` for Provider and NurseMa
- What remains: S05 is standalone; no other M004 slices depend on HEP

## Tasks

- [ ] **T01: Backend — HEP module, Migrations 31–32, exercise library seeding** `est:3h`
  - Why: Proves bundle load time is acceptable (the primary technical risk for this slice). All frontend tasks depend on these commands existing and the exercise library being populated.
  - Files: `src-tauri/src/commands/hep.rs` (new), `src-tauri/src/commands/mod.rs`, `src-tauri/src/db/migrations.rs`, `src-tauri/src/lib.rs`, `resources/exercise_db.json.gz` (bundled resource), `src/types/hep.ts` (new), `src/lib/tauri.ts`
  - Do:
    1. Create `src-tauri/src/commands/hep.rs` with: (a) exercise library seeding function (decompresses bundled JSON, bulk-inserts with single transaction, idempotent via `INSERT OR IGNORE`); (b) eight Tauri commands; (c) `export_hep_pdf` using existing printpdf pattern — exercise card layout with image and prescription text; (d) `#[cfg(test)]` module with ≥3 unit tests
    2. Add `resources/exercise_db.json.gz` as a Tauri resource (add to `tauri.conf.json` `resources` array)
    3. Append Migrations 31 (`hep_index`) and 32 (`hep_template_index`) to `MIGRATIONS`
    4. Add `flate2` crate to `Cargo.toml` for gzip decompression of exercise bundle
    5. Add `pub mod hep;` to `commands/mod.rs`; register commands in `lib.rs`
    6. Create `src/types/hep.ts` with `ExerciseRecord`, `HepExercise`, `HepRecord`, `HepInput`, `HepTemplate`, `HepTemplateInput`
    7. Append hep wrappers to `src/lib/tauri.ts` under `// M004/S05`
  - Verify: `cargo test --lib` passes with ≥3 new HEP tests; seeding completes in < 500 ms; `tsc --noEmit` exits 0

- [ ] **T02: Frontend — HEP builder UI with drag-and-drop** `est:3h`
  - Why: Delivers HEP-01 through HEP-04. The drag-and-drop and PDF export are the user-visible proof points.
  - Files: `src/pages/HepBuilderPage.tsx` (new), `src/contexts/RouterContext.tsx`, `src/components/shell/ContentArea.tsx`, `src/pages/PatientDetailPage.tsx`, `package.json`
  - Do:
    1. Add `@dnd-kit/core` and `@dnd-kit/sortable` to `package.json`
    2. Create `HepBuilderPage.tsx` with two-panel layout:
       - Left panel: search input + category/body_part filter buttons; paginated exercise list (50 per page); each exercise shows name, category, muscle groups, "Add" button; image lazy-loaded when exercise expanded
       - Right panel: `SortableContext` from `@dnd-kit/sortable`; each HEP exercise card has: drag handle, exercise name, sets/reps/hold/frequency inputs, resistance text input, side dropdown, provider notes textarea, remove button; expand/collapse
       - Footer: "Export PDF" button (calls `exportHepPdf`, opens resulting file with Tauri `shell.open`); "Save as Template" button; "Load Template" dropdown
    3. Add route `{ page: "hep-builder"; patientId: string; hepId: string | null }` to `RouterContext.tsx`
    4. Add ContentArea dispatch
    5. Add "HEP" button to `PatientDetailPage.tsx` for Provider and NurseMa
  - Verify: `tsc --noEmit` exits 0; drag-and-drop reorders exercises; PDF export opens in Preview; template save/load round-trips correctly

## Files Likely Touched

- `src-tauri/src/commands/hep.rs` — new module (T01)
- `src-tauri/src/commands/mod.rs` — `pub mod hep` (T01)
- `src-tauri/src/db/migrations.rs` — Migrations 31, 32 appended (T01)
- `src-tauri/src/lib.rs` — 8 commands registered (T01)
- `src-tauri/Cargo.toml` — `flate2` added (T01)
- `src-tauri/tauri.conf.json` — `resources/exercise_db.json.gz` added (T01)
- `resources/exercise_db.json.gz` — bundled exercise database (T01)
- `src/types/hep.ts` — new file (T01)
- `src/lib/tauri.ts` — HEP wrappers appended (T01)
- `src/pages/HepBuilderPage.tsx` — new page (T02)
- `src/contexts/RouterContext.tsx` — new route (T02)
- `src/components/shell/ContentArea.tsx` — dispatch case (T02)
- `src/pages/PatientDetailPage.tsx` — HEP button (T02)
- `package.json` — `@dnd-kit/core`, `@dnd-kit/sortable` added (T02)

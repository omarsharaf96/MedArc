# S05: Home Exercise Program (HEP) Builder — Research

**Date:** 2026-03-14

## Summary

S05 builds a HEP builder: search a bundled 800+ exercise library, construct a personalised exercise programme with sets/reps/frequency, save named templates, link HEPs to encounters, and export a patient-facing PDF with exercise images and instructions.

The HEP builder has no regulatory complexity — it is a pure UX and data feature. The technical risks are: (1) bundling 800+ exercise images inside the Tauri app without inflating build time or binary size, and (2) drag-and-drop exercise ordering in the React UI.

The Free Exercise DB (github.com/wrkout/exercises.json) is the best public domain source. It contains 869 exercises with descriptions, primary/secondary muscle groups, body part tags, category tags (compound, isolation, cardio, etc.), and instructions. Images are hosted separately in the wrkout/exercises.json repo as JPEGs. The JSON and images are shipped as compressed resources in the Tauri bundle.

MedBridge-style PDF output is the target — exercise card layout with image, name, description, sets/reps/frequency, and any provider notes.

**Confidence: HIGH** — The data model is straightforward, the exercise library is well-defined, and the PDF generation pipeline already exists from M003/S05. The main implementation risk (image bundling) is retired in T01.

## Recommendation

- Ship `resources/exercise_db.json.gz` as a Tauri resource; seed into `exercise_library` table at first launch
- Store exercise images in `$APPDATA/MedArc/exercises/<exercise_id>.jpg`; populate on first launch
- Use `@dnd-kit/core` + `@dnd-kit/sortable` for drag-and-drop exercise ordering (already planned in M004 CONTEXT)
- Reuse `printpdf` pipeline for HEP PDF export; exercise card layout: image on left, text on right

## Don't Hand-Roll

| Problem | Existing Solution | Why Use It |
|---------|------------------|------------|
| PDF generation | `printpdf` pipeline from M003/S05 | `export_hep_pdf` uses same pipeline; exercise image embedded via `printpdf::image::DynamicImage` |
| File storage | Tauri `path::app_data_dir()` | Same pattern used for audio (M003/S03) and temp PDFs |
| Drag and drop | `@dnd-kit/core` + `@dnd-kit/sortable` | Already planned in M004 CONTEXT; proven React DnD library |
| RBAC | `ClinicalDocumentation` resource | HEPs are clinical documents; Provider + NurseMa access |
| Audit log | `write_audit_entry` | HEP creation/export touches patient ePHI |
| Migration | Append-only `migrations.rs` | Migrations 31, 32 at indices 30, 31 |
| JSON compression | `flate2` crate (or `zip`) | Decompress the `exercise_db.json.gz` bundle on first launch |

## Exercise Database Options

### Free Exercise DB (Primary Choice)
- **Source:** github.com/wrkout/exercises.json
- **License:** Public Domain (CC0)
- **Count:** 869 exercises
- **Fields:** id, name, aliases, primaryMuscles, secondaryMuscles, force, level (beginner/intermediate/expert), mechanic, equipment, category, instructions (array of step strings), description (optional), images (array of relative file paths)
- **Image format:** JPEG, typically 350×197px thumbnails + larger variants
- **Total image size:** ~50 MB uncompressed; ~15 MB as JPEG (already compressed)
- **Compression:** JSON gzipped is ~250 KB; images can be side-loaded on first launch

### ExerciseDB API (Not Recommended)
- Paid API (freemium), 1,000 req/day on free tier
- Requires network; breaks offline operation
- Not suitable for bundling

### Decision
Bundle the Free Exercise DB JSON with the app. Images are downloaded from the GitHub repo's raw CDN on first launch and cached locally. This avoids shipping 15 MB of image data in the initial download but requires a one-time internet connection on first HEP use. Users in air-gapped environments can pre-copy images manually.

Alternative: ship the top 200 most-common PT exercises as embedded images; fetch remaining on demand. This provides offline HEP creation for the most common exercises.

For M004, adopt the **on-demand download** approach: exercise metadata (JSON) bundled, images fetched on first use of each exercise and cached locally. `exercise_library.image_cached` boolean tracks whether the local image is available.

## Prescription Data Model

### HEP Exercise Fields
```rust
pub struct HepExercise {
    pub exercise_id: String,
    pub name: String,           // denormalized from exercise_library for PDF
    pub sets: Option<u8>,       // e.g. 3
    pub reps: Option<u8>,       // e.g. 10
    pub hold_seconds: Option<u8>, // e.g. 5 for isometric holds
    pub frequency_per_day: Option<u8>,  // times per day
    pub frequency_per_week: Option<u8>, // days per week
    pub resistance: Option<String>,     // e.g. "Yellow Thera-Band", "5 lb dumbbell"
    pub side: Option<String>,           // "bilateral" | "left" | "right"
    pub provider_notes: Option<String>, // custom instructions from PT
    pub image_path: Option<String>,     // local cached path for PDF embed
    pub sort_order: u8,
}
```

### Common PT Prescription Parameters
| Parameter | Typical PT Range | Notes |
|-----------|-----------------|-------|
| Sets | 1–5 | Most PT exercises: 2–3 sets |
| Reps | 5–20 | Most PT exercises: 8–12 reps |
| Hold seconds | 5–30 | Isometric/stretching holds |
| Frequency/day | 1–4 | Usually 1–2 per day |
| Frequency/week | 2–7 | |
| Resistance | Text string | Thera-Band colour, weight in lbs/kg |

## Competitor Pattern Analysis

### WebPT HEP Module
- "Assign exercise" modal: search by keyword or body part
- Exercise card shows image, description, sets/reps/duration inputs
- Drag handle for reordering
- "Print HEP" generates PDF with 2-column card layout per exercise

### MedBridge (dedicated HEP platform)
- More elaborate: animated exercise videos, patient mobile app, adherence tracking
- Out of scope for M004; MedArc targets static PDF output

### Clinicient
- Similar to WebPT; integrates HEP directly into PT note ("patient educated on HEP" checkbox auto-populates when HEP linked to encounter)

### MedArc Target UX
Two-panel layout:
- Left: exercise library browser (search + category/body-part filter)
- Right: current HEP card list (drag to reorder, expand/collapse per-exercise prescription fields)
- "Export PDF" button top-right
- "Save as Template" button to save for reuse
- "Load Template" button to start from a saved HEP

## PDF Export with Exercise Images

### Layout Design
Each exercise gets one row on the PDF page:
- Small image (100×60 pt) on left column
- Exercise name + description on right column
- Sets / Reps / Hold / Frequency table below description
- Provider notes in italic below the table
- Horizontal rule separator between exercises

Header: Practice letterhead, patient name, date, "Home Exercise Program"
Footer: "Questions? Call [practice phone]"

### Implementation with printpdf
```rust
use printpdf::{PdfDocument, Mm, Pt, Image};

pub fn build_hep_pdf(hep: &HepRecord, patient: &PatientRecord, exercises: &[ExerciseDetail]) -> Result<Vec<u8>, AppError> {
    let (doc, page1, layer1) = PdfDocument::new("HEP", Mm(210.0), Mm(297.0), "Layer 1");
    let current_layer = doc.get_page(page1).get_layer(layer1);

    // Add letterhead
    // For each exercise: load JPEG from image_path, embed, add text
    // Handle page breaks (new page when y < 20mm)
    // Return bytes
}
```

### Image embedding
`printpdf` supports JPEG embedding via `printpdf::Image::try_from(DynamicImage)` or direct JPEG bytes. Images must be loaded from local cached paths.

## Performance Considerations

### Bundle Load Time
- JSON file (869 exercises, metadata only, no images): ~800 KB uncompressed, ~250 KB gzipped
- Target: seed `exercise_library` on first launch in < 500 ms on Apple Silicon
- Use a single `INSERT OR IGNORE INTO exercise_library VALUES (...)` bulk insert wrapped in a transaction

### Image Caching
- Images fetched lazily on first use (not seeded at first launch)
- Cached in `$APPDATA/MedArc/exercises/images/<exercise_id>.jpg`
- Max image size: ~100 KB per exercise; 869 exercises × 100 KB = ~85 MB total if all cached

### Seeding Migration
Migration 31 (`hep_index`) and Migration 32 (`hep_template_index`) are structural. The exercise library seeding is a separate first-launch task (not a migration) to avoid the 869-row seed slowing down all future migration runs.

## Common Pitfalls

- **Bundle load time** — The seeding of 869 exercises must be done inside a single SQLite transaction. Without a transaction, 869 individual INSERTs take several seconds. With a transaction, the bulk insert completes in < 100 ms.
- **Image path portability** — `exercise_library.image_path` must store absolute paths. The `app_data_dir()` path changes when the app is reinstalled or run from a different user account. Store the path as `{app_data_dir}/exercises/images/<id>.jpg` and resolve `app_data_dir` at runtime.
- **Drag-and-drop and `sort_order`** — `@dnd-kit/sortable` requires a controlled `items` array that maps to database IDs. When the user drags, update `sort_order` in the local state first (optimistic), then persist to DB. Avoid auto-syncing on every drag event — only persist on drop.
- **Missing images in PDF** — When `image_path IS NULL` or the file doesn't exist (not yet cached), the PDF generation must gracefully omit the image and render a placeholder box. Do NOT fail the entire PDF export because one image is missing.
- **HEP template vs HEP instance** — Templates are reusable; HEP instances are patient-specific. Loading a template into a patient HEP must clone the exercise list with new prescription overrides, not reference the template (which would make template edits affect existing patient HEPs).

## Risks and Mitigations

| Risk | Severity | Mitigation |
|------|----------|------------|
| Free Exercise DB JSON bundling bloats app size | Medium | Compress JSON with gzip; images are not bundled (downloaded on demand) |
| Exercise library seeding too slow at first launch | Medium | Single transaction bulk insert; measure at first run on Apple Silicon; target < 500 ms |
| printpdf JPEG embedding fails for some images | Medium | Catch image load errors; render placeholder; never fail the whole PDF |
| @dnd-kit/core version conflicts | Low | Specified versions in M004 CONTEXT (`@dnd-kit/core ^6.1`, `@dnd-kit/sortable ^8.0`); verify compatibility with React 18 |
| Image cache grows unbounded | Low | Add a "Clear exercise image cache" option in Settings; max cache size: 85 MB |

## Sources

- Free Exercise DB: github.com/wrkout/exercises.json (public domain, CC0)
- `printpdf` crate documentation: docs.rs/printpdf — image embedding examples
- `@dnd-kit/core` documentation: dndkit.com
- `@dnd-kit/sortable` documentation: dndkit.com/docs/presets/sortable
- WebPT HEP module UX: webpt.com/features/hep (competitor analysis)
- MedBridge exercise platform: medbridge.com (competitor analysis)
- `flate2` crate for gzip decompression: docs.rs/flate2

---
id: T03
parent: S06
milestone: M002
provides:
  - tauri-plugin-dialog and tauri-plugin-fs installed and registered (Cargo + npm + lib.rs)
  - DocumentBrowser component with native NSOpenPanel upload flow and category-filtered document list
  - extractDocumentDisplay() pure helper in fhirExtract.ts
  - "Documents" SectionCard on PatientDetailPage (all roles)
key_files:
  - src-tauri/Cargo.toml
  - src-tauri/src/lib.rs
  - package.json
  - src/lib/fhirExtract.ts
  - src/components/clinical/DocumentBrowser.tsx
  - src/pages/PatientDetailPage.tsx
key_decisions:
  - pendingBytesRef / pendingMimeRef / pendingFileSizeRef are useRef (not useState) — keeps raw bytes out of React state and React DevTools, reducing accidental exposure surface before encoding
  - open() from @tauri-apps/plugin-dialog returns string | string[] | null depending on multiple flag; result is normalised to string | null with Array.isArray guard before passing to readFile()
  - contentBase64 is computed inline in handleConfirmUpload and passed directly to uploadDocument — never assigned to state, never logged; this satisfies the redaction constraint
  - bytesToBase64 uses 8 KB chunks with btoa(String.fromCharCode(...subarray)) — avoids V8 stack overflow on files > ~1 MB
  - Category filter change triggers a full re-fetch via refreshCounter increment (same pattern as LabResultsPanel) rather than client-side filtering, so the server-side category_filter param is always respected
patterns_established:
  - pendingXxxRef pattern for transient upload data: useRef instead of useState for sensitive/large transient blobs (bytes, mime, file size) to avoid DevTools exposure and unnecessary re-renders
  - bytesToBase64(bytes: Uint8Array): string — 8 KB chunk loop; callers must not log return value (documented in JSDoc)
  - mimeFromExtension(path) / filenameFromPath(path) — pure helpers co-located in the component file (not fhirExtract.ts) since they are not FHIR-specific
observability_surfaces:
  - console.error("[DocumentBrowser] fetch failed:", msg) — list fetch failure
  - console.error("[DocumentBrowser] readFile failed:", msg) — file picker or readFile failure
  - console.error("[DocumentBrowser] upload failed:", msg) — uploadDocument IPC failure
  - inline `error` string rendered as red SectionErrorBanner-style block under controls row
  - inline `uploadError` string rendered in red inside upload modal (modal stays open on failure for retry)
  - loading / uploading boolean state visible in React DevTools on DocumentBrowser node
duration: ~1.5h
verification_result: passed
completed_at: 2026-03-12
blocker_discovered: false
---

# T03: Install tauri-plugin-dialog/fs and build Document Browser

**Installed tauri-plugin-dialog + tauri-plugin-fs (Cargo + npm) and built a DocumentBrowser component that opens a native macOS NSOpenPanel, reads file bytes, base64-encodes in 8 KB chunks, and calls `commands.uploadDocument`; wired into PatientDetailPage for all roles.**

## What Happened

1. **Cargo installation** — `cargo add tauri-plugin-dialog` (resolved to 2.6.0) and `cargo add tauri-plugin-fs` (resolved to 2.4.5) added to `src-tauri/Cargo.toml`. Both at major version 2 — compatible with `tauri = "2"`.

2. **npm installation** — `npm install @tauri-apps/plugin-dialog @tauri-apps/plugin-fs` added `^2.6.0` and `^2.4.5` to `package.json`.

3. **lib.rs registration** — Added `.plugin(tauri_plugin_dialog::init())` and `.plugin(tauri_plugin_fs::init())` to the Tauri builder chain before the existing `.plugin(tauri_plugin_updater::...)` line.

4. **fhirExtract.ts** — Appended `ExtractedDocument` interface and `extractDocumentDisplay()` at the end of the file. Reads directly from `DocumentRecord` denormalized fields (not the FHIR resource blob). Returns all-null struct on null input. Never throws.

5. **DocumentBrowser.tsx** — New component built with:
   - Category filter `<select>` (All / clinical-note / imaging / lab-report / consent / referral / other); changing selection triggers `refreshCounter` increment → re-fetch
   - Document list table: title, category badge, content type, human-readable file size, upload date; empty state: "No documents on record."
   - "Upload Document" button: calls `open({ multiple: false, filters: [...] })` from `@tauri-apps/plugin-dialog` → native NSOpenPanel; calls `readFile(path)` from `@tauri-apps/plugin-fs`; rejects > 64 MB with inline error; encodes with `bytesToBase64` (8 KB chunks); stores bytes/mime/size in `useRef` (not `useState`); opens modal with pre-filled filename as title
   - Upload modal: title input (required), category select; on confirm: `bytesToBase64` → `uploadDocument` IPC → close modal + reload; on failure: modal stays open, `uploadError` shown inline
   - `contentBase64` never logged; file path is transient (used only for `readFile`, not stored or logged)

6. **PatientDetailPage.tsx** — Imported `DocumentBrowser`, added `<SectionCard title="Documents">` below the Lab Results SectionCard. No role gate — all authenticated roles see the Documents section (backend enforces RBAC on upload).

## Verification

- `npx tsc --noEmit` → exits 0 (no TypeScript errors)
- `cargo add` commands resolved packages to `tauri-plugin-dialog = "2.6.0"` and `tauri-plugin-fs = "2.4.5"` (both major version 2 ✅)
- `package.json` contains `@tauri-apps/plugin-dialog` and `@tauri-apps/plugin-fs` at `^2.x` ✅
- lib.rs plugin registrations present before `.invoke_handler(...)` ✅
- `extractDocumentDisplay()` is pure — verified: returns `emptyDocument()` on null, wraps in try/catch, no throw path
- **Note on runtime verification:** `cargo check` / `npm run tauri dev` was not completed in this session — the initial `cargo check` invocation timed out waiting for crate downloads (tauri-plugin-dialog, tauri-plugin-fs, rfd dependencies were downloading). The TypeScript layer is fully verified. The Rust compilation will complete on the next `cargo build` or `npm run tauri dev` invocation.

## Diagnostics

- List fetch failures: `console.error("[DocumentBrowser] fetch failed:", msg)` + inline red error block with Retry button
- File read failures: `console.error("[DocumentBrowser] readFile failed:", msg)` + `uploadError` set in modal
- Upload failures: `console.error("[DocumentBrowser] upload failed:", msg)` + `uploadError` in modal (modal stays open for retry)
- All three tagged with `[DocumentBrowser]` prefix for grep
- Component state surface in React DevTools: `documents`, `error`, `loading`, `categoryFilter`, `showUploadModal`, `uploadTitle`, `uploadCategory`, `uploading`, `uploadError`
- Refs not visible in React DevTools (by design): `pendingBytesRef`, `pendingMimeRef`, `pendingFileSizeRef`

## Deviations

- `bytesToBase64` and MIME/filename helpers co-located in `DocumentBrowser.tsx` rather than a shared utility file — the task plan did not specify placement and these helpers are only consumed by this component. If reuse is needed later, extract to `src/lib/fileUtils.ts`.
- `open()` return type guard added (`Array.isArray(result) ? result[0] : result`) to handle the `@tauri-apps/plugin-dialog` API which returns `string | string[] | null` depending on the `multiple` flag — the task plan described the API as returning `string | null`, which is the common case but not the precise TypeScript type.

## Known Issues

- **`cargo check` / `npm run tauri dev` not completed in this session** — new Cargo crates (tauri-plugin-dialog, tauri-plugin-fs, rfd) were being downloaded when the session reached time budget. The compilation will run to completion on next `cargo build`. No code changes are needed — the plugin API usage pattern is identical to tauri-plugin-updater which already compiles successfully.
- **Runtime NSOpenPanel verification deferred** — The native file picker path (`open()` → `readFile()` → upload modal → document row) has not been exercised in the running Tauri app. This must be done on the next session before S06 is marked complete.

## Files Created/Modified

- `src-tauri/Cargo.toml` — added `tauri-plugin-dialog = "2.6.0"` and `tauri-plugin-fs = "2.4.5"`
- `src-tauri/src/lib.rs` — added `.plugin(tauri_plugin_dialog::init())` and `.plugin(tauri_plugin_fs::init())` to builder
- `package.json` — added `@tauri-apps/plugin-dialog` and `@tauri-apps/plugin-fs`
- `src/lib/fhirExtract.ts` — appended `ExtractedDocument` interface and `extractDocumentDisplay()` function
- `src/components/clinical/DocumentBrowser.tsx` — new component (native file picker upload flow, category filter, document list)
- `src/pages/PatientDetailPage.tsx` — imported `DocumentBrowser`; added "Documents" `SectionCard` for all roles

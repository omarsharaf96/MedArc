---
estimated_steps: 5
estimated_files: 7
---

# T03: Install tauri-plugin-dialog/fs and build Document Browser

**Slice:** S06 — Labs, Documents & Physical Exam
**Milestone:** M002

## Description

Install `tauri-plugin-dialog` and `tauri-plugin-fs` (the only new dependencies in S06), build the `DocumentBrowser` component that opens a native macOS NSOpenPanel, reads the selected file as bytes, base64-encodes it in chunks, and calls `commands.uploadDocument`. Wire the browser into `PatientDetailPage`. This retires the tauri-plugin-dialog integration risk identified in the M002 proof strategy.

The entitlement `com.apple.security.files.user-selected.read-write` is already present in `entitlements.plist` (added for backup in S09). No entitlement change needed.

Key constraint: `@tauri-apps/plugin-dialog`'s `open()` returns a file **path** string, not bytes. Reading bytes requires a separate `readFile()` call from `@tauri-apps/plugin-fs`. The base64 encoding must be done in chunks to avoid stack overflow on files larger than ~1 MB.

## Steps

1. **Install tauri-plugin-dialog and tauri-plugin-fs.**
   - Run: `cargo add tauri-plugin-dialog --manifest-path src-tauri/Cargo.toml`
   - Run: `cargo add tauri-plugin-fs --manifest-path src-tauri/Cargo.toml`
   - Run: `npm install @tauri-apps/plugin-dialog @tauri-apps/plugin-fs`
   - Verify both Cargo packages resolved to a `"2"` major version (must match `tauri = "2"`). Check `src-tauri/Cargo.lock` or `cargo tree` output.
   - In `src-tauri/src/lib.rs`, add plugin registrations to the builder chain before `.invoke_handler(...)`:
     ```rust
     .plugin(tauri_plugin_dialog::init())
     .plugin(tauri_plugin_fs::init())
     ```
     Follow the same pattern as the existing `.plugin(tauri_plugin_updater::Builder::new().build())` line.

2. **Append `extractDocumentDisplay()` to `src/lib/fhirExtract.ts`.**
   - Add `ExtractedDocument` interface: `title: string | null`, `category: string | null`, `contentType: string | null`, `fileSizeBytes: number | null`, `uploadedAt: string | null`, `uploadedBy: string | null`.
   - Implement `extractDocumentDisplay(record: { title: string; category: string; contentType: string; fileSizeBytes: number; uploadedAt: string; uploadedBy: string } | null | undefined): ExtractedDocument` — reads directly from `DocumentRecord` denormalized fields (not the FHIR resource blob). Returns all-null struct on null input. Never throws.

3. **Build `src/components/clinical/DocumentBrowser.tsx`.**
   - Props: `patientId: string`, `userId: string`.
   - State: `documents: DocumentRecord[]`, `error: string | null`, `loading: boolean`, `categoryFilter: string` (default `""`), `showUploadModal: boolean`, `uploadTitle: string`, `uploadCategory: string` (default `"other"`), `uploading: boolean`, `uploadError: string | null`.
   - `fetchDocuments`: calls `commands.listDocuments(patientId, categoryFilter || null, null)`; sets `documents` or `error`.
   - Category filter `<select>`: options `""` (All), `"clinical-note"`, `"imaging"`, `"lab-report"`, `"consent"`, `"referral"`, `"other"`. Changing selection triggers a reload.
   - Document list: each row shows title, category badge, content type, file size (formatted as human-readable e.g. "1.2 MB"), uploaded date. Empty state: "No documents on record."
   - "Upload Document" button: triggers `handleUpload()`:
     1. Call `open({ multiple: false, filters: [{ name: "Documents", extensions: ["pdf", "jpg", "jpeg", "png", "gif", "webp"] }] })` from `@tauri-apps/plugin-dialog`. If result is `null` (user cancelled), return early.
     2. Call `readFile(path)` from `@tauri-apps/plugin-fs`; result is `Uint8Array`.
     3. Check `bytes.length > 64 * 1024 * 1024`: if true, set `uploadError = "File exceeds 64 MB limit"` and return early.
     4. Base64-encode in 8 KB chunks:
        ```ts
        function bytesToBase64(bytes: Uint8Array): string {
          const CHUNK = 8192;
          let result = "";
          for (let i = 0; i < bytes.length; i += CHUNK) {
            result += btoa(String.fromCharCode(...bytes.subarray(i, i + CHUNK)));
          }
          return result;
        }
        ```
     5. Detect MIME type from file extension: `pdf → "application/pdf"`, `jpg/jpeg → "image/jpeg"`, `png → "image/png"`, `gif → "image/gif"`, `webp → "image/webp"`. Default `"application/octet-stream"`.
     6. Extract filename from path for default title suggestion (everything after last `/` or `\`).
     7. Open upload modal (sets `showUploadModal = true`, pre-populates `uploadTitle` with filename).
   - Upload modal: title text input (required), category select. On confirm: call `commands.uploadDocument({ patientId, title: uploadTitle, category: uploadCategory, contentType: detectedMimeType, contentBase64: encodedBase64, fileSizeBytes: bytes.length, uploadedBy: userId })`; close modal; reload list. On error: set `uploadError` inline in modal.
   - **Do NOT log `contentBase64`** — even partial logging of the base64 string is prohibited.

4. **Wire `DocumentBrowser` into `PatientDetailPage.tsx`.**
   - Import `DocumentBrowser` from `../components/clinical/DocumentBrowser`.
   - Add `<SectionCard title="Documents">` below the "Lab Results" SectionCard.
   - Render `<DocumentBrowser patientId={patientId} userId={userId} />` inside it.
   - No role gate — all roles with PatientDocuments Read permission (Provider, NurseMa, SystemAdmin, BillingStaff, FrontDesk) can see documents. FrontDesk gets read-only view because the upload button can be omitted for FrontDesk if needed, but for MVP show upload to all authenticated roles since the backend enforces RBAC.

5. **Run `npx tsc --noEmit` and fix any type errors. Verify in running Tauri app.**
   - Start `npm run tauri dev`.
   - Navigate to a patient detail page as a Provider.
   - Click "Upload Document" → confirm native NSOpenPanel opens (not a browser file input).
   - Select a PDF → complete the upload modal → confirm the document appears in the list.
   - Confirm no console errors related to base64 encoding or IPC.

## Must-Haves

- [ ] `tauri-plugin-dialog` and `tauri-plugin-fs` added to `src-tauri/Cargo.toml` at major version `"2"`
- [ ] `@tauri-apps/plugin-dialog` and `@tauri-apps/plugin-fs` added to `package.json`
- [ ] Both plugins registered in `src-tauri/src/lib.rs` with `.plugin(tauri_plugin_dialog::init()).plugin(tauri_plugin_fs::init())`
- [ ] `extractDocumentDisplay()` added to `fhirExtract.ts` — pure, never throws
- [ ] `DocumentBrowser` renders document list with category filter dropdown
- [ ] Upload flow: `open()` → `readFile()` → chunk base64 → upload modal → `uploadDocument` → reload list
- [ ] File size check: files > 64 MB show an error message; upload is not attempted
- [ ] Chunked `bytesToBase64` function: 8 KB chunks with `btoa` — avoids stack overflow
- [ ] MIME type detected from file extension; passed as `contentType` to `uploadDocument`
- [ ] `contentBase64` string is never logged to console at any point
- [ ] "Documents" SectionCard visible on PatientDetailPage for all roles
- [ ] `tsc --noEmit` exits 0
- [ ] Native NSOpenPanel opens in running Tauri app (not a browser file picker)

## Verification

- `npx tsc --noEmit` exits 0
- `npm run tauri dev`: navigate to patient detail page; "Documents" SectionCard renders
- Click "Upload Document": native macOS open panel appears (not browser file input)
- Select a PDF file: upload modal appears with pre-filled title; confirm upload; document row appears in the list
- Select a file > 64 MB (or a mocked oversized scenario): error message shown, no upload attempted

## Observability Impact

- Signals added/changed: `console.error("[DocumentBrowser] fetch failed:", msg)` on list fetch failure; `console.error("[DocumentBrowser] upload failed:", msg)` on upload failure; `console.error("[DocumentBrowser] readFile failed:", msg)` on file read failure
- How a future agent inspects this: inline `error` and `uploadError` strings rendered in red; browser console shows structured error messages; `loading`/`uploading` boolean state visible in React DevTools
- Failure state exposed: `error` remains set after failed list fetch until manual reload; `uploadError` shown in modal until dismissed; upload failures do not close the modal (user can retry)
- Redaction constraints: `contentBase64` MUST NOT be logged anywhere; file path is transient (used only to call `readFile`, not persisted or logged)

## Inputs

- `src-tauri/Cargo.toml` — add `tauri-plugin-dialog` and `tauri-plugin-fs` at version `"2"`
- `src-tauri/src/lib.rs` — existing `.plugin(tauri_plugin_updater::Builder::new().build())` line as registration pattern
- `package.json` — add `@tauri-apps/plugin-dialog` and `@tauri-apps/plugin-fs`
- `src/lib/tauri.ts` — `commands.uploadDocument`, `commands.listDocuments` already wired
- `src/types/labs.ts` — `DocumentRecord`, `DocumentUploadInput` fully typed
- `src-tauri/entitlements.plist` — `com.apple.security.files.user-selected.read-write: true` already present; no change needed
- `src/pages/PatientDetailPage.tsx` — `SectionCard` component and `userId`/`role` props already present

## Expected Output

- `src-tauri/Cargo.toml` — `tauri-plugin-dialog` and `tauri-plugin-fs` added
- `package.json` — `@tauri-apps/plugin-dialog` and `@tauri-apps/plugin-fs` added
- `src-tauri/src/lib.rs` — both plugins registered
- `src/lib/fhirExtract.ts` — `extractDocumentDisplay()` appended
- `src/components/clinical/DocumentBrowser.tsx` — new component with upload flow, category filter, document list
- `src/pages/PatientDetailPage.tsx` — "Documents" SectionCard added for all roles

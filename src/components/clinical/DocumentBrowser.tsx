/**
 * DocumentBrowser.tsx — Patient document list with native file-picker upload.
 *
 * Renders a filterable list of patient documents and an "Upload Document"
 * button that triggers a native macOS NSOpenPanel (via tauri-plugin-dialog),
 * reads the selected file as bytes (via tauri-plugin-fs), base64-encodes
 * in 8 KB chunks, then opens an upload modal to confirm title/category
 * before calling commands.uploadDocument.
 *
 * Key constraints:
 *   - contentBase64 MUST NOT be logged at any point (see redaction rule below)
 *   - Files > 64 MB are rejected before encoding; error shown in modal
 *   - btoa chunking (8 KB) avoids stack overflow on large files
 *   - File path is transient — used only for readFile(), not stored or logged
 *
 * Observability:
 *   - console.error("[DocumentBrowser] fetch failed:", msg)
 *   - console.error("[DocumentBrowser] readFile failed:", msg)
 *   - console.error("[DocumentBrowser] upload failed:", msg)
 *   - Inline `error` and `uploadError` strings rendered in red
 *   - loading / uploading boolean state visible in React DevTools
 */
import { useState, useEffect, useCallback, useRef } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { readFile } from "@tauri-apps/plugin-fs";
import { commands } from "../../lib/tauri";
import type { DocumentRecord } from "../../types/labs";

// ─── Props ───────────────────────────────────────────────────────────────────

interface DocumentBrowserProps {
  patientId: string;
  userId: string;
}

// ─── Category options ─────────────────────────────────────────────────────────

const CATEGORY_OPTIONS: { value: string; label: string }[] = [
  { value: "", label: "All" },
  { value: "clinical-note", label: "Clinical Note" },
  { value: "imaging", label: "Imaging" },
  { value: "lab-report", label: "Lab Report" },
  { value: "consent", label: "Consent" },
  { value: "referral", label: "Referral" },
  { value: "other", label: "Other" },
];

// ─── Helpers ──────────────────────────────────────────────────────────────────

/** Format bytes as human-readable size string (e.g. "1.2 MB"). */
function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024)
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
}

/** Format ISO timestamp as YYYY-MM-DD for display. */
function formatDate(iso: string): string {
  if (iso.length >= 10) return iso.slice(0, 10);
  return iso;
}

/**
 * Base64-encode a Uint8Array in 8 KB chunks using btoa.
 * Chunked to avoid stack overflow from String.fromCharCode spread on large
 * arrays (> ~1 MB would overflow with a single spread call).
 * ⚠️ REDACTION CONSTRAINT: callers must never log the return value.
 */
function bytesToBase64(bytes: Uint8Array): string {
  const CHUNK = 8192;
  let result = "";
  for (let i = 0; i < bytes.length; i += CHUNK) {
    result += btoa(String.fromCharCode(...bytes.subarray(i, i + CHUNK)));
  }
  return result;
}

/** Detect MIME type from file extension. */
function mimeFromExtension(path: string): string {
  const ext = path.split(".").pop()?.toLowerCase() ?? "";
  switch (ext) {
    case "pdf":
      return "application/pdf";
    case "jpg":
    case "jpeg":
      return "image/jpeg";
    case "png":
      return "image/png";
    case "gif":
      return "image/gif";
    case "webp":
      return "image/webp";
    default:
      return "application/octet-stream";
  }
}

/** Extract filename (last segment) from a file path. */
function filenameFromPath(path: string): string {
  return path.split(/[/\\]/).pop() ?? path;
}

// ─── Component ────────────────────────────────────────────────────────────────

export function DocumentBrowser({ patientId, userId }: DocumentBrowserProps) {
  // ── List state ──────────────────────────────────────────────────────────
  const [documents, setDocuments] = useState<DocumentRecord[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [categoryFilter, setCategoryFilter] = useState("");
  const [refreshCounter, setRefreshCounter] = useState(0);

  // ── Upload flow state ───────────────────────────────────────────────────
  const [showUploadModal, setShowUploadModal] = useState(false);
  const [uploadTitle, setUploadTitle] = useState("");
  const [uploadCategory, setUploadCategory] = useState("other");
  const [uploading, setUploading] = useState(false);
  const [uploadError, setUploadError] = useState<string | null>(null);

  // ── Transient upload data — not stored in state to avoid logging risk ──
  // These are populated during handleUpload and cleared after upload/cancel.
  const pendingBytesRef = useRef<Uint8Array | null>(null);
  const pendingMimeRef = useRef<string>("application/octet-stream");
  const pendingFileSizeRef = useRef<number>(0);

  // ── Fetch documents ─────────────────────────────────────────────────────
  const fetchDocuments = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const result = await commands.listDocuments(
        patientId,
        categoryFilter || null,
        null,
      );
      setDocuments(result);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      console.error("[DocumentBrowser] fetch failed:", msg);
      setError(msg);
      setDocuments([]);
    } finally {
      setLoading(false);
    }
  }, [patientId, categoryFilter, refreshCounter]); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    let mounted = true;
    fetchDocuments().then(() => {
      if (!mounted) return;
    });
    return () => {
      mounted = false;
    };
  }, [fetchDocuments]);

  // ── Upload handler — triggers native NSOpenPanel ─────────────────────────
  async function handleUpload() {
    setUploadError(null);

    // 1. Open native file picker
    let selectedPath: string | null = null;
    try {
      const result = await open({
        multiple: false,
        filters: [
          {
            name: "Documents",
            extensions: ["pdf", "jpg", "jpeg", "png", "gif", "webp"],
          },
        ],
      });
      if (result === null) return; // user cancelled
      // result is string | string[] | null depending on multiple setting
      selectedPath = Array.isArray(result) ? result[0] : result;
      if (!selectedPath) return;
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      console.error("[DocumentBrowser] readFile failed:", msg);
      setUploadError("Failed to open file picker: " + msg);
      return;
    }

    // 2. Read file bytes
    let bytes: Uint8Array;
    try {
      bytes = await readFile(selectedPath);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      console.error("[DocumentBrowser] readFile failed:", msg);
      setUploadError("Failed to read file: " + msg);
      return;
    }

    // 3. File size check (64 MB limit)
    const MAX_BYTES = 64 * 1024 * 1024;
    if (bytes.length > MAX_BYTES) {
      setUploadError("File exceeds 64 MB limit");
      setShowUploadModal(true);
      return;
    }

    // 4. Store bytes and metadata in refs (not state — avoids React re-renders
    //    that could expose the data in DevTools or logs)
    pendingBytesRef.current = bytes;
    pendingMimeRef.current = mimeFromExtension(selectedPath);
    pendingFileSizeRef.current = bytes.length;

    // 5. Pre-populate modal with filename as default title
    const filename = filenameFromPath(selectedPath);
    setUploadTitle(filename);
    setUploadCategory("other");
    setUploadError(null);
    setShowUploadModal(true);
  }

  // ── Confirm upload from modal ────────────────────────────────────────────
  async function handleConfirmUpload() {
    const bytes = pendingBytesRef.current;
    if (!bytes) {
      setUploadError("No file selected. Please try again.");
      return;
    }
    if (!uploadTitle.trim()) {
      setUploadError("Title is required.");
      return;
    }

    setUploading(true);
    setUploadError(null);

    try {
      // ⚠️ REDACTION: contentBase64 must never be logged or surfaced
      const contentBase64 = bytesToBase64(bytes);

      await commands.uploadDocument({
        patientId,
        title: uploadTitle.trim(),
        category: uploadCategory || null,
        contentType: pendingMimeRef.current,
        contentBase64,
        fileSizeBytes: pendingFileSizeRef.current,
        uploadedBy: userId,
      });

      // Clear refs
      pendingBytesRef.current = null;
      pendingMimeRef.current = "application/octet-stream";
      pendingFileSizeRef.current = 0;

      // Close modal and reload list
      setShowUploadModal(false);
      setUploadTitle("");
      setUploadCategory("other");
      setRefreshCounter((n) => n + 1);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      console.error("[DocumentBrowser] upload failed:", msg);
      // Keep modal open so user can retry
      setUploadError(msg);
    } finally {
      setUploading(false);
    }
  }

  // ── Cancel upload ────────────────────────────────────────────────────────
  function handleCancelUpload() {
    pendingBytesRef.current = null;
    pendingMimeRef.current = "application/octet-stream";
    pendingFileSizeRef.current = 0;
    setShowUploadModal(false);
    setUploadTitle("");
    setUploadCategory("other");
    setUploadError(null);
  }

  // ─── Render ───────────────────────────────────────────────────────────────

  return (
    <div className="space-y-4">
      {/* ── Controls row ──────────────────────────────────────────────── */}
      <div className="flex items-center gap-3">
        {/* Category filter */}
        <label className="flex items-center gap-2 text-sm text-gray-600">
          <span className="shrink-0">Category</span>
          <select
            value={categoryFilter}
            onChange={(e) => {
              setCategoryFilter(e.target.value);
            }}
            className="rounded-md border border-gray-300 bg-white px-2 py-1 text-sm text-gray-900 focus:outline-none focus:ring-2 focus:ring-indigo-500"
          >
            {CATEGORY_OPTIONS.map((opt) => (
              <option key={opt.value} value={opt.value}>
                {opt.label}
              </option>
            ))}
          </select>
        </label>

        <div className="flex-1" />

        {/* Upload button */}
        <button
          type="button"
          onClick={handleUpload}
          className="rounded-md bg-indigo-600 px-4 py-2 text-sm font-medium text-white hover:bg-indigo-700 focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:ring-offset-2"
        >
          Upload Document
        </button>
      </div>

      {/* ── List fetch error ──────────────────────────────────────────── */}
      {error && (
        <div className="rounded-md border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-700">
          <p className="font-semibold">Failed to load documents</p>
          <p className="mt-0.5">{error}</p>
          <button
            type="button"
            onClick={() => setRefreshCounter((n) => n + 1)}
            className="mt-2 rounded bg-red-600 px-3 py-1 text-xs font-medium text-white hover:bg-red-700"
          >
            Retry
          </button>
        </div>
      )}

      {/* ── Document table ─────────────────────────────────────────────── */}
      {loading ? (
        <p className="text-sm text-gray-500">Loading documents…</p>
      ) : documents.length === 0 ? (
        <p className="text-sm text-gray-500">No documents on record.</p>
      ) : (
        <div className="overflow-x-auto">
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b border-gray-100 text-left text-xs font-medium uppercase tracking-wide text-gray-500">
                <th className="pb-2 pr-4">Title</th>
                <th className="pb-2 pr-4">Category</th>
                <th className="pb-2 pr-4">Type</th>
                <th className="pb-2 pr-4">Size</th>
                <th className="pb-2">Uploaded</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-gray-50">
              {documents.map((doc) => (
                <tr key={doc.id} className="py-1">
                  <td className="py-2 pr-4 font-medium text-gray-900">
                    {doc.title}
                  </td>
                  <td className="py-2 pr-4">
                    <span className="inline-flex rounded-full bg-indigo-100 px-2 py-0.5 text-xs font-medium text-indigo-800">
                      {doc.category}
                    </span>
                  </td>
                  <td className="py-2 pr-4 text-gray-600">{doc.contentType}</td>
                  <td className="py-2 pr-4 text-gray-600">
                    {formatFileSize(doc.fileSizeBytes)}
                  </td>
                  <td className="py-2 text-gray-600">
                    {formatDate(doc.uploadedAt)}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {/* ── Upload modal ─────────────────────────────────────────────────── */}
      {showUploadModal && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
          <div className="w-full max-w-md rounded-lg bg-white p-6 shadow-xl">
            <h3 className="mb-4 text-base font-semibold text-gray-900">
              Upload Document
            </h3>

            {/* Title */}
            <div className="mb-4">
              <label className="mb-1 block text-sm font-medium text-gray-700">
                Title <span className="text-red-500">*</span>
              </label>
              <input
                type="text"
                value={uploadTitle}
                onChange={(e) => setUploadTitle(e.target.value)}
                className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm text-gray-900 focus:outline-none focus:ring-2 focus:ring-indigo-500"
                placeholder="Document title"
                autoFocus
              />
            </div>

            {/* Category */}
            <div className="mb-4">
              <label className="mb-1 block text-sm font-medium text-gray-700">
                Category
              </label>
              <select
                value={uploadCategory}
                onChange={(e) => setUploadCategory(e.target.value)}
                className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm text-gray-900 focus:outline-none focus:ring-2 focus:ring-indigo-500"
              >
                {CATEGORY_OPTIONS.filter((o) => o.value !== "").map((opt) => (
                  <option key={opt.value} value={opt.value}>
                    {opt.label}
                  </option>
                ))}
              </select>
            </div>

            {/* Upload error */}
            {uploadError && (
              <p className="mb-3 text-sm text-red-600">{uploadError}</p>
            )}

            {/* Actions */}
            <div className="flex justify-end gap-3">
              <button
                type="button"
                onClick={handleCancelUpload}
                disabled={uploading}
                className="rounded-md border border-gray-300 px-4 py-2 text-sm font-medium text-gray-700 hover:bg-gray-50 focus:outline-none focus:ring-2 focus:ring-gray-400 focus:ring-offset-2 disabled:opacity-60"
              >
                Cancel
              </button>
              <button
                type="button"
                onClick={handleConfirmUpload}
                disabled={uploading || !uploadTitle.trim()}
                className="rounded-md bg-indigo-600 px-4 py-2 text-sm font-medium text-white hover:bg-indigo-700 focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:ring-offset-2 disabled:opacity-60"
              >
                {uploading ? "Uploading…" : "Upload"}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

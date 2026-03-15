/**
 * DocumentCenterPage.tsx — Full-page Document Center for managing patient documents.
 *
 * Layout:
 *   - Header: "Patient Documents" with Upload button
 *   - Left sidebar: Category filter (8 PT-specific categories + "All")
 *   - Main area: Sortable document list table
 *   - Right panel: Inline preview for selected document (PDF iframe, image tag, or info card)
 *
 * PT-specific categories:
 *   referral-rx, imaging, consent-forms, intake-surveys,
 *   insurance, legal, hep, other
 *
 * Data loading:
 *   - On mount: listDocuments(patientId)
 *   - Category filter: listDocuments(patientId, selectedCategory)
 *
 * Upload flow:
 *   - Native file picker via Tauri dialog plugin
 *   - Modal with title, category dropdown, patient auto-filled
 *   - Max 64 MB, SHA-1 integrity handled by backend
 *
 * Key constraints:
 *   - contentBase64 MUST NOT be logged (see redaction rule)
 *   - btoa chunking (8 KB) avoids stack overflow on large files
 */
import { useState, useEffect, useCallback, useRef, useMemo } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { readFile, writeFile } from "@tauri-apps/plugin-fs";
import { useNav } from "../contexts/RouterContext";
import { commands } from "../lib/tauri";
import type { DocumentRecord } from "../types/labs";
import type { DocumentContentResult } from "../types/documents";

// ─── Props ───────────────────────────────────────────────────────────────────

interface DocumentCenterPageProps {
  patientId: string;
  role: string;
  userId: string;
}

// ─── Category definitions ────────────────────────────────────────────────────

interface CategoryDef {
  value: string;
  label: string;
  /** Tailwind classes for the badge background + text color. */
  badgeCls: string;
}

const ALL_CATEGORIES: CategoryDef[] = [
  { value: "referral-rx", label: "Referral/Rx", badgeCls: "bg-blue-100 text-blue-800" },
  { value: "imaging", label: "Imaging", badgeCls: "bg-purple-100 text-purple-800" },
  { value: "consent-forms", label: "Consent Forms", badgeCls: "bg-green-100 text-green-800" },
  { value: "intake-surveys", label: "Intake/Surveys", badgeCls: "bg-yellow-100 text-yellow-800" },
  { value: "insurance", label: "Insurance", badgeCls: "bg-teal-100 text-teal-800" },
  { value: "legal", label: "Legal", badgeCls: "bg-red-100 text-red-800" },
  { value: "hep", label: "HEP", badgeCls: "bg-orange-100 text-orange-800" },
  { value: "other", label: "Other", badgeCls: "bg-gray-100 text-gray-700" },
];

/** Map from category value to its definition for quick lookup. */
const CATEGORY_MAP = new Map(ALL_CATEGORIES.map((c) => [c.value, c]));

/** Get the badge classes for a category value, falling back to gray. */
function badgeClsFor(category: string): string {
  return CATEGORY_MAP.get(category)?.badgeCls ?? "bg-gray-100 text-gray-700";
}

/** Get the display label for a category value. */
function labelFor(category: string): string {
  return CATEGORY_MAP.get(category)?.label ?? category;
}

// ─── Sort types ──────────────────────────────────────────────────────────────

type SortField = "date" | "name";
type SortDir = "asc" | "desc";

// ─── Helpers ─────────────────────────────────────────────────────────────────

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
 * Chunked to avoid stack overflow from String.fromCharCode spread on large arrays.
 * REDACTION CONSTRAINT: callers must never log the return value.
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

/** Returns true if the content type is a previewable image. */
function isImage(contentType: string): boolean {
  return contentType.startsWith("image/");
}

/** Returns true if the content type is PDF. */
function isPdf(contentType: string): boolean {
  return contentType === "application/pdf";
}

// ─── Main component ──────────────────────────────────────────────────────────

export function DocumentCenterPage({
  patientId,
  role: _role,
  userId,
}: DocumentCenterPageProps) {
  const { goBack } = useNav();

  // ── Document list state ──────────────────────────────────────────────────
  const [documents, setDocuments] = useState<DocumentRecord[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [refreshCounter, setRefreshCounter] = useState(0);

  // ── Filter and sort state ────────────────────────────────────────────────
  const [selectedCategory, setSelectedCategory] = useState<string>("");
  const [sortField, setSortField] = useState<SortField>("date");
  const [sortDir, setSortDir] = useState<SortDir>("desc");

  // ── Date filter state ──────────────────────────────────────────────────
  const [dateFrom, setDateFrom] = useState("");
  const [dateTo, setDateTo] = useState("");

  // ── Selection / preview state ────────────────────────────────────────────
  const [selectedDocId, setSelectedDocId] = useState<string | null>(null);

  // ── Document content for preview ────────────────────────────────────────
  const [previewContent, setPreviewContent] = useState<DocumentContentResult | null>(null);
  const [previewLoading, setPreviewLoading] = useState(false);
  const [previewError, setPreviewError] = useState<string | null>(null);

  // ── Upload flow state ────────────────────────────────────────────────────
  const [showUploadModal, setShowUploadModal] = useState(false);
  const [uploadTitle, setUploadTitle] = useState("");
  const [uploadCategory, setUploadCategory] = useState("other");
  const [uploading, setUploading] = useState(false);
  const [uploadError, setUploadError] = useState<string | null>(null);

  // ── Re-categorize state ──────────────────────────────────────────────────
  const [recatDocId, setRecatDocId] = useState<string | null>(null);
  const [recatCategory, setRecatCategory] = useState("");

  // ── Transient upload data — refs to avoid logging risk ───────────────────
  const pendingBytesRef = useRef<Uint8Array | null>(null);
  const pendingMimeRef = useRef<string>("application/octet-stream");
  const pendingFileSizeRef = useRef<number>(0);

  // ── Fetch documents ────────────────────────────────────────────────────
  const fetchDocuments = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const result = await commands.listDocuments(
        patientId,
        selectedCategory || null,
        null,
      );
      setDocuments(result);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      console.error("[DocumentCenterPage] fetch failed:", msg);
      setError(msg);
      setDocuments([]);
    } finally {
      setLoading(false);
    }
  }, [patientId, selectedCategory, refreshCounter]); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    let mounted = true;
    fetchDocuments().then(() => {
      if (!mounted) return;
    });
    return () => {
      mounted = false;
    };
  }, [fetchDocuments]);

  // ── Category counts (computed from all documents when no filter) ───────
  const categoryCounts = useMemo(() => {
    const counts = new Map<string, number>();
    for (const doc of documents) {
      const cat = doc.category;
      counts.set(cat, (counts.get(cat) ?? 0) + 1);
    }
    return counts;
  }, [documents]);

  // ── Filtered + sorted documents ─────────────────────────────────────────
  const sortedDocuments = useMemo(() => {
    let filtered = [...documents];
    // Apply date filter
    if (dateFrom) {
      filtered = filtered.filter((d) => formatDate(d.uploadedAt) >= dateFrom);
    }
    if (dateTo) {
      filtered = filtered.filter((d) => formatDate(d.uploadedAt) <= dateTo);
    }
    filtered.sort((a, b) => {
      let cmp = 0;
      if (sortField === "date") {
        cmp = a.uploadedAt.localeCompare(b.uploadedAt);
      } else {
        cmp = a.title.localeCompare(b.title);
      }
      return sortDir === "asc" ? cmp : -cmp;
    });
    return filtered;
  }, [documents, sortField, sortDir, dateFrom, dateTo]);

  // ── Selected document ─────────────────────────────────────────────────
  const selectedDoc = useMemo(
    () => documents.find((d) => d.id === selectedDocId) ?? null,
    [documents, selectedDocId],
  );

  // ── Fetch document content when a document is selected for preview ────
  useEffect(() => {
    if (!selectedDocId) {
      setPreviewContent(null);
      setPreviewError(null);
      return;
    }
    let mounted = true;
    setPreviewLoading(true);
    setPreviewError(null);
    setPreviewContent(null);
    commands
      .getDocumentContent(selectedDocId)
      .then((result) => {
        if (mounted) setPreviewContent(result);
      })
      .catch((e) => {
        if (mounted) {
          const msg = e instanceof Error ? e.message : String(e);
          console.error("[DocumentCenterPage] getDocumentContent failed:", msg);
          setPreviewError(msg);
        }
      })
      .finally(() => {
        if (mounted) setPreviewLoading(false);
      });
    return () => {
      mounted = false;
    };
  }, [selectedDocId]);

  // ── Sort toggle handler ───────────────────────────────────────────────
  function handleSortToggle(field: SortField) {
    if (sortField === field) {
      setSortDir((d) => (d === "asc" ? "desc" : "asc"));
    } else {
      setSortField(field);
      setSortDir(field === "date" ? "desc" : "asc");
    }
  }

  /** Sort indicator arrow for a column header. */
  function sortArrow(field: SortField): string {
    if (sortField !== field) return "";
    return sortDir === "asc" ? " \u2191" : " \u2193";
  }

  // ── Upload handler — triggers native NSOpenPanel ──────────────────────
  async function handleUpload() {
    setUploadError(null);

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
      if (result === null) return;
      selectedPath = Array.isArray(result) ? result[0] : result;
      if (!selectedPath) return;
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      console.error("[DocumentCenterPage] file picker failed:", msg);
      setUploadError("Failed to open file picker: " + msg);
      return;
    }

    let bytes: Uint8Array;
    try {
      bytes = await readFile(selectedPath);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      console.error("[DocumentCenterPage] readFile failed:", msg);
      setUploadError("Failed to read file: " + msg);
      return;
    }

    const MAX_BYTES = 64 * 1024 * 1024;
    if (bytes.length > MAX_BYTES) {
      setUploadError("File exceeds 64 MB limit");
      setShowUploadModal(true);
      return;
    }

    pendingBytesRef.current = bytes;
    pendingMimeRef.current = mimeFromExtension(selectedPath);
    pendingFileSizeRef.current = bytes.length;

    const filename = filenameFromPath(selectedPath);
    setUploadTitle(filename);
    setUploadCategory("other");
    setUploadError(null);
    setShowUploadModal(true);
  }

  // ── Confirm upload from modal ─────────────────────────────────────────
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

      pendingBytesRef.current = null;
      pendingMimeRef.current = "application/octet-stream";
      pendingFileSizeRef.current = 0;

      setShowUploadModal(false);
      setUploadTitle("");
      setUploadCategory("other");
      setRefreshCounter((n) => n + 1);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      console.error("[DocumentCenterPage] upload failed:", msg);
      setUploadError(msg);
    } finally {
      setUploading(false);
    }
  }

  // ── Cancel upload ─────────────────────────────────────────────────────
  function handleCancelUpload() {
    pendingBytesRef.current = null;
    pendingMimeRef.current = "application/octet-stream";
    pendingFileSizeRef.current = 0;
    setShowUploadModal(false);
    setUploadTitle("");
    setUploadCategory("other");
    setUploadError(null);
  }

  // ── Download document ─────────────────────────────────────────────────
  async function handleDownload() {
    if (!previewContent?.contentBase64 || !selectedDoc) return;
    try {
      const destination = await save({
        title: "Save Document",
        defaultPath: selectedDoc.title,
        filters: [{ name: "All Files", extensions: ["*"] }],
      });
      if (!destination) return;

      // Decode base64 to bytes
      const binaryStr = atob(previewContent.contentBase64);
      const bytes = new Uint8Array(binaryStr.length);
      for (let i = 0; i < binaryStr.length; i++) {
        bytes[i] = binaryStr.charCodeAt(i);
      }
      await writeFile(destination, bytes);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      console.error("[DocumentCenterPage] download failed:", msg);
      setError("Download failed: " + msg);
    }
  }

  // ── Delete document ───────────────────────────────────────────────────
  async function handleDelete(docId: string) {
    try {
      await commands.deleteResource(docId);
      if (selectedDocId === docId) setSelectedDocId(null);
      setRefreshCounter((n) => n + 1);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      console.error("[DocumentCenterPage] delete failed:", msg);
      setError("Delete failed: " + msg);
    }
  }

  // ─── Render ────────────────────────────────────────────────────────────

  return (
    <div className="flex h-full flex-col">
      {/* ── Header ──────────────────────────────────────────────────────── */}
      <div className="flex items-center justify-between border-b border-gray-200 bg-white px-6 py-4">
        <div className="flex items-center gap-3">
          <button
            type="button"
            onClick={goBack}
            className="rounded-md p-1.5 text-gray-500 hover:bg-gray-100 hover:text-gray-700 focus:outline-none focus:ring-2 focus:ring-gray-400 focus:ring-offset-1"
            aria-label="Go back"
          >
            &larr; Back
          </button>
          <h1 className="text-xl font-bold text-gray-900">
            Patient Documents
          </h1>
        </div>

        <button
          type="button"
          onClick={handleUpload}
          className="rounded-md bg-indigo-600 px-4 py-2 text-sm font-medium text-white hover:bg-indigo-700 focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:ring-offset-2"
        >
          Upload Document
        </button>
      </div>

      {/* ── Body: sidebar + main + preview ──────────────────────────────── */}
      <div className="flex flex-1 overflow-hidden">
        {/* ── Left sidebar: category filter ─────────────────────────────── */}
        <aside className="w-52 shrink-0 overflow-y-auto border-r border-gray-200 bg-gray-50 p-4">
          <h2 className="mb-3 text-xs font-semibold uppercase tracking-wide text-gray-500">
            Categories
          </h2>
          <ul className="space-y-1">
            {/* "All" option */}
            <li>
              <button
                type="button"
                onClick={() => {
                  setSelectedCategory("");
                  setSelectedDocId(null);
                }}
                aria-pressed={selectedCategory === ""}
                aria-label="Show all categories"
                className={[
                  "flex w-full items-center justify-between rounded-md px-3 py-2 text-left text-sm font-medium",
                  selectedCategory === ""
                    ? "bg-indigo-100 text-indigo-800"
                    : "text-gray-700 hover:bg-gray-100",
                ].join(" ")}
              >
                <span>All</span>
                <span className="text-xs text-gray-500">
                  {documents.length}
                </span>
              </button>
            </li>
            {/* Per-category options */}
            {ALL_CATEGORIES.map((cat) => (
              <li key={cat.value}>
                <button
                  type="button"
                  onClick={() => {
                    setSelectedCategory(cat.value);
                    setSelectedDocId(null);
                  }}
                  aria-pressed={selectedCategory === cat.value}
                  aria-label={`Filter by ${cat.label}`}
                  className={[
                    "flex w-full items-center justify-between rounded-md px-3 py-2 text-left text-sm font-medium",
                    selectedCategory === cat.value
                      ? "bg-indigo-100 text-indigo-800"
                      : "text-gray-700 hover:bg-gray-100",
                  ].join(" ")}
                >
                  <span>{cat.label}</span>
                  <span className="text-xs text-gray-500">
                    {categoryCounts.get(cat.value) ?? 0}
                  </span>
                </button>
              </li>
            ))}
          </ul>

          {/* ── Date range filter ─────────────────────────────────────── */}
          <div className="mt-6 border-t border-gray-200 pt-4">
            <h2 className="mb-3 text-xs font-semibold uppercase tracking-wide text-gray-500">
              Date Range
            </h2>
            <div className="space-y-2">
              <div>
                <label htmlFor="date-from" className="mb-0.5 block text-xs text-gray-500">
                  From
                </label>
                <input
                  id="date-from"
                  type="date"
                  value={dateFrom}
                  onChange={(e) => setDateFrom(e.target.value)}
                  className="w-full rounded-md border border-gray-300 px-2 py-1 text-xs text-gray-700 focus:border-indigo-500 focus:outline-none focus:ring-1 focus:ring-indigo-500"
                />
              </div>
              <div>
                <label htmlFor="date-to" className="mb-0.5 block text-xs text-gray-500">
                  To
                </label>
                <input
                  id="date-to"
                  type="date"
                  value={dateTo}
                  onChange={(e) => setDateTo(e.target.value)}
                  className="w-full rounded-md border border-gray-300 px-2 py-1 text-xs text-gray-700 focus:border-indigo-500 focus:outline-none focus:ring-1 focus:ring-indigo-500"
                />
              </div>
              {(dateFrom || dateTo) && (
                <button
                  type="button"
                  onClick={() => {
                    setDateFrom("");
                    setDateTo("");
                  }}
                  className="text-xs text-indigo-600 hover:text-indigo-800"
                >
                  Clear dates
                </button>
              )}
            </div>
          </div>
        </aside>

        {/* ── Main area: document list ──────────────────────────────────── */}
        <main className={[
          "flex-1 overflow-y-auto p-4",
          selectedDoc ? "w-[60%]" : "w-full",
        ].join(" ")}>
          {/* Fetch error */}
          {error && (
            <div className="mb-4 rounded-md border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-700">
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

          {loading ? (
            <div className="animate-pulse space-y-3">
              <div className="h-4 w-1/3 rounded bg-gray-200" />
              <div className="h-8 w-full rounded bg-gray-200" />
              <div className="h-8 w-full rounded bg-gray-200" />
              <div className="h-8 w-full rounded bg-gray-200" />
            </div>
          ) : sortedDocuments.length === 0 ? (
            <p className="text-sm text-gray-500">
              {selectedCategory
                ? `No documents in "${labelFor(selectedCategory)}" category.`
                : "No documents on record."}
            </p>
          ) : (
            <div className="overflow-x-auto">
              <table className="w-full text-sm">
                <thead>
                  <tr className="border-b border-gray-100 text-left text-xs font-medium uppercase tracking-wide text-gray-500">
                    <th className="pb-2 pr-4" aria-sort={sortField === "name" ? (sortDir === "asc" ? "ascending" : "descending") : "none"}>
                      <button
                        type="button"
                        onClick={() => handleSortToggle("name")}
                        aria-label="Sort by name"
                        className="hover:text-gray-700"
                      >
                        File Name{sortArrow("name")}
                      </button>
                    </th>
                    <th className="pb-2 pr-4">Category</th>
                    <th className="pb-2 pr-4" aria-sort={sortField === "date" ? (sortDir === "asc" ? "ascending" : "descending") : "none"}>
                      <button
                        type="button"
                        onClick={() => handleSortToggle("date")}
                        aria-label="Sort by date"
                        className="hover:text-gray-700"
                      >
                        Upload Date{sortArrow("date")}
                      </button>
                    </th>
                    <th className="pb-2 pr-4">Size</th>
                    <th className="pb-2">Actions</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-gray-50">
                  {sortedDocuments.map((doc) => (
                    <tr
                      key={doc.id}
                      onClick={() => setSelectedDocId(doc.id)}
                      className={[
                        "cursor-pointer transition-colors",
                        selectedDocId === doc.id
                          ? "bg-indigo-50"
                          : "hover:bg-gray-50",
                      ].join(" ")}
                    >
                      <td className="py-2.5 pr-4 font-medium text-gray-900">
                        {doc.title}
                      </td>
                      <td className="py-2.5 pr-4">
                        <span
                          className={[
                            "inline-flex rounded-full px-2 py-0.5 text-xs font-medium",
                            badgeClsFor(doc.category),
                          ].join(" ")}
                        >
                          {labelFor(doc.category)}
                        </span>
                      </td>
                      <td className="py-2.5 pr-4 text-gray-600">
                        {formatDate(doc.uploadedAt)}
                      </td>
                      <td className="py-2.5 pr-4 text-gray-600">
                        {formatFileSize(doc.fileSizeBytes)}
                      </td>
                      <td className="py-2.5">
                        <div className="flex items-center gap-2">
                          {/* Re-categorize */}
                          <button
                            type="button"
                            onClick={(e) => {
                              e.stopPropagation();
                              setRecatDocId(doc.id);
                              setRecatCategory(doc.category);
                            }}
                            className="rounded px-2 py-1 text-xs text-indigo-600 hover:bg-indigo-50"
                            title="Re-categorize"
                          >
                            Tag
                          </button>
                          {/* Delete */}
                          <button
                            type="button"
                            onClick={(e) => {
                              e.stopPropagation();
                              handleDelete(doc.id);
                            }}
                            className="rounded px-2 py-1 text-xs text-red-600 hover:bg-red-50"
                            title="Delete"
                            aria-label={`Delete document ${doc.title}`}
                          >
                            Delete
                          </button>
                        </div>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </main>

        {/* ── Right panel: inline preview ───────────────────────────────── */}
        {selectedDoc && (
          <aside className="w-[40%] shrink-0 overflow-y-auto border-l border-gray-200 bg-white p-4">
            <div className="mb-4 flex items-center justify-between">
              <h2 className="text-sm font-semibold text-gray-800 truncate">
                {selectedDoc.title}
              </h2>
              <button
                type="button"
                onClick={() => setSelectedDocId(null)}
                className="rounded p-1 text-gray-400 hover:bg-gray-100 hover:text-gray-600"
                aria-label="Close preview"
              >
                &times;
              </button>
            </div>

            {/* Document metadata */}
            <div className="mb-4 space-y-1 text-xs text-gray-500">
              <p>
                <span className="font-medium text-gray-600">Category:</span>{" "}
                <span
                  className={[
                    "inline-flex rounded-full px-2 py-0.5 text-xs font-medium",
                    badgeClsFor(selectedDoc.category),
                  ].join(" ")}
                >
                  {labelFor(selectedDoc.category)}
                </span>
              </p>
              <p>
                <span className="font-medium text-gray-600">Type:</span>{" "}
                {selectedDoc.contentType}
              </p>
              <p>
                <span className="font-medium text-gray-600">Size:</span>{" "}
                {formatFileSize(selectedDoc.fileSizeBytes)}
              </p>
              <p>
                <span className="font-medium text-gray-600">Uploaded:</span>{" "}
                {formatDate(selectedDoc.uploadedAt)}
              </p>
              <p>
                <span className="font-medium text-gray-600">SHA-1:</span>{" "}
                <span className="font-mono">{selectedDoc.sha1Checksum}</span>
              </p>
            </div>

            {/* Preview content */}
            <div className="rounded-lg border border-gray-200 bg-gray-50 p-2">
              {previewLoading ? (
                <div className="flex h-64 items-center justify-center">
                  <p className="text-sm text-gray-500">Loading preview...</p>
                </div>
              ) : previewError ? (
                <div className="flex h-32 flex-col items-center justify-center gap-2">
                  <p className="text-sm text-red-600">Preview unavailable</p>
                  <p className="text-xs text-gray-500">{previewError}</p>
                </div>
              ) : previewContent?.contentBase64 && isImage(selectedDoc.contentType) ? (
                /* Image preview */
                <div className="flex items-center justify-center">
                  <img
                    src={`data:${selectedDoc.contentType};base64,${previewContent.contentBase64}`}
                    alt={selectedDoc.title}
                    className="max-h-[500px] max-w-full rounded object-contain"
                  />
                </div>
              ) : previewContent?.contentBase64 && isPdf(selectedDoc.contentType) ? (
                /* PDF preview via iframe */
                <div className="h-[500px] w-full">
                  <iframe
                    src={`data:application/pdf;base64,${previewContent.contentBase64}`}
                    title={`Preview: ${selectedDoc.title}`}
                    className="h-full w-full rounded border-0"
                  />
                </div>
              ) : previewContent && !previewContent.contentBase64 ? (
                /* Content not stored (pre-content-storage document) */
                <div className="flex flex-col items-center justify-center gap-3 py-8">
                  <div className="rounded-lg bg-gray-200 p-4">
                    <svg
                      className="h-8 w-8 text-gray-400"
                      fill="none"
                      viewBox="0 0 24 24"
                      stroke="currentColor"
                      strokeWidth={1.5}
                    >
                      <path
                        strokeLinecap="round"
                        strokeLinejoin="round"
                        d="M19.5 14.25v-2.625a3.375 3.375 0 00-3.375-3.375h-1.5A1.125 1.125 0 0113.5 7.125v-1.5a3.375 3.375 0 00-3.375-3.375H8.25m2.25 0H5.625c-.621 0-1.125.504-1.125 1.125v17.25c0 .621.504 1.125 1.125 1.125h12.75c.621 0 1.125-.504 1.125-1.125V11.25a9 9 0 00-9-9z"
                      />
                    </svg>
                  </div>
                  <p className="text-sm font-medium text-gray-700">
                    {selectedDoc.title}
                  </p>
                  <p className="text-xs text-gray-500">
                    Preview not available for this document.
                  </p>
                  <p className="text-xs text-gray-400">
                    {selectedDoc.contentType} &middot;{" "}
                    {formatFileSize(selectedDoc.fileSizeBytes)}
                  </p>
                </div>
              ) : (
                /* Other file types with content: info card + download */
                <div className="flex flex-col items-center justify-center gap-3 py-8">
                  <div className="rounded-lg bg-gray-200 p-4">
                    <svg
                      className="h-8 w-8 text-gray-400"
                      fill="none"
                      viewBox="0 0 24 24"
                      stroke="currentColor"
                      strokeWidth={1.5}
                    >
                      <path
                        strokeLinecap="round"
                        strokeLinejoin="round"
                        d="M19.5 14.25v-2.625a3.375 3.375 0 00-3.375-3.375h-1.5A1.125 1.125 0 0113.5 7.125v-1.5a3.375 3.375 0 00-3.375-3.375H8.25m2.25 0H5.625c-.621 0-1.125.504-1.125 1.125v17.25c0 .621.504 1.125 1.125 1.125h12.75c.621 0 1.125-.504 1.125-1.125V11.25a9 9 0 00-9-9z"
                      />
                    </svg>
                  </div>
                  <p className="text-sm font-medium text-gray-700">
                    {selectedDoc.title}
                  </p>
                  <p className="text-xs text-gray-500">
                    {selectedDoc.contentType} &middot;{" "}
                    {formatFileSize(selectedDoc.fileSizeBytes)}
                  </p>
                  {previewContent?.contentBase64 && (
                    <button
                      type="button"
                      onClick={handleDownload}
                      className="mt-2 rounded-md bg-indigo-600 px-4 py-2 text-sm font-medium text-white hover:bg-indigo-700 focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:ring-offset-2"
                    >
                      Download
                    </button>
                  )}
                </div>
              )}
            </div>

            {/* Download button for all previewable types too */}
            {previewContent?.contentBase64 && (
              <div className="mt-3">
                <button
                  type="button"
                  onClick={handleDownload}
                  className="w-full rounded-md border border-gray-300 bg-white px-4 py-2 text-sm font-medium text-gray-700 shadow-sm hover:bg-gray-50 focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:ring-offset-2"
                >
                  Download File
                </button>
              </div>
            )}
          </aside>
        )}
      </div>

      {/* ── Upload modal ──────────────────────────────────────────────────── */}
      {showUploadModal && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40" role="dialog" aria-modal="true" aria-labelledby="upload-modal-title">
          <div className="w-full max-w-md rounded-lg bg-white p-6 shadow-xl">
            <h3 id="upload-modal-title" className="mb-4 text-base font-semibold text-gray-900">
              Upload Document
            </h3>

            {/* Title */}
            <div className="mb-4">
              <label htmlFor="upload-title" className="mb-1 block text-sm font-medium text-gray-700">
                Title <span className="text-red-500">*</span>
              </label>
              <input
                id="upload-title"
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
              <label htmlFor="upload-category" className="mb-1 block text-sm font-medium text-gray-700">
                Category
              </label>
              <select
                id="upload-category"
                value={uploadCategory}
                onChange={(e) => setUploadCategory(e.target.value)}
                className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm text-gray-900 focus:outline-none focus:ring-2 focus:ring-indigo-500"
              >
                {ALL_CATEGORIES.map((cat) => (
                  <option key={cat.value} value={cat.value}>
                    {cat.label}
                  </option>
                ))}
              </select>
            </div>

            {/* Patient (auto-filled, read-only) */}
            <div className="mb-4">
              <label htmlFor="upload-patient-id" className="mb-1 block text-sm font-medium text-gray-700">
                Patient ID
              </label>
              <input
                id="upload-patient-id"
                type="text"
                value={patientId}
                readOnly
                className="w-full rounded-md border border-gray-200 bg-gray-50 px-3 py-2 text-sm text-gray-500"
              />
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
                {uploading ? "Uploading\u2026" : "Upload"}
              </button>
            </div>
          </div>
        </div>
      )}

      {/* ── Re-categorize modal ───────────────────────────────────────────── */}
      {recatDocId !== null && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40" role="dialog" aria-modal="true" aria-labelledby="recat-modal-title">
          <div className="w-full max-w-sm rounded-lg bg-white p-6 shadow-xl">
            <h3 id="recat-modal-title" className="mb-4 text-base font-semibold text-gray-900">
              Re-categorize Document
            </h3>

            <div className="mb-4">
              <label htmlFor="recat-category" className="mb-1 block text-sm font-medium text-gray-700">
                New Category
              </label>
              <select
                id="recat-category"
                value={recatCategory}
                onChange={(e) => setRecatCategory(e.target.value)}
                className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm text-gray-900 focus:outline-none focus:ring-2 focus:ring-indigo-500"
              >
                {ALL_CATEGORIES.map((cat) => (
                  <option key={cat.value} value={cat.value}>
                    {cat.label}
                  </option>
                ))}
              </select>
            </div>

            <div className="flex justify-end gap-3">
              <button
                type="button"
                onClick={() => {
                  setRecatDocId(null);
                  setRecatCategory("");
                }}
                className="rounded-md border border-gray-300 px-4 py-2 text-sm font-medium text-gray-700 hover:bg-gray-50 focus:outline-none focus:ring-2 focus:ring-gray-400 focus:ring-offset-2"
              >
                Cancel
              </button>
              <button
                type="button"
                onClick={async () => {
                  if (!recatDocId) return;
                  try {
                    const doc = documents.find((d) => d.id === recatDocId);
                    if (!doc) return;
                    // Update the resource's category field via updateResource
                    const updatedResource = { ...doc.resource };
                    // Set category in the FHIR resource
                    updatedResource["category"] = [
                      {
                        coding: [
                          {
                            system: "http://medarc.local/document-category",
                            code: recatCategory,
                          },
                        ],
                        text: labelFor(recatCategory),
                      },
                    ];
                    await commands.updateResource({
                      id: doc.id,
                      resource: updatedResource,
                    });
                    setRecatDocId(null);
                    setRecatCategory("");
                    setRefreshCounter((n) => n + 1);
                  } catch (e) {
                    const msg = e instanceof Error ? e.message : String(e);
                    console.error("[DocumentCenterPage] re-categorize failed:", msg);
                    setError("Re-categorize failed: " + msg);
                    setRecatDocId(null);
                  }
                }}
                className="rounded-md bg-indigo-600 px-4 py-2 text-sm font-medium text-white hover:bg-indigo-700 focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:ring-offset-2"
              >
                Save
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

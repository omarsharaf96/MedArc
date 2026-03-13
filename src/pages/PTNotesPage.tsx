/**
 * PTNotesPage.tsx — PT note list page for a patient.
 *
 * Provider-only access: displays Initial Evaluations, Progress Notes,
 * and Discharge Summaries in collapsible sections with status badges.
 * Providers can create new notes of any type and navigate to existing ones.
 *
 * RBAC: Only "Provider" and "SystemAdmin" roles have access.
 * All other roles see an "Access denied" message.
 *
 * Observability:
 *   - listPtNotes failures surface as an inline error banner with Retry.
 *   - Status badges give at-a-glance lifecycle state (draft/signed/locked).
 *   - Backend audit rows (`pt_note.*`) are written on every create/update/cosign/lock.
 */
import { useState, useEffect } from "react";
import { commands } from "../lib/tauri";
import { useNav } from "../contexts/RouterContext";
import type { PtNoteRecord, PtNoteType, PtNoteStatus } from "../types/pt";

// ─── Props ───────────────────────────────────────────────────────────────────

interface PTNotesPageProps {
  patientId: string;
  role: string;
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/** Map lifecycle status to a Tailwind badge style. */
function statusBadgeClass(status: PtNoteStatus): string {
  switch (status) {
    case "draft":
      return "bg-gray-100 text-gray-700";
    case "signed":
      return "bg-blue-100 text-blue-800";
    case "locked":
      return "bg-green-100 text-green-800";
  }
}

/** Format a date string (ISO 8601) as YYYY-MM-DD. */
function formatDate(iso: string): string {
  if (iso.length >= 10) return iso.slice(0, 10);
  return iso;
}

// ─── Sub-components ──────────────────────────────────────────────────────────

/** Status pill badge. */
function StatusBadge({ status }: { status: PtNoteStatus }) {
  return (
    <span
      className={[
        "inline-flex rounded-full px-2 py-0.5 text-xs font-medium capitalize",
        statusBadgeClass(status),
      ].join(" ")}
    >
      {status}
    </span>
  );
}

/** A single collapsible section grouping notes of one type. */
function NoteSection({
  title,
  noteType,
  notes,
  patientId,
  onNewNote,
  newLabel,
}: {
  title: string;
  noteType: PtNoteType;
  notes: PtNoteRecord[];
  patientId: string;
  onNewNote: (noteType: PtNoteType) => void;
  newLabel: string;
}) {
  const [open, setOpen] = useState(true);
  const { navigate } = useNav();

  return (
    <div className="rounded-lg border border-gray-200 bg-white shadow-sm">
      {/* Section header */}
      <div className="flex items-center justify-between px-5 py-3">
        <button
          type="button"
          onClick={() => setOpen((v) => !v)}
          className="flex items-center gap-2 text-left text-sm font-semibold text-gray-800 hover:text-gray-900 focus:outline-none focus:ring-2 focus:ring-indigo-400 focus:ring-offset-1 rounded"
        >
          <span className={["transition-transform", open ? "rotate-90" : ""].join(" ")}>
            ▶
          </span>
          {title}
          <span className="ml-1 rounded-full bg-gray-100 px-2 py-0.5 text-xs font-normal text-gray-500">
            {notes.length}
          </span>
        </button>
        <button
          type="button"
          onClick={() => onNewNote(noteType)}
          className="rounded-md bg-indigo-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-indigo-700 focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:ring-offset-2"
        >
          {newLabel}
        </button>
      </div>

      {/* Note rows */}
      {open && (
        <div className="border-t border-gray-100">
          {notes.length === 0 ? (
            <p className="px-5 py-4 text-sm text-gray-500">No {title.toLowerCase()} on file.</p>
          ) : (
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b border-gray-100 text-left text-xs font-medium uppercase tracking-wide text-gray-500">
                  <th className="px-5 pb-2 pt-3">Date</th>
                  <th className="pb-2 pt-3">Status</th>
                  <th className="pb-2 pt-3 pr-5 text-right">Action</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-gray-50">
                {notes.map((note) => (
                  <tr
                    key={note.id}
                    className="group hover:bg-indigo-50 transition-colors"
                  >
                    <td className="px-5 py-3 text-gray-700">
                      {formatDate(note.createdAt)}
                    </td>
                    <td className="py-3">
                      <StatusBadge status={note.status} />
                    </td>
                    <td className="py-3 pr-5 text-right">
                      <button
                        type="button"
                        onClick={() =>
                          navigate({
                            page: "pt-note-detail",
                            patientId,
                            noteType: note.noteType,
                            ptNoteId: note.id,
                          })
                        }
                        className="rounded text-xs text-indigo-600 hover:text-indigo-800 focus:outline-none focus:ring-2 focus:ring-indigo-400 focus:ring-offset-1"
                      >
                        Open →
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>
      )}
    </div>
  );
}

// ─── Main component ──────────────────────────────────────────────────────────

export function PTNotesPage({ patientId, role }: PTNotesPageProps) {
  const { goBack, navigate } = useNav();

  // ── RBAC guard ─────────────────────────────────────────────────────────
  const canAccess = role === "Provider" || role === "SystemAdmin";

  // ── State ──────────────────────────────────────────────────────────────
  const [notes, setNotes] = useState<PtNoteRecord[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [refreshKey, setRefreshKey] = useState(0);

  // ── Fetch notes on mount and refresh ──────────────────────────────────
  useEffect(() => {
    if (!canAccess) return;

    let mounted = true;
    setLoading(true);
    setError(null);

    commands
      .listPtNotes(patientId)
      .then((result) => {
        if (!mounted) return;
        setNotes(result);
      })
      .catch((e) => {
        if (!mounted) return;
        const msg = e instanceof Error ? e.message : String(e);
        console.error(`[PTNotesPage] listPtNotes failed for patient ${patientId}:`, msg);
        setError(msg);
        setNotes([]);
      })
      .finally(() => {
        if (mounted) setLoading(false);
      });

    return () => {
      mounted = false;
    };
  }, [patientId, canAccess, refreshKey]);

  // ── Navigation helpers ─────────────────────────────────────────────────
  function handleNewNote(noteType: PtNoteType) {
    navigate({ page: "pt-note-detail", patientId, noteType, ptNoteId: "new" });
  }

  // ── Access denied ──────────────────────────────────────────────────────
  if (!canAccess) {
    return (
      <div className="p-6">
        <button
          type="button"
          onClick={goBack}
          className="mb-4 rounded-md p-1.5 text-gray-500 hover:bg-gray-100 hover:text-gray-700 focus:outline-none focus:ring-2 focus:ring-gray-400 focus:ring-offset-1"
        >
          ← Back to Patient
        </button>
        <div className="rounded-lg border border-red-200 bg-red-50 px-5 py-4">
          <p className="font-semibold text-red-700">Access denied</p>
          <p className="mt-1 text-sm text-red-600">
            PT Notes are only accessible to Providers and System Administrators.
          </p>
        </div>
      </div>
    );
  }

  // ── Partition notes by type ────────────────────────────────────────────
  const initialEvals = notes.filter((n) => n.noteType === "initial_eval");
  const progressNotes = notes.filter((n) => n.noteType === "progress_note");
  const dischargeSummaries = notes.filter((n) => n.noteType === "discharge_summary");

  return (
    <div className="space-y-6 p-6">
      {/* ── Header ──────────────────────────────────────────────────────── */}
      <div className="flex items-center gap-3">
        <button
          type="button"
          onClick={goBack}
          className="rounded-md p-1.5 text-gray-500 hover:bg-gray-100 hover:text-gray-700 focus:outline-none focus:ring-2 focus:ring-gray-400 focus:ring-offset-1"
          aria-label="Back to patient"
        >
          ← Back to Patient
        </button>
        <h1 className="text-xl font-bold text-gray-900">PT Notes</h1>
      </div>

      {/* ── Loading spinner ──────────────────────────────────────────────── */}
      {loading && (
        <div className="flex items-center gap-3 py-8 text-sm text-gray-500">
          <svg
            className="h-5 w-5 animate-spin text-indigo-500"
            xmlns="http://www.w3.org/2000/svg"
            fill="none"
            viewBox="0 0 24 24"
          >
            <circle
              className="opacity-25"
              cx="12"
              cy="12"
              r="10"
              stroke="currentColor"
              strokeWidth="4"
            />
            <path
              className="opacity-75"
              fill="currentColor"
              d="M4 12a8 8 0 018-8v8H4z"
            />
          </svg>
          Loading PT notes…
        </div>
      )}

      {/* ── Error banner ─────────────────────────────────────────────────── */}
      {error && !loading && (
        <div className="rounded-md border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700">
          <p className="font-semibold">Failed to load PT notes</p>
          <p className="mt-0.5">{error}</p>
          <button
            type="button"
            onClick={() => setRefreshKey((k) => k + 1)}
            className="mt-2 rounded bg-red-600 px-3 py-1 text-xs font-medium text-white hover:bg-red-700"
          >
            Retry
          </button>
        </div>
      )}

      {/* ── Note sections ─────────────────────────────────────────────────── */}
      {!loading && !error && (
        <>
          <NoteSection
            title="Initial Evaluations"
            noteType="initial_eval"
            notes={initialEvals}
            patientId={patientId}
            onNewNote={handleNewNote}
            newLabel="New IE"
          />
          <NoteSection
            title="Progress Notes"
            noteType="progress_note"
            notes={progressNotes}
            patientId={patientId}
            onNewNote={handleNewNote}
            newLabel="New Progress Note"
          />
          <NoteSection
            title="Discharge Summaries"
            noteType="discharge_summary"
            notes={dischargeSummaries}
            patientId={patientId}
            onNewNote={handleNewNote}
            newLabel="New Discharge Summary"
          />
        </>
      )}
    </div>
  );
}

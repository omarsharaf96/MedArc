/**
 * EncounterWorkspace.tsx — Clinical encounter workspace.
 *
 * SOAP-structured note editor with split and merged view modes:
 *   - Four labeled textareas for Subjective / Objective / Assessment / Plan
 *   - Optional merged single-textarea view via toggle
 *   - Auto-detects SOAP sections in legacy single-field text
 *   - Per-section dictation support (useDictation hook)
 *   - Finalized encounters show a PDF preview inline
 *   - "Edit" button triggers amendment mode to return to the editable view
 *
 * RBAC context is passed in as props (from ContentArea via useAuth).
 *
 * Observability:
 *   - `console.error("[useEncounter] ...")` logged by the hook on fetch failure
 *   - Inline error banner with "Retry" button visible without DevTools
 *   - `soapState`, `savingSoap`, `soapSaveError`, `isFinalized`
 *     all visible as component state on EncounterWorkspace in React DevTools
 */
import { useState, useEffect, useCallback } from "react";
import { useEncounter } from "../hooks/useEncounter";
import { useNav } from "../contexts/RouterContext";
import { commands } from "../lib/tauri";
import { save } from "@tauri-apps/plugin-dialog";
import { copyFile, readFile } from "@tauri-apps/plugin-fs";
import { AuthAlertBanner } from "../components/clinical/AuthTrackingPanel";
import { TherapyCapBanner } from "../components/clinical/TherapyCapBanner";
import { useDictation } from "../hooks/useDictation";
import type { SoapInput } from "../types/documentation";
import type { FaxContact } from "../types/fax";

// ─── Props ───────────────────────────────────────────────────────────────────

interface EncounterWorkspaceProps {
  patientId: string;
  encounterId: string;
  role: string;
  userId: string;
}

// ─── Tailwind class constants ────────────────────────────────────────────────

const INPUT_CLS =
  "w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500";
const LABEL_CLS = "mb-1 block text-sm font-medium text-gray-700";

// ─── Helpers ─────────────────────────────────────────────────────────────────

/** Format encounter type string for display: "office_visit" -> "Office Visit" */
function formatEncounterType(raw: string): string {
  return raw
    .split("_")
    .map((w) => w.charAt(0).toUpperCase() + w.slice(1))
    .join(" ");
}

/** Extract the encounter date (YYYY-MM-DD) from a FHIR resource object. */
function extractEncounterDate(resource: Record<string, unknown>): string | null {
  const period = resource["period"] as Record<string, unknown> | undefined;
  const start = period?.["start"];
  if (typeof start === "string" && start.length >= 10) {
    return start.slice(0, 10);
  }
  const date = resource["date"];
  if (typeof date === "string" && date.length >= 10) {
    return date.slice(0, 10);
  }
  return null;
}

/** Extract encounter type from FHIR resource. */
function extractEncounterTypeFromResource(
  resource: Record<string, unknown>,
): string | null {
  const types = resource["type"] as Array<Record<string, unknown>> | undefined;
  const typeText = types?.[0]?.["text"];
  if (typeof typeText === "string") return typeText;
  // Fallback: check coding[0].display or coding[0].code (for pre-existing encounters without text)
  const coding = types?.[0]?.["coding"] as Array<Record<string, unknown>> | undefined;
  const codingDisplay = coding?.[0]?.["display"];
  if (typeof codingDisplay === "string") return codingDisplay;
  const codingCode = coding?.[0]?.["code"];
  if (typeof codingCode === "string") return codingCode;

  const cls = resource["class"] as Record<string, unknown> | undefined;
  const code = cls?.["code"];
  if (typeof code === "string") return formatEncounterType(code);

  return null;
}

/**
 * Merge all four SOAP fields into a single note string for display.
 * When loading an encounter that was saved with separate SOAP fields,
 * combine them into one text block with section headers.
 */
function mergeNoteContent(soap: SoapInput): string {
  const parts: string[] = [];
  if (soap.subjective?.trim()) parts.push(`SUBJECTIVE:\n${soap.subjective.trim()}`);
  if (soap.objective?.trim()) parts.push(`OBJECTIVE:\n${soap.objective.trim()}`);
  if (soap.assessment?.trim()) parts.push(`ASSESSMENT:\n${soap.assessment.trim()}`);
  if (soap.plan?.trim()) parts.push(`PLAN:\n${soap.plan.trim()}`);
  // Fallback: if no sections have headers, just join the raw text
  if (parts.length === 0) {
    const raw: string[] = [];
    if (soap.subjective?.trim()) raw.push(soap.subjective.trim());
    if (soap.objective?.trim()) raw.push(soap.objective.trim());
    if (soap.assessment?.trim()) raw.push(soap.assessment.trim());
    if (soap.plan?.trim()) raw.push(soap.plan.trim());
    return raw.join("\n\n");
  }
  return parts.join("\n\n");
}

/** SOAP section keys in display order. */
type SoapKey = "subjective" | "objective" | "assessment" | "plan";

/** SOAP section display metadata. */
const SOAP_SECTIONS: { key: SoapKey; label: string; placeholder: string }[] = [
  {
    key: "subjective",
    label: "SUBJECTIVE",
    placeholder: "Patient complaints, symptoms, self-reported history...",
  },
  {
    key: "objective",
    label: "OBJECTIVE",
    placeholder: "Exam findings, measurements, treatments performed...",
  },
  {
    key: "assessment",
    label: "ASSESSMENT",
    placeholder: "Clinical assessment, progress status, diagnosis...",
  },
  {
    key: "plan",
    label: "PLAN",
    placeholder: "Treatment plan, goals, next steps, follow-up...",
  },
];

/**
 * Parse SOAP sections from a single text string.
 * Detects section headers like "SUBJECTIVE:", "S:", "Subjective:", etc.
 * and splits the text into the four SOAP fields.
 *
 * If no section headers are detected, returns the entire text in subjective.
 */
function parseSoapFromText(text: string): SoapInput {
  const sections: SoapInput = {
    subjective: null,
    objective: null,
    assessment: null,
    plan: null,
  };

  if (!text || !text.trim()) return sections;

  const markers: { key: SoapKey; patterns: RegExp[] }[] = [
    { key: "subjective", patterns: [/^SUBJECTIVE:/im, /^S:/im] },
    { key: "objective", patterns: [/^OBJECTIVE:/im, /^O:/im] },
    { key: "assessment", patterns: [/^ASSESSMENT:/im, /^A:/im] },
    { key: "plan", patterns: [/^PLAN:/im, /^P:/im] },
  ];

  // Find position of each marker in the text
  const found: { key: SoapKey; pos: number; matchLen: number }[] = [];
  for (const marker of markers) {
    for (const pattern of marker.patterns) {
      const match = pattern.exec(text);
      if (match) {
        found.push({ key: marker.key, pos: match.index, matchLen: match[0].length });
        break; // Use the first matching pattern for this section
      }
    }
  }

  // If no section headers found, put everything in subjective
  if (found.length === 0) {
    sections.subjective = text.trim();
    return sections;
  }

  // Sort by position
  found.sort((a, b) => a.pos - b.pos);

  // Extract text between markers
  for (let i = 0; i < found.length; i++) {
    const start = found[i].pos + found[i].matchLen;
    const end = i + 1 < found.length ? found[i + 1].pos : text.length;
    const content = text.slice(start, end).trim();
    if (content) {
      sections[found[i].key] = content;
    }
  }

  // If there's text before the first marker, prepend it to the first section
  if (found[0].pos > 0) {
    const preamble = text.slice(0, found[0].pos).trim();
    if (preamble) {
      const firstKey = found[0].key;
      sections[firstKey] = preamble + (sections[firstKey] ? "\n\n" + sections[firstKey] : "");
    }
  }

  return sections;
}

/** View mode for the note editor. */
type NoteViewMode = "split" | "merged";

// ─── Loading skeleton ────────────────────────────────────────────────────────

function LoadingSkeleton() {
  return (
    <div className="animate-pulse space-y-4 p-6">
      <div className="h-8 w-1/3 rounded bg-gray-200" />
      <div className="h-4 w-1/2 rounded bg-gray-200" />
      <div className="h-64 rounded bg-gray-200" />
    </div>
  );
}

// ─── Dictation Button ─────────────────────────────────────────────────────────

interface DictationButtonProps {
  /** Called with transcribed text to append to a field. */
  onTranscript: (text: string) => void;
  disabled?: boolean;
  /** Compact size for individual section textareas. */
  size?: "sm" | "md";
}

function DictationButton({ onTranscript, disabled, size = "sm" }: DictationButtonProps) {
  const { isRecording, isTranscribing, toggle, error } = useDictation({
    onTranscript,
  });

  const sizeClasses =
    size === "sm"
      ? "px-2 py-1 text-xs"
      : "px-3 py-1.5 text-sm";

  return (
    <div className="inline-flex items-center gap-1.5">
      <button
        type="button"
        onClick={() => void toggle()}
        disabled={disabled || isTranscribing}
        className={[
          "rounded-md font-medium focus:outline-none focus:ring-2 focus:ring-offset-1 disabled:opacity-60",
          sizeClasses,
          isRecording
            ? "bg-red-600 text-white hover:bg-red-700 focus:ring-red-500 animate-pulse"
            : "border border-gray-300 bg-white text-gray-700 hover:bg-gray-50 focus:ring-indigo-500",
        ].join(" ")}
        title={isRecording ? "Stop recording" : isTranscribing ? "Transcribing..." : "Start dictation"}
      >
        {isTranscribing ? "Transcribing..." : isRecording ? "Stop" : "Dictate"}
      </button>
      {error && (
        <span className="text-xs text-red-500 max-w-[200px] truncate" title={error}>
          {error}
        </span>
      )}
    </div>
  );
}

// ─── Note Editor ──────────────────────────────────────────────────────────────

interface NoteEditorProps {
  encounterId: string;
  role: string;
  noteContent: string;
  setNoteContent: (s: string) => void;
  saveSoap: (soap: SoapInput, amendmentReason?: string | null) => Promise<void>;
  finalizeEncounter: (soap: SoapInput) => Promise<void>;
  isFinalized: boolean;
  isAmending: boolean;
  templates: import("../types/documentation").TemplateRecord[];
  soapState: SoapInput;
  setSoapState: (s: SoapInput) => void;
}

function NoteEditor({
  encounterId: _encounterId,
  role,
  noteContent,
  setNoteContent,
  saveSoap,
  finalizeEncounter,
  isFinalized,
  isAmending,
  templates,
  soapState,
  setSoapState,
}: NoteEditorProps) {
  // ── View mode state ───────────────────────────────────────────────
  const [viewMode, setViewMode] = useState<NoteViewMode>("split");

  // ── Collapsible section state ─────────────────────────────────────
  const [collapsedSections, setCollapsedSections] = useState<Record<SoapKey, boolean>>({
    subjective: false,
    objective: false,
    assessment: false,
    plan: false,
  });

  const toggleSection = useCallback((key: SoapKey) => {
    setCollapsedSections((prev) => ({ ...prev, [key]: !prev[key] }));
  }, []);

  // ── Save state ────────────────────────────────────────────────────
  const [savingSoap, setSavingSoap] = useState(false);
  const [soapSaveError, setSoapSaveError] = useState<string | null>(null);

  // ── Finalize state ────────────────────────────────────────────────
  const [finalizing, setFinalizing] = useState(false);
  const [finalizeError, setFinalizeError] = useState<string | null>(null);

  // ── Template picker state ─────────────────────────────────────────
  const [loadingTemplate, setLoadingTemplate] = useState(false);

  // RBAC: NurseMa and BillingStaff get read-only mode
  const isReadOnly =
    (isFinalized && !isAmending) || role === "NurseMa" || role === "BillingStaff";

  // ── Helper: update a single SOAP field ────────────────────────────
  const updateSoapField = useCallback(
    (key: SoapKey, value: string) => {
      const updated = { ...soapState, [key]: value || null };
      setSoapState(updated);
      // Keep merged noteContent in sync
      setNoteContent(mergeNoteContent(updated));
    },
    [soapState, setSoapState, setNoteContent],
  );

  // ── Sync from merged view back to split fields ────────────────────
  const syncMergedToSplit = useCallback(
    (text: string) => {
      setNoteContent(text);
      const parsed = parseSoapFromText(text);
      setSoapState(parsed);
    },
    [setNoteContent, setSoapState],
  );

  // ── Template picker onChange ────────────────────────────────────────
  const handleTemplateChange = useCallback(
    async (templateId: string) => {
      if (!templateId) return;
      try {
        setLoadingTemplate(true);
        const tpl = await commands.getTemplate(templateId);
        setSoapState(tpl.defaultSoap);
        setNoteContent(mergeNoteContent(tpl.defaultSoap));
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e);
        setSoapSaveError(`Failed to load template: ${msg}`);
      } finally {
        setLoadingTemplate(false);
      }
    },
    [setNoteContent, setSoapState],
  );

  // ── Build current SOAP from state ─────────────────────────────────
  const getCurrentSoap = useCallback((): SoapInput => {
    if (viewMode === "merged") {
      // In merged mode, parse the single text into SOAP sections
      return parseSoapFromText(noteContent);
    }
    // In split mode, use soapState directly
    return {
      subjective: soapState.subjective || null,
      objective: soapState.objective || null,
      assessment: soapState.assessment || null,
      plan: soapState.plan || null,
    };
  }, [viewMode, noteContent, soapState]);

  // ── Save Note ─────────────────────────────────────────────────────
  const handleSave = useCallback(async () => {
    setSavingSoap(true);
    setSoapSaveError(null);
    try {
      const soap = getCurrentSoap();
      await saveSoap(soap, isAmending ? "Amended" : null);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setSoapSaveError(msg);
      console.error("[EncounterWorkspace] saveSoap failed:", msg);
    } finally {
      setSavingSoap(false);
    }
  }, [saveSoap, getCurrentSoap, isAmending]);

  // ── Finalize Encounter ────────────────────────────────────────────
  const handleFinalize = useCallback(async () => {
    setFinalizing(true);
    setFinalizeError(null);
    try {
      const soap = getCurrentSoap();
      await finalizeEncounter(soap);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setFinalizeError(msg);
      console.error("[EncounterWorkspace] finalizeEncounter failed:", msg);
    } finally {
      setFinalizing(false);
    }
  }, [finalizeEncounter, getCurrentSoap]);

  return (
    <div className="space-y-5">
      {/* ── Finalized badge ────────────────────────────────────────────── */}
      {isFinalized && !isAmending && (
        <div className="flex items-center gap-2 rounded-md border border-green-200 bg-green-50 px-4 py-2 text-sm font-medium text-green-700">
          <span>Finalized — click "Amend Encounter" in the header to make changes</span>
        </div>
      )}

      {/* ── Amendment mode notice ─────────────────────────────────────── */}
      {isAmending && (
        <div className="rounded-md border border-amber-200 bg-amber-50 px-4 py-3 text-sm">
          <p className="font-medium text-amber-800">Amendment Mode</p>
          <p className="mt-0.5 text-amber-700">
            You are amending a finalized encounter.
            The previous version will be preserved for audit trail.
          </p>
        </div>
      )}

      {/* ── Role read-only notice ─────────────────────────────────────── */}
      {!isFinalized && !isAmending && (role === "NurseMa" || role === "BillingStaff") && (
        <p className="text-xs text-gray-400 italic">Read-only for your role</p>
      )}

      {/* ── Template picker ───────────────────────────────────────────── */}
      {templates.length > 0 && !isReadOnly && (
        <div>
          <label className={LABEL_CLS} htmlFor="template-select">
            Note template
          </label>
          <select
            id="template-select"
            className={INPUT_CLS}
            defaultValue=""
            disabled={isReadOnly || loadingTemplate}
            onChange={(e) => {
              void handleTemplateChange(e.target.value);
              e.target.value = "";
            }}
          >
            <option value="">— Select template —</option>
            {templates.map((tpl) => (
              <option key={tpl.id} value={tpl.id}>
                {tpl.name}
                {tpl.specialty ? ` (${tpl.specialty})` : ""}
              </option>
            ))}
          </select>
        </div>
      )}

      {/* ── View mode toggle ──────────────────────────────────────────── */}
      <div className="flex items-center gap-1">
        <span className="mr-2 text-xs font-medium text-gray-500 uppercase tracking-wide">View:</span>
        <button
          type="button"
          onClick={() => setViewMode("split")}
          className={[
            "rounded-l-md px-3 py-1 text-xs font-medium border focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:ring-offset-1",
            viewMode === "split"
              ? "bg-indigo-600 text-white border-indigo-600"
              : "bg-white text-gray-600 border-gray-300 hover:bg-gray-50",
          ].join(" ")}
        >
          Split View
        </button>
        <button
          type="button"
          onClick={() => {
            // Sync current split state to merged content before switching
            setNoteContent(mergeNoteContent(soapState));
            setViewMode("merged");
          }}
          className={[
            "rounded-r-md px-3 py-1 text-xs font-medium border focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:ring-offset-1",
            viewMode === "merged"
              ? "bg-indigo-600 text-white border-indigo-600"
              : "bg-white text-gray-600 border-gray-300 hover:bg-gray-50",
          ].join(" ")}
        >
          Merged View
        </button>
      </div>

      {/* ── Split View: 4 SOAP section textareas ──────────────────────── */}
      {viewMode === "split" && (
        <div className="space-y-3">
          {SOAP_SECTIONS.map(({ key, label, placeholder }) => {
            const isCollapsed = collapsedSections[key];
            const content = soapState[key] ?? "";
            const hasContent = !!content.trim();

            return (
              <div
                key={key}
                className="rounded-lg border border-gray-200 bg-gray-50/50 overflow-hidden"
              >
                {/* Section header — clickable to collapse/expand */}
                <button
                  type="button"
                  onClick={() => toggleSection(key)}
                  className="flex w-full items-center justify-between px-3 py-2 text-left hover:bg-gray-100 focus:outline-none focus:ring-2 focus:ring-inset focus:ring-indigo-500"
                >
                  <div className="flex items-center gap-2">
                    <span
                      className={[
                        "text-xs font-bold tracking-wider",
                        hasContent ? "text-indigo-700" : "text-gray-500",
                      ].join(" ")}
                    >
                      {label}
                    </span>
                    {hasContent && (
                      <span className="inline-block h-1.5 w-1.5 rounded-full bg-indigo-500" />
                    )}
                  </div>
                  <span className="text-xs text-gray-400">
                    {isCollapsed ? "+" : "\u2212"}
                  </span>
                </button>

                {/* Section body */}
                {!isCollapsed && (
                  <div className="border-t border-gray-200 px-3 py-2">
                    <div className="flex items-center justify-end mb-1">
                      {!isReadOnly && (
                        <DictationButton
                          onTranscript={(text) => {
                            const current = soapState[key] ?? "";
                            const separator = current.trim() ? " " : "";
                            updateSoapField(key, current + separator + text);
                          }}
                          disabled={isReadOnly}
                        />
                      )}
                    </div>
                    <textarea
                      id={`soap-${key}`}
                      className={[
                        INPUT_CLS,
                        "bg-white min-h-[100px] resize-y",
                      ].join(" ")}
                      rows={4}
                      readOnly={isReadOnly}
                      value={content}
                      onChange={(e) => updateSoapField(key, e.target.value)}
                      placeholder={isReadOnly ? "" : placeholder}
                    />
                  </div>
                )}
              </div>
            );
          })}
        </div>
      )}

      {/* ── Merged View: single textarea ──────────────────────────────── */}
      {viewMode === "merged" && (
        <div>
          <div className="flex items-center justify-between mb-1">
            <label className={LABEL_CLS} htmlFor="encounter-note-merged">
              Clinical Note
            </label>
            {!isReadOnly && (
              <DictationButton
                onTranscript={(text) => {
                  const separator = noteContent.trim() ? " " : "";
                  syncMergedToSplit(noteContent + separator + text);
                }}
                disabled={isReadOnly}
                size="md"
              />
            )}
          </div>
          <textarea
            id="encounter-note-merged"
            className={INPUT_CLS}
            rows={20}
            readOnly={isReadOnly}
            value={noteContent}
            onChange={(e) => syncMergedToSplit(e.target.value)}
            placeholder={isReadOnly ? "" : "Enter your clinical note here..."}
          />
        </div>
      )}

      {/* ── Save error ───────────────────────────────────────────────────── */}
      {soapSaveError && (
        <p className="text-sm text-red-600">{soapSaveError}</p>
      )}
      {finalizeError && (
        <p className="text-sm text-red-600">{finalizeError}</p>
      )}

      {/* ── Action buttons ────────────────────────────────────────────────── */}
      {!isReadOnly && (
        <div className="flex flex-wrap items-center gap-3 pt-1">
          {/* Save Note / Save Amendment */}
          <button
            type="button"
            onClick={() => void handleSave()}
            disabled={savingSoap || finalizing}
            className={[
              "rounded-md px-4 py-2 text-sm font-medium focus:outline-none focus:ring-2 focus:ring-offset-2 disabled:opacity-60",
              isAmending
                ? "bg-amber-600 text-white hover:bg-amber-700 focus:ring-amber-500"
                : "bg-indigo-600 text-white hover:bg-indigo-700 focus:ring-indigo-500",
            ].join(" ")}
          >
            {savingSoap
              ? "Saving..."
              : isAmending
                ? "Save Amendment"
                : "Save Note"}
          </button>

          {/* Finalize Encounter — hidden when amending */}
          {!isAmending && (
            <button
              type="button"
              onClick={() => void handleFinalize()}
              disabled={savingSoap || finalizing}
              className="rounded-md border-2 border-amber-500 bg-white px-4 py-2 text-sm font-medium text-amber-700 hover:bg-amber-50 focus:outline-none focus:ring-2 focus:ring-amber-400 focus:ring-offset-2 disabled:opacity-60"
            >
              {finalizing ? "Finalizing..." : "Finalize Encounter"}
            </button>
          )}
        </div>
      )}
    </div>
  );
}

// ─── PDF Preview ──────────────────────────────────────────────────────────────

interface PdfPreviewProps {
  encounterId: string;
}

function PdfPreview({ encounterId }: PdfPreviewProps) {
  const [pdfBase64, setPdfBase64] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let mounted = true;

    async function loadPdf() {
      setLoading(true);
      setError(null);
      try {
        const result = await commands.generateEncounterNotePdf(encounterId);
        // Read the generated PDF file and convert to base64 for inline display
        const bytes = await readFile(result.filePath);
        // Convert Uint8Array to base64
        let binary = "";
        const len = bytes.length;
        for (let i = 0; i < len; i++) {
          binary += String.fromCharCode(bytes[i]);
        }
        const base64 = btoa(binary);
        if (mounted) {
          setPdfBase64(base64);
        }
      } catch (e) {
        if (mounted) {
          const msg = e instanceof Error ? e.message : String(e);
          console.error("[EncounterWorkspace] PdfPreview load failed:", msg);
          setError(msg);
        }
      } finally {
        if (mounted) {
          setLoading(false);
        }
      }
    }

    loadPdf();
    return () => { mounted = false; };
  }, [encounterId]);

  if (loading) {
    return (
      <div className="flex items-center justify-center h-[600px] rounded-lg border border-gray-200 bg-gray-50">
        <div className="text-center">
          <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-indigo-600 mx-auto mb-3" />
          <p className="text-sm text-gray-500">Generating PDF preview...</p>
        </div>
      </div>
    );
  }

  if (error) {
    return (
      <div className="rounded-md border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700">
        <p className="font-semibold">Failed to generate PDF preview</p>
        <p className="mt-1">{error}</p>
      </div>
    );
  }

  if (!pdfBase64) return null;

  return (
    <div className="h-[700px] w-full rounded-lg border border-gray-200 overflow-hidden">
      <iframe
        src={`data:application/pdf;base64,${pdfBase64}`}
        title="Encounter Note PDF Preview"
        className="h-full w-full border-0"
      />
    </div>
  );
}

// ─── Main component ──────────────────────────────────────────────────────────

export function EncounterWorkspace({
  patientId,
  encounterId,
  role,
  userId: _userId,
}: EncounterWorkspaceProps) {
  const { goBack, navigate } = useNav();
  const {
    encounter,
    loading,
    error,
    reload,
    templates,
    soapState,
    setSoapState,
    saveSoap,
    finalizeEncounter,
    isFinalized,
    reopenForAmendment,
  } = useEncounter({
    patientId,
    encounterId,
  });

  // ── Single note content state (merged view) ──────────────────────────
  const [noteContent, setNoteContent] = useState("");
  const [noteSeededForId, setNoteSeededForId] = useState<string | null>(null);

  // Seed noteContent and soapState from encounter when it loads
  useEffect(() => {
    if (!encounter) return;
    if (noteSeededForId === encounter.id) return;

    // Check if only subjective is filled (legacy single-field mode)
    const hasOnlySubjective =
      soapState.subjective &&
      !soapState.objective &&
      !soapState.assessment &&
      !soapState.plan;

    if (hasOnlySubjective && soapState.subjective) {
      // Try to auto-detect SOAP sections in the subjective text
      const parsed = parseSoapFromText(soapState.subjective);
      // Only apply parsed sections if we found at least 2 distinct sections
      const filledCount = [parsed.subjective, parsed.objective, parsed.assessment, parsed.plan]
        .filter((v) => v !== null && v.trim() !== "")
        .length;
      if (filledCount >= 2) {
        setSoapState(parsed);
        setNoteContent(mergeNoteContent(parsed));
      } else {
        setNoteContent(mergeNoteContent(soapState));
      }
    } else {
      setNoteContent(mergeNoteContent(soapState));
    }
    setNoteSeededForId(encounter.id);
  }, [encounter, soapState, noteSeededForId, setSoapState]);

  // ── Amendment state ──────────────────────────────────────────────────
  const [isAmending, setIsAmending] = useState(false);
  /** True when the encounter was originally finalized (even if currently open for amendment). */
  const encounterWasFinalized = encounter?.resource?.["status"] === "finished";

  // ── Delete encounter state ──────────────────────────────────────────
  const [showDeleteConfirm, setShowDeleteConfirm] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [deleteError, setDeleteError] = useState<string | null>(null);

  const handleDeleteEncounter = useCallback(async () => {
    setDeleting(true);
    setDeleteError(null);
    try {
      await commands.deleteEncounter(encounterId);
      navigate({ page: "patient-detail", patientId });
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      console.error("[EncounterWorkspace] deleteEncounter failed:", msg);
      setDeleteError(msg);
    } finally {
      setDeleting(false);
      setShowDeleteConfirm(false);
    }
  }, [encounterId, patientId, navigate]);

  // ── Export to PDF state ──────────────────────────────────────────────
  const [exportingPdf, setExportingPdf] = useState(false);
  const [exportPdfError, setExportPdfError] = useState<string | null>(null);
  const [exportPdfSuccess, setExportPdfSuccess] = useState<string | null>(null);

  // ── Fax Note state ──────────────────────────────────────────────────
  const [showFaxModal, setShowFaxModal] = useState(false);
  const [faxContacts, setFaxContacts] = useState<FaxContact[]>([]);
  const [faxContactsLoading, setFaxContactsLoading] = useState(false);
  const [faxRecipientName, setFaxRecipientName] = useState("");
  const [faxRecipientNumber, setFaxRecipientNumber] = useState("");
  const [faxing, setFaxing] = useState(false);
  const [faxError, setFaxError] = useState<string | null>(null);
  const [faxSuccess, setFaxSuccess] = useState<string | null>(null);

  // ── Export to PDF handler ───────────────────────────────────────────
  const handleExportPdf = useCallback(async () => {
    setExportingPdf(true);
    setExportPdfError(null);
    setExportPdfSuccess(null);
    try {
      const result = await commands.generateEncounterNotePdf(encounterId);
      // Extract just the filename for a cleaner save dialog
      const fileName = result.filePath.split("/").pop() ?? "encounter-note.pdf";
      const destination = await save({
        title: "Save Encounter Note PDF",
        defaultPath: fileName,
        filters: [{ name: "PDF Documents", extensions: ["pdf"] }],
      });
      if (destination) {
        await copyFile(result.filePath, destination);
        setExportPdfSuccess(`PDF saved to ${destination}`);
      } else {
        setExportPdfSuccess(`PDF generated successfully`);
      }
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      console.error("[EncounterWorkspace] exportPdf failed:", msg);
      setExportPdfError(msg);
    } finally {
      setExportingPdf(false);
    }
  }, [encounterId]);

  // ── Fax Note: open modal ────────────────────────────────────────────
  const handleOpenFaxModal = useCallback(async () => {
    setShowFaxModal(true);
    setFaxError(null);
    setFaxSuccess(null);
    setFaxRecipientName("");
    setFaxRecipientNumber("");
    setFaxContactsLoading(true);
    try {
      const contacts = await commands.listFaxContacts(null);
      setFaxContacts(contacts);
    } catch (e) {
      console.error("[EncounterWorkspace] listFaxContacts failed:", e instanceof Error ? e.message : String(e));
      setFaxContacts([]);
    } finally {
      setFaxContactsLoading(false);
    }
  }, []);

  // ── Fax Note: select contact from list ──────────────────────────────
  const handleSelectFaxContact = useCallback((contact: FaxContact) => {
    setFaxRecipientName(contact.name);
    setFaxRecipientNumber(contact.faxNumber);
  }, []);

  // ── Fax Note: send ──────────────────────────────────────────────────
  const handleSendFax = useCallback(async () => {
    if (!faxRecipientName.trim() || !faxRecipientNumber.trim()) {
      setFaxError("Recipient name and fax number are required.");
      return;
    }
    setFaxing(true);
    setFaxError(null);
    setFaxSuccess(null);
    try {
      const result = await commands.faxEncounterNote({
        encounterId,
        recipientFax: faxRecipientNumber.trim(),
        recipientName: faxRecipientName.trim(),
        patientId,
      });
      if (result.status === "failed") {
        setFaxError("Fax queuing failed. Check Phaxio configuration.");
      } else {
        setFaxSuccess(`Fax queued successfully (ID: ${result.faxId})`);
      }
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      console.error("[EncounterWorkspace] faxEncounterNote failed:", msg);
      setFaxError(msg);
    } finally {
      setFaxing(false);
    }
  }, [encounterId, patientId, faxRecipientName, faxRecipientNumber]);

  // Clear success messages after a delay
  useEffect(() => {
    if (exportPdfSuccess) {
      const t = setTimeout(() => setExportPdfSuccess(null), 6000);
      return () => clearTimeout(t);
    }
  }, [exportPdfSuccess]);

  // ── Loading state ──────────────────────────────────────────────────
  if (loading) {
    return <LoadingSkeleton />;
  }

  // ── Error state ────────────────────────────────────────────────────
  if (error) {
    return (
      <div className="p-6">
        <div className="mb-4 rounded-md border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700">
          <p className="font-semibold">Failed to load encounter data</p>
          <p className="mt-1">{error}</p>
        </div>
        <div className="flex gap-3">
          <button
            type="button"
            onClick={reload}
            className="rounded-md bg-red-600 px-4 py-2 text-sm font-medium text-white hover:bg-red-700 focus:outline-none focus:ring-2 focus:ring-red-500 focus:ring-offset-2"
          >
            Retry
          </button>
          <button
            type="button"
            onClick={goBack}
            className="rounded-md bg-gray-100 px-4 py-2 text-sm font-medium text-gray-700 hover:bg-gray-200 focus:outline-none focus:ring-2 focus:ring-gray-400 focus:ring-offset-2"
          >
            Back
          </button>
        </div>
      </div>
    );
  }

  // ── Extract display info from encounter resource ───────────────────
  const resource = encounter?.resource ?? {};
  const rawType = extractEncounterTypeFromResource(resource);
  const encounterLabel = rawType
    ? formatEncounterType(rawType)
    : "Encounter";
  const encounterDate = extractEncounterDate(resource) ?? "";

  // Determine if we should show PDF preview (finalized and not amending)
  const showPdfPreview = isFinalized && !isAmending;

  // ── Render workspace ───────────────────────────────────────────────
  return (
    <div className="flex flex-col space-y-0 p-6">
      {/* ── Auth tracking alert banner ──────────────────────────────────── */}
      <div className="mb-2">
        <AuthAlertBanner patientId={patientId} />
      </div>

      {/* ── Therapy cap status banner ────────────────────────────────────── */}
      <div className="mb-4">
        <TherapyCapBanner patientId={patientId} />
      </div>

      {/* ── Page header ─────────────────────────────────────────────────── */}
      <div className="mb-5 flex items-center gap-3">
        <button
          type="button"
          onClick={goBack}
          className="rounded-md p-1.5 text-gray-500 hover:bg-gray-100 hover:text-gray-700 focus:outline-none focus:ring-2 focus:ring-gray-400 focus:ring-offset-1"
          aria-label="Go back"
        >
          Back
        </button>
        <div className="flex flex-1 items-center gap-3">
          <div>
            <h1 className="text-xl font-bold text-gray-900">
              {encounterLabel}
            </h1>
            {encounterDate && (
              <p className="mt-0.5 text-sm text-gray-500">{encounterDate}</p>
            )}
          </div>
          {/* Finalized badge in header */}
          {isFinalized && (
            <span className="ml-2 inline-flex items-center gap-1 rounded-full bg-green-100 px-2.5 py-0.5 text-xs font-semibold text-green-700">
              Finalized
            </span>
          )}
          {/* Amending badge */}
          {!isFinalized && isAmending && encounterWasFinalized && (
            <span className="ml-2 inline-flex items-center gap-1 rounded-full bg-amber-100 px-2.5 py-0.5 text-xs font-semibold text-amber-700">
              Amending
            </span>
          )}
          {/* Amend / Edit button — visible when finalized (showing PDF preview) */}
          {isFinalized && (role === "Provider" || role === "SystemAdmin") && (
            <button
              type="button"
              onClick={() => {
                setIsAmending(true);
                reopenForAmendment();
              }}
              className="ml-2 rounded-md border border-amber-400 bg-white px-3 py-1 text-xs font-medium text-amber-700 hover:bg-amber-50 focus:outline-none focus:ring-2 focus:ring-amber-400 focus:ring-offset-1"
            >
              Edit
            </button>
          )}
        </div>

        {/* ── Export / Fax / Delete action buttons ────────────────────── */}
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={() => void handleExportPdf()}
            disabled={exportingPdf}
            aria-label="Export encounter note to PDF"
            className="rounded-md border border-gray-300 bg-white px-3 py-1.5 text-sm font-medium text-gray-700 shadow-sm hover:bg-gray-50 focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:ring-offset-1 disabled:opacity-60"
          >
            {exportingPdf ? "Exporting..." : "Export to PDF"}
          </button>
          <button
            type="button"
            onClick={() => void handleOpenFaxModal()}
            disabled={faxing}
            aria-label="Fax encounter note"
            className="rounded-md border border-gray-300 bg-white px-3 py-1.5 text-sm font-medium text-gray-700 shadow-sm hover:bg-gray-50 focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:ring-offset-1 disabled:opacity-60"
          >
            Fax Note
          </button>
          {(role === "Provider" || role === "SystemAdmin") && (
            <button
              type="button"
              onClick={() => setShowDeleteConfirm(true)}
              aria-label="Delete encounter"
              className="rounded-md border border-red-300 bg-white px-3 py-1.5 text-sm font-medium text-red-700 shadow-sm hover:bg-red-50 focus:outline-none focus:ring-2 focus:ring-red-500 focus:ring-offset-1"
            >
              Delete Encounter
            </button>
          )}
        </div>
      </div>

      {/* ── Export/Fax status messages ──────────────────────────────────── */}
      {exportPdfError && (
        <div className="mb-2 rounded-md border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-700">
          Export failed: {exportPdfError}
        </div>
      )}
      {exportPdfSuccess && (
        <div className="mb-2 rounded-md border border-green-200 bg-green-50 px-3 py-2 text-sm text-green-700">
          {exportPdfSuccess}
        </div>
      )}

      {/* ── Content area: PDF preview (finalized) or note editor ────────── */}
      <div className="rounded-lg border border-gray-200 bg-white p-5">
        {showPdfPreview ? (
          <div className="space-y-4">
            <div className="flex items-center gap-2 rounded-md border border-green-200 bg-green-50 px-4 py-2 text-sm font-medium text-green-700">
              <span>Finalized encounter — PDF preview below. Click "Edit" to amend.</span>
            </div>
            <PdfPreview encounterId={encounterId} />
          </div>
        ) : (
          <NoteEditor
            encounterId={encounterId}
            role={role}
            noteContent={noteContent}
            setNoteContent={setNoteContent}
            saveSoap={saveSoap}
            finalizeEncounter={finalizeEncounter}
            isFinalized={isFinalized}
            isAmending={isAmending}
            templates={templates}
            soapState={soapState}
            setSoapState={setSoapState}
          />
        )}
      </div>

      {/* ── Fax Note modal ──────────────────────────────────────────────── */}
      {showFaxModal && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/40"
          role="dialog"
          aria-modal="true"
          aria-labelledby="fax-note-modal-title"
        >
          <div className="w-full max-w-lg rounded-lg bg-white p-6 shadow-xl">
            <div className="mb-4 flex items-center justify-between">
              <h3
                id="fax-note-modal-title"
                className="text-base font-semibold text-gray-900"
              >
                Fax Encounter Note
              </h3>
              <button
                type="button"
                onClick={() => setShowFaxModal(false)}
                className="rounded p-1 text-gray-400 hover:bg-gray-100 hover:text-gray-600"
                aria-label="Close fax modal"
              >
                &times;
              </button>
            </div>

            {/* Contact picker */}
            {faxContactsLoading ? (
              <p className="mb-4 text-sm text-gray-500">Loading contacts...</p>
            ) : faxContacts.length > 0 ? (
              <div className="mb-4">
                <label className="mb-1 block text-sm font-medium text-gray-700">
                  Select from contacts
                </label>
                <select
                  className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-indigo-500 focus:outline-none focus:ring-1 focus:ring-indigo-500"
                  defaultValue=""
                  onChange={(e) => {
                    const contact = faxContacts.find(
                      (c) => c.contactId === e.target.value,
                    );
                    if (contact) handleSelectFaxContact(contact);
                  }}
                >
                  <option value="">-- Select a contact --</option>
                  {faxContacts.map((c) => (
                    <option key={c.contactId} value={c.contactId}>
                      {c.name}
                      {c.organization ? ` (${c.organization})` : ""} -{" "}
                      {c.faxNumber}
                    </option>
                  ))}
                </select>
              </div>
            ) : null}

            <div className="mb-3 text-xs font-medium uppercase tracking-wide text-gray-500">
              Or enter manually
            </div>

            {/* Manual entry */}
            <div className="mb-4">
              <label
                htmlFor="fax-recipient-name"
                className="mb-1 block text-sm font-medium text-gray-700"
              >
                Recipient Name <span className="text-red-500">*</span>
              </label>
              <input
                id="fax-recipient-name"
                type="text"
                value={faxRecipientName}
                onChange={(e) => setFaxRecipientName(e.target.value)}
                className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm text-gray-900 focus:outline-none focus:ring-2 focus:ring-indigo-500"
                placeholder="Dr. John Smith"
              />
            </div>

            <div className="mb-4">
              <label
                htmlFor="fax-recipient-number"
                className="mb-1 block text-sm font-medium text-gray-700"
              >
                Fax Number <span className="text-red-500">*</span>
              </label>
              <input
                id="fax-recipient-number"
                type="tel"
                value={faxRecipientNumber}
                onChange={(e) => setFaxRecipientNumber(e.target.value)}
                className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm text-gray-900 focus:outline-none focus:ring-2 focus:ring-indigo-500"
                placeholder="+15551234567"
              />
            </div>

            {/* Error / success */}
            {faxError && (
              <div className="mb-3 rounded-md border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-700">
                {faxError}
              </div>
            )}
            {faxSuccess && (
              <div className="mb-3 rounded-md border border-green-200 bg-green-50 px-3 py-2 text-sm text-green-700">
                {faxSuccess}
              </div>
            )}

            {/* Actions */}
            <div className="flex justify-end gap-3">
              <button
                type="button"
                onClick={() => setShowFaxModal(false)}
                disabled={faxing}
                className="rounded-md border border-gray-300 px-4 py-2 text-sm font-medium text-gray-700 hover:bg-gray-50 focus:outline-none focus:ring-2 focus:ring-gray-400 focus:ring-offset-2 disabled:opacity-60"
              >
                {faxSuccess ? "Close" : "Cancel"}
              </button>
              {!faxSuccess && (
                <button
                  type="button"
                  onClick={() => void handleSendFax()}
                  disabled={
                    faxing ||
                    !faxRecipientName.trim() ||
                    !faxRecipientNumber.trim()
                  }
                  className="rounded-md bg-indigo-600 px-4 py-2 text-sm font-medium text-white hover:bg-indigo-700 focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:ring-offset-2 disabled:opacity-60"
                >
                  {faxing ? "Sending..." : "Send Fax"}
                </button>
              )}
            </div>
          </div>
        </div>
      )}

      {/* ── Delete confirmation dialog ──────────────────────────────────── */}
      {showDeleteConfirm && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/40"
          role="dialog"
          aria-modal="true"
          aria-labelledby="delete-encounter-modal-title"
        >
          <div className="w-full max-w-md rounded-lg bg-white p-6 shadow-xl">
            <h3
              id="delete-encounter-modal-title"
              className="text-base font-semibold text-gray-900"
            >
              Delete Encounter
            </h3>
            <p className="mt-2 text-sm text-gray-600">
              Are you sure you want to delete this encounter? This action cannot be undone.
            </p>
            {deleteError && (
              <div className="mt-3 rounded-md border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-700">
                {deleteError}
              </div>
            )}
            <div className="mt-4 flex justify-end gap-3">
              <button
                type="button"
                onClick={() => setShowDeleteConfirm(false)}
                disabled={deleting}
                className="rounded-md border border-gray-300 px-4 py-2 text-sm font-medium text-gray-700 hover:bg-gray-50 focus:outline-none focus:ring-2 focus:ring-gray-400 focus:ring-offset-2 disabled:opacity-60"
              >
                Cancel
              </button>
              <button
                type="button"
                onClick={() => void handleDeleteEncounter()}
                disabled={deleting}
                className="rounded-md bg-red-600 px-4 py-2 text-sm font-medium text-white hover:bg-red-700 focus:outline-none focus:ring-2 focus:ring-red-500 focus:ring-offset-2 disabled:opacity-60"
              >
                {deleting ? "Deleting..." : "Delete"}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

// Export props type for use in child tab components
export type { EncounterWorkspaceProps };

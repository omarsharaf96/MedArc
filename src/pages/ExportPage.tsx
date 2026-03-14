/**
 * ExportPage.tsx — Export & Report Generation UI.
 *
 * Presents 5 export types as selectable cards, a configuration panel for
 * the selected type, and result actions (Save As, Open) after generation.
 *
 * Export types:
 *   1. Single Note PDF — select a note from dropdown
 *   2. Progress Report — date range picker
 *   3. Insurance Narrative — date range picker (utilization review)
 *   4. Legal/IME Report — date range picker (for attorneys)
 *   5. Full Chart Export — date range picker (all notes bundled)
 *
 * RBAC: All authenticated roles can access exports. The backend enforces
 * any additional role restrictions.
 */
import { useState, useEffect } from "react";
import { useNav } from "../contexts/RouterContext";
import { commands } from "../lib/tauri";
import { save } from "@tauri-apps/plugin-dialog";
import { copyFile } from "@tauri-apps/plugin-fs";
import type { PdfExportResult } from "../types/export";
import type { EncounterRecord } from "../types/documentation";

// ─── Props ───────────────────────────────────────────────────────────────────

interface ExportPageProps {
  patientId: string;
  role: string;
  userId: string;
}

// ─── Export type definitions ─────────────────────────────────────────────────

type ExportType =
  | "note_pdf"
  | "progress_report"
  | "insurance_narrative"
  | "legal_report"
  | "chart_export";

interface ExportTypeCard {
  type: ExportType;
  title: string;
  description: string;
  icon: string;
  needsDateRange: boolean;
  needsEncounter: boolean;
}

const EXPORT_TYPES: ExportTypeCard[] = [
  {
    type: "note_pdf",
    title: "Single Note PDF",
    description: "Export a single encounter note as a formatted PDF document.",
    icon: "D",
    needsDateRange: false,
    needsEncounter: true,
  },
  {
    type: "progress_report",
    title: "Progress Report",
    description:
      "Generate a progress summary from patient data over a date range.",
    icon: "P",
    needsDateRange: true,
    needsEncounter: false,
  },
  {
    type: "insurance_narrative",
    title: "Insurance Narrative",
    description:
      "Produce a utilization review narrative for insurance submissions.",
    icon: "I",
    needsDateRange: true,
    needsEncounter: false,
  },
  {
    type: "legal_report",
    title: "Legal / IME Report",
    description:
      "Create a legal or independent medical examination report for attorneys.",
    icon: "L",
    needsDateRange: true,
    needsEncounter: false,
  },
  {
    type: "chart_export",
    title: "Full Chart Export",
    description:
      "Bundle all notes into a single PDF with cover page and table of contents.",
    icon: "C",
    needsDateRange: true,
    needsEncounter: false,
  },
];

// ─── Tailwind class constants ────────────────────────────────────────────────

const CARD_BASE =
  "cursor-pointer rounded-lg border-2 p-4 text-left transition-colors focus:outline-none focus:ring-2 focus:ring-indigo-400 focus:ring-offset-2";
const CARD_IDLE = "border-gray-200 bg-white hover:border-indigo-300 hover:bg-indigo-50";
const CARD_ACTIVE = "border-indigo-500 bg-indigo-50";

const BTN_PRIMARY =
  "rounded-md bg-indigo-600 px-4 py-2 text-sm font-medium text-white hover:bg-indigo-700 focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:ring-offset-2 disabled:opacity-60";

const INPUT_CLS =
  "block w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-indigo-500 focus:outline-none focus:ring-1 focus:ring-indigo-500";
const LABEL_CLS = "mb-1 block text-sm font-medium text-gray-700";

// ─── Helper: extract encounter date and type for display ─────────────────────

function extractEncounterDate(resource: Record<string, unknown>): string {
  const period = resource["period"] as Record<string, unknown> | undefined;
  const start = period?.["start"];
  if (typeof start === "string" && start.length >= 10) return start.slice(0, 10);
  const date = resource["date"];
  if (typeof date === "string" && date.length >= 10) return date.slice(0, 10);
  return "Unknown date";
}

function extractEncounterTypeLabel(resource: Record<string, unknown>): string {
  const types = resource["type"] as Array<Record<string, unknown>> | undefined;
  const typeText = types?.[0]?.["text"];
  if (typeof typeText === "string") {
    return typeText
      .split("_")
      .map((w) => w.charAt(0).toUpperCase() + w.slice(1))
      .join(" ");
  }
  return "Encounter";
}

// ─── Helper: format bytes ────────────────────────────────────────────────────

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

// ─── Component ───────────────────────────────────────────────────────────────

export function ExportPage({ patientId, role: _role, userId: _userId }: ExportPageProps) {
  const { goBack } = useNav();

  // ── Selection state ─────────────────────────────────────────────────────
  const [selectedType, setSelectedType] = useState<ExportType | null>(null);

  // ── Encounter list (for single note PDF) ────────────────────────────────
  const [encounters, setEncounters] = useState<EncounterRecord[]>([]);
  const [encountersLoading, setEncountersLoading] = useState(true);
  const [encountersError, setEncountersError] = useState<string | null>(null);
  const [selectedEncounterId, setSelectedEncounterId] = useState<string>("");

  // ── Date range (for reports and chart export) ───────────────────────────
  const [startDate, setStartDate] = useState("");
  const [endDate, setEndDate] = useState("");

  // ── Generation state ────────────────────────────────────────────────────
  const [generating, setGenerating] = useState(false);
  const [generateError, setGenerateError] = useState<string | null>(null);
  const [result, setResult] = useState<PdfExportResult | null>(null);

  // ── Save As state ───────────────────────────────────────────────────────
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [savedPath, setSavedPath] = useState<string | null>(null);

  // ── Fetch encounters on mount ───────────────────────────────────────────
  useEffect(() => {
    let mounted = true;
    setEncountersLoading(true);
    setEncountersError(null);

    commands
      .listEncounters(patientId, null, null, null)
      .then((list) => {
        if (!mounted) return;
        setEncounters(list);
      })
      .catch((e) => {
        if (!mounted) return;
        const msg = e instanceof Error ? e.message : String(e);
        console.error("[ExportPage] listEncounters failed:", msg);
        setEncountersError(msg);
      })
      .finally(() => {
        if (mounted) setEncountersLoading(false);
      });

    return () => {
      mounted = false;
    };
  }, [patientId]);

  // ── Set default dates when a date-range type is selected ────────────────
  useEffect(() => {
    if (selectedType && selectedType !== "note_pdf") {
      if (!startDate) {
        // Default: 90 days ago
        const d = new Date();
        d.setDate(d.getDate() - 90);
        setStartDate(d.toISOString().slice(0, 10));
      }
      if (!endDate) {
        setEndDate(new Date().toISOString().slice(0, 10));
      }
    }
  }, [selectedType, startDate, endDate]);

  // ── Reset result/error when selection changes ───────────────────────────
  useEffect(() => {
    setResult(null);
    setGenerateError(null);
    setSaveError(null);
    setSavedPath(null);
  }, [selectedType, selectedEncounterId, startDate, endDate]);

  // ── Determine current export type config ────────────────────────────────
  const currentConfig = EXPORT_TYPES.find((t) => t.type === selectedType) ?? null;

  // ── Can generate? ───────────────────────────────────────────────────────
  const canGenerate = (() => {
    if (!selectedType || generating) return false;
    if (currentConfig?.needsEncounter && !selectedEncounterId) return false;
    if (currentConfig?.needsDateRange && (!startDate || !endDate)) return false;
    return true;
  })();

  // ── Generate handler ────────────────────────────────────────────────────
  async function handleGenerate() {
    if (!selectedType) return;
    setGenerating(true);
    setGenerateError(null);
    setResult(null);
    setSavedPath(null);
    setSaveError(null);

    try {
      let exportResult: PdfExportResult;

      switch (selectedType) {
        case "note_pdf":
          exportResult = await commands.generateNotePdf(selectedEncounterId || patientId);
          break;
        case "progress_report":
          exportResult = await commands.generateProgressReport(patientId, startDate, endDate);
          break;
        case "insurance_narrative":
          exportResult = await commands.generateInsuranceNarrative(patientId);
          break;
        case "legal_report":
          exportResult = await commands.generateLegalReport(patientId);
          break;
        case "chart_export":
          exportResult = await commands.generateChartExport(patientId, startDate, endDate);
          break;
      }

      setResult(exportResult);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      console.error(`[ExportPage] generate ${selectedType} failed:`, msg);
      setGenerateError(msg);
    } finally {
      setGenerating(false);
    }
  }

  // ── Save As handler ─────────────────────────────────────────────────────
  async function handleSaveAs() {
    if (!result) return;
    setSaving(true);
    setSaveError(null);
    setSavedPath(null);

    try {
      const destination = await save({
        title: "Save PDF As",
        defaultPath: result.filePath,
        filters: [{ name: "PDF Documents", extensions: ["pdf"] }],
      });

      if (!destination) {
        // User cancelled
        setSaving(false);
        return;
      }

      await copyFile(result.filePath, destination);
      setSavedPath(destination);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      console.error("[ExportPage] saveAs failed:", msg);
      setSaveError(msg);
    } finally {
      setSaving(false);
    }
  }

  // ── Render ──────────────────────────────────────────────────────────────
  return (
    <div className="space-y-6 p-6">
      {/* ── Header ──────────────────────────────────────────────────────── */}
      <div className="flex items-center gap-3">
        <button
          type="button"
          onClick={goBack}
          className="rounded-md p-1.5 text-gray-500 hover:bg-gray-100 hover:text-gray-700 focus:outline-none focus:ring-2 focus:ring-gray-400 focus:ring-offset-1"
          aria-label="Go back"
        >
          &larr; Back
        </button>
        <div>
          <h1 className="text-xl font-bold text-gray-900">
            Export &amp; Reports
          </h1>
          <p className="mt-0.5 text-sm text-gray-500">
            Generate PDF exports and clinical reports for this patient.
          </p>
        </div>
      </div>

      {/* ── Export Type Cards ────────────────────────────────────────────── */}
      <section>
        <h2 className="mb-3 text-sm font-semibold uppercase tracking-wide text-gray-500">
          Select Export Type
        </h2>
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3">
          {EXPORT_TYPES.map((card) => (
            <button
              key={card.type}
              type="button"
              onClick={() => setSelectedType(card.type)}
              role="button"
              aria-pressed={selectedType === card.type}
              aria-label={`Select ${card.title} export type`}
              className={[
                CARD_BASE,
                selectedType === card.type ? CARD_ACTIVE : CARD_IDLE,
              ].join(" ")}
            >
              <div className="mb-2 flex items-center gap-2">
                <span className="flex h-8 w-8 items-center justify-center rounded-md bg-indigo-100 text-sm font-bold text-indigo-700">
                  {card.icon}
                </span>
                <span className="text-sm font-semibold text-gray-900">
                  {card.title}
                </span>
              </div>
              <p className="text-xs text-gray-500">{card.description}</p>
            </button>
          ))}
        </div>
      </section>

      {/* ── Configuration Panel ──────────────────────────────────────────── */}
      {currentConfig && (
        <section className="rounded-lg border border-gray-200 bg-white p-5 shadow-sm">
          <h2 className="mb-4 text-base font-semibold text-gray-800">
            Configure: {currentConfig.title}
          </h2>

          <div className="space-y-4">
            {/* ── Encounter selector (Single Note PDF) ─────────────────── */}
            {currentConfig.needsEncounter && (
              <div>
                <label htmlFor="encounter-select" className={LABEL_CLS}>
                  Select Note / Encounter
                </label>
                {encountersLoading ? (
                  <p className="text-sm text-gray-500">
                    Loading encounters...
                  </p>
                ) : encountersError ? (
                  <p className="text-sm text-red-600">
                    Failed to load encounters: {encountersError}
                  </p>
                ) : encounters.length === 0 ? (
                  <p className="text-sm text-gray-500">
                    No encounters found for this patient.
                  </p>
                ) : (
                  <select
                    id="encounter-select"
                    value={selectedEncounterId}
                    onChange={(e) => setSelectedEncounterId(e.target.value)}
                    className={INPUT_CLS}
                  >
                    <option value="">-- Select an encounter --</option>
                    {encounters.map((enc) => {
                      const date = extractEncounterDate(enc.resource);
                      const typeLabel = extractEncounterTypeLabel(enc.resource);
                      return (
                        <option key={enc.id} value={enc.id}>
                          {date} - {typeLabel}
                        </option>
                      );
                    })}
                  </select>
                )}
              </div>
            )}

            {/* ── Date range picker (reports and chart export) ──────────── */}
            {currentConfig.needsDateRange && (
              <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
                <div>
                  <label htmlFor="start-date" className={LABEL_CLS}>
                    Start Date
                  </label>
                  <input
                    id="start-date"
                    type="date"
                    value={startDate}
                    onChange={(e) => setStartDate(e.target.value)}
                    className={INPUT_CLS}
                  />
                </div>
                <div>
                  <label htmlFor="end-date" className={LABEL_CLS}>
                    End Date
                  </label>
                  <input
                    id="end-date"
                    type="date"
                    value={endDate}
                    onChange={(e) => setEndDate(e.target.value)}
                    className={INPUT_CLS}
                  />
                </div>
              </div>
            )}

            {/* ── Generate button ───────────────────────────────────────── */}
            <div className="flex items-center gap-3 pt-2">
              <button
                type="button"
                onClick={handleGenerate}
                disabled={!canGenerate}
                aria-label={generating ? "Generating PDF, please wait" : "Generate PDF"}
                aria-busy={generating}
                aria-disabled={!canGenerate}
                className={BTN_PRIMARY}
              >
                {generating ? "Generating..." : "Generate PDF"}
              </button>
            </div>

            {/* ── Generation error ──────────────────────────────────────── */}
            {generateError && (
              <div className="rounded-md border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700">
                <p className="font-semibold">Export failed</p>
                <p className="mt-1">{generateError}</p>
              </div>
            )}
          </div>
        </section>
      )}

      {/* ── Export Result ─────────────────────────────────────────────────── */}
      {result && (
        <section className="rounded-lg border border-green-200 bg-green-50 p-5 shadow-sm">
          <h2 className="mb-3 text-base font-semibold text-green-800">
            Export Complete
          </h2>

          <div className="space-y-2 text-sm">
            <div className="flex gap-2">
              <span className="w-28 shrink-0 font-medium text-gray-600">
                File Path
              </span>
              <span className="break-all text-gray-900">
                {result.filePath}
              </span>
            </div>
            <div className="flex gap-2">
              <span className="w-28 shrink-0 font-medium text-gray-600">
                File Size
              </span>
              <span className="text-gray-900">
                {formatBytes(result.fileSizeBytes)}
              </span>
            </div>
            <div className="flex gap-2">
              <span className="w-28 shrink-0 font-medium text-gray-600">
                Pages
              </span>
              <span className="text-gray-900">{result.pageCount}</span>
            </div>
            <div className="flex gap-2">
              <span className="w-28 shrink-0 font-medium text-gray-600">
                Generated At
              </span>
              <span className="text-gray-900">{result.completedAt}</span>
            </div>
          </div>

          {/* ── Actions ──────────────────────────────────────────────────── */}
          <div className="mt-4 flex items-center gap-3">
            <button
              type="button"
              onClick={handleSaveAs}
              disabled={saving}
              className={BTN_PRIMARY}
            >
              {saving ? "Saving..." : "Save As..."}
            </button>
          </div>

          {/* ── Save result / error ──────────────────────────────────────── */}
          {savedPath && (
            <p className="mt-3 text-sm text-green-700">
              Saved to: <span className="font-medium">{savedPath}</span>
            </p>
          )}
          {saveError && (
            <p className="mt-3 text-sm text-red-600">
              Save failed: {saveError}
            </p>
          )}
        </section>
      )}

      {/* ── Bulk Export hint (for Full Chart Export) ──────────────────────── */}
      {selectedType === "chart_export" && !result && (
        <section className="rounded-lg border border-blue-200 bg-blue-50 p-4">
          <h3 className="text-sm font-semibold text-blue-800">
            Full Chart Export
          </h3>
          <p className="mt-1 text-xs text-blue-700">
            This export bundles all clinical notes within the selected date
            range into a single PDF. The generated document includes a cover
            page with patient demographics and a table of contents listing
            each encounter by date and type.
          </p>
        </section>
      )}
    </div>
  );
}

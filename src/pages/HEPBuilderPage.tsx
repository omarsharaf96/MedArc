/**
 * HEPBuilderPage.tsx — Home Exercise Program Builder
 *
 * Two-panel layout:
 *   Left:  Exercise library browser (filterable by body region + category, searchable)
 *   Right: Current program (selected exercises with per-exercise prescription editor)
 *
 * Features:
 *   - Browse and search the ~50-exercise built-in library
 *   - Filter by body region and category
 *   - Add exercises to the program with default prescription values
 *   - Edit sets, reps, duration, hold time, frequency, resistance, pain limit, notes
 *   - Reorder exercises with up/down buttons
 *   - Remove exercises from the program
 *   - Save program for a patient / encounter
 *   - Save program as a reusable template
 *   - Load an existing template as a starting point
 *
 * Props:
 *   patientId   — required; the patient this HEP is for
 *   encounterId — optional; links program to an encounter
 *   role        — user's RBAC role (Provider/NurseMa required for write access)
 *   userId      — current user's ID
 *
 * RBAC:
 *   Provider / SystemAdmin / NurseMa — full access (create/update programs + templates)
 *   BillingStaff / FrontDesk        — read-only (browse exercises; cannot save)
 */
import { useState, useEffect, useCallback } from "react";
import { commands } from "../lib/tauri";
import { useNav } from "../contexts/RouterContext";
import type {
  Exercise,
  ExerciseRegion,
  ExerciseCategory,
  ExercisePrescription,
  HEPTemplate,
} from "../types/hep";

// ─── Props ───────────────────────────────────────────────────────────────────

interface HEPBuilderPageProps {
  patientId: string;
  encounterId?: string;
  role: string;
  userId: string;
}

// ─── Constants ───────────────────────────────────────────────────────────────

const BODY_REGIONS: { value: ExerciseRegion; label: string }[] = [
  { value: "cervical", label: "Cervical" },
  { value: "thoracic", label: "Thoracic" },
  { value: "lumbar", label: "Lumbar" },
  { value: "shoulder", label: "Shoulder" },
  { value: "elbow", label: "Elbow" },
  { value: "wrist", label: "Wrist" },
  { value: "hip", label: "Hip" },
  { value: "knee", label: "Knee" },
  { value: "ankle", label: "Ankle" },
  { value: "general", label: "General" },
];

const CATEGORIES: { value: ExerciseCategory; label: string }[] = [
  { value: "rom", label: "ROM" },
  { value: "strengthening", label: "Strengthening" },
  { value: "stretching", label: "Stretching" },
  { value: "balance", label: "Balance" },
  { value: "functional", label: "Functional" },
  { value: "cardio", label: "Cardio" },
];

const DIFFICULTY_COLORS: Record<string, string> = {
  beginner: "bg-green-100 text-green-700",
  intermediate: "bg-yellow-100 text-yellow-700",
  advanced: "bg-red-100 text-red-700",
};

const REGION_COLORS: Record<string, string> = {
  cervical: "bg-blue-100 text-blue-700",
  thoracic: "bg-indigo-100 text-indigo-700",
  lumbar: "bg-purple-100 text-purple-700",
  shoulder: "bg-pink-100 text-pink-700",
  elbow: "bg-orange-100 text-orange-700",
  wrist: "bg-amber-100 text-amber-700",
  hip: "bg-teal-100 text-teal-700",
  knee: "bg-cyan-100 text-cyan-700",
  ankle: "bg-sky-100 text-sky-700",
  general: "bg-gray-100 text-gray-700",
};

const CATEGORY_COLORS: Record<string, string> = {
  rom: "bg-blue-50 text-blue-600",
  strengthening: "bg-red-50 text-red-600",
  stretching: "bg-green-50 text-green-600",
  balance: "bg-yellow-50 text-yellow-600",
  functional: "bg-purple-50 text-purple-600",
  cardio: "bg-orange-50 text-orange-600",
};

// ─── Helpers ─────────────────────────────────────────────────────────────────

function defaultPrescription(exerciseId: string): ExercisePrescription {
  return {
    exerciseId,
    sets: 3,
    reps: 10,
    durationSeconds: null,
    holdSeconds: null,
    timesPerDay: 1,
    daysPerWeek: 7,
    resistance: null,
    painLimit: 4,
    notes: null,
  };
}

function canWrite(role: string): boolean {
  return role === "Provider" || role === "SystemAdmin" || role === "NurseMa";
}

// ─── Sub-components ──────────────────────────────────────────────────────────

/** Small pill badge. */
function Badge({
  label,
  colorClass,
}: {
  label: string;
  colorClass: string;
}) {
  return (
    <span
      className={[
        "inline-flex items-center rounded-full px-2 py-0.5 text-xs font-medium capitalize",
        colorClass,
      ].join(" ")}
    >
      {label}
    </span>
  );
}

/** Exercise card in the library panel. */
function ExerciseCard({
  exercise,
  onAdd,
  alreadyAdded,
  writeEnabled,
}: {
  exercise: Exercise;
  onAdd: (ex: Exercise) => void;
  alreadyAdded: boolean;
  writeEnabled: boolean;
}) {
  return (
    <div className="rounded-lg border border-gray-200 bg-white p-3 shadow-sm hover:shadow-md transition-shadow">
      <div className="flex items-start justify-between gap-2">
        <div className="min-w-0 flex-1">
          <p className="truncate text-sm font-semibold text-gray-800">
            {exercise.name}
          </p>
          {exercise.description && (
            <p className="mt-0.5 text-xs text-gray-500 line-clamp-2">
              {exercise.description}
            </p>
          )}
          <div className="mt-1.5 flex flex-wrap gap-1">
            <Badge
              label={exercise.bodyRegion}
              colorClass={REGION_COLORS[exercise.bodyRegion] ?? "bg-gray-100 text-gray-600"}
            />
            <Badge
              label={exercise.category}
              colorClass={CATEGORY_COLORS[exercise.category] ?? "bg-gray-100 text-gray-600"}
            />
            {exercise.difficulty && (
              <Badge
                label={exercise.difficulty}
                colorClass={DIFFICULTY_COLORS[exercise.difficulty] ?? "bg-gray-100 text-gray-600"}
              />
            )}
            {exercise.equipment && (
              <span className="text-xs text-gray-400">
                {exercise.equipment}
              </span>
            )}
          </div>
        </div>
        {writeEnabled && (
          <button
            type="button"
            onClick={() => onAdd(exercise)}
            disabled={alreadyAdded}
            className={[
              "ml-2 shrink-0 rounded-md px-2.5 py-1 text-xs font-medium transition-colors focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:ring-offset-1",
              alreadyAdded
                ? "cursor-default bg-gray-100 text-gray-400"
                : "bg-indigo-600 text-white hover:bg-indigo-700",
            ].join(" ")}
          >
            {alreadyAdded ? "Added" : "Add"}
          </button>
        )}
      </div>
    </div>
  );
}

/** Per-exercise prescription editor row. */
function PrescriptionRow({
  prescription,
  exerciseName,
  index,
  total,
  onChange,
  onRemove,
  onMoveUp,
  onMoveDown,
}: {
  prescription: ExercisePrescription;
  exerciseName: string;
  index: number;
  total: number;
  onChange: (updated: ExercisePrescription) => void;
  onRemove: () => void;
  onMoveUp: () => void;
  onMoveDown: () => void;
}) {
  function numField(
    label: string,
    field: keyof ExercisePrescription,
    min = 0,
    max = 999,
  ) {
    const value = prescription[field] as number | null;
    return (
      <div className="flex flex-col gap-0.5">
        <label className="text-xs font-medium text-gray-500">{label}</label>
        <input
          type="number"
          min={min}
          max={max}
          value={value ?? ""}
          onChange={(e) => {
            const v = e.target.value === "" ? null : parseInt(e.target.value, 10);
            onChange({ ...prescription, [field]: isNaN(v as number) ? null : v });
          }}
          className="w-16 rounded border border-gray-300 px-2 py-1 text-sm focus:border-indigo-400 focus:outline-none focus:ring-1 focus:ring-indigo-400"
        />
      </div>
    );
  }

  return (
    <div className="rounded-lg border border-gray-200 bg-gray-50 p-3">
      {/* Header */}
      <div className="mb-2 flex items-center justify-between gap-2">
        <p className="flex-1 text-sm font-semibold text-gray-800 truncate">
          <span className="mr-2 text-xs text-gray-400">#{index + 1}</span>
          {exerciseName}
        </p>
        <div className="flex items-center gap-1 shrink-0">
          <button
            type="button"
            onClick={onMoveUp}
            disabled={index === 0}
            title="Move up"
            className="rounded p-1 text-gray-400 hover:bg-gray-200 hover:text-gray-700 disabled:cursor-default disabled:opacity-30 focus:outline-none focus:ring-2 focus:ring-indigo-400 focus:ring-offset-1"
          >
            ↑
          </button>
          <button
            type="button"
            onClick={onMoveDown}
            disabled={index === total - 1}
            title="Move down"
            className="rounded p-1 text-gray-400 hover:bg-gray-200 hover:text-gray-700 disabled:cursor-default disabled:opacity-30 focus:outline-none focus:ring-2 focus:ring-indigo-400 focus:ring-offset-1"
          >
            ↓
          </button>
          <button
            type="button"
            onClick={onRemove}
            title="Remove exercise"
            className="rounded p-1 text-red-400 hover:bg-red-100 hover:text-red-600 focus:outline-none focus:ring-2 focus:ring-red-400 focus:ring-offset-1"
          >
            ✕
          </button>
        </div>
      </div>

      {/* Prescription fields */}
      <div className="flex flex-wrap gap-3">
        {numField("Sets", "sets", 1, 20)}
        {numField("Reps", "reps", 1, 100)}
        {numField("Duration (s)", "durationSeconds", 0, 3600)}
        {numField("Hold (s)", "holdSeconds", 0, 120)}
        {numField("×/day", "timesPerDay", 1, 10)}
        {numField("d/week", "daysPerWeek", 1, 7)}

        {/* Pain limit */}
        <div className="flex flex-col gap-0.5">
          <label className="text-xs font-medium text-gray-500">
            Pain limit (0–10)
          </label>
          <input
            type="number"
            min={0}
            max={10}
            value={prescription.painLimit ?? ""}
            onChange={(e) => {
              const v = e.target.value === "" ? null : parseInt(e.target.value, 10);
              onChange({
                ...prescription,
                painLimit: isNaN(v as number) ? null : Math.min(10, Math.max(0, v as number)),
              });
            }}
            className="w-16 rounded border border-gray-300 px-2 py-1 text-sm focus:border-indigo-400 focus:outline-none focus:ring-1 focus:ring-indigo-400"
          />
        </div>

        {/* Resistance */}
        <div className="flex flex-col gap-0.5">
          <label className="text-xs font-medium text-gray-500">Resistance / Load</label>
          <input
            type="text"
            placeholder="e.g. red band, 5 lbs"
            value={prescription.resistance ?? ""}
            onChange={(e) =>
              onChange({ ...prescription, resistance: e.target.value || null })
            }
            className="w-36 rounded border border-gray-300 px-2 py-1 text-sm focus:border-indigo-400 focus:outline-none focus:ring-1 focus:ring-indigo-400"
          />
        </div>
      </div>

      {/* Notes */}
      <div className="mt-2">
        <label className="text-xs font-medium text-gray-500">Notes / Precautions</label>
        <textarea
          rows={2}
          value={prescription.notes ?? ""}
          onChange={(e) =>
            onChange({ ...prescription, notes: e.target.value || null })
          }
          placeholder="Special instructions, precautions..."
          className="mt-0.5 w-full rounded border border-gray-300 px-2 py-1 text-sm focus:border-indigo-400 focus:outline-none focus:ring-1 focus:ring-indigo-400"
        />
      </div>
    </div>
  );
}

// ─── Main component ──────────────────────────────────────────────────────────

export function HEPBuilderPage({
  patientId,
  encounterId,
  role,
  userId: _userId,
}: HEPBuilderPageProps) {
  const { goBack } = useNav();
  const writeEnabled = canWrite(role);

  // ── Library state ────────────────────────────────────────────────────────
  const [exercises, setExercises] = useState<Exercise[]>([]);
  const [searchQuery, setSearchQuery] = useState("");
  const [regionFilter, setRegionFilter] = useState<ExerciseRegion | "">("");
  const [categoryFilter, setCategoryFilter] = useState<ExerciseCategory | "">("");
  const [libLoading, setLibLoading] = useState(true);
  const [libError, setLibError] = useState<string | null>(null);

  // ── Program state ────────────────────────────────────────────────────────
  const [programExercises, setProgramExercises] = useState<ExercisePrescription[]>([]);
  const [programNotes, setProgramNotes] = useState("");

  // ── Template state ───────────────────────────────────────────────────────
  const [templates, setTemplates] = useState<HEPTemplate[]>([]);
  const [templateName, setTemplateName] = useState("");
  const [selectedTemplateId, setSelectedTemplateId] = useState("");

  // ── Save state ───────────────────────────────────────────────────────────
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [saveSuccess, setSaveSuccess] = useState(false);

  // ── Load exercise library ────────────────────────────────────────────────
  const loadExercises = useCallback(() => {
    setLibLoading(true);
    setLibError(null);

    const promise = searchQuery.trim()
      ? commands.searchExercises(searchQuery.trim())
      : commands.listExercises(
          regionFilter || null,
          categoryFilter || null,
        );

    promise
      .then(setExercises)
      .catch((e) => {
        const msg = e instanceof Error ? e.message : String(e);
        console.error("[HEPBuilderPage] loadExercises failed:", msg);
        setLibError(msg);
      })
      .finally(() => setLibLoading(false));
  }, [searchQuery, regionFilter, categoryFilter]);

  useEffect(() => {
    loadExercises();
  }, [loadExercises]);

  // ── Load templates ───────────────────────────────────────────────────────
  useEffect(() => {
    commands
      .listHepTemplates()
      .then(setTemplates)
      .catch((e) => {
        console.error("[HEPBuilderPage] listHepTemplates failed:", e);
      });
  }, []);

  // ── Program mutation helpers ──────────────────────────────────────────────
  function addExercise(ex: Exercise) {
    if (programExercises.some((p) => p.exerciseId === ex.exerciseId)) return;
    setProgramExercises((prev) => [...prev, defaultPrescription(ex.exerciseId)]);
  }

  function removeExercise(idx: number) {
    setProgramExercises((prev) => prev.filter((_, i) => i !== idx));
  }

  function updatePrescription(idx: number, updated: ExercisePrescription) {
    setProgramExercises((prev) =>
      prev.map((p, i) => (i === idx ? updated : p)),
    );
  }

  function moveUp(idx: number) {
    if (idx === 0) return;
    setProgramExercises((prev) => {
      const next = [...prev];
      [next[idx - 1], next[idx]] = [next[idx], next[idx - 1]];
      return next;
    });
  }

  function moveDown(idx: number) {
    setProgramExercises((prev) => {
      if (idx >= prev.length - 1) return prev;
      const next = [...prev];
      [next[idx], next[idx + 1]] = [next[idx + 1], next[idx]];
      return next;
    });
  }

  // ── Template loading ─────────────────────────────────────────────────────
  async function loadTemplate(templateId: string) {
    if (!templateId) return;
    try {
      const tmpl = await commands.getHepTemplate(templateId);
      setProgramExercises(tmpl.exercises);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      console.error("[HEPBuilderPage] loadTemplate failed:", msg);
    }
  }

  // ── Save program ─────────────────────────────────────────────────────────
  async function handleSaveProgram() {
    if (!writeEnabled || programExercises.length === 0) return;
    setSaving(true);
    setSaveError(null);
    setSaveSuccess(false);
    try {
      await commands.createHepProgram({
        patientId,
        encounterId: encounterId ?? null,
        exercises: programExercises,
        notes: programNotes.trim() || null,
      });
      setSaveSuccess(true);
      setTimeout(() => setSaveSuccess(false), 3000);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      console.error("[HEPBuilderPage] createHepProgram failed:", msg);
      setSaveError(msg);
    } finally {
      setSaving(false);
    }
  }

  // ── Save template ────────────────────────────────────────────────────────
  async function handleSaveTemplate() {
    if (!writeEnabled || programExercises.length === 0 || !templateName.trim()) return;
    setSaving(true);
    setSaveError(null);
    try {
      const tmpl = await commands.createHepTemplate({
        name: templateName.trim(),
        bodyRegion: null,
        conditionName: null,
        exercises: programExercises,
      });
      setTemplates((prev) => [...prev, tmpl]);
      setTemplateName("");
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      console.error("[HEPBuilderPage] createHepTemplate failed:", msg);
      setSaveError(msg);
    } finally {
      setSaving(false);
    }
  }

  // ── Build a lookup map for exercise names ────────────────────────────────
  const exerciseMap = Object.fromEntries(exercises.map((e) => [e.exerciseId, e.name]));

  // ─── Render ───────────────────────────────────────────────────────────────

  return (
    <div className="flex h-full flex-col">
      {/* ── Page header ──────────────────────────────────────────────── */}
      <div className="flex items-center gap-3 border-b border-gray-200 bg-white px-6 py-4">
        <button
          type="button"
          onClick={goBack}
          className="rounded-md p-1.5 text-gray-500 hover:bg-gray-100 hover:text-gray-700 focus:outline-none focus:ring-2 focus:ring-gray-400 focus:ring-offset-1"
          aria-label="Back"
        >
          ← Back
        </button>
        <h1 className="text-xl font-bold text-gray-900">HEP Builder</h1>
        {!writeEnabled && (
          <span className="ml-auto rounded-full bg-amber-100 px-3 py-0.5 text-xs font-medium text-amber-700">
            Read-only
          </span>
        )}
      </div>

      {/* ── Two-panel body ───────────────────────────────────────────── */}
      <div className="flex min-h-0 flex-1 overflow-hidden">
        {/* ── LEFT: Exercise Library ────────────────────────────────── */}
        <div className="flex w-80 flex-none flex-col border-r border-gray-200 bg-gray-50">
          {/* Search + filters */}
          <div className="space-y-2 border-b border-gray-200 bg-white p-3">
            <input
              type="search"
              placeholder="Search exercises..."
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              className="w-full rounded-md border border-gray-300 px-3 py-1.5 text-sm focus:border-indigo-400 focus:outline-none focus:ring-1 focus:ring-indigo-400"
            />
            <div className="flex gap-2">
              <select
                value={regionFilter}
                onChange={(e) => {
                  setRegionFilter(e.target.value as ExerciseRegion | "");
                  setSearchQuery("");
                }}
                className="flex-1 rounded border border-gray-300 px-2 py-1 text-xs focus:border-indigo-400 focus:outline-none focus:ring-1 focus:ring-indigo-400"
              >
                <option value="">All regions</option>
                {BODY_REGIONS.map((r) => (
                  <option key={r.value} value={r.value}>
                    {r.label}
                  </option>
                ))}
              </select>
              <select
                value={categoryFilter}
                onChange={(e) => {
                  setCategoryFilter(e.target.value as ExerciseCategory | "");
                  setSearchQuery("");
                }}
                className="flex-1 rounded border border-gray-300 px-2 py-1 text-xs focus:border-indigo-400 focus:outline-none focus:ring-1 focus:ring-indigo-400"
              >
                <option value="">All categories</option>
                {CATEGORIES.map((c) => (
                  <option key={c.value} value={c.value}>
                    {c.label}
                  </option>
                ))}
              </select>
            </div>
          </div>

          {/* Exercise list */}
          <div className="flex-1 overflow-y-auto p-2 space-y-2">
            {libLoading && (
              <p className="py-8 text-center text-sm text-gray-400">
                Loading library…
              </p>
            )}
            {libError && !libLoading && (
              <div className="rounded border border-red-200 bg-red-50 p-2 text-xs text-red-700">
                {libError}
                <button
                  type="button"
                  onClick={loadExercises}
                  className="ml-2 underline"
                >
                  Retry
                </button>
              </div>
            )}
            {!libLoading &&
              !libError &&
              exercises.length === 0 && (
                <p className="py-8 text-center text-sm text-gray-400">
                  No exercises match your filters.
                </p>
              )}
            {!libLoading &&
              exercises.map((ex) => (
                <ExerciseCard
                  key={ex.exerciseId}
                  exercise={ex}
                  onAdd={addExercise}
                  alreadyAdded={programExercises.some(
                    (p) => p.exerciseId === ex.exerciseId,
                  )}
                  writeEnabled={writeEnabled}
                />
              ))}
          </div>
        </div>

        {/* ── RIGHT: Program Builder ────────────────────────────────── */}
        <div className="flex flex-1 flex-col overflow-hidden">
          {/* Template controls */}
          <div className="flex flex-wrap items-center gap-3 border-b border-gray-200 bg-white px-4 py-3">
            <span className="text-xs font-semibold uppercase tracking-wide text-gray-500">
              Templates:
            </span>
            <select
              value={selectedTemplateId}
              onChange={(e) => setSelectedTemplateId(e.target.value)}
              className="rounded border border-gray-300 px-2 py-1 text-sm focus:border-indigo-400 focus:outline-none focus:ring-1 focus:ring-indigo-400"
            >
              <option value="">-- Load template --</option>
              {templates.map((t) => (
                <option key={t.templateId} value={t.templateId}>
                  {t.isBuiltin ? `[Built-in] ` : ""}{t.name}
                </option>
              ))}
            </select>
            {writeEnabled && (
              <button
                type="button"
                onClick={() => loadTemplate(selectedTemplateId)}
                disabled={!selectedTemplateId}
                className="rounded-md bg-gray-100 px-3 py-1 text-xs font-medium text-gray-700 hover:bg-gray-200 disabled:cursor-default disabled:opacity-50 focus:outline-none focus:ring-2 focus:ring-gray-400 focus:ring-offset-1"
              >
                Load
              </button>
            )}

            {/* Save as template */}
            {writeEnabled && (
              <div className="ml-auto flex items-center gap-2">
                <input
                  type="text"
                  placeholder="Template name..."
                  value={templateName}
                  onChange={(e) => setTemplateName(e.target.value)}
                  className="w-44 rounded border border-gray-300 px-2 py-1 text-sm focus:border-indigo-400 focus:outline-none focus:ring-1 focus:ring-indigo-400"
                />
                <button
                  type="button"
                  onClick={handleSaveTemplate}
                  disabled={
                    saving ||
                    !templateName.trim() ||
                    programExercises.length === 0
                  }
                  className="rounded-md bg-indigo-100 px-3 py-1 text-xs font-medium text-indigo-700 hover:bg-indigo-200 disabled:cursor-default disabled:opacity-50 focus:outline-none focus:ring-2 focus:ring-indigo-400 focus:ring-offset-1"
                >
                  Save as Template
                </button>
              </div>
            )}
          </div>

          {/* Program header */}
          <div className="flex items-center justify-between border-b border-gray-200 bg-white px-4 py-2">
            <div className="flex items-center gap-2">
              <h2 className="text-sm font-semibold text-gray-800">
                Current Program
              </h2>
              <span className="rounded-full bg-indigo-100 px-2 py-0.5 text-xs font-medium text-indigo-700">
                {programExercises.length} exercise
                {programExercises.length !== 1 ? "s" : ""}
              </span>
            </div>
            {writeEnabled && (
              <button
                type="button"
                onClick={handleSaveProgram}
                disabled={saving || programExercises.length === 0}
                className="rounded-md bg-indigo-600 px-3 py-1.5 text-xs font-semibold text-white hover:bg-indigo-700 disabled:cursor-default disabled:opacity-50 focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:ring-offset-2"
              >
                {saving ? "Saving…" : "Save Program"}
              </button>
            )}
          </div>

          {/* Status banners */}
          {saveSuccess && (
            <div className="mx-4 mt-2 rounded border border-green-200 bg-green-50 px-3 py-2 text-sm text-green-700">
              Program saved successfully.
            </div>
          )}
          {saveError && (
            <div className="mx-4 mt-2 rounded border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-700">
              <span className="font-semibold">Save failed:</span> {saveError}
            </div>
          )}

          {/* Prescription list */}
          <div className="flex-1 overflow-y-auto p-4 space-y-3">
            {programExercises.length === 0 ? (
              <div className="flex flex-col items-center justify-center py-16 text-center">
                <p className="text-base font-medium text-gray-400">
                  No exercises added yet.
                </p>
                <p className="mt-1 text-sm text-gray-400">
                  Select exercises from the library on the left to build a program.
                </p>
              </div>
            ) : (
              <>
                {programExercises.map((p, idx) => (
                  <PrescriptionRow
                    key={`${p.exerciseId}-${idx}`}
                    prescription={p}
                    exerciseName={
                      exerciseMap[p.exerciseId] ?? p.exerciseId
                    }
                    index={idx}
                    total={programExercises.length}
                    onChange={(updated) => updatePrescription(idx, updated)}
                    onRemove={() => removeExercise(idx)}
                    onMoveUp={() => moveUp(idx)}
                    onMoveDown={() => moveDown(idx)}
                  />
                ))}

                {/* Program-level notes */}
                <div className="rounded-lg border border-gray-200 bg-white p-3">
                  <label className="text-xs font-medium text-gray-500">
                    Program Notes (optional)
                  </label>
                  <textarea
                    rows={3}
                    value={programNotes}
                    onChange={(e) => setProgramNotes(e.target.value)}
                    placeholder="General instructions, precautions for the entire program..."
                    disabled={!writeEnabled}
                    className="mt-1 w-full rounded border border-gray-300 px-2 py-1 text-sm focus:border-indigo-400 focus:outline-none focus:ring-1 focus:ring-indigo-400 disabled:bg-gray-50"
                  />
                </div>
              </>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

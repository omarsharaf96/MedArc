/**
 * PTNoteFormPage.tsx — PT note editing / viewing form.
 *
 * Supports all three note types: Initial Evaluation, Progress Note,
 * Discharge Summary. Manages the full draft → signed → locked lifecycle.
 *
 * Behaviour:
 *   - ptNoteId === "new": creates on first save via createPtNote.
 *   - ptNoteId !== "new": loads existing note via getPtNote on mount.
 *   - Locked notes: all inputs are readOnly; no action buttons shown.
 *   - Draft notes: Save Draft, and (if existing) Co-sign Note available.
 *   - Signed notes: Lock Note available.
 *
 * Observability:
 *   - Inline success/error feedback on every action.
 *   - Backend audit rows written by T03 commands for every state transition.
 *   - console.error logs all backend failures with note/patient context.
 */
import { useState, useEffect, useRef } from "react";
import { commands } from "../lib/tauri";
import { useNav } from "../contexts/RouterContext";
import type {
  PtNoteType,
  PtNoteStatus,
  PtNoteRecord,
  PtNoteInput,
  InitialEvalFields,
  ProgressNoteFields,
  DischargeSummaryFields,
} from "../types/pt";

// ─── Props ───────────────────────────────────────────────────────────────────

interface PTNoteFormPageProps {
  patientId: string;
  noteType: PtNoteType;
  ptNoteId: string;
  role: string;
}

// ─── Tailwind constants ───────────────────────────────────────────────────────

const INPUT_CLS =
  "w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500 disabled:bg-gray-50 disabled:text-gray-500";
const TEXTAREA_CLS =
  "w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500 resize-y disabled:bg-gray-50 disabled:text-gray-500";
const LABEL_CLS = "mb-1 block text-sm font-medium text-gray-700";
const FIELD_CLS = "flex flex-col gap-1";

// ─── Helpers ─────────────────────────────────────────────────────────────────

/** Human-readable page title for each note type. */
function noteTypeTitle(noteType: PtNoteType): string {
  switch (noteType) {
    case "initial_eval":
      return "Initial Evaluation";
    case "progress_note":
      return "Progress Note";
    case "discharge_summary":
      return "Discharge Summary";
  }
}

/** Extract typed fields from the resource blob stored on a PtNoteRecord. */
function extractFields(record: PtNoteRecord): Record<string, string | null> {
  const resource = record.resource;
  // Fields are stored under the "fields" key in the serde tag+content encoding
  const raw = resource["fields"] as Record<string, unknown> | undefined;
  if (!raw) return {};
  const result: Record<string, string | null> = {};
  for (const [k, v] of Object.entries(raw)) {
    result[k] = typeof v === "string" ? v : null;
  }
  return result;
}

// ─── Blank field defaults ─────────────────────────────────────────────────────

function blankInitialEval(): InitialEvalFields {
  return {
    chiefComplaint: null,
    mechanismOfInjury: null,
    priorLevelOfFunction: null,
    painNrs: null,
    functionalLimitations: null,
    icd10Codes: null,
    physicalExamFindings: null,
    shortTermGoals: null,
    longTermGoals: null,
    planOfCare: null,
    frequencyDuration: null,
    cptCodes: null,
    referringPhysician: null,
    referralDocumentId: null,
  };
}

function blankProgressNote(): ProgressNoteFields {
  return {
    subjective: null,
    patientReportPainNrs: null,
    hepCompliance: null,
    barriers: null,
    treatments: null,
    exercises: null,
    assessment: null,
    progressTowardGoals: null,
    plan: null,
    hepUpdates: null,
    totalTreatmentMinutes: null,
  };
}

function blankDischargeSummary(): DischargeSummaryFields {
  return {
    totalVisitsAttended: null,
    totalVisitsAuthorized: null,
    treatmentSummary: null,
    goalAchievement: null,
    outcomeComparisonPlaceholder: null,
    dischargeRecommendations: null,
    hepNarrative: null,
    returnToCare: null,
  };
}

// ─── Field-set sub-components ─────────────────────────────────────────────────

interface InitialEvalFormProps {
  fields: InitialEvalFields;
  readOnly: boolean;
  onChange: (key: keyof InitialEvalFields, value: string) => void;
}

function InitialEvalForm({ fields, readOnly, onChange }: InitialEvalFormProps) {
  const rows: { key: keyof InitialEvalFields; label: string; rows?: number }[] = [
    { key: "chiefComplaint", label: "Chief Complaint", rows: 3 },
    { key: "mechanismOfInjury", label: "Mechanism of Injury", rows: 3 },
    { key: "priorLevelOfFunction", label: "Prior Level of Function", rows: 3 },
    { key: "painNrs", label: "Pain NRS (0–10)", rows: 1 },
    { key: "functionalLimitations", label: "Functional Limitations", rows: 3 },
    { key: "icd10Codes", label: "ICD-10 Codes", rows: 2 },
    { key: "physicalExamFindings", label: "Physical Exam Findings", rows: 4 },
    { key: "shortTermGoals", label: "Short-Term Goals", rows: 3 },
    { key: "longTermGoals", label: "Long-Term Goals", rows: 3 },
    { key: "planOfCare", label: "Plan of Care", rows: 3 },
    { key: "frequencyDuration", label: "Frequency / Duration", rows: 2 },
    { key: "cptCodes", label: "CPT Codes", rows: 2 },
    { key: "referringPhysician", label: "Referring Physician", rows: 1 },
    { key: "referralDocumentId", label: "Referral Document ID", rows: 1 },
  ];

  return (
    <div className="space-y-4">
      {rows.map(({ key, label, rows: numRows }) => (
        <div key={key} className={FIELD_CLS}>
          <label className={LABEL_CLS}>{label}</label>
          {numRows && numRows > 1 ? (
            <textarea
              className={TEXTAREA_CLS}
              rows={numRows}
              value={fields[key] ?? ""}
              readOnly={readOnly}
              disabled={readOnly}
              onChange={(e) => onChange(key, e.target.value)}
            />
          ) : (
            <input
              type="text"
              className={INPUT_CLS}
              value={fields[key] ?? ""}
              readOnly={readOnly}
              disabled={readOnly}
              onChange={(e) => onChange(key, e.target.value)}
            />
          )}
        </div>
      ))}
    </div>
  );
}

interface ProgressNoteFormProps {
  fields: ProgressNoteFields;
  readOnly: boolean;
  onChange: (key: keyof ProgressNoteFields, value: string) => void;
}

function ProgressNoteForm({ fields, readOnly, onChange }: ProgressNoteFormProps) {
  return (
    <div className="space-y-4">
      <div className={FIELD_CLS}>
        <label className={LABEL_CLS}>Subjective</label>
        <textarea
          className={TEXTAREA_CLS}
          rows={3}
          value={fields.subjective ?? ""}
          readOnly={readOnly}
          disabled={readOnly}
          onChange={(e) => onChange("subjective", e.target.value)}
        />
      </div>

      <div className={FIELD_CLS}>
        <label className={LABEL_CLS}>Patient-Reported Pain NRS (0–10)</label>
        <input
          type="text"
          className={INPUT_CLS}
          value={fields.patientReportPainNrs ?? ""}
          readOnly={readOnly}
          disabled={readOnly}
          onChange={(e) => onChange("patientReportPainNrs", e.target.value)}
        />
      </div>

      <div className={FIELD_CLS}>
        <label className={LABEL_CLS}>HEP Compliance</label>
        <select
          className={INPUT_CLS}
          value={fields.hepCompliance ?? ""}
          disabled={readOnly}
          onChange={(e) => onChange("hepCompliance", e.target.value)}
        >
          <option value="">— Select —</option>
          <option value="yes">Yes</option>
          <option value="no">No</option>
          <option value="partial">Partial</option>
        </select>
      </div>

      <div className={FIELD_CLS}>
        <label className={LABEL_CLS}>Barriers</label>
        <textarea
          className={TEXTAREA_CLS}
          rows={2}
          value={fields.barriers ?? ""}
          readOnly={readOnly}
          disabled={readOnly}
          onChange={(e) => onChange("barriers", e.target.value)}
        />
      </div>

      <div className={FIELD_CLS}>
        <label className={LABEL_CLS}>Treatments</label>
        <textarea
          className={TEXTAREA_CLS}
          rows={3}
          value={fields.treatments ?? ""}
          readOnly={readOnly}
          disabled={readOnly}
          onChange={(e) => onChange("treatments", e.target.value)}
        />
      </div>

      <div className={FIELD_CLS}>
        <label className={LABEL_CLS}>Exercises</label>
        <textarea
          className={TEXTAREA_CLS}
          rows={3}
          value={fields.exercises ?? ""}
          readOnly={readOnly}
          disabled={readOnly}
          onChange={(e) => onChange("exercises", e.target.value)}
        />
      </div>

      <div className={FIELD_CLS}>
        <label className={LABEL_CLS}>Assessment</label>
        <textarea
          className={TEXTAREA_CLS}
          rows={3}
          value={fields.assessment ?? ""}
          readOnly={readOnly}
          disabled={readOnly}
          onChange={(e) => onChange("assessment", e.target.value)}
        />
      </div>

      <div className={FIELD_CLS}>
        <label className={LABEL_CLS}>Progress Toward Goals</label>
        <textarea
          className={TEXTAREA_CLS}
          rows={3}
          value={fields.progressTowardGoals ?? ""}
          readOnly={readOnly}
          disabled={readOnly}
          onChange={(e) => onChange("progressTowardGoals", e.target.value)}
        />
      </div>

      <div className={FIELD_CLS}>
        <label className={LABEL_CLS}>Plan</label>
        <textarea
          className={TEXTAREA_CLS}
          rows={3}
          value={fields.plan ?? ""}
          readOnly={readOnly}
          disabled={readOnly}
          onChange={(e) => onChange("plan", e.target.value)}
        />
      </div>

      <div className={FIELD_CLS}>
        <label className={LABEL_CLS}>HEP Updates</label>
        <textarea
          className={TEXTAREA_CLS}
          rows={2}
          value={fields.hepUpdates ?? ""}
          readOnly={readOnly}
          disabled={readOnly}
          onChange={(e) => onChange("hepUpdates", e.target.value)}
        />
      </div>

      <div className={FIELD_CLS}>
        <label className={LABEL_CLS}>Total Treatment Minutes</label>
        <input
          type="text"
          className={INPUT_CLS}
          value={fields.totalTreatmentMinutes ?? ""}
          readOnly={readOnly}
          disabled={readOnly}
          onChange={(e) => onChange("totalTreatmentMinutes", e.target.value)}
        />
      </div>
    </div>
  );
}

interface DischargeSummaryFormProps {
  fields: DischargeSummaryFields;
  readOnly: boolean;
  onChange: (key: keyof DischargeSummaryFields, value: string) => void;
}

function DischargeSummaryForm({ fields, readOnly, onChange }: DischargeSummaryFormProps) {
  return (
    <div className="space-y-4">
      <div className={FIELD_CLS}>
        <label className={LABEL_CLS}>Total Visits Attended</label>
        <input
          type="text"
          className={INPUT_CLS}
          value={fields.totalVisitsAttended ?? ""}
          readOnly={readOnly}
          disabled={readOnly}
          onChange={(e) => onChange("totalVisitsAttended", e.target.value)}
        />
      </div>

      <div className={FIELD_CLS}>
        <label className={LABEL_CLS}>Total Visits Authorized</label>
        <input
          type="text"
          className={INPUT_CLS}
          value={fields.totalVisitsAuthorized ?? ""}
          readOnly={readOnly}
          disabled={readOnly}
          onChange={(e) => onChange("totalVisitsAuthorized", e.target.value)}
        />
      </div>

      <div className={FIELD_CLS}>
        <label className={LABEL_CLS}>Treatment Summary</label>
        <textarea
          className={TEXTAREA_CLS}
          rows={4}
          value={fields.treatmentSummary ?? ""}
          readOnly={readOnly}
          disabled={readOnly}
          onChange={(e) => onChange("treatmentSummary", e.target.value)}
        />
      </div>

      <div className={FIELD_CLS}>
        <label className={LABEL_CLS}>Goal Achievement</label>
        <textarea
          className={TEXTAREA_CLS}
          rows={3}
          value={fields.goalAchievement ?? ""}
          readOnly={readOnly}
          disabled={readOnly}
          onChange={(e) => onChange("goalAchievement", e.target.value)}
        />
      </div>

      {/* Outcome comparison — reserved for S02, shown as informational */}
      <div className="rounded-md border border-amber-200 bg-amber-50 px-4 py-3 text-sm text-amber-700">
        <p className="font-medium">Outcome Comparison</p>
        <p className="mt-0.5">
          Outcome comparison will be available after S02 (outcome-measure
          integration). This section will display pre- and post-treatment
          functional scores automatically.
        </p>
      </div>

      <div className={FIELD_CLS}>
        <label className={LABEL_CLS}>Discharge Recommendations</label>
        <textarea
          className={TEXTAREA_CLS}
          rows={3}
          value={fields.dischargeRecommendations ?? ""}
          readOnly={readOnly}
          disabled={readOnly}
          onChange={(e) => onChange("dischargeRecommendations", e.target.value)}
        />
      </div>

      <div className={FIELD_CLS}>
        <label className={LABEL_CLS}>HEP Narrative</label>
        <textarea
          className={TEXTAREA_CLS}
          rows={3}
          value={fields.hepNarrative ?? ""}
          readOnly={readOnly}
          disabled={readOnly}
          onChange={(e) => onChange("hepNarrative", e.target.value)}
        />
      </div>

      <div className={FIELD_CLS}>
        <label className={LABEL_CLS}>Return to Care</label>
        <textarea
          className={TEXTAREA_CLS}
          rows={2}
          value={fields.returnToCare ?? ""}
          readOnly={readOnly}
          disabled={readOnly}
          onChange={(e) => onChange("returnToCare", e.target.value)}
        />
      </div>
    </div>
  );
}

// ─── Main component ──────────────────────────────────────────────────────────

export function PTNoteFormPage({
  patientId,
  noteType,
  ptNoteId,
  role: _role,
}: PTNoteFormPageProps) {
  const { goBack } = useNav();
  const isNew = ptNoteId === "new";

  // ── Field state ────────────────────────────────────────────────────────
  const [ieFields, setIeFields] = useState<InitialEvalFields>(blankInitialEval());
  const [pnFields, setPnFields] = useState<ProgressNoteFields>(blankProgressNote());
  const [dsFields, setDsFields] = useState<DischargeSummaryFields>(blankDischargeSummary());

  // ── Note record state ──────────────────────────────────────────────────
  const [noteRecord, setNoteRecord] = useState<PtNoteRecord | null>(null);
  const [status, setStatus] = useState<PtNoteStatus>("draft");
  const [currentNoteId, setCurrentNoteId] = useState<string | null>(
    isNew ? null : ptNoteId,
  );

  // ── UI state ───────────────────────────────────────────────────────────
  const [loadingNote, setLoadingNote] = useState(!isNew);
  const [loadError, setLoadError] = useState<string | null>(null);

  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [saveSuccess, setSaveSuccess] = useState(false);
  const timeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const [cosigning, setCosigning] = useState(false);
  const [cosignError, setCosignError] = useState<string | null>(null);

  const [locking, setLocking] = useState(false);
  const [lockError, setLockError] = useState<string | null>(null);

  // ── Cleanup timeout on unmount ────────────────────────────────────────
  useEffect(() => {
    return () => {
      if (timeoutRef.current) clearTimeout(timeoutRef.current);
    };
  }, []);

  // ── Load existing note ─────────────────────────────────────────────────
  useEffect(() => {
    if (isNew) return;

    let mounted = true;
    setLoadingNote(true);
    setLoadError(null);

    commands
      .getPtNote(ptNoteId)
      .then((record) => {
        if (!mounted) return;
        applyRecord(record);
      })
      .catch((e) => {
        if (!mounted) return;
        const msg = e instanceof Error ? e.message : String(e);
        console.error(`[PTNoteFormPage] getPtNote(${ptNoteId}) failed:`, msg);
        setLoadError(msg);
      })
      .finally(() => {
        if (mounted) setLoadingNote(false);
      });

    return () => {
      mounted = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ptNoteId, isNew]);

  /** Apply a loaded/refreshed PtNoteRecord to local state. */
  function applyRecord(record: PtNoteRecord) {
    setNoteRecord(record);
    setStatus(record.status);
    setCurrentNoteId(record.id);

    const raw = extractFields(record);

    if (record.noteType === "initial_eval") {
      setIeFields({
        chiefComplaint: raw["chiefComplaint"] ?? null,
        mechanismOfInjury: raw["mechanismOfInjury"] ?? null,
        priorLevelOfFunction: raw["priorLevelOfFunction"] ?? null,
        painNrs: raw["painNrs"] ?? null,
        functionalLimitations: raw["functionalLimitations"] ?? null,
        icd10Codes: raw["icd10Codes"] ?? null,
        physicalExamFindings: raw["physicalExamFindings"] ?? null,
        shortTermGoals: raw["shortTermGoals"] ?? null,
        longTermGoals: raw["longTermGoals"] ?? null,
        planOfCare: raw["planOfCare"] ?? null,
        frequencyDuration: raw["frequencyDuration"] ?? null,
        cptCodes: raw["cptCodes"] ?? null,
        referringPhysician: raw["referringPhysician"] ?? null,
        referralDocumentId: raw["referralDocumentId"] ?? null,
      });
    } else if (record.noteType === "progress_note") {
      setPnFields({
        subjective: raw["subjective"] ?? null,
        patientReportPainNrs: raw["patientReportPainNrs"] ?? null,
        hepCompliance: raw["hepCompliance"] ?? null,
        barriers: raw["barriers"] ?? null,
        treatments: raw["treatments"] ?? null,
        exercises: raw["exercises"] ?? null,
        assessment: raw["assessment"] ?? null,
        progressTowardGoals: raw["progressTowardGoals"] ?? null,
        plan: raw["plan"] ?? null,
        hepUpdates: raw["hepUpdates"] ?? null,
        totalTreatmentMinutes: raw["totalTreatmentMinutes"] ?? null,
      });
    } else {
      setDsFields({
        totalVisitsAttended: raw["totalVisitsAttended"] ?? null,
        totalVisitsAuthorized: raw["totalVisitsAuthorized"] ?? null,
        treatmentSummary: raw["treatmentSummary"] ?? null,
        goalAchievement: raw["goalAchievement"] ?? null,
        outcomeComparisonPlaceholder: raw["outcomeComparisonPlaceholder"] ?? null,
        dischargeRecommendations: raw["dischargeRecommendations"] ?? null,
        hepNarrative: raw["hepNarrative"] ?? null,
        returnToCare: raw["returnToCare"] ?? null,
      });
    }
  }

  /** Build the current fields value for the active note type. */
  function currentFields(): InitialEvalFields | ProgressNoteFields | DischargeSummaryFields {
    switch (noteType) {
      case "initial_eval":
        return ieFields;
      case "progress_note":
        return pnFields;
      case "discharge_summary":
        return dsFields;
    }
  }

  // ── Save Draft ─────────────────────────────────────────────────────────
  async function handleSaveDraft() {
    setSaving(true);
    setSaveError(null);
    setSaveSuccess(false);

    const input: PtNoteInput = {
      patientId,
      encounterId: null,
      noteType,
      fields: currentFields(),
      addendumOf: null,
    };

    try {
      let updated: PtNoteRecord;
      if (currentNoteId === null) {
        // New note — create
        updated = await commands.createPtNote(input);
      } else {
        // Existing note — update
        updated = await commands.updatePtNote(currentNoteId, input);
      }
      applyRecord(updated);
      setSaveSuccess(true);
      // Clear success feedback after 3 s
      if (timeoutRef.current) clearTimeout(timeoutRef.current);
      timeoutRef.current = setTimeout(() => setSaveSuccess(false), 3000);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      console.error(
        `[PTNoteFormPage] save failed for note ${currentNoteId ?? "new"} (patient ${patientId}):`,
        msg,
      );
      setSaveError(msg);
    } finally {
      setSaving(false);
    }
  }

  // ── Co-sign Note ───────────────────────────────────────────────────────
  async function handleCosign() {
    if (!currentNoteId) return;
    setCosigning(true);
    setCosignError(null);
    try {
      const updated = await commands.cosignPtNote(currentNoteId);
      applyRecord(updated);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      console.error(
        `[PTNoteFormPage] cosign failed for note ${currentNoteId} (patient ${patientId}):`,
        msg,
      );
      setCosignError(msg);
    } finally {
      setCosigning(false);
    }
  }

  // ── Lock Note ──────────────────────────────────────────────────────────
  async function handleLock() {
    if (!currentNoteId) return;
    setLocking(true);
    setLockError(null);
    try {
      const updated = await commands.lockPtNote(currentNoteId);
      applyRecord(updated);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      console.error(
        `[PTNoteFormPage] lock failed for note ${currentNoteId} (patient ${patientId}):`,
        msg,
      );
      setLockError(msg);
    } finally {
      setLocking(false);
    }
  }

  const isLocked = status === "locked";
  const isDraft = status === "draft";
  const isSigned = status === "signed";

  // ── Loading state ──────────────────────────────────────────────────────
  if (loadingNote) {
    return (
      <div className="flex items-center gap-3 p-6 text-sm text-gray-500">
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
        Loading note…
      </div>
    );
  }

  // ── Load error ─────────────────────────────────────────────────────────
  if (loadError) {
    return (
      <div className="p-6">
        <button
          type="button"
          onClick={goBack}
          className="mb-4 rounded-md p-1.5 text-gray-500 hover:bg-gray-100"
        >
          ← Back
        </button>
        <div className="rounded-md border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700">
          <p className="font-semibold">Failed to load note</p>
          <p className="mt-0.5">{loadError}</p>
        </div>
      </div>
    );
  }

  // ── Derived display values ─────────────────────────────────────────────
  const pageTitle = noteTypeTitle(noteType);
  const createdAt = noteRecord ? noteRecord.createdAt.slice(0, 10) : "New";

  return (
    <div className="space-y-6 p-6">
      {/* ── Header ──────────────────────────────────────────────────────── */}
      <div className="flex items-center justify-between gap-4">
        <div className="flex items-center gap-3">
          <button
            type="button"
            onClick={goBack}
            className="rounded-md p-1.5 text-gray-500 hover:bg-gray-100 hover:text-gray-700 focus:outline-none focus:ring-2 focus:ring-gray-400 focus:ring-offset-1"
            aria-label="Back"
          >
            ← Back
          </button>
          <div>
            <h1 className="text-xl font-bold text-gray-900">{pageTitle}</h1>
            <p className="text-xs text-gray-500">
              {isNew ? "New note" : `Created ${createdAt}`}
              {" · "}
              <span
                className={[
                  "inline-flex rounded-full px-2 py-0.5 text-xs font-medium capitalize",
                  status === "draft"
                    ? "bg-gray-100 text-gray-700"
                    : status === "signed"
                      ? "bg-blue-100 text-blue-800"
                      : "bg-green-100 text-green-800",
                ].join(" ")}
              >
                {status}
              </span>
            </p>
          </div>
        </div>

        {/* ── Action buttons ───────────────────────────────────────────── */}
        {!isLocked && (
          <div className="flex items-center gap-2">
            {/* Save Draft — visible when draft or new */}
            {(isDraft || isNew) && (
              <button
                type="button"
                onClick={handleSaveDraft}
                disabled={saving}
                className="rounded-md bg-indigo-600 px-4 py-2 text-sm font-medium text-white hover:bg-indigo-700 focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:ring-offset-2 disabled:opacity-60"
              >
                {saving ? "Saving…" : "Save Draft"}
              </button>
            )}

            {/* Co-sign Note — draft only, existing note only */}
            {isDraft && currentNoteId !== null && (
              <button
                type="button"
                onClick={handleCosign}
                disabled={cosigning}
                className="rounded-md bg-blue-600 px-4 py-2 text-sm font-medium text-white hover:bg-blue-700 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:ring-offset-2 disabled:opacity-60"
              >
                {cosigning ? "Co-signing…" : "Co-sign Note"}
              </button>
            )}

            {/* Lock Note — signed only */}
            {isSigned && (
              <button
                type="button"
                onClick={handleLock}
                disabled={locking}
                className="rounded-md bg-green-700 px-4 py-2 text-sm font-medium text-white hover:bg-green-800 focus:outline-none focus:ring-2 focus:ring-green-600 focus:ring-offset-2 disabled:opacity-60"
              >
                {locking ? "Locking…" : "Lock Note"}
              </button>
            )}
          </div>
        )}
      </div>

      {/* ── Locked banner ───────────────────────────────────────────────── */}
      {isLocked && (
        <div className="rounded-md border border-green-200 bg-green-50 px-4 py-3 text-sm text-green-800">
          <p className="font-semibold">This note is locked and cannot be edited.</p>
          <p className="mt-0.5 text-green-700">
            All fields are read-only. No further changes can be made to this note.
          </p>
        </div>
      )}

      {/* ── Inline feedback ──────────────────────────────────────────────── */}
      {saveSuccess && (
        <div className="rounded-md border border-green-200 bg-green-50 px-4 py-2 text-sm text-green-700">
          Note saved successfully.
        </div>
      )}
      {saveError && (
        <div className="rounded-md border border-red-200 bg-red-50 px-4 py-2 text-sm text-red-700">
          <p className="font-semibold">Save failed</p>
          <p className="mt-0.5">{saveError}</p>
        </div>
      )}
      {cosignError && (
        <div className="rounded-md border border-red-200 bg-red-50 px-4 py-2 text-sm text-red-700">
          <p className="font-semibold">Co-sign failed</p>
          <p className="mt-0.5">{cosignError}</p>
        </div>
      )}
      {lockError && (
        <div className="rounded-md border border-red-200 bg-red-50 px-4 py-2 text-sm text-red-700">
          <p className="font-semibold">Lock failed</p>
          <p className="mt-0.5">{lockError}</p>
        </div>
      )}

      {/* ── Field set ────────────────────────────────────────────────────── */}
      <div className="rounded-lg border border-gray-200 bg-white p-6 shadow-sm">
        {noteType === "initial_eval" && (
          <InitialEvalForm
            fields={ieFields}
            readOnly={isLocked}
            onChange={(key, value) =>
              setIeFields((prev) => ({ ...prev, [key]: value }))
            }
          />
        )}
        {noteType === "progress_note" && (
          <ProgressNoteForm
            fields={pnFields}
            readOnly={isLocked}
            onChange={(key, value) =>
              setPnFields((prev) => ({ ...prev, [key]: value }))
            }
          />
        )}
        {noteType === "discharge_summary" && (
          <DischargeSummaryForm
            fields={dsFields}
            readOnly={isLocked}
            onChange={(key, value) =>
              setDsFields((prev) => ({ ...prev, [key]: value }))
            }
          />
        )}
      </div>
    </div>
  );
}

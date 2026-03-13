/**
 * EncounterWorkspace.tsx — Clinical encounter workspace shell.
 *
 * Renders the three-tab encounter workspace: SOAP | Vitals | ROS.
 * This shell handles loading / error states and tab chrome. Functional
 * form content is added in T02 (SOAP), T03 (Vitals), T04 (ROS).
 *
 * RBAC context is passed in as props (from ContentArea via useAuth) and
 * forwarded to tab content components in later tasks.
 *
 * Observability:
 *   - `console.error("[useEncounter] …")` logged by the hook on fetch failure
 *   - Inline error banner with "Retry" button visible without DevTools
 *   - Tab state inspectable via React DevTools (`activeTab`, `encounter`, etc.)
 *   - SOAP tab: `soapState`, `savingSoap`, `soapSaveError`, `isFinalized`
 *     all visible as component state on EncounterWorkspace in React DevTools
 */
import { useState, useEffect, useCallback } from "react";
import { useEncounter } from "../hooks/useEncounter";
import { useNav } from "../contexts/RouterContext";
import { commands } from "../lib/tauri";
import type {
  SoapInput,
  VitalsInput,
  VitalsRecord,
  ReviewOfSystemsInput,
  RosRecord,
  RosStatus,
  PhysicalExamInput,
  PhysicalExamRecord,
} from "../types/documentation";

// ─── Props ───────────────────────────────────────────────────────────────────

interface EncounterWorkspaceProps {
  patientId: string;
  encounterId: string;
  role: string;
  userId: string;
}

// ─── Tab type ────────────────────────────────────────────────────────────────

type ActiveTab = "soap" | "vitals" | "ros" | "exam";

// ─── Tailwind class constants (mirrors PatientFormModal pattern) ─────────────

const INPUT_CLS =
  "w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500";
const LABEL_CLS = "mb-1 block text-sm font-medium text-gray-700";

// ─── Helpers ─────────────────────────────────────────────────────────────────

/** Format encounter type string for display: "office_visit" → "Office Visit" */
function formatEncounterType(raw: string): string {
  return raw
    .split("_")
    .map((w) => w.charAt(0).toUpperCase() + w.slice(1))
    .join(" ");
}

/** Extract the encounter date (YYYY-MM-DD) from a FHIR resource object. */
function extractEncounterDate(resource: Record<string, unknown>): string | null {
  // FHIR Encounter period.start or extension date
  const period = resource["period"] as Record<string, unknown> | undefined;
  const start = period?.["start"];
  if (typeof start === "string" && start.length >= 10) {
    return start.slice(0, 10);
  }
  // Fallback: check resource-level date field
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
  // MedArc stores encounter type in resource.type[0].text or resource.class.code
  const types = resource["type"] as Array<Record<string, unknown>> | undefined;
  const typeText = types?.[0]?.["text"];
  if (typeof typeText === "string") return typeText;

  const cls = resource["class"] as Record<string, unknown> | undefined;
  const code = cls?.["code"];
  if (typeof code === "string") return formatEncounterType(code);

  return null;
}

/** True when all four SOAP sections are null or empty string. */
function isSoapEmpty(soap: SoapInput): boolean {
  return (
    !soap.subjective?.trim() &&
    !soap.objective?.trim() &&
    !soap.assessment?.trim() &&
    !soap.plan?.trim()
  );
}

// ─── Loading skeleton ────────────────────────────────────────────────────────

function LoadingSkeleton() {
  return (
    <div className="animate-pulse space-y-4 p-6">
      <div className="h-8 w-1/3 rounded bg-gray-200" />
      <div className="h-4 w-1/2 rounded bg-gray-200" />
      <div className="flex gap-3">
        <div className="h-9 w-20 rounded bg-gray-200" />
        <div className="h-9 w-20 rounded bg-gray-200" />
        <div className="h-9 w-20 rounded bg-gray-200" />
      </div>
      <div className="h-64 rounded bg-gray-200" />
    </div>
  );
}

// ─── SOAP Tab ────────────────────────────────────────────────────────────────

interface SoapTabProps {
  encounterId: string;
  role: string;
  soapState: SoapInput;
  setSoapState: (s: SoapInput) => void;
  saveSoap: (soap: SoapInput) => Promise<void>;
  finalizeEncounter: (soap: SoapInput) => Promise<void>;
  isFinalized: boolean;
  templates: import("../types/documentation").TemplateRecord[];
}

function SoapTab({
  encounterId: _encounterId,
  role,
  soapState,
  setSoapState,
  saveSoap,
  finalizeEncounter,
  isFinalized,
  templates,
}: SoapTabProps) {
  // ── Template picker state ──────────────────────────────────────────────
  const [pendingTemplateId, setPendingTemplateId] = useState<string | null>(null);
  const [loadingTemplate, setLoadingTemplate] = useState(false);

  // ── Save state ────────────────────────────────────────────────────────
  const [savingSoap, setSavingSoap] = useState(false);
  const [soapSaveError, setSoapSaveError] = useState<string | null>(null);

  // ── Finalize state ────────────────────────────────────────────────────
  const [finalizing, setFinalizing] = useState(false);
  const [finalizeError, setFinalizeError] = useState<string | null>(null);

  // RBAC: NurseMa and BillingStaff get read-only mode
  const isReadOnly =
    isFinalized || role === "NurseMa" || role === "BillingStaff";

  // ── Template picker onChange ───────────────────────────────────────────
  const handleTemplateChange = useCallback(
    async (templateId: string) => {
      if (!templateId) return;

      if (!isSoapEmpty(soapState)) {
        // Non-empty note: show confirmation banner
        setPendingTemplateId(templateId);
      } else {
        // Empty note: apply immediately
        try {
          setLoadingTemplate(true);
          const tpl = await commands.getTemplate(templateId);
          setSoapState(tpl.defaultSoap);
        } catch (e) {
          const msg = e instanceof Error ? e.message : String(e);
          setSoapSaveError(`Failed to load template: ${msg}`);
        } finally {
          setLoadingTemplate(false);
        }
      }
    },
    [soapState, setSoapState],
  );

  // ── Apply confirmed template ───────────────────────────────────────────
  const applyPendingTemplate = useCallback(async () => {
    if (!pendingTemplateId) return;
    try {
      setLoadingTemplate(true);
      const tpl = await commands.getTemplate(pendingTemplateId);
      setSoapState(tpl.defaultSoap);
      setPendingTemplateId(null);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setSoapSaveError(`Failed to apply template: ${msg}`);
    } finally {
      setLoadingTemplate(false);
    }
  }, [pendingTemplateId, setSoapState]);

  // ── Save Note ─────────────────────────────────────────────────────────
  const handleSave = useCallback(async () => {
    setSavingSoap(true);
    setSoapSaveError(null);
    try {
      await saveSoap(soapState);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setSoapSaveError(msg);
      console.error("[EncounterWorkspace] saveSoap failed:", msg);
    } finally {
      setSavingSoap(false);
    }
  }, [saveSoap, soapState]);

  // ── Finalize Encounter ────────────────────────────────────────────────
  const handleFinalize = useCallback(async () => {
    setFinalizing(true);
    setFinalizeError(null);
    try {
      await finalizeEncounter(soapState);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setFinalizeError(msg);
      console.error("[EncounterWorkspace] finalizeEncounter failed:", msg);
    } finally {
      setFinalizing(false);
    }
  }, [finalizeEncounter, soapState]);

  // ── Find pending template name for the banner ─────────────────────────
  const pendingTemplateName = pendingTemplateId
    ? (templates.find((t) => t.id === pendingTemplateId)?.name ?? pendingTemplateId)
    : null;

  return (
    <div className="space-y-5">
      {/* ── Finalized badge ────────────────────────────────────────────── */}
      {isFinalized && (
        <div className="flex items-center gap-2 rounded-md border border-green-200 bg-green-50 px-4 py-2 text-sm font-medium text-green-700">
          <span className="text-base">✓</span>
          <span>Finalized — this encounter is read-only</span>
        </div>
      )}

      {/* ── Role read-only notice ─────────────────────────────────────── */}
      {!isFinalized && (role === "NurseMa" || role === "BillingStaff") && (
        <p className="text-xs text-gray-400 italic">Read-only for your role</p>
      )}

      {/* ── Template picker ───────────────────────────────────────────── */}
      {templates.length > 0 && (
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
              // Reset the select back to default so it can be re-selected if needed
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

          {/* Template confirmation banner */}
          {pendingTemplateId && pendingTemplateName && (
            <div className="mt-2 flex items-center gap-3 rounded-md border border-amber-200 bg-amber-50 px-4 py-2 text-sm">
              <span className="flex-1 text-amber-800">
                Apply &ldquo;{pendingTemplateName}&rdquo;? This will replace your
                current note.
              </span>
              <button
                type="button"
                onClick={() => void applyPendingTemplate()}
                disabled={loadingTemplate}
                className="rounded bg-amber-600 px-3 py-1 text-xs font-semibold text-white hover:bg-amber-700 disabled:opacity-60"
              >
                Apply
              </button>
              <button
                type="button"
                onClick={() => setPendingTemplateId(null)}
                className="rounded bg-white px-3 py-1 text-xs font-semibold text-gray-600 hover:bg-gray-100 border border-gray-300"
              >
                Cancel
              </button>
            </div>
          )}
        </div>
      )}

      {/* ── Subjective ───────────────────────────────────────────────────── */}
      <div>
        <label className={LABEL_CLS} htmlFor="soap-subjective">
          Subjective
        </label>
        <textarea
          id="soap-subjective"
          className={INPUT_CLS}
          rows={5}
          readOnly={isReadOnly}
          value={soapState.subjective ?? ""}
          onChange={(e) =>
            setSoapState({ ...soapState, subjective: e.target.value || null })
          }
          placeholder={isReadOnly ? "" : "Patient-reported symptoms, HPI, chief complaint…"}
        />
      </div>

      {/* ── Objective ────────────────────────────────────────────────────── */}
      <div>
        <label className={LABEL_CLS} htmlFor="soap-objective">
          Objective
        </label>
        <textarea
          id="soap-objective"
          className={INPUT_CLS}
          rows={5}
          readOnly={isReadOnly}
          value={soapState.objective ?? ""}
          onChange={(e) =>
            setSoapState({ ...soapState, objective: e.target.value || null })
          }
          placeholder={isReadOnly ? "" : "Exam findings, vitals summary…"}
        />
      </div>

      {/* ── Assessment ───────────────────────────────────────────────────── */}
      <div>
        <label className={LABEL_CLS} htmlFor="soap-assessment">
          Assessment
        </label>
        <textarea
          id="soap-assessment"
          className={INPUT_CLS}
          rows={5}
          readOnly={isReadOnly}
          value={soapState.assessment ?? ""}
          onChange={(e) =>
            setSoapState({ ...soapState, assessment: e.target.value || null })
          }
          placeholder={isReadOnly ? "" : "Diagnoses, ICD-10 codes, clinical impressions…"}
        />
      </div>

      {/* ── Plan ─────────────────────────────────────────────────────────── */}
      <div>
        <label className={LABEL_CLS} htmlFor="soap-plan">
          Plan
        </label>
        <textarea
          id="soap-plan"
          className={INPUT_CLS}
          rows={5}
          readOnly={isReadOnly}
          value={soapState.plan ?? ""}
          onChange={(e) =>
            setSoapState({ ...soapState, plan: e.target.value || null })
          }
          placeholder={isReadOnly ? "" : "Treatment orders, prescriptions, referrals, follow-up…"}
        />
      </div>

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
          {/* Save Note */}
          <button
            type="button"
            onClick={() => void handleSave()}
            disabled={savingSoap || finalizing}
            className="rounded-md bg-indigo-600 px-4 py-2 text-sm font-medium text-white hover:bg-indigo-700 focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:ring-offset-2 disabled:opacity-60"
          >
            {savingSoap ? "Saving…" : "Save Note"}
          </button>

          {/* Finalize Encounter — destructive amber styling */}
          <button
            type="button"
            onClick={() => void handleFinalize()}
            disabled={savingSoap || finalizing}
            className="rounded-md border-2 border-amber-500 bg-white px-4 py-2 text-sm font-medium text-amber-700 hover:bg-amber-50 focus:outline-none focus:ring-2 focus:ring-amber-400 focus:ring-offset-2 disabled:opacity-60"
          >
            {finalizing ? "Finalizing…" : "Finalize Encounter"}
          </button>
        </div>
      )}
    </div>
  );
}

// ─── Vitals form state type ───────────────────────────────────────────────────
// HTML <input type="number"> always returns strings, so we store strings here
// and parse to number | null on save.

interface VitalsFormState {
  systolicBp: string;
  diastolicBp: string;
  heartRate: string;
  respiratoryRate: string;
  temperatureCelsius: string;
  spo2Percent: string;
  weightKg: string;
  heightCm: string;
  painScore: string;
  notes: string;
}

const EMPTY_VITALS_FORM: VitalsFormState = {
  systolicBp: "",
  diastolicBp: "",
  heartRate: "",
  respiratoryRate: "",
  temperatureCelsius: "",
  spo2Percent: "",
  weightKg: "",
  heightCm: "",
  painScore: "",
  notes: "",
};

/** Extract a FHIR Observation component value by LOINC code (or any coding code). */
function extractObsComponent(
  resource: Record<string, unknown>,
  code: string,
): number | null {
  const components = resource["component"] as
    | Array<Record<string, unknown>>
    | undefined;
  if (!components) return null;
  for (const comp of components) {
    const coding = (comp["code"] as Record<string, unknown> | undefined)
      ?.["coding"] as Array<Record<string, unknown>> | undefined;
    const matches = coding?.some((c) => c["code"] === code);
    if (matches) {
      const vq = comp["valueQuantity"] as Record<string, unknown> | undefined;
      const val = vq?.["value"];
      return typeof val === "number" ? val : null;
    }
  }
  return null;
}

/** Extract a top-level FHIR Observation valueQuantity (single-component obs). */
function extractTopLevelValue(resource: Record<string, unknown>): number | null {
  const vq = resource["valueQuantity"] as Record<string, unknown> | undefined;
  const val = vq?.["value"];
  return typeof val === "number" ? val : null;
}

/** Seed VitalsFormState from a VitalsRecord's FHIR resource.
 *  LOINC codes used: 8480-6 (systolic), 8462-4 (diastolic), 8867-4 (HR),
 *  9279-1 (RR), 8310-5 (temp), 59408-5 (SpO2), 29463-7 (weight),
 *  8302-2 (height), 72514-3 (pain). */
function vitalsRecordToForm(record: VitalsRecord): VitalsFormState {
  const res = record.resource;
  // The resource may store vitals as a multi-component Observation or as
  // individual top-level valueQuantity fields. Try component extraction first.
  const systolicBp = extractObsComponent(res, "8480-6") ?? extractTopLevelValue(res);
  const diastolicBp = extractObsComponent(res, "8462-4");
  const heartRate = extractObsComponent(res, "8867-4");
  const respiratoryRate = extractObsComponent(res, "9279-1");
  const temperatureCelsius = extractObsComponent(res, "8310-5");
  const spo2Percent = extractObsComponent(res, "59408-5");
  const weightKg = extractObsComponent(res, "29463-7");
  const heightCm = extractObsComponent(res, "8302-2");
  const painScore = extractObsComponent(res, "72514-3");
  // Notes stored in resource.note[0].text or resource.valueString
  const notes =
    ((res["note"] as Array<Record<string, unknown>> | undefined)?.[0]?.[
      "text"
    ] as string | undefined) ??
    (res["valueString"] as string | undefined) ??
    "";

  return {
    systolicBp: systolicBp != null ? String(systolicBp) : "",
    diastolicBp: diastolicBp != null ? String(diastolicBp) : "",
    heartRate: heartRate != null ? String(heartRate) : "",
    respiratoryRate: respiratoryRate != null ? String(respiratoryRate) : "",
    temperatureCelsius: temperatureCelsius != null ? String(temperatureCelsius) : "",
    spo2Percent: spo2Percent != null ? String(spo2Percent) : "",
    weightKg: weightKg != null ? String(weightKg) : "",
    heightCm: heightCm != null ? String(heightCm) : "",
    painScore: painScore != null ? String(painScore) : "",
    notes,
  };
}

// ─── Vitals Tab ───────────────────────────────────────────────────────────────

interface VitalsTabProps {
  patientId: string;
  encounterId: string;
  role: string;
  latestVitals: VitalsRecord | null;
  saveVitals: (input: VitalsInput) => Promise<void>;
  isFinalized: boolean;
}

function VitalsTab({
  patientId,
  encounterId,
  role: _role,
  latestVitals,
  saveVitals,
  isFinalized,
}: VitalsTabProps) {
  // ── Form state — all string (HTML input values) ───────────────────────
  const [vitalsForm, setVitalsForm] = useState<VitalsFormState>(EMPTY_VITALS_FORM);
  const [savingVitals, setSavingVitals] = useState(false);
  const [vitalsError, setVitalsError] = useState<string | null>(null);

  // ── Seed form from latestVitals on first load or encounter change ─────
  // Uses a seeded-ID guard so in-progress edits aren't overwritten on reload.
  const [seededVitalsId, setSeededVitalsId] = useState<string | null>(null);
  useEffect(() => {
    if (!latestVitals) return;
    if (seededVitalsId === latestVitals.id) return;
    setVitalsForm(vitalsRecordToForm(latestVitals));
    setSeededVitalsId(latestVitals.id);
  }, [latestVitals, seededVitalsId]);

  // NurseMa and all clinical roles have full edit access on non-finalized encounters.
  const isReadOnly = isFinalized;

  // ── Field update helper ───────────────────────────────────────────────
  const setField = useCallback(
    (field: keyof VitalsFormState, value: string) => {
      setVitalsForm((prev) => ({ ...prev, [field]: value }));
    },
    [],
  );

  // ── Save handler ──────────────────────────────────────────────────────
  const handleSave = useCallback(async () => {
    setSavingVitals(true);
    setVitalsError(null);
    try {
      // Parse integer fields (whole numbers)
      const parseInt_ = (s: string): number | null =>
        s === "" ? null : parseInt(s, 10);
      // Parse float fields (allow decimals)
      const parseFloat_ = (s: string): number | null =>
        s === "" ? null : parseFloat(s);

      const rawPain = parseInt_(vitalsForm.painScore);
      const clampedPain =
        rawPain != null ? Math.min(10, Math.max(0, rawPain)) : null;

      const input: VitalsInput = {
        patientId,
        encounterId,
        recordedAt: new Date().toISOString().slice(0, 19),
        systolicBp: parseInt_(vitalsForm.systolicBp),
        diastolicBp: parseInt_(vitalsForm.diastolicBp),
        heartRate: parseInt_(vitalsForm.heartRate),
        respiratoryRate: parseInt_(vitalsForm.respiratoryRate),
        temperatureCelsius: parseFloat_(vitalsForm.temperatureCelsius),
        spo2Percent: parseInt_(vitalsForm.spo2Percent),
        weightKg: parseFloat_(vitalsForm.weightKg),
        heightCm: parseFloat_(vitalsForm.heightCm),
        painScore: clampedPain,
        notes: vitalsForm.notes === "" ? null : vitalsForm.notes,
      };

      await saveVitals(input);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setVitalsError(msg);
      console.error("[EncounterWorkspace] saveVitals failed:", msg);
    } finally {
      setSavingVitals(false);
    }
  }, [patientId, encounterId, vitalsForm, saveVitals]);

  // ── Numeric input helper ──────────────────────────────────────────────
  function NumericField({
    id,
    label,
    unit,
    field,
    min = "0",
    step,
  }: {
    id: string;
    label: string;
    unit: string;
    field: keyof VitalsFormState;
    min?: string;
    step?: string;
  }) {
    return (
      <div>
        <label className={LABEL_CLS} htmlFor={id}>
          {label}
          <span className="ml-1 text-xs font-normal text-gray-400">({unit})</span>
        </label>
        <input
          id={id}
          type="number"
          min={min}
          step={step}
          className={INPUT_CLS}
          value={vitalsForm[field]}
          readOnly={isReadOnly}
          onChange={(e) => setField(field, e.target.value)}
          placeholder={isReadOnly ? "" : "—"}
        />
      </div>
    );
  }

  return (
    <div className="space-y-5">
      {/* ── Finalized notice ──────────────────────────────────────────── */}
      {isFinalized && (
        <div className="flex items-center gap-2 rounded-md border border-green-200 bg-green-50 px-4 py-2 text-sm font-medium text-green-700">
          <span className="text-base">✓</span>
          <span>Vitals locked — encounter finalized</span>
        </div>
      )}

      {/* ── BMI display (server-computed, read-only) ──────────────────── */}
      <div className="flex items-center gap-3 rounded-md border border-blue-100 bg-blue-50 px-4 py-3">
        <span className="text-sm font-semibold text-blue-700">BMI</span>
        <span className="text-lg font-bold text-blue-900">
          {latestVitals?.bmi != null
            ? `${latestVitals.bmi.toFixed(1)} kg/m²`
            : "— kg/m²"}
        </span>
        <span className="text-xs text-blue-400">(server-computed after save)</span>
      </div>

      {/* ── Two-column vitals grid ────────────────────────────────────── */}
      <div className="grid grid-cols-2 gap-4">
        {/* Row 1 — Blood Pressure */}
        <NumericField
          id="vitals-systolic"
          label="Systolic BP"
          unit="mmHg"
          field="systolicBp"
          min="0"
        />
        <NumericField
          id="vitals-diastolic"
          label="Diastolic BP"
          unit="mmHg"
          field="diastolicBp"
          min="0"
        />

        {/* Row 2 — Heart Rate / Respiratory Rate */}
        <NumericField
          id="vitals-hr"
          label="Heart Rate"
          unit="bpm"
          field="heartRate"
          min="0"
        />
        <NumericField
          id="vitals-rr"
          label="Respiratory Rate"
          unit="breaths/min"
          field="respiratoryRate"
          min="0"
        />

        {/* Row 3 — Temperature / SpO2 */}
        <NumericField
          id="vitals-temp"
          label="Temperature"
          unit="°C"
          field="temperatureCelsius"
          min="0"
          step="0.1"
        />
        <NumericField
          id="vitals-spo2"
          label="SpO2"
          unit="%"
          field="spo2Percent"
          min="0"
        />

        {/* Row 4 — Weight / Height */}
        <NumericField
          id="vitals-weight"
          label="Weight"
          unit="kg"
          field="weightKg"
          min="0"
          step="0.1"
        />
        <NumericField
          id="vitals-height"
          label="Height"
          unit="cm"
          field="heightCm"
          min="0"
          step="0.1"
        />

        {/* Row 5 — Pain Score (col 1) | Notes spans both cols */}
        <NumericField
          id="vitals-pain"
          label="Pain Score (NRS)"
          unit="0–10"
          field="painScore"
          min="0"
        />

        {/* Notes: full-width textarea in the second column of row 5 */}
        <div>
          <label className={LABEL_CLS} htmlFor="vitals-notes">
            Notes
          </label>
          <textarea
            id="vitals-notes"
            className={INPUT_CLS}
            rows={2}
            readOnly={isReadOnly}
            value={vitalsForm.notes}
            onChange={(e) => setField("notes", e.target.value)}
            placeholder={isReadOnly ? "" : "Additional observations…"}
          />
        </div>
      </div>

      {/* ── Save error ────────────────────────────────────────────────── */}
      {vitalsError && (
        <p className="text-sm text-red-600">{vitalsError}</p>
      )}

      {/* ── Save button (hidden when finalized) ──────────────────────── */}
      {!isReadOnly && (
        <div className="pt-1">
          <button
            type="button"
            onClick={() => void handleSave()}
            disabled={savingVitals}
            className="rounded-md bg-teal-600 px-4 py-2 text-sm font-medium text-white hover:bg-teal-700 focus:outline-none focus:ring-2 focus:ring-teal-500 focus:ring-offset-2 disabled:opacity-60"
          >
            {savingVitals ? "Saving…" : "Save Vitals"}
          </button>
        </div>
      )}
    </div>
  );
}

// ─── ROS system definitions ───────────────────────────────────────────────────

/**
 * Pick of the 14 status-field keys from ReviewOfSystemsInput.
 * Used to make ROS_SYSTEMS key-typed without fully duplicating the interface.
 */
type ReviewOfSystemsInputSystems = Pick<
  ReviewOfSystemsInput,
  | "constitutional"
  | "eyes"
  | "ent"
  | "cardiovascular"
  | "respiratory"
  | "gastrointestinal"
  | "genitourinary"
  | "musculoskeletal"
  | "integumentary"
  | "neurological"
  | "psychiatric"
  | "endocrine"
  | "hematologic"
  | "allergicImmunologic"
>;

const ROS_SYSTEMS: {
  key: keyof ReviewOfSystemsInputSystems;
  label: string;
}[] = [
  { key: "constitutional", label: "Constitutional" },
  { key: "eyes", label: "Eyes" },
  { key: "ent", label: "ENT / Head" },
  { key: "cardiovascular", label: "Cardiovascular" },
  { key: "respiratory", label: "Respiratory" },
  { key: "gastrointestinal", label: "Gastrointestinal" },
  { key: "genitourinary", label: "Genitourinary" },
  { key: "musculoskeletal", label: "Musculoskeletal" },
  { key: "integumentary", label: "Integumentary / Skin" },
  { key: "neurological", label: "Neurological" },
  { key: "psychiatric", label: "Psychiatric" },
  { key: "endocrine", label: "Endocrine" },
  { key: "hematologic", label: "Hematologic / Lymphatic" },
  { key: "allergicImmunologic", label: "Allergic / Immunologic" },
];

// Derive the notes-field key from the system key (e.g. "constitutional" → "constitutionalNotes").
type RosNotesKey = keyof Pick<
  ReviewOfSystemsInput,
  | "constitutionalNotes"
  | "eyesNotes"
  | "entNotes"
  | "cardiovascularNotes"
  | "respiratoryNotes"
  | "gastrointestinalNotes"
  | "genitourinaryNotes"
  | "musculoskeletalNotes"
  | "integumentaryNotes"
  | "neurologicalNotes"
  | "psychiatricNotes"
  | "endocrineNotes"
  | "hematologicNotes"
  | "allergicImmunologicNotes"
>;

function statusKey(
  key: keyof ReviewOfSystemsInputSystems,
): keyof ReviewOfSystemsInput {
  return key as keyof ReviewOfSystemsInput;
}

function notesKey(
  key: keyof ReviewOfSystemsInputSystems,
): RosNotesKey {
  return (key + "Notes") as RosNotesKey;
}

// ─── ROS state initializers ───────────────────────────────────────────────────

/** All 28 ROS fields initialized to null. */
function emptyRosState(): ReviewOfSystemsInput {
  return {
    patientId: "",
    encounterId: "",
    constitutional: null,
    constitutionalNotes: null,
    eyes: null,
    eyesNotes: null,
    ent: null,
    entNotes: null,
    cardiovascular: null,
    cardiovascularNotes: null,
    respiratory: null,
    respiratoryNotes: null,
    gastrointestinal: null,
    gastrointestinalNotes: null,
    genitourinary: null,
    genitourinaryNotes: null,
    musculoskeletal: null,
    musculoskeletalNotes: null,
    integumentary: null,
    integumentaryNotes: null,
    neurological: null,
    neurologicalNotes: null,
    psychiatric: null,
    psychiatricNotes: null,
    endocrine: null,
    endocrineNotes: null,
    hematologic: null,
    hematologicNotes: null,
    allergicImmunologic: null,
    allergicImmunologicNotes: null,
  };
}

/**
 * Parse a persisted RosRecord's QuestionnaireResponse resource to restore
 * toggle states and notes for each system.
 *
 * The FHIR QuestionnaireResponse stores items with:
 *   item[].linkId — matches the system key (e.g. "constitutional")
 *   item[].answer[0].valueCoding.code — the RosStatus value
 *   item[].item[].linkId — "constitutionalNotes" etc.
 *   item[].item[].answer[0].valueString — the notes text
 *
 * Warns to console if an unrecognized linkId is encountered (schema drift
 * detection for future agents).
 */
function initRosFromRecord(record: RosRecord | null): ReviewOfSystemsInput {
  const base = emptyRosState();
  if (!record) return base;

  const resource = record.resource;
  const items = resource["item"] as Array<Record<string, unknown>> | undefined;
  if (!Array.isArray(items)) return base;

  const validSystemKeys = new Set(ROS_SYSTEMS.map((s) => s.key));

  for (const item of items) {
    const linkId = item["linkId"] as string | undefined;
    if (!linkId) continue;

    if (validSystemKeys.has(linkId as keyof ReviewOfSystemsInputSystems)) {
      const systemKey = linkId as keyof ReviewOfSystemsInputSystems;

      // Extract status from answer[0].valueCoding.code
      const answers = item["answer"] as
        | Array<Record<string, unknown>>
        | undefined;
      if (Array.isArray(answers) && answers.length > 0) {
        const valueCoding = answers[0]["valueCoding"] as
          | Record<string, unknown>
          | undefined;
        const code = valueCoding?.["code"];
        if (
          code === "positive" ||
          code === "negative" ||
          code === "not_reviewed"
        ) {
          (base as unknown as Record<string, unknown>)[statusKey(systemKey)] =
            code as RosStatus;
        }
      }

      // Extract notes from nested item (linkId = systemKey + "Notes")
      const nestedItems = item["item"] as
        | Array<Record<string, unknown>>
        | undefined;
      if (Array.isArray(nestedItems)) {
        for (const nested of nestedItems) {
          const nestedLinkId = nested["linkId"] as string | undefined;
          const expectedNotesKey = systemKey + "Notes";
          if (nestedLinkId === expectedNotesKey) {
            const nestedAnswers = nested["answer"] as
              | Array<Record<string, unknown>>
              | undefined;
            if (Array.isArray(nestedAnswers) && nestedAnswers.length > 0) {
              const valueString = nestedAnswers[0]["valueString"] as
                | string
                | undefined;
              if (typeof valueString === "string") {
                (base as unknown as Record<string, unknown>)[notesKey(systemKey)] =
                  valueString;
              }
            }
          }
        }
      }
    } else {
      console.warn(
        `[initRosFromRecord] Unrecognized linkId "${linkId}" in QuestionnaireResponse — possible schema drift`,
      );
    }
  }

  return base;
}

// ─── ROS Tab ──────────────────────────────────────────────────────────────────

interface RosTabProps {
  patientId: string;
  encounterId: string;
  role: string;
  rosRecord: RosRecord | null;
  saveRos: (input: ReviewOfSystemsInput) => Promise<void>;
  isFinalized: boolean;
}

function RosTab({
  patientId,
  encounterId,
  role: _role,
  rosRecord,
  saveRos,
  isFinalized,
}: RosTabProps) {
  // ── ROS form state (28 fields, all null initially) ─────────────────────
  const [rosState, setRosState] = useState<ReviewOfSystemsInput>(
    emptyRosState,
  );
  const [savingRos, setSavingRos] = useState(false);
  const [rosError, setRosError] = useState<string | null>(null);

  // ── Seed from rosRecord on load / encounter change ─────────────────────
  // Uses the rosRecord.id as a seeded-ID guard — only re-seeds when the
  // persisted record changes identity (new save or new encounter), not on
  // every reload, to avoid overwriting in-progress edits.
  const [seededRosId, setSeededRosId] = useState<string | null>(null);

  useEffect(() => {
    // If no record yet, only seed once (seededRosId "none" sentinel)
    if (!rosRecord) {
      if (seededRosId === "none") return;
      setRosState(emptyRosState());
      setSeededRosId("none");
      return;
    }
    if (seededRosId === rosRecord.id) return;
    setRosState(initRosFromRecord(rosRecord));
    setSeededRosId(rosRecord.id);
  }, [rosRecord, seededRosId]);

  // ── RBAC: isReadOnly ──────────────────────────────────────────────────
  // NurseMa can assist with ROS (CRU on ClinicalDocumentation).
  const isReadOnly = isFinalized;

  // ── System count summary ──────────────────────────────────────────────
  const reviewedCount = ROS_SYSTEMS.filter((sys) => {
    const status = rosState[statusKey(sys.key)] as RosStatus | null;
    return status === "positive" || status === "negative";
  }).length;

  // ── Update a single system's status ──────────────────────────────────
  const setSystemStatus = useCallback(
    (key: keyof ReviewOfSystemsInputSystems, value: RosStatus) => {
      setRosState((prev) => {
        const next = { ...prev };
        (next as Record<string, unknown>)[statusKey(key)] = value;
        // When changing away from "positive", clear notes
        if (value !== "positive") {
          (next as Record<string, unknown>)[notesKey(key)] = null;
        }
        return next;
      });
    },
    [],
  );

  // ── Update notes for a system ─────────────────────────────────────────
  const setSystemNotes = useCallback(
    (key: keyof ReviewOfSystemsInputSystems, value: string) => {
      setRosState((prev) => {
        const next = { ...prev };
        (next as Record<string, unknown>)[notesKey(key)] =
          value === "" ? null : value;
        return next;
      });
    },
    [],
  );

  // ── Save handler ──────────────────────────────────────────────────────
  const handleSave = useCallback(async () => {
    setSavingRos(true);
    setRosError(null);
    try {
      const input: ReviewOfSystemsInput = {
        ...rosState,
        patientId,
        encounterId,
      };
      await saveRos(input);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setRosError(msg);
      console.error("[EncounterWorkspace] saveRos failed:", msg);
      // rosState is NOT reset — user edits are preserved for retry
    } finally {
      setSavingRos(false);
    }
  }, [rosState, patientId, encounterId, saveRos]);

  // ── Radio button styles ───────────────────────────────────────────────
  function radioButtonCls(
    systemKey: keyof ReviewOfSystemsInputSystems,
    value: RosStatus,
  ): string {
    const current = rosState[statusKey(systemKey)] as RosStatus | null;
    const isActive = current === value;
    const base =
      "rounded border px-2 py-0.5 text-xs font-medium transition-colors focus:outline-none focus:ring-2 focus:ring-offset-1 disabled:cursor-not-allowed disabled:opacity-50";
    if (value === "positive") {
      return isActive
        ? `${base} bg-red-50 border-red-400 text-red-700 focus:ring-red-400`
        : `${base} border-gray-200 text-gray-400 hover:bg-red-50 hover:border-red-300 hover:text-red-600 focus:ring-red-300`;
    }
    if (value === "negative") {
      return isActive
        ? `${base} bg-green-50 border-green-400 text-green-700 focus:ring-green-400`
        : `${base} border-gray-200 text-gray-400 hover:bg-green-50 hover:border-green-300 hover:text-green-600 focus:ring-green-300`;
    }
    // not_reviewed
    return isActive
      ? `${base} bg-gray-50 border-gray-300 text-gray-500 focus:ring-gray-300`
      : `${base} border-gray-200 text-gray-300 hover:bg-gray-50 hover:border-gray-300 hover:text-gray-500 focus:ring-gray-300`;
  }

  return (
    <div className="space-y-4">
      {/* ── Finalized notice ──────────────────────────────────────────── */}
      {isFinalized && (
        <div className="flex items-center gap-2 rounded-md border border-green-200 bg-green-50 px-4 py-2 text-sm font-medium text-green-700">
          <span className="text-base">✓</span>
          <span>ROS locked — encounter finalized</span>
        </div>
      )}

      {/* ── Heading + count summary ───────────────────────────────────── */}
      <div className="flex items-baseline gap-3">
        <h2 className="text-base font-semibold text-gray-900">
          Review of Systems
        </h2>
        <span className="text-sm text-gray-500">
          {reviewedCount} of {ROS_SYSTEMS.length} systems reviewed
        </span>
      </div>

      {/* ── 14-system toggle grid ─────────────────────────────────────── */}
      <div className="divide-y divide-gray-100 rounded-md border border-gray-200 bg-white">
        {ROS_SYSTEMS.map((sys) => {
          const currentStatus = rosState[statusKey(sys.key)] as
            | RosStatus
            | null;
          const currentNotes = rosState[notesKey(sys.key)] as string | null;

          return (
            <div key={sys.key} className="px-4 py-2">
              {/* System row: label + three radio buttons */}
              <div className="flex items-center gap-3">
                {/* System label */}
                <span className="w-48 shrink-0 text-sm font-medium text-gray-700">
                  {sys.label}
                </span>

                {/* Positive */}
                <button
                  type="button"
                  disabled={isReadOnly}
                  className={radioButtonCls(sys.key, "positive")}
                  onClick={() => setSystemStatus(sys.key, "positive")}
                  aria-pressed={currentStatus === "positive"}
                >
                  Positive
                </button>

                {/* Negative */}
                <button
                  type="button"
                  disabled={isReadOnly}
                  className={radioButtonCls(sys.key, "negative")}
                  onClick={() => setSystemStatus(sys.key, "negative")}
                  aria-pressed={currentStatus === "negative"}
                >
                  Negative
                </button>

                {/* Not Reviewed */}
                <button
                  type="button"
                  disabled={isReadOnly}
                  className={radioButtonCls(sys.key, "not_reviewed")}
                  onClick={() => setSystemStatus(sys.key, "not_reviewed")}
                  aria-pressed={currentStatus === "not_reviewed"}
                >
                  Not Reviewed
                </button>
              </div>

              {/* Notes input — only visible when status is "positive" */}
              {currentStatus === "positive" && (
                <div className="mt-1.5 pl-[12.5rem]">
                  <input
                    type="text"
                    className="w-full rounded border border-gray-300 px-2.5 py-1 text-sm shadow-sm focus:border-red-400 focus:outline-none focus:ring-1 focus:ring-red-300"
                    placeholder="Notes…"
                    readOnly={isReadOnly}
                    value={currentNotes ?? ""}
                    onChange={(e) =>
                      setSystemNotes(sys.key, e.target.value)
                    }
                    aria-label={`${sys.label} notes`}
                  />
                </div>
              )}
            </div>
          );
        })}
      </div>

      {/* ── Error banner ─────────────────────────────────────────────── */}
      {rosError && (
        <p className="text-sm text-red-600">{rosError}</p>
      )}

      {/* ── Save button + count (hidden when finalized) ──────────────── */}
      {!isReadOnly && (
        <div className="flex items-center gap-4 pt-1">
          <button
            type="button"
            onClick={() => void handleSave()}
            disabled={savingRos}
            className="rounded-md bg-violet-600 px-4 py-2 text-sm font-medium text-white hover:bg-violet-700 focus:outline-none focus:ring-2 focus:ring-violet-500 focus:ring-offset-2 disabled:opacity-60"
          >
            {savingRos ? "Saving…" : "Save ROS"}
          </button>
          <span className="text-sm text-gray-500">
            {reviewedCount} of {ROS_SYSTEMS.length} systems reviewed
          </span>
        </div>
      )}
    </div>
  );
}

// ─── Physical Exam system definitions ────────────────────────────────────────

const PHYSICAL_EXAM_SYSTEMS: { key: keyof PhysicalExamFormState; label: string }[] = [
  { key: "general", label: "General" },
  { key: "heent", label: "HEENT" },
  { key: "neck", label: "Neck" },
  { key: "cardiovascular", label: "Cardiovascular" },
  { key: "pulmonary", label: "Pulmonary" },
  { key: "abdomen", label: "Abdomen" },
  { key: "extremities", label: "Extremities" },
  { key: "neurological", label: "Neurological" },
  { key: "skin", label: "Skin" },
  { key: "psychiatric", label: "Psychiatric" },
  { key: "musculoskeletal", label: "Musculoskeletal" },
  { key: "genitourinary", label: "Genitourinary" },
  { key: "rectal", label: "Rectal" },
];

// ─── Physical Exam form state ─────────────────────────────────────────────────

interface PhysicalExamFormState {
  general: string;
  heent: string;
  neck: string;
  cardiovascular: string;
  pulmonary: string;
  abdomen: string;
  extremities: string;
  neurological: string;
  skin: string;
  psychiatric: string;
  musculoskeletal: string;
  genitourinary: string;
  rectal: string;
  additionalNotes: string;
}

const EMPTY_PHYSICAL_EXAM_FORM: PhysicalExamFormState = {
  general: "",
  heent: "",
  neck: "",
  cardiovascular: "",
  pulmonary: "",
  abdomen: "",
  extremities: "",
  neurological: "",
  skin: "",
  psychiatric: "",
  musculoskeletal: "",
  genitourinary: "",
  rectal: "",
  additionalNotes: "",
};

/** Seed form state from a PhysicalExamRecord's FHIR ClinicalImpression resource.
 *  Reads finding[].itemCodeableConcept — code maps to field key, text is the value. */
function physicalExamRecordToForm(record: PhysicalExamRecord): PhysicalExamFormState {
  const res = record.resource;
  const findings = res["finding"];
  const form: PhysicalExamFormState = { ...EMPTY_PHYSICAL_EXAM_FORM };

  if (!Array.isArray(findings)) return form;

  for (const finding of findings as Array<Record<string, unknown>>) {
    const itemConcept = finding["itemCodeableConcept"] as
      | Record<string, unknown>
      | undefined;
    if (!itemConcept) continue;

    const coding = itemConcept["coding"] as
      | Array<Record<string, unknown>>
      | undefined;
    const code =
      Array.isArray(coding) && coding.length > 0
        ? (coding[0]["code"] as string | undefined)
        : undefined;
    if (!code) continue;

    const text = itemConcept["text"];
    if (typeof text !== "string") continue;

    if (Object.prototype.hasOwnProperty.call(EMPTY_PHYSICAL_EXAM_FORM, code)) {
      (form as unknown as Record<string, string>)[code] = text;
    }
  }

  return form;
}

// ─── Physical Exam Tab ────────────────────────────────────────────────────────

interface PhysicalExamTabProps {
  patientId: string;
  encounterId: string;
  physicalExamRecord: PhysicalExamRecord | null;
  isReadOnly: boolean;
  onSave: (input: PhysicalExamInput) => Promise<void>;
}

function PhysicalExamTab({
  patientId,
  encounterId,
  physicalExamRecord,
  isReadOnly,
  onSave,
}: PhysicalExamTabProps) {
  const [examForm, setExamForm] = useState<PhysicalExamFormState>(
    EMPTY_PHYSICAL_EXAM_FORM,
  );
  const [savingExam, setSavingExam] = useState(false);
  const [examError, setExamError] = useState<string | null>(null);

  // ── Seed from physicalExamRecord on load / encounter change ────────────
  // Uses physicalExamRecord.id as seeded-ID guard — only re-seeds when the
  // persisted record changes identity (new save or new encounter).
  const [seededPhysicalExamId, setSeededPhysicalExamId] = useState<
    string | null
  >(null);

  useEffect(() => {
    if (!physicalExamRecord) {
      if (seededPhysicalExamId === "none") return;
      setExamForm(EMPTY_PHYSICAL_EXAM_FORM);
      setSeededPhysicalExamId("none");
      return;
    }
    if (seededPhysicalExamId === physicalExamRecord.id) return;
    setExamForm(physicalExamRecordToForm(physicalExamRecord));
    setSeededPhysicalExamId(physicalExamRecord.id);
  }, [physicalExamRecord, seededPhysicalExamId]);

  // ── Field update helper ───────────────────────────────────────────────
  const setField = useCallback(
    (field: keyof PhysicalExamFormState, value: string) => {
      setExamForm((prev) => ({ ...prev, [field]: value }));
    },
    [],
  );

  // ── Save handler ──────────────────────────────────────────────────────
  const handleSave = useCallback(async () => {
    setSavingExam(true);
    setExamError(null);
    try {
      const input: PhysicalExamInput = {
        patientId,
        encounterId,
        general: examForm.general || null,
        heent: examForm.heent || null,
        neck: examForm.neck || null,
        cardiovascular: examForm.cardiovascular || null,
        pulmonary: examForm.pulmonary || null,
        abdomen: examForm.abdomen || null,
        extremities: examForm.extremities || null,
        neurological: examForm.neurological || null,
        skin: examForm.skin || null,
        psychiatric: examForm.psychiatric || null,
        musculoskeletal: examForm.musculoskeletal || null,
        genitourinary: examForm.genitourinary || null,
        rectal: examForm.rectal || null,
        additionalNotes: examForm.additionalNotes || null,
      };
      await onSave(input);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setExamError(msg);
      console.error("[EncounterWorkspace] savePhysicalExam failed:", msg);
      // examForm is NOT reset — user edits are preserved for retry
    } finally {
      setSavingExam(false);
    }
  }, [patientId, encounterId, examForm, onSave]);

  return (
    <div className="space-y-4">
      {/* ── Finalized notice ──────────────────────────────────────────── */}
      {isReadOnly && (
        <div className="flex items-center gap-2 rounded-md border border-green-200 bg-green-50 px-4 py-2 text-sm font-medium text-green-700">
          <span className="text-base">✓</span>
          <span>Physical Exam locked — encounter finalized</span>
        </div>
      )}

      {/* ── Heading ───────────────────────────────────────────────────── */}
      <h2 className="text-base font-semibold text-gray-900">
        Physical Examination
      </h2>

      {/* ── 13-system textarea grid ───────────────────────────────────── */}
      <div className="space-y-3">
        {PHYSICAL_EXAM_SYSTEMS.map(({ key, label }) => (
          <div key={key}>
            <label
              className={LABEL_CLS}
              htmlFor={`exam-${key}`}
            >
              {label}
            </label>
            <textarea
              id={`exam-${key}`}
              className={INPUT_CLS}
              rows={2}
              disabled={isReadOnly}
              value={examForm[key]}
              onChange={(e) => setField(key, e.target.value)}
              placeholder={
                isReadOnly ? "" : `${label} findings…`
              }
            />
          </div>
        ))}

        {/* ── Additional Notes ─────────────────────────────────────── */}
        <div>
          <label className={LABEL_CLS} htmlFor="exam-additionalNotes">
            Additional Notes
          </label>
          <textarea
            id="exam-additionalNotes"
            className={INPUT_CLS}
            rows={3}
            disabled={isReadOnly}
            value={examForm.additionalNotes}
            onChange={(e) => setField("additionalNotes", e.target.value)}
            placeholder={isReadOnly ? "" : "Additional exam observations…"}
          />
        </div>
      </div>

      {/* ── Error banner ──────────────────────────────────────────────── */}
      {examError && (
        <p className="text-sm text-red-600">{examError}</p>
      )}

      {/* ── Save button (hidden when finalized) ───────────────────────── */}
      {!isReadOnly && (
        <div className="pt-1">
          <button
            type="button"
            onClick={() => void handleSave()}
            disabled={savingExam}
            className="rounded-md bg-emerald-600 px-4 py-2 text-sm font-medium text-white hover:bg-emerald-700 focus:outline-none focus:ring-2 focus:ring-emerald-500 focus:ring-offset-2 disabled:opacity-60"
          >
            {savingExam ? "Saving…" : "Save Exam"}
          </button>
        </div>
      )}
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
  const { goBack } = useNav();
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
    latestVitals,
    saveVitals,
    rosRecord,
    saveRos,
    physicalExamRecord,
    savePhysicalExam,
  } = useEncounter({
    patientId,
    encounterId,
  });

  const [activeTab, setActiveTab] = useState<ActiveTab>("soap");

  // ── Loading state ──────────────────────────────────────────────────────
  if (loading) {
    return <LoadingSkeleton />;
  }

  // ── Error state ────────────────────────────────────────────────────────
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
            ← Back
          </button>
        </div>
      </div>
    );
  }

  // ── Extract display info from encounter resource ───────────────────────
  const resource = encounter?.resource ?? {};
  const rawType = extractEncounterTypeFromResource(resource);
  const encounterLabel = rawType
    ? formatEncounterType(rawType)
    : "Encounter";
  const encounterDate = extractEncounterDate(resource) ?? "";

  // ── Render workspace ───────────────────────────────────────────────────
  return (
    <div className="flex flex-col space-y-0 p-6">
      {/* ── Page header ─────────────────────────────────────────────────── */}
      <div className="mb-5 flex items-center gap-3">
        <button
          type="button"
          onClick={goBack}
          className="rounded-md p-1.5 text-gray-500 hover:bg-gray-100 hover:text-gray-700 focus:outline-none focus:ring-2 focus:ring-gray-400 focus:ring-offset-1"
          aria-label="Go back"
        >
          ← Back
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
              ✓ Finalized
            </span>
          )}
        </div>
      </div>

      {/* ── Tab bar ──────────────────────────────────────────────────────── */}
      <div className="flex gap-1 border-b border-gray-200 pb-0">
        {(
          [
            { id: "soap" as const, label: "SOAP" },
            { id: "vitals" as const, label: "Vitals" },
            { id: "ros" as const, label: "ROS" },
            { id: "exam" as const, label: "Exam" },
          ] satisfies { id: ActiveTab; label: string }[]
        ).map(({ id, label }) => (
          <button
            key={id}
            type="button"
            onClick={() => setActiveTab(id)}
            className={[
              "rounded-t-md px-5 py-2 text-sm font-medium focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:ring-offset-1",
              activeTab === id
                ? "border-b-2 border-indigo-600 text-indigo-700 bg-white"
                : "text-gray-500 hover:text-gray-700 hover:bg-gray-50",
            ].join(" ")}
            aria-selected={activeTab === id}
            role="tab"
          >
            {label}
          </button>
        ))}
      </div>

      {/* ── Tab body ──────────────────────────────────────────────────────── */}
      <div className="mt-4 rounded-b-lg rounded-tr-lg border border-gray-200 bg-white p-5">
        {activeTab === "soap" && (
          <SoapTab
            encounterId={encounterId}
            role={role}
            soapState={soapState}
            setSoapState={setSoapState}
            saveSoap={saveSoap}
            finalizeEncounter={finalizeEncounter}
            isFinalized={isFinalized}
            templates={templates}
          />
        )}
        {activeTab === "vitals" && (
          <VitalsTab
            patientId={patientId}
            encounterId={encounterId}
            role={role}
            latestVitals={latestVitals}
            saveVitals={saveVitals}
            isFinalized={isFinalized}
          />
        )}
        {activeTab === "ros" && (
          <RosTab
            patientId={patientId}
            encounterId={encounterId}
            role={role}
            rosRecord={rosRecord}
            saveRos={saveRos}
            isFinalized={isFinalized}
          />
        )}
        {activeTab === "exam" && (
          <PhysicalExamTab
            patientId={patientId}
            encounterId={encounterId}
            physicalExamRecord={physicalExamRecord}
            isReadOnly={isFinalized}
            onSave={savePhysicalExam}
          />
        )}
      </div>
    </div>
  );
}

// Export props type for use in child tab components (T02–T04)
export type { EncounterWorkspaceProps };

/**
 * useEncounter.ts — Data-fetching hook for encounter workspace data.
 *
 * Fetches the encounter record, vitals list, available templates, and the
 * ROS record in parallel. Mirrors the usePatient mounted-boolean /
 * refreshCounter / reload pattern exactly.
 *
 * Returns typed state plus a `reload` callback that re-triggers all fetches
 * by incrementing a refreshCounter — the counter (not `reload`) is in the
 * useEffect deps to avoid navigate-stability issues.
 *
 * T02 additions:
 *   - `soapState` / `setSoapState` — in-progress SOAP note edits
 *   - `saveSoap` — calls updateEncounter (status null) then reload
 *   - `finalizeEncounter` — calls updateEncounter (status "finished"), sets isFinalized
 *   - `isFinalized` — derived from encounter.resource.status on load + optimistic local flag
 *
 * T03 additions:
 *   - `latestVitals` — first element of the fetched vitals array (VitalsRecord | null)
 *   - `saveVitals` — calls recordVitals(input) then reload(); errors logged to console
 *
 * T04 additions:
 *   - `rosRecord` — the persisted RosRecord for this encounter (RosRecord | null)
 *   - `saveRos` — calls commands.saveRos(input) then reload(); errors thrown to caller
 *   NOTE: getRos requires BOTH encounterId AND patientId.
 *
 * T01 (S06) additions:
 *   - `physicalExamRecord` — the persisted PhysicalExamRecord for this encounter (PhysicalExamRecord | null)
 *   - `savePhysicalExam` — calls commands.savePhysicalExam(input) then reload(); errors thrown to caller
 *   NOTE: getPhysicalExam requires BOTH encounterId AND patientId.
 */
import { useState, useEffect, useCallback } from "react";
import { commands } from "../lib/tauri";
import { extractSoapSections } from "../lib/fhirExtract";
import type {
  EncounterRecord,
  VitalsRecord,
  TemplateRecord,
  SoapInput,
  VitalsInput,
  ReviewOfSystemsInput,
  RosRecord,
  PhysicalExamRecord,
  PhysicalExamInput,
} from "../types/documentation";

// ─── Return type ─────────────────────────────────────────────────────────────

export interface UseEncounterReturn {
  encounter: EncounterRecord | null;
  vitals: VitalsRecord[];
  templates: TemplateRecord[];
  loading: boolean;
  error: string | null;
  reload: () => void;
  // ── T02: SOAP editing surface ────────────────────────────────────
  /** Current (possibly uncommitted) SOAP note state. */
  soapState: SoapInput;
  /** Directly update soapState for controlled textarea bindings. */
  setSoapState: (s: SoapInput) => void;
  /**
   * Persist soapState via updateEncounter (status unchanged).
   * Throws on Rust error — callers catch and surface inline.
   * If the encounter is finalized, `amendmentReason` is required.
   */
  saveSoap: (soap: SoapInput, amendmentReason?: string | null) => Promise<void>;
  /**
   * Finalize the encounter: persist SOAP and set status → "finished".
   * Sets `isFinalized` to true immediately on success (optimistic update).
   * Throws on Rust error — callers catch and surface inline.
   */
  finalizeEncounter: (soap: SoapInput) => Promise<void>;
  /** True when encounter status is "finished" (loaded or just finalized). */
  isFinalized: boolean;
  /**
   * Re-open a finalized encounter for editing. Sets isFinalized to false
   * locally so the UI enables editing. The actual status is NOT changed in
   * the DB — the encounter remains "finished" and saves require amendment_reason.
   */
  reopenForAmendment: () => void;
  // ── T03: Vitals surface ───────────────────────────────────────────
  /**
   * Most-recent vitals record for this encounter (vitals[0] ?? null).
   * Includes server-computed `bmi` field — do NOT recompute client-side.
   */
  latestVitals: VitalsRecord | null;
  /**
   * Record a new vitals set, then reload to refresh latestVitals.
   * Throws on Rust error — callers catch and surface inline.
   */
  saveVitals: (input: VitalsInput) => Promise<void>;
  // ── T04: ROS surface ──────────────────────────────────────────────
  /**
   * The persisted Review of Systems record for this encounter, or null if
   * no ROS has been saved yet.
   * NOTE: getRos requires both encounterId and patientId — always use
   * commands.getRos(encounterId, patientId), never raw invoke.
   */
  rosRecord: RosRecord | null;
  /**
   * Save the full ROS input, then reload to refresh rosRecord.
   * Throws on Rust error — callers catch and surface inline.
   */
  saveRos: (input: ReviewOfSystemsInput) => Promise<void>;
  // ── S06/T01: Physical Exam surface ────────────────────────────────
  /**
   * The persisted Physical Exam record for this encounter, or null if no
   * exam has been saved yet.
   * NOTE: getPhysicalExam requires both encounterId and patientId — always
   * use commands.getPhysicalExam(encounterId, patientId).
   */
  physicalExamRecord: PhysicalExamRecord | null;
  /**
   * Save the full Physical Exam input, then reload to refresh physicalExamRecord.
   * Throws on Rust error — callers catch and surface inline.
   */
  savePhysicalExam: (input: PhysicalExamInput) => Promise<void>;
}

// ─── Empty SOAP helper ────────────────────────────────────────────────────────

const EMPTY_SOAP: SoapInput = {
  subjective: null,
  objective: null,
  assessment: null,
  plan: null,
};

// ─── Hook ─────────────────────────────────────────────────────────────────────

/**
 * Load an encounter record, its vitals, and available templates in parallel.
 *
 * @param patientId   - The patient UUID (needed for listVitals).
 * @param encounterId - The encounter UUID to load.
 */
export function useEncounter({
  patientId,
  encounterId,
}: {
  patientId: string;
  encounterId: string;
}): UseEncounterReturn {
  const [encounter, setEncounter] = useState<EncounterRecord | null>(null);
  const [vitals, setVitals] = useState<VitalsRecord[]>([]);
  const [templates, setTemplates] = useState<TemplateRecord[]>([]);
  const [rosRecord, setRosRecord] = useState<RosRecord | null>(null);
  const [physicalExamRecord, setPhysicalExamRecord] = useState<PhysicalExamRecord | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  // Incrementing this causes useEffect to re-run and re-fetch all data.
  const [refreshCounter, setRefreshCounter] = useState(0);

  // ── T02: SOAP state ───────────────────────────────────────────────────
  const [soapState, setSoapState] = useState<SoapInput>(EMPTY_SOAP);
  const [isFinalized, setIsFinalized] = useState(false);
  // Track which encounter ID was last used to seed soapState so we don't
  // overwrite in-progress edits when an unrelated reload fires.
  const [soapSeededForId, setSoapSeededForId] = useState<string | null>(null);

  // ── Main data fetch ───────────────────────────────────────────────────
  useEffect(() => {
    // Mounted guard: prevents state updates after the component has unmounted.
    let mounted = true;

    setLoading(true);
    setError(null);

    async function fetchAll() {
      try {
        const [encounterResult, vitalsResult, templatesResult, rosResult, physicalExamResult] =
          await Promise.all([
            commands.getEncounter(encounterId),
            commands.listVitals(patientId, encounterId),
            commands.listTemplates(null),
            // getRos requires BOTH encounterId AND patientId — do not omit patientId.
            commands.getRos(encounterId, patientId),
            // getPhysicalExam requires BOTH encounterId AND patientId — do not omit patientId.
            commands.getPhysicalExam(encounterId, patientId),
          ]);

        if (!mounted) return;
        setEncounter(encounterResult);
        setVitals(vitalsResult);
        setTemplates(templatesResult);
        setRosRecord(rosResult);
        setPhysicalExamRecord(physicalExamResult);
        console.log(`[useEncounter] rosRecord:`, rosResult);
      } catch (e) {
        if (!mounted) return;
        const msg = e instanceof Error ? e.message : String(e);
        console.error(
          `[useEncounter] fetchAll failed for ${encounterId}:`,
          msg,
        );
        setError(msg);
        // Reset data on error so stale data is not shown
        setEncounter(null);
        setVitals([]);
        setTemplates([]);
        setRosRecord(null);
        setPhysicalExamRecord(null);
      } finally {
        if (mounted) {
          setLoading(false);
        }
      }
    }

    fetchAll();

    return () => {
      mounted = false;
    };
    // Note: `reload` is intentionally excluded from deps — only patientId,
    // encounterId, and refreshCounter drive re-fetches to avoid
    // navigate-stability issues.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [patientId, encounterId, refreshCounter]);

  // ── T02: Seed soapState from encounter resource ───────────────────────
  // Runs whenever `encounter` changes. Guard: only re-seed when the encounter
  // ID changes (e.g. navigating to a different encounter), NOT on every
  // save/reload for the same encounter, to avoid overwriting in-progress edits.
  useEffect(() => {
    if (!encounter) return;
    // Re-seed on initial load (soapSeededForId === null) or when the
    // encounter ID changes (navigated to a different encounter).
    if (soapSeededForId === encounter.id) return;

    const sections = extractSoapSections(encounter.resource);
    setSoapState({
      subjective: sections.subjective,
      objective: sections.objective,
      assessment: sections.assessment,
      plan: sections.plan,
    });
    setSoapSeededForId(encounter.id);

    // Derive finalization status from resource
    const status = encounter.resource["status"];
    setIsFinalized(status === "finished");
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [encounter]);

  /**
   * Re-trigger all three fetches.
   * Stable reference — safe to use in event handlers without causing re-renders.
   */
  const reload = useCallback(() => {
    setRefreshCounter((n) => n + 1);
  }, []);

  // ── T02: saveSoap ─────────────────────────────────────────────────────
  const saveSoap = useCallback(
    async (soap: SoapInput, amendmentReason?: string | null): Promise<void> => {
      await commands.updateEncounter(encounterId, {
        soap,
        status: null,
        chiefComplaint: null,
        amendmentReason: amendmentReason ?? null,
      });
      // Reload to re-hydrate from the server (confirms write and refreshes versionId)
      reload();
    },
    [encounterId, reload],
  );

  // ── T02: finalizeEncounter ────────────────────────────────────────────
  const finalizeEncounter = useCallback(
    async (soap: SoapInput): Promise<void> => {
      await commands.updateEncounter(encounterId, {
        soap,
        status: "finished",
        chiefComplaint: null,
        amendmentReason: null,
      });
      // Optimistic: set finalized immediately so UI transitions without waiting
      // for reload to complete
      setIsFinalized(true);
      reload();
    },
    [encounterId, reload],
  );

  // ── Reopen for amendment ────────────────────────────────────────────
  const reopenForAmendment = useCallback(() => {
    setIsFinalized(false);
  }, []);

  // ── T03: latestVitals ─────────────────────────────────────────────────
  // Derived from the fetched vitals array — the first item is the most recent
  // record for this encounter. Includes server-computed `bmi` — do NOT
  // recompute BMI client-side.
  const latestVitals: VitalsRecord | null = vitals[0] ?? null;

  // ── T03: saveVitals ───────────────────────────────────────────────────
  const saveVitals = useCallback(
    async (input: VitalsInput): Promise<void> => {
      await commands.recordVitals(input);
      // Reload re-fetches listVitals → updates latestVitals → BMI display refreshes
      reload();
    },
    [reload],
  );

  // ── T04: saveRos ──────────────────────────────────────────────────────
  const saveRos = useCallback(
    async (input: ReviewOfSystemsInput): Promise<void> => {
      await commands.saveRos(input);
      // Reload re-fetches getRos → updates rosRecord → ROS toggle states restore
      reload();
    },
    [reload],
  );

  // ── S06/T01: savePhysicalExam ─────────────────────────────────────────
  const savePhysicalExam = useCallback(
    async (input: PhysicalExamInput): Promise<void> => {
      await commands.savePhysicalExam(input);
      // Reload re-fetches getPhysicalExam → updates physicalExamRecord → form re-seeds
      reload();
    },
    [encounterId, reload],
  );

  return {
    encounter,
    vitals,
    templates,
    loading,
    error,
    reload,
    soapState,
    setSoapState,
    saveSoap,
    finalizeEncounter,
    isFinalized,
    reopenForAmendment,
    latestVitals,
    saveVitals,
    rosRecord,
    saveRos,
    physicalExamRecord,
    savePhysicalExam,
  };
}

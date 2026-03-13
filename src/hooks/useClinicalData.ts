/**
 * useClinicalData.ts — Data-fetching hook for clinical patient data.
 *
 * Loads allergies, problems, medications, immunizations, and drug-allergy CDS
 * alerts in parallel for a given patient. Follows the usePatient.ts pattern:
 *   - mounted boolean guard to prevent stale state updates after unmount
 *   - refreshCounter state incremented by the reload callback
 *   - per-domain error isolation: one failing domain does not block the others
 *
 * Mutation callbacks (addAllergy, updateAllergy, …) are async, propagate errors
 * to callers, and call reload() on success to refresh all data.
 */
import { useState, useEffect, useCallback } from "react";
import { commands } from "../lib/tauri";
import type {
  AllergyRecord,
  AllergyInput,
  ProblemRecord,
  ProblemInput,
  MedicationRecord,
  MedicationInput,
  ImmunizationRecord,
  ImmunizationInput,
} from "../types/patient";
import type { DrugAllergyAlert } from "../types/documentation";

// ─── Return type ──────────────────────────────────────────────────────────────

export interface UseClinicalDataReturn {
  // Lists
  allergies: AllergyRecord[];
  problems: ProblemRecord[];
  medications: MedicationRecord[];
  immunizations: ImmunizationRecord[];
  alerts: DrugAllergyAlert[];

  // Overall skeleton-spinner loading flag
  loading: boolean;

  // Per-domain loading flags
  loadingAllergies: boolean;
  loadingProblems: boolean;
  loadingMedications: boolean;
  loadingImmunizations: boolean;
  loadingAlerts: boolean;

  // Per-domain error state (one domain can fail without affecting others)
  errorAllergies: string | null;
  errorProblems: string | null;
  errorMedications: string | null;
  errorImmunizations: string | null;
  errorAlerts: string | null;

  // Reload — re-triggers all fetches
  reload: () => void;

  // Allergy mutations
  addAllergy: (input: AllergyInput) => Promise<void>;
  updateAllergy: (id: string, input: AllergyInput) => Promise<void>;
  deleteAllergy: (id: string) => Promise<void>;

  // Problem mutations
  addProblem: (input: ProblemInput) => Promise<void>;
  updateProblem: (id: string, input: ProblemInput) => Promise<void>;

  // Medication mutations
  addMedication: (input: MedicationInput) => Promise<void>;
  updateMedication: (id: string, input: MedicationInput) => Promise<void>;

  // Immunization mutations
  addImmunization: (input: ImmunizationInput) => Promise<void>;
}

// ─── Hook ─────────────────────────────────────────────────────────────────────

/**
 * Load all clinical data for a patient in parallel, with per-domain error
 * isolation. One failing domain sets only that domain's error state; the
 * others continue loading.
 *
 * @param patientId - The patient UUID whose clinical data to load.
 */
export function useClinicalData(patientId: string): UseClinicalDataReturn {
  const [allergies, setAllergies] = useState<AllergyRecord[]>([]);
  const [problems, setProblems] = useState<ProblemRecord[]>([]);
  const [medications, setMedications] = useState<MedicationRecord[]>([]);
  const [immunizations, setImmunizations] = useState<ImmunizationRecord[]>([]);
  const [alerts, setAlerts] = useState<DrugAllergyAlert[]>([]);

  // Overall skeleton spinner
  const [loading, setLoading] = useState(true);

  // Per-domain loading
  const [loadingAllergies, setLoadingAllergies] = useState(true);
  const [loadingProblems, setLoadingProblems] = useState(true);
  const [loadingMedications, setLoadingMedications] = useState(true);
  const [loadingImmunizations, setLoadingImmunizations] = useState(true);
  const [loadingAlerts, setLoadingAlerts] = useState(true);

  // Per-domain error
  const [errorAllergies, setErrorAllergies] = useState<string | null>(null);
  const [errorProblems, setErrorProblems] = useState<string | null>(null);
  const [errorMedications, setErrorMedications] = useState<string | null>(null);
  const [errorImmunizations, setErrorImmunizations] = useState<string | null>(null);
  const [errorAlerts, setErrorAlerts] = useState<string | null>(null);

  // Incrementing this causes useEffect to re-run and re-fetch all data.
  const [refreshCounter, setRefreshCounter] = useState(0);

  useEffect(() => {
    // Mounted guard: prevents state updates after the component has unmounted.
    let mounted = true;

    setLoading(true);
    setLoadingAllergies(true);
    setLoadingProblems(true);
    setLoadingMedications(true);
    setLoadingImmunizations(true);
    setLoadingAlerts(true);
    setErrorAllergies(null);
    setErrorProblems(null);
    setErrorMedications(null);
    setErrorImmunizations(null);
    setErrorAlerts(null);

    async function fetchAll() {
      // Each domain runs independently so one failure does not block others.
      await Promise.all([
        // Allergies
        (async () => {
          try {
            const result = await commands.listAllergies(patientId);
            if (!mounted) return;
            setAllergies(result);
          } catch (e) {
            if (!mounted) return;
            const msg = e instanceof Error ? e.message : String(e);
            console.error(`[useClinicalData] listAllergies failed for ${patientId}:`, msg);
            setErrorAllergies(msg);
            setAllergies([]);
          } finally {
            if (mounted) setLoadingAllergies(false);
          }
        })(),

        // Problems
        (async () => {
          try {
            const result = await commands.listProblems(patientId);
            if (!mounted) return;
            setProblems(result);
          } catch (e) {
            if (!mounted) return;
            const msg = e instanceof Error ? e.message : String(e);
            console.error(`[useClinicalData] listProblems failed for ${patientId}:`, msg);
            setErrorProblems(msg);
            setProblems([]);
          } finally {
            if (mounted) setLoadingProblems(false);
          }
        })(),

        // Medications
        (async () => {
          try {
            const result = await commands.listMedications(patientId);
            if (!mounted) return;
            setMedications(result);
          } catch (e) {
            if (!mounted) return;
            const msg = e instanceof Error ? e.message : String(e);
            console.error(`[useClinicalData] listMedications failed for ${patientId}:`, msg);
            setErrorMedications(msg);
            setMedications([]);
          } finally {
            if (mounted) setLoadingMedications(false);
          }
        })(),

        // Immunizations
        (async () => {
          try {
            const result = await commands.listImmunizations(patientId);
            if (!mounted) return;
            setImmunizations(result);
          } catch (e) {
            if (!mounted) return;
            const msg = e instanceof Error ? e.message : String(e);
            console.error(`[useClinicalData] listImmunizations failed for ${patientId}:`, msg);
            setErrorImmunizations(msg);
            setImmunizations([]);
          } finally {
            if (mounted) setLoadingImmunizations(false);
          }
        })(),

        // Drug-allergy CDS alerts
        (async () => {
          try {
            const result = await commands.checkDrugAllergyAlerts(patientId);
            if (!mounted) return;
            setAlerts(result);
          } catch (e) {
            if (!mounted) return;
            const msg = e instanceof Error ? e.message : String(e);
            console.error(`[useClinicalData] checkDrugAllergyAlerts failed for ${patientId}:`, msg);
            setErrorAlerts(msg);
            setAlerts([]);
          } finally {
            if (mounted) setLoadingAlerts(false);
          }
        })(),
      ]);

      if (mounted) {
        setLoading(false);
      }
    }

    fetchAll();

    return () => {
      mounted = false;
    };
    // Note: `reload` is intentionally excluded from deps — only patientId and
    // refreshCounter drive re-fetches to avoid navigate-stability issues.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [patientId, refreshCounter]);

  /**
   * Re-trigger all five fetches.
   * Stable reference — safe to use in event handlers without causing re-renders.
   */
  const reload = useCallback(() => {
    setRefreshCounter((n) => n + 1);
  }, []);

  // ─── Allergy mutations ──────────────────────────────────────────────────────

  const addAllergy = useCallback(
    async (input: AllergyInput): Promise<void> => {
      await commands.addAllergy(input);
      reload();
    },
    [reload],
  );

  const updateAllergy = useCallback(
    async (id: string, input: AllergyInput): Promise<void> => {
      await commands.updateAllergy(id, input);
      reload();
    },
    [reload],
  );

  const deleteAllergy = useCallback(
    async (id: string): Promise<void> => {
      await commands.deleteAllergy(id, patientId);
      reload();
    },
    [patientId, reload],
  );

  // ─── Problem mutations ──────────────────────────────────────────────────────

  const addProblem = useCallback(
    async (input: ProblemInput): Promise<void> => {
      await commands.addProblem(input);
      reload();
    },
    [reload],
  );

  const updateProblem = useCallback(
    async (id: string, input: ProblemInput): Promise<void> => {
      await commands.updateProblem(id, input);
      reload();
    },
    [reload],
  );

  // ─── Medication mutations ───────────────────────────────────────────────────

  const addMedication = useCallback(
    async (input: MedicationInput): Promise<void> => {
      await commands.addMedication(input);
      reload();
    },
    [reload],
  );

  const updateMedication = useCallback(
    async (id: string, input: MedicationInput): Promise<void> => {
      await commands.updateMedication(id, input);
      reload();
    },
    [reload],
  );

  // ─── Immunization mutations ─────────────────────────────────────────────────

  const addImmunization = useCallback(
    async (input: ImmunizationInput): Promise<void> => {
      await commands.addImmunization(input);
      reload();
    },
    [reload],
  );

  return {
    allergies,
    problems,
    medications,
    immunizations,
    alerts,
    loading,
    loadingAllergies,
    loadingProblems,
    loadingMedications,
    loadingImmunizations,
    loadingAlerts,
    errorAllergies,
    errorProblems,
    errorMedications,
    errorImmunizations,
    errorAlerts,
    reload,
    addAllergy,
    updateAllergy,
    deleteAllergy,
    addProblem,
    updateProblem,
    addMedication,
    updateMedication,
    addImmunization,
  };
}

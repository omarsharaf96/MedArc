/**
 * usePatient.ts — Data-fetching hook for patient chart data.
 *
 * Fetches a patient record, care team, and related persons in parallel.
 * Uses the mounted-boolean guard pattern (from useAuth.ts) to prevent
 * stale state updates after component unmount.
 *
 * Returns typed state plus a `reload` callback that re-triggers all three
 * fetches by incrementing a refreshCounter — the counter (not `reload`) is
 * in the useEffect deps to avoid navigate-stability issues.
 */
import { useState, useEffect, useCallback } from "react";
import { commands } from "../lib/tauri";
import type {
  PatientRecord,
  CareTeamRecord,
  RelatedPersonRecord,
} from "../types/patient";

// ─── Return type ─────────────────────────────────────────────────────────────

export interface UsePatientReturn {
  patient: PatientRecord | null;
  careTeam: CareTeamRecord | null;
  relatedPersons: RelatedPersonRecord[];
  loading: boolean;
  error: string | null;
  reload: () => void;
}

// ─── Hook ─────────────────────────────────────────────────────────────────────

/**
 * Load a patient's full record, care team, and related persons in parallel.
 *
 * @param patientId - The patient UUID to load.
 */
export function usePatient(patientId: string): UsePatientReturn {
  const [patient, setPatient] = useState<PatientRecord | null>(null);
  const [careTeam, setCareTeam] = useState<CareTeamRecord | null>(null);
  const [relatedPersons, setRelatedPersons] = useState<RelatedPersonRecord[]>(
    [],
  );
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  // Incrementing this causes useEffect to re-run and re-fetch all data.
  const [refreshCounter, setRefreshCounter] = useState(0);

  useEffect(() => {
    // Mounted guard: prevents state updates after the component has unmounted.
    let mounted = true;

    setLoading(true);
    setError(null);

    async function fetchAll() {
      try {
        const [patientResult, careTeamResult, relatedPersonsResult] =
          await Promise.all([
            commands.getPatient(patientId),
            commands.getCareTeam(patientId),
            commands.listRelatedPersons(patientId),
          ]);

        if (!mounted) return;
        setPatient(patientResult);
        setCareTeam(careTeamResult);
        setRelatedPersons(relatedPersonsResult);
      } catch (e) {
        if (!mounted) return;
        const msg = e instanceof Error ? e.message : String(e);
        console.error(`[usePatient] fetchAll failed for ${patientId}:`, msg);
        setError(msg);
        // Reset data on error so stale data is not shown
        setPatient(null);
        setCareTeam(null);
        setRelatedPersons([]);
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
    // Note: `reload` is intentionally excluded from deps — only patientId and
    // refreshCounter drive re-fetches to avoid navigate-stability issues.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [patientId, refreshCounter]);

  /**
   * Re-trigger all three fetches.
   * Stable reference — safe to use in event handlers without causing re-renders.
   */
  const reload = useCallback(() => {
    setRefreshCounter((n) => n + 1);
  }, []);

  return { patient, careTeam, relatedPersons, loading, error, reload };
}

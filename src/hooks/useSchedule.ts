/**
 * useSchedule.ts — Data-fetching hook for scheduling, flow board, waitlist,
 * and recall data.
 *
 * Loads four independent domains in parallel for a given date range (and
 * optional provider filter). Follows the useClinicalData.ts pattern exactly:
 *   - mounted boolean guard to prevent stale state updates after unmount
 *   - refreshCounter state incremented by the reload callback
 *   - per-domain error isolation: one failing domain does not block the others
 *
 * Mutation callbacks are async, propagate errors to callers, and call reload()
 * on success to refresh all data.
 *
 * Provider-scoped queries: listWaitlist and listRecalls are called without
 * patient_id — they are provider-scoped list endpoints (see DECISIONS.md).
 */
import { useState, useEffect, useCallback } from "react";
import { commands } from "../lib/tauri";
import type {
  AppointmentRecord,
  AppointmentInput,
  FlowBoardEntry,
  UpdateFlowStatusInput,
  WaitlistRecord,
  WaitlistInput,
  RecallRecord,
  RecallInput,
} from "../types/scheduling";

// ─── Open slot helper (non-FHIR, scheduling-domain only) ─────────────────────

/** Displayable fields extracted from a raw open-slot response object. */
export interface OpenSlot {
  startTime: string | null;
  endTime: string | null;
  durationMinutes: number | null;
  available: boolean;
  apptType: string | null;
}

/**
 * Extract an OpenSlot from a raw backend response object.
 * Never throws. Uses strict typeof guards — no `as any`.
 */
export function extractOpenSlot(obj: Record<string, unknown>): OpenSlot {
  return {
    startTime:
      typeof obj["start_time"] === "string" ? obj["start_time"] : null,
    endTime:
      typeof obj["end_time"] === "string" ? obj["end_time"] : null,
    durationMinutes:
      typeof obj["duration_minutes"] === "number"
        ? obj["duration_minutes"]
        : null,
    available: obj["available"] === true,
    apptType:
      typeof obj["appt_type"] === "string" ? obj["appt_type"] : null,
  };
}

// ─── Return type ──────────────────────────────────────────────────────────────

export interface UseScheduleReturn {
  // Domain lists
  appointments: AppointmentRecord[];
  flowBoard: FlowBoardEntry[];
  waitlist: WaitlistRecord[];
  recalls: RecallRecord[];

  // Overall skeleton-spinner loading flag
  loading: boolean;

  // Per-domain loading flags
  loadingAppointments: boolean;
  loadingFlowBoard: boolean;
  loadingWaitlist: boolean;
  loadingRecalls: boolean;

  // Per-domain error state (one domain failing does not affect others)
  errorAppointments: string | null;
  errorFlowBoard: string | null;
  errorWaitlist: string | null;
  errorRecalls: string | null;

  // Reload — re-triggers all domain fetches
  reload: () => void;

  // reloadFlowBoard — alias for reload (MVP: full reload avoids stale-mounted issue)
  reloadFlowBoard: () => void;

  // Appointment mutations
  createAppointment: (input: AppointmentInput) => Promise<AppointmentRecord[]>;
  cancelAppointment: (id: string, reason: string | null) => Promise<AppointmentRecord>;
  updateFlowStatus: (input: UpdateFlowStatusInput) => Promise<FlowBoardEntry>;

  // Waitlist mutations
  addToWaitlist: (input: WaitlistInput) => Promise<WaitlistRecord>;
  dischargeWaitlist: (id: string, reason: string | null) => Promise<void>;

  // Recall mutations
  createRecall: (input: RecallInput) => Promise<RecallRecord>;
  completeRecall: (id: string, notes: string | null) => Promise<void>;

  // Open slot search (returns mapped OpenSlot[])
  searchOpenSlots: (
    startDate: string,
    endDate: string,
    providerId: string,
    apptType?: string | null,
    durationMinutes?: number | null,
  ) => Promise<OpenSlot[]>;
}

// ─── Hook ─────────────────────────────────────────────────────────────────────

/**
 * Load all scheduling data in parallel for a date range, with per-domain error
 * isolation. One failing domain sets only that domain's error; the others
 * continue loading.
 *
 * @param startDate  - ISO YYYY-MM-DD date — range start for listAppointments.
 * @param endDate    - ISO YYYY-MM-DD date — range end for listAppointments.
 * @param providerId - Optional provider UUID filter (null = all providers).
 */
export function useSchedule(
  startDate: string,
  endDate: string,
  providerId?: string | null,
): UseScheduleReturn {
  const [appointments, setAppointments] = useState<AppointmentRecord[]>([]);
  const [flowBoard, setFlowBoard] = useState<FlowBoardEntry[]>([]);
  const [waitlist, setWaitlist] = useState<WaitlistRecord[]>([]);
  const [recalls, setRecalls] = useState<RecallRecord[]>([]);

  // Overall skeleton spinner
  const [loading, setLoading] = useState(true);

  // Per-domain loading
  const [loadingAppointments, setLoadingAppointments] = useState(true);
  const [loadingFlowBoard, setLoadingFlowBoard] = useState(true);
  const [loadingWaitlist, setLoadingWaitlist] = useState(true);
  const [loadingRecalls, setLoadingRecalls] = useState(true);

  // Per-domain error
  const [errorAppointments, setErrorAppointments] = useState<string | null>(null);
  const [errorFlowBoard, setErrorFlowBoard] = useState<string | null>(null);
  const [errorWaitlist, setErrorWaitlist] = useState<string | null>(null);
  const [errorRecalls, setErrorRecalls] = useState<string | null>(null);

  // Incrementing this causes useEffect to re-run and re-fetch all data.
  const [refreshCounter, setRefreshCounter] = useState(0);

  useEffect(() => {
    // Mounted guard: prevents state updates after the component has unmounted.
    let mounted = true;

    setLoading(true);
    setLoadingAppointments(true);
    setLoadingFlowBoard(true);
    setLoadingWaitlist(true);
    setLoadingRecalls(true);
    setErrorAppointments(null);
    setErrorFlowBoard(null);
    setErrorWaitlist(null);
    setErrorRecalls(null);

    // Compute today as YYYY-MM-DD using "sv" locale (Swedish ISO 8601, no TZ shift)
    const todayDateString = new Date().toLocaleDateString("sv");

    async function fetchAll() {
      // Each domain runs independently so one failure does not block others.
      await Promise.all([
        // Appointments — date-range scoped, optionally provider-filtered
        (async () => {
          try {
            const result = await commands.listAppointments(
              startDate,
              endDate,
              null,
              providerId ?? null,
            );
            if (!mounted) return;
            setAppointments(result);
          } catch (e) {
            if (!mounted) return;
            const msg = e instanceof Error ? e.message : String(e);
            console.error("[useSchedule] listAppointments failed:", msg);
            setErrorAppointments(msg);
            setAppointments([]);
          } finally {
            if (mounted) setLoadingAppointments(false);
          }
        })(),

        // Flow board — today only, optionally provider-filtered
        (async () => {
          try {
            const result = await commands.getFlowBoard(
              todayDateString,
              providerId ?? null,
            );
            if (!mounted) return;
            setFlowBoard(result);
          } catch (e) {
            if (!mounted) return;
            const msg = e instanceof Error ? e.message : String(e);
            console.error("[useSchedule] getFlowBoard failed:", msg);
            setErrorFlowBoard(msg);
            setFlowBoard([]);
          } finally {
            if (mounted) setLoadingFlowBoard(false);
          }
        })(),

        // Waitlist — provider-scoped (no patient_id filter per DECISIONS.md)
        (async () => {
          try {
            const result = await commands.listWaitlist(
              providerId ?? null,
              null,
              null,
            );
            if (!mounted) return;
            setWaitlist(result);
          } catch (e) {
            if (!mounted) return;
            const msg = e instanceof Error ? e.message : String(e);
            console.error("[useSchedule] listWaitlist failed:", msg);
            setErrorWaitlist(msg);
            setWaitlist([]);
          } finally {
            if (mounted) setLoadingWaitlist(false);
          }
        })(),

        // Recalls — provider-scoped (no patient_id filter per DECISIONS.md)
        (async () => {
          try {
            const result = await commands.listRecalls(
              providerId ?? null,
              null,
              null,
            );
            if (!mounted) return;
            setRecalls(result);
          } catch (e) {
            if (!mounted) return;
            const msg = e instanceof Error ? e.message : String(e);
            console.error("[useSchedule] listRecalls failed:", msg);
            setErrorRecalls(msg);
            setRecalls([]);
          } finally {
            if (mounted) setLoadingRecalls(false);
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
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [startDate, endDate, providerId, refreshCounter]);

  /**
   * Re-trigger all four domain fetches.
   * Stable reference — safe to use in event handlers without causing re-renders.
   */
  const reload = useCallback(() => {
    setRefreshCounter((n) => n + 1);
  }, []);

  /**
   * reloadFlowBoard — aliased to reload for MVP.
   * A full reload avoids the stale-mounted closure issue that a per-domain
   * reload would introduce. Acceptable for MVP cadence.
   */
  const reloadFlowBoard = reload;

  // ─── Appointment mutations ──────────────────────────────────────────────────

  const createAppointment = useCallback(
    async (input: AppointmentInput): Promise<AppointmentRecord[]> => {
      const result = await commands.createAppointment(input);
      reload();
      return result;
    },
    [reload],
  );

  const cancelAppointment = useCallback(
    async (id: string, reason: string | null): Promise<AppointmentRecord> => {
      const result = await commands.cancelAppointment(id, reason ?? null);
      reload();
      return result;
    },
    [reload],
  );

  const updateFlowStatus = useCallback(
    async (input: UpdateFlowStatusInput): Promise<FlowBoardEntry> => {
      const result = await commands.updateFlowStatus(input);
      reloadFlowBoard();
      return result;
    },
    // reloadFlowBoard is a stable alias for reload (same reference)
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [reload],
  );

  // ─── Waitlist mutations ─────────────────────────────────────────────────────

  const addToWaitlist = useCallback(
    async (input: WaitlistInput): Promise<WaitlistRecord> => {
      const result = await commands.addToWaitlist(input);
      reload();
      return result;
    },
    [reload],
  );

  const dischargeWaitlist = useCallback(
    async (id: string, reason: string | null): Promise<void> => {
      await commands.dischargeWaitlist(id, reason ?? null);
      reload();
    },
    [reload],
  );

  // ─── Recall mutations ───────────────────────────────────────────────────────

  const createRecall = useCallback(
    async (input: RecallInput): Promise<RecallRecord> => {
      const result = await commands.createRecall(input);
      reload();
      return result;
    },
    [reload],
  );

  const completeRecall = useCallback(
    async (id: string, notes: string | null): Promise<void> => {
      await commands.completeRecall(id, notes ?? null);
      reload();
    },
    [reload],
  );

  // ─── Open slot search ───────────────────────────────────────────────────────

  const searchOpenSlots = useCallback(
    async (
      slotStartDate: string,
      slotEndDate: string,
      slotProviderId: string,
      apptType?: string | null,
      durationMinutes?: number | null,
    ): Promise<OpenSlot[]> => {
      const raw = await commands.searchOpenSlots(
        slotStartDate,
        slotEndDate,
        slotProviderId,
        apptType ?? null,
        durationMinutes ?? null,
      );
      return raw.map(extractOpenSlot);
    },
    [],
  );

  return {
    appointments,
    flowBoard,
    waitlist,
    recalls,
    loading,
    loadingAppointments,
    loadingFlowBoard,
    loadingWaitlist,
    loadingRecalls,
    errorAppointments,
    errorFlowBoard,
    errorWaitlist,
    errorRecalls,
    reload,
    reloadFlowBoard,
    createAppointment,
    cancelAppointment,
    updateFlowStatus,
    addToWaitlist,
    dischargeWaitlist,
    createRecall,
    completeRecall,
    searchOpenSlots,
  };
}

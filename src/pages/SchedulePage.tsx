/**
 * SchedulePage.tsx — Full scheduling page replacing the S03 stub.
 *
 * Owns:
 *   - RBAC: derives role + userId from useAuth; gates canWrite
 *   - View state: day | week toggle, currentDate, prev/next navigation
 *   - Date range: getDateRange(currentDate, view) → { start, end } strings
 *   - Data: calls useSchedule(startDate, endDate, userId)
 *   - Modal state: createOpen (new appointment), cancelTarget (cancel an appt)
 *   - Layout: page header → CalendarPage → FlowBoardPage → WaitlistPanel → RecallPanel
 *
 * CalendarPage, FlowBoardPage, WaitlistPanel, and RecallPanel receive
 * pre-fetched data as props — they are pure presentational components.
 *
 * Observability:
 *   - errorAppointments and errorFlowBoard rendered as inline red banners
 *   - React DevTools: SchedulePage → view, currentDate, createOpen, cancelTarget
 *   - Runtime: grep "[useSchedule]" in browser console for per-domain errors
 */

import { useState } from "react";
import { useAuth } from "../hooks/useAuth";
import { useSchedule } from "../hooks/useSchedule";
import { CalendarPage } from "../components/scheduling/CalendarPage";
import { FlowBoardPage } from "../components/scheduling/FlowBoardPage";
import { WaitlistPanel } from "../components/scheduling/WaitlistPanel";
import { RecallPanel } from "../components/scheduling/RecallPanel";
import { AppointmentFormModal } from "../components/scheduling/AppointmentFormModal";
import { extractAppointmentDisplay } from "../lib/fhirExtract";
import type { AppointmentRecord, UpdateFlowStatusInput, WaitlistInput, RecallInput } from "../types/scheduling";

// ─── RBAC helper ──────────────────────────────────────────────────────────────

/**
 * Returns true for roles that can create/edit/cancel appointments and update
 * flow statuses. BillingStaff and unknown roles are read-only.
 */
function canWrite(role: string): boolean {
  return (
    role === "FrontDesk" ||
    role === "NurseMa" ||
    role === "Provider" ||
    role === "SystemAdmin"
  );
}

// ─── Date range helpers ───────────────────────────────────────────────────────

function getDateRange(
  date: Date,
  view: "day" | "week",
): { start: string; end: string } {
  if (view === "day") {
    const start = date.toLocaleDateString("sv");
    const next = new Date(date);
    next.setDate(date.getDate() + 1);
    const end = next.toLocaleDateString("sv");
    return { start, end };
  }

  const dayOfWeek = date.getDay();
  const sunday = new Date(date);
  sunday.setDate(date.getDate() - dayOfWeek);
  const nextSunday = new Date(sunday);
  nextSunday.setDate(sunday.getDate() + 7);

  return {
    start: sunday.toLocaleDateString("sv"),
    end: nextSunday.toLocaleDateString("sv"),
  };
}

// ─── Component ────────────────────────────────────────────────────────────────

export function SchedulePage() {
  const { user } = useAuth();

  const role = user?.role ?? "";
  const userId = user?.id ?? "";
  const writeAllowed = canWrite(role);

  // View state
  const [view, setView] = useState<"day" | "week">("week");
  const [currentDate, setCurrentDate] = useState<Date>(() => new Date());

  // Modal state (T03)
  const [createOpen, setCreateOpen] = useState(false);
  const [cancelTarget, setCancelTarget] = useState<AppointmentRecord | null>(null);

  // Date range strings derived from currentDate + view
  const { start: startDate, end: endDate } = getDateRange(currentDate, view);

  // Data layer — wired to the current date range and current user
  const {
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
    updateFlowStatus,
    createAppointment,
    cancelAppointment,
    addToWaitlist,
    dischargeWaitlist,
    createRecall,
    completeRecall,
  } = useSchedule(startDate, endDate, userId || null);

  // Navigation
  function handlePrev() {
    setCurrentDate((d) => {
      const next = new Date(d);
      next.setDate(d.getDate() - (view === "day" ? 1 : 7));
      return next;
    });
  }

  function handleNext() {
    setCurrentDate((d) => {
      const next = new Date(d);
      next.setDate(d.getDate() + (view === "day" ? 1 : 7));
      return next;
    });
  }

  // Appointment card click — CalendarPage manages popover internally
  function handleCardClick(_appt: AppointmentRecord) {
    // CalendarPage owns popover state; SchedulePage listens for cancel action
  }

  // Cancel target set from CalendarPage's InfoPopover
  function handleCancelAppointment(appt: AppointmentRecord) {
    setCancelTarget(appt);
  }

  // Flow status mutation
  async function handleUpdateStatus(input: UpdateFlowStatusInput): Promise<void> {
    await updateFlowStatus(input);
  }

  // Waitlist mutations
  async function handleAddToWaitlist(input: WaitlistInput): Promise<void> {
    await addToWaitlist(input);
  }

  async function handleDischargeWaitlist(id: string, reason: string | null): Promise<void> {
    await dischargeWaitlist(id, reason);
  }

  // Recall mutations
  async function handleCreateRecall(input: RecallInput): Promise<void> {
    await createRecall(input);
  }

  async function handleCompleteRecall(id: string, notes: string | null): Promise<void> {
    // completeRecall returns void — do not read return value
    await completeRecall(id, notes);
  }

  // Build appointment summary string for cancel modal
  function buildAppointmentSummary(appt: AppointmentRecord): string {
    const display = extractAppointmentDisplay(appt.resource);
    const type = display.apptTypeDisplay ?? display.apptType ?? "Appointment";
    const time = display.start
      ? display.start.replace("T", " at ").slice(0, 19)
      : "";
    return time ? `${type} — ${time}` : type;
  }

  // ── Skeleton spinner ────────────────────────────────────────────────────────
  if (loading) {
    return (
      <div className="p-6 space-y-4" aria-label="Loading schedule">
        <div className="h-8 w-48 rounded-lg bg-gray-200 animate-pulse" />
        <div className="h-96 rounded-xl bg-gray-100 animate-pulse" />
        <div className="h-32 rounded-xl bg-gray-100 animate-pulse" />
      </div>
    );
  }

  return (
    <div className="p-6 space-y-6">
      {/* Page header */}
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-bold text-gray-900">Schedule</h1>
        {/* New Appointment button — write-gated */}
        {writeAllowed && (
          <button
            type="button"
            onClick={() => setCreateOpen(true)}
            className="rounded-md bg-blue-600 px-4 py-2 text-sm font-medium text-white hover:bg-blue-700 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:ring-offset-2"
          >
            + New Appointment
          </button>
        )}
      </div>

      {/* ── Calendar section ─────────────────────────────────────────────── */}
      <section>
        {/* Appointments error banner */}
        {errorAppointments && (
          <div
            role="alert"
            className="mb-3 rounded-lg border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-800"
          >
            <span className="font-medium">Could not load appointments: </span>
            {errorAppointments}
          </div>
        )}

        {loadingAppointments ? (
          <div className="h-96 rounded-xl bg-gray-100 animate-pulse" />
        ) : (
          <CalendarPage
            appointments={appointments}
            view={view}
            currentDate={currentDate}
            onPrev={handlePrev}
            onNext={handleNext}
            onViewChange={setView}
            onCardClick={handleCardClick}
            onCancelAppointment={handleCancelAppointment}
            canWrite={writeAllowed}
          />
        )}
      </section>

      {/* ── Flow Board section ────────────────────────────────────────────── */}
      <section>
        <h2 className="text-lg font-semibold text-gray-800 mb-3">
          Today's Flow Board
        </h2>

        {/* Flow board error banner */}
        {errorFlowBoard && (
          <div
            role="alert"
            className="mb-3 rounded-lg border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-800"
          >
            <span className="font-medium">Could not load flow board: </span>
            {errorFlowBoard}
          </div>
        )}

        <FlowBoardPage
          flowBoard={flowBoard}
          loading={loadingFlowBoard}
          error={null /* banner above handles it */}
          canWrite={writeAllowed}
          onUpdateStatus={handleUpdateStatus}
        />
      </section>

      {/* ── Waitlist section ──────────────────────────────────────────────── */}
      <section>
        {/* Waitlist error banner */}
        {errorWaitlist && (
          <div
            role="alert"
            className="mb-3 rounded-lg border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-800"
          >
            <span className="font-medium">Could not load waitlist: </span>
            {errorWaitlist}
          </div>
        )}

        <WaitlistPanel
          waitlist={waitlist}
          loading={loadingWaitlist}
          error={null /* banner above handles it */}
          canWrite={writeAllowed}
          onAdd={handleAddToWaitlist}
          onDischarge={handleDischargeWaitlist}
        />
      </section>

      {/* ── Recall Board section ──────────────────────────────────────────── */}
      <section>
        {/* Recall error banner */}
        {errorRecalls && (
          <div
            role="alert"
            className="mb-3 rounded-lg border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-800"
          >
            <span className="font-medium">Could not load recall board: </span>
            {errorRecalls}
          </div>
        )}

        <RecallPanel
          recalls={recalls}
          loading={loadingRecalls}
          error={null /* banner above handles it */}
          canWrite={writeAllowed}
          onCreateRecall={handleCreateRecall}
          onCompleteRecall={handleCompleteRecall}
        />
      </section>

      {/* ── AppointmentFormModal — create mode ───────────────────────────── */}
      {createOpen && (
        <AppointmentFormModal
          mode="create"
          onSubmitCreate={async (input) => {
            const result = await createAppointment(input);
            reload();
            return result;
          }}
          onSubmitCancel={cancelAppointment}
          onClose={() => setCreateOpen(false)}
          canWrite={writeAllowed}
        />
      )}

      {/* ── AppointmentFormModal — cancel mode ───────────────────────────── */}
      {cancelTarget && (
        <AppointmentFormModal
          mode="cancel"
          appointmentId={cancelTarget.id}
          appointmentSummary={buildAppointmentSummary(cancelTarget)}
          onSubmitCreate={createAppointment}
          onSubmitCancel={async (id, reason) => {
            const result = await cancelAppointment(id, reason ?? null);
            reload();
            setCancelTarget(null);
            return result;
          }}
          onClose={() => setCancelTarget(null)}
          canWrite={writeAllowed}
        />
      )}
    </div>
  );
}

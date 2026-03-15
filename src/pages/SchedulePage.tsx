/**
 * SchedulePage.tsx — Full scheduling page.
 *
 * Owns:
 *   - RBAC: derives role + userId from useAuth; gates canWrite
 *   - View state: day | week toggle, currentDate, prev/next navigation
 *   - Date range: getDateRange(currentDate, view) → { start, end } strings
 *   - Data: calls useSchedule(startDate, endDate)
 *   - Modal state: createOpen (new appointment), cancelTarget (cancel an appt)
 *   - Provider data: loads providers + provider appointment types for the form
 *   - Click-to-schedule: clicking empty calendar slots opens pre-filled modal
 *   - Privacy mode: toggle patient names to initials
 *   - Layout: page header → CalendarPage → FlowBoardPage → WaitlistPanel → RecallPanel
 */

import { useState, useEffect, useCallback, useMemo } from "react";
import { useAuth } from "../hooks/useAuth";
import { useSchedule } from "../hooks/useSchedule";
import { usePatientNames, toInitials } from "../hooks/usePatientNames";
import { CalendarPage } from "../components/scheduling/CalendarPage";
import { FlowBoardPage } from "../components/scheduling/FlowBoardPage";
import { WaitlistPanel } from "../components/scheduling/WaitlistPanel";
import { RecallPanel } from "../components/scheduling/RecallPanel";
import { AppointmentFormModal } from "../components/scheduling/AppointmentFormModal";
import type { ProviderOption } from "../components/scheduling/AppointmentFormModal";
import { extractAppointmentDisplay } from "../lib/fhirExtract";
import { commands } from "../lib/tauri";
import type { AppointmentRecord, UpdateAppointmentInput, UpdateFlowStatusInput, WaitlistInput, RecallInput } from "../types/scheduling";
import type { ReminderLog, ReminderResult } from "../types/reminders";

// ─── RBAC helper ──────────────────────────────────────────────────────────────

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
  const writeAllowed = canWrite(role);

  // View state
  const [view, setView] = useState<"day" | "week">("week");
  const [currentDate, setCurrentDate] = useState<Date>(() => new Date());

  // Modal state
  const [createOpen, setCreateOpen] = useState(false);
  const [cancelTarget, setCancelTarget] = useState<AppointmentRecord | null>(null);
  const [editTarget, setEditTarget] = useState<AppointmentRecord | null>(null);

  // Pre-fill start time for click-to-schedule
  const [prefillStartTime, setPrefillStartTime] = useState<string | undefined>(undefined);

  // Privacy mode — converts patient names to initials
  const [privacyMode, setPrivacyMode] = useState(false);

  // Provider data for appointment form
  const [providers, setProviders] = useState<ProviderOption[]>([]);
  const [providerApptTypes, setProviderApptTypes] = useState<Record<string, string[]>>({});

  // Reminder panel state
  const [reminderLogs, setReminderLogs] = useState<Record<string, ReminderLog[]>>({});
  const [reminderSending, setReminderSending] = useState<Record<string, boolean>>({});
  const [reminderResults, setReminderResults] = useState<Record<string, ReminderResult[] | null>>({});
  const [noShowSending, setNoShowSending] = useState<Record<string, boolean>>({});
  const [expandedReminder, setExpandedReminder] = useState<string | null>(null);

  // Date range strings derived from currentDate + view
  const { start: startDate, end: endDate } = getDateRange(currentDate, view);

  // Data layer — all providers, RBAC gates access
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
  } = useSchedule(startDate, endDate, null);

  // Load providers and provider appointment types on mount
  useEffect(() => {
    let mounted = true;

    async function loadProviderData() {
      try {
        const [providerList, typesMap] = await Promise.all([
          commands.listProviders(),
          commands.getProviderAppointmentTypes(),
        ]);
        if (!mounted) return;
        setProviders(providerList);
        setProviderApptTypes(typesMap.types);
      } catch {
        // Silently ignore — providers will be empty and the form will
        // fall back to generic appointment types
      }
    }

    loadProviderData();
    return () => { mounted = false; };
  }, []);

  // Collect all patient IDs from appointments, flow board, waitlist, recalls
  const allPatientIds = useMemo(() => {
    const ids = new Set<string>();
    for (const a of appointments) ids.add(a.patientId);
    for (const f of flowBoard) ids.add(f.patientId);
    for (const w of waitlist) ids.add(w.patientId);
    for (const r of recalls) ids.add(r.patientId);
    return [...ids];
  }, [appointments, flowBoard, waitlist, recalls]);

  // Resolve patient IDs → display names
  const patientNames = usePatientNames(allPatientIds);

  /** Get display text for a patient: full name or initials depending on privacy mode. */
  function patientLabel(patientId: string): string {
    const name = patientNames.get(patientId);
    if (!name) return privacyMode ? "**" : patientId;
    return privacyMode ? toInitials(name) : name;
  }

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

  // Today button handler
  function handleToday() {
    setCurrentDate(new Date());
  }

  // Click-to-schedule handler
  function handleSlotClick(date: string, hour: number, minute: number) {
    if (!writeAllowed) return;
    const hh = hour.toString().padStart(2, "0");
    const mm = minute.toString().padStart(2, "0");
    setPrefillStartTime(`${date}T${hh}:${mm}`);
    setCreateOpen(true);
  }

  // Appointment card click — CalendarPage manages popover internally
  function handleCardClick(_appt: AppointmentRecord) {
    // CalendarPage owns popover state; SchedulePage listens for cancel action
  }

  // Cancel target set from CalendarPage's InfoPopover
  function handleCancelAppointment(appt: AppointmentRecord) {
    setCancelTarget(appt);
  }

  // Edit target set from CalendarPage's InfoPopover
  function handleEditAppointment(appt: AppointmentRecord) {
    setEditTarget(appt);
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

  // ── Reminder handlers ────────────────────────────────────────────────────────

  const handleLoadReminderLog = useCallback(async (appointmentId: string) => {
    if (expandedReminder === appointmentId) {
      setExpandedReminder(null);
      return;
    }
    setExpandedReminder(appointmentId);
    try {
      const logs = await commands.listReminderLog(null, null, null);
      const filtered = logs.filter((l) => l.appointmentId === appointmentId);
      setReminderLogs((prev) => ({ ...prev, [appointmentId]: filtered }));
    } catch {
      // silently ignore
    }
  }, [expandedReminder]);

  const handleSendReminder = useCallback(async (appointmentId: string, type: string) => {
    setReminderSending((prev) => ({ ...prev, [appointmentId]: true }));
    setReminderResults((prev) => ({ ...prev, [appointmentId]: null }));
    try {
      const results = await commands.sendReminder(appointmentId, type);
      setReminderResults((prev) => ({ ...prev, [appointmentId]: results }));
      // Refresh the log
      const logs = await commands.listReminderLog(null, null, null);
      const filtered = logs.filter((l) => l.appointmentId === appointmentId);
      setReminderLogs((prev) => ({ ...prev, [appointmentId]: filtered }));
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setReminderResults((prev) => ({
        ...prev,
        [appointmentId]: [{ reminderId: "", status: "failed", channel: "sms", recipient: "", externalId: null, errorMessage: msg }],
      }));
    } finally {
      setReminderSending((prev) => ({ ...prev, [appointmentId]: false }));
    }
  }, []);

  const handleSendNoShow = useCallback(async (appointmentId: string) => {
    setNoShowSending((prev) => ({ ...prev, [appointmentId]: true }));
    try {
      await commands.sendNoShowFollowup(appointmentId);
    } catch {
      // silently ignore
    } finally {
      setNoShowSending((prev) => ({ ...prev, [appointmentId]: false }));
    }
  }, []);

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
        <div className="flex items-center gap-3">
          {/* Privacy toggle — hides patient names, shows initials */}
          <button
            type="button"
            onClick={() => setPrivacyMode((p) => !p)}
            title={privacyMode ? "Show patient names" : "Hide patient names (initials only)"}
            className={`rounded-md border px-3 py-2 text-sm font-medium focus:outline-none focus:ring-2 focus:ring-blue-500 focus:ring-offset-2 transition-colors ${
              privacyMode
                ? "border-blue-600 bg-blue-600 text-white"
                : "border-gray-300 bg-white text-gray-700 hover:bg-gray-50"
            }`}
          >
            {privacyMode ? "ABC" : "A.B."}
          </button>
          {/* New Appointment button — write-gated */}
          {writeAllowed && (
            <button
              type="button"
              onClick={() => {
                setPrefillStartTime(undefined);
                setCreateOpen(true);
              }}
              className="rounded-md bg-blue-600 px-4 py-2 text-sm font-medium text-white hover:bg-blue-700 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:ring-offset-2"
            >
              + New Appointment
            </button>
          )}
        </div>
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
            onEditAppointment={handleEditAppointment}
            onCancelAppointment={handleCancelAppointment}
            canWrite={writeAllowed}
            onSlotClick={writeAllowed ? handleSlotClick : undefined}
            onToday={handleToday}
            patientLabel={patientLabel}
          />
        )}
      </section>

      {/* ── Flow Board section ────────────────────────────────────────────── */}
      <section>
        <h2 className="text-lg font-semibold text-gray-800 mb-3">
          Today's Flow Board
        </h2>

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
          error={null}
          canWrite={writeAllowed}
          onUpdateStatus={handleUpdateStatus}
          patientLabel={patientLabel}
        />
      </section>

      {/* ── Waitlist section ──────────────────────────────────────────────── */}
      <section>
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
          error={null}
          canWrite={writeAllowed}
          onAdd={handleAddToWaitlist}
          onDischarge={handleDischargeWaitlist}
          patientLabel={patientLabel}
        />
      </section>

      {/* ── Recall Board section ──────────────────────────────────────────── */}
      <section>
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
          error={null}
          canWrite={writeAllowed}
          onCreateRecall={handleCreateRecall}
          onCompleteRecall={handleCompleteRecall}
          patientLabel={patientLabel}
        />
      </section>

      {/* ── Reminders Panel ──────────────────────────────────────────────── */}
      {appointments.length > 0 && (
        <section>
          <h2 className="text-lg font-semibold text-gray-800 mb-3">Appointment Reminders</h2>
          <div className="rounded-xl border border-gray-200 bg-white shadow-sm overflow-hidden">
            <table className="min-w-full divide-y divide-gray-100">
              <thead className="bg-gray-50">
                <tr>
                  <th className="px-4 py-3 text-left text-xs font-semibold text-gray-500 uppercase">Patient</th>
                  <th className="px-4 py-3 text-left text-xs font-semibold text-gray-500 uppercase">Time</th>
                  <th className="px-4 py-3 text-left text-xs font-semibold text-gray-500 uppercase">Status</th>
                  <th className="px-4 py-3 text-left text-xs font-semibold text-gray-500 uppercase">Reminders Sent</th>
                  {writeAllowed && (
                    <th className="px-4 py-3 text-right text-xs font-semibold text-gray-500 uppercase">Actions</th>
                  )}
                </tr>
              </thead>
              <tbody className="divide-y divide-gray-100">
                {appointments.map((appt) => {
                  const display = extractAppointmentDisplay(appt.resource);
                  const isSending = reminderSending[appt.id];
                  const isNoShowSending = noShowSending[appt.id];
                  const isExpanded = expandedReminder === appt.id;
                  const logs = reminderLogs[appt.id] ?? [];
                  const results = reminderResults[appt.id];
                  const statusStr = display.status ?? "booked";
                  const isPastNoShow =
                    statusStr === "noshow" ||
                    (display.start && new Date(display.start) < new Date() && statusStr !== "fulfilled" && statusStr !== "cancelled");
                  return (
                    <>
                      <tr key={appt.id} className="hover:bg-gray-50 transition-colors">
                        <td className="px-4 py-3 text-sm text-gray-900">
                          {patientLabel(appt.patientId)}
                        </td>
                        <td className="px-4 py-3 text-sm text-gray-600">
                          {display.start
                            ? new Date(display.start).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })
                            : "—"}
                        </td>
                        <td className="px-4 py-3">
                          <span className={[
                            "inline-flex items-center rounded-full px-2 py-0.5 text-xs font-medium",
                            statusStr === "fulfilled" ? "bg-green-100 text-green-700" :
                            statusStr === "cancelled" ? "bg-red-100 text-red-600" :
                            statusStr === "noshow" || isPastNoShow ? "bg-orange-100 text-orange-700" :
                            "bg-blue-100 text-blue-700",
                          ].join(" ")}>
                            {statusStr}
                          </span>
                        </td>
                        <td className="px-4 py-3">
                          <button
                            type="button"
                            onClick={() => handleLoadReminderLog(appt.id)}
                            className="text-xs text-blue-600 hover:underline"
                          >
                            {isExpanded ? "Hide" : "View log"}
                          </button>
                        </td>
                        {writeAllowed && (
                          <td className="px-4 py-3 text-right">
                            <div className="flex justify-end gap-2 flex-wrap">
                              <button
                                type="button"
                                disabled={isSending || statusStr === "cancelled"}
                                onClick={() => handleSendReminder(appt.id, "24hr")}
                                className="rounded-md border border-gray-300 bg-white px-2 py-1 text-xs font-medium text-gray-700 hover:bg-gray-50 disabled:cursor-not-allowed disabled:opacity-40 transition-colors"
                              >
                                {isSending ? "Sending..." : "Send Reminder"}
                              </button>
                              {(isPastNoShow) && (
                                <button
                                  type="button"
                                  disabled={isNoShowSending}
                                  onClick={() => handleSendNoShow(appt.id)}
                                  className="rounded-md border border-orange-200 bg-orange-50 px-2 py-1 text-xs font-medium text-orange-700 hover:bg-orange-100 disabled:cursor-not-allowed disabled:opacity-40 transition-colors"
                                >
                                  {isNoShowSending ? "Sending..." : "No-Show Follow-up"}
                                </button>
                              )}
                            </div>
                            {results && results.length > 0 && (
                              <div className="mt-1 text-xs">
                                {results.map((r, i) => (
                                  <span key={i} className={`ml-1 ${r.status === "sent" ? "text-green-600" : "text-red-600"}`}>
                                    {r.channel}: {r.status}
                                  </span>
                                ))}
                              </div>
                            )}
                          </td>
                        )}
                      </tr>
                      {isExpanded && (
                        <tr key={`${appt.id}-log`}>
                          <td colSpan={writeAllowed ? 5 : 4} className="px-4 py-3 bg-gray-50">
                            {logs.length === 0 ? (
                              <span className="text-xs text-gray-400">No reminders sent yet.</span>
                            ) : (
                              <div className="space-y-1">
                                {logs.map((log) => (
                                  <div key={log.reminderId} className="flex items-center gap-3 text-xs text-gray-600">
                                    <span className={`rounded-full px-2 py-0.5 font-medium ${log.status === "sent" || log.status === "delivered" ? "bg-green-100 text-green-700" : log.status === "failed" ? "bg-red-100 text-red-600" : "bg-gray-100 text-gray-500"}`}>
                                      {log.status}
                                    </span>
                                    <span className="font-medium">{log.reminderType}</span>
                                    <span>{log.channel}</span>
                                    <span className="text-gray-400">{log.recipient}</span>
                                    {log.sentAt && (
                                      <span className="text-gray-400">
                                        {new Date(log.sentAt).toLocaleString()}
                                      </span>
                                    )}
                                    {log.errorMessage && (
                                      <span className="text-red-500">{log.errorMessage}</span>
                                    )}
                                  </div>
                                ))}
                              </div>
                            )}
                          </td>
                        </tr>
                      )}
                    </>
                  );
                })}
              </tbody>
            </table>
          </div>
        </section>
      )}

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
          onClose={() => {
            setCreateOpen(false);
            setPrefillStartTime(undefined);
          }}
          canWrite={writeAllowed}
          providers={providers}
          providerAppointmentTypes={providerApptTypes}
          initialStartTime={prefillStartTime}
        />
      )}

      {/* ── AppointmentFormModal — edit mode ─────────────────────────────── */}
      {editTarget && (() => {
        const d = extractAppointmentDisplay(editTarget.resource);
        return (
          <AppointmentFormModal
            mode="edit"
            appointmentId={editTarget.id}
            editData={{
              patientId: editTarget.patientId,
              providerId: editTarget.providerId,
              startTime: d.start ?? "",
              durationMinutes: d.durationMin ?? 30,
              apptType: d.apptType ?? "",
              color: d.color ?? null,
              reason: d.reason ?? null,
              notes: d.notes ?? null,
            }}
            onSubmitCreate={createAppointment}
            onSubmitEdit={async (id: string, input: UpdateAppointmentInput) => {
              const result = await commands.updateAppointment(id, input);
              reload();
              setEditTarget(null);
              return result;
            }}
            onSubmitCancel={cancelAppointment}
            onClose={() => setEditTarget(null)}
            canWrite={writeAllowed}
            providers={providers}
            providerAppointmentTypes={providerApptTypes}
          />
        );
      })()}

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

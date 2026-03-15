/**
 * CalendarPage.tsx — Day/week CSS Grid calendar with read-only appointment cards.
 *
 * Layout:
 *   - CalendarHeader: date/range title + prev/next navigation + Today + day/week toggle
 *   - CalendarGrid: time gutter (08:00–18:00) + appointment columns with
 *     absolutely-positioned appointment cards
 *
 * Features:
 *   - Click-to-schedule: clicking an empty time slot fires onSlotClick
 *   - Today button: quick navigation to current date
 *   - Privacy mode: patientLabel prop converts names to initials
 *   - Auto-color: appointment cards colored by type (no manual picker)
 *   - Drag-to-reschedule: mouse-event drag (not HTML5 drag) fires onReschedule on drop
 *   - Open Encounter: InfoPopover finds/creates encounter linked to appointment
 *
 * Positioning math:
 *   - Time gutter rows: 60px per hour, 08:00 = 480 minutes offset from midnight
 *   - Card top:   ((startMinuteOfDay - 480) / 60) * 60  px  (= startMin - 480)
 *   - Card height: durationMin * 1 px  (1 minute = 1px, 30-min = 30px min height)
 *
 * Date helpers co-located here; all use string splits + vanilla Date, no "Z"
 * suffix (avoids timezone-shift bug).
 */

import { useState, useRef, useEffect, useCallback } from "react";
import type { AppointmentRecord } from "../../types/scheduling";
import { extractAppointmentDisplay } from "../../lib/fhirExtract";
import { useNav } from "../../contexts/RouterContext";
import { useAuth } from "../../hooks/useAuth";
import { commands } from "../../lib/tauri";

// ─── Date / time helpers ──────────────────────────────────────────────────────

/**
 * Format an ISO date string as a readable date, e.g. "Mon Apr 6, 2026".
 * Uses "T12:00:00" anchor to avoid midnight timezone rollover on date-only strings.
 */
export function formatDisplayDate(dateStr: string): string {
  const datePart = dateStr.split("T")[0];
  const d = new Date(datePart + "T12:00:00");
  return d.toLocaleDateString("en-US", {
    weekday: "short",
    month: "short",
    day: "numeric",
    year: "numeric",
  });
}

/**
 * Format an ISO 8601 datetime string (no trailing Z) as "9:00 AM" / "2:30 PM".
 * Splits on "T", parses hours and minutes as integers — no Date constructor used
 * so there is no timezone shift regardless of local offset.
 */
export function formatTime(datetimeStr: string): string {
  const timePart = datetimeStr.split("T")[1];
  if (!timePart) return datetimeStr;
  const [hStr, mStr] = timePart.split(":");
  const h = parseInt(hStr, 10);
  const m = parseInt(mStr, 10);
  if (isNaN(h) || isNaN(m)) return datetimeStr;
  const suffix = h >= 12 ? "PM" : "AM";
  const displayH = h % 12 === 0 ? 12 : h % 12;
  const displayM = m.toString().padStart(2, "0");
  return `${displayH}:${displayM} ${suffix}`;
}

/**
 * Return the minute-of-day for an ISO 8601 datetime string.
 * Splits on "T", parses hours and minutes — no Date constructor.
 */
export function startMinuteOfDay(datetimeStr: string): number {
  const timePart = datetimeStr.split("T")[1];
  if (!timePart) return 0;
  const [hStr, mStr] = timePart.split(":");
  const h = parseInt(hStr, 10);
  const m = parseInt(mStr, 10);
  if (isNaN(h) || isNaN(m)) return 0;
  return h * 60 + m;
}

// ─── Auto-color mapping by appointment type ──────────────────────────────────

/** Automatic color assignment per appointment type (matches AppointmentFormModal). */
const APPT_TYPE_COLOR_MAP: Record<string, string> = {
  initial_pt_evaluation: "#22C55E", // green
  pt_treatment: "#3B82F6",         // blue
};

// ─── Calendar layout constants ────────────────────────────────────────────────

/** Visible hour range — 08:00 through 18:00 (10 hours × 60px = 600px). */
const START_HOUR = 8;
const END_HOUR = 18;
const HOUR_HEIGHT_PX = 60;

/** Minute offset from midnight for the first visible row. */
const GRID_START_MIN = START_HOUR * 60; // 480

/** Day labels for the week-view header (Sun–Sat). */
const DAY_LABELS = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

// ─── Props ────────────────────────────────────────────────────────────────────

export interface CalendarPageProps {
  appointments: AppointmentRecord[];
  view: "day" | "week";
  currentDate: Date;
  onPrev: () => void;
  onNext: () => void;
  onViewChange: (v: "day" | "week") => void;
  onCardClick: (appt: AppointmentRecord) => void;
  onEditAppointment: (appt: AppointmentRecord) => void;
  onCancelAppointment: (appt: AppointmentRecord) => void;
  canWrite: boolean;
  /** Called when an empty time slot on the calendar grid is clicked. */
  onSlotClick?: (date: string, hour: number, minute: number) => void;
  /** Called when the Today button is clicked — navigate to today's date. */
  onToday?: () => void;
  /** Resolves a patientId to a display label (name or initials in privacy mode). */
  patientLabel?: (patientId: string) => string;
  /** Called when an appointment card is dragged to a new time slot. */
  onReschedule?: (appointmentId: string, newStartTime: string) => void;
  /** Called when the user marks an appointment as Attended / Canceled / No Show. */
  onUpdateStatus?: (appointmentId: string, status: string) => Promise<void>;
  /** Called when the user confirms deletion of an appointment. */
  onDeleteAppointment?: (appointmentId: string) => Promise<void>;
}

// ─── CalendarHeader ───────────────────────────────────────────────────────────

interface CalendarHeaderProps {
  view: "day" | "week";
  currentDate: Date;
  onPrev: () => void;
  onNext: () => void;
  onViewChange: (v: "day" | "week") => void;
  onToday?: () => void;
}

function CalendarHeader({
  view,
  currentDate,
  onPrev,
  onNext,
  onViewChange,
  onToday,
}: CalendarHeaderProps) {
  // Build the title: "Mon Apr 6, 2026" (day) or "Mon Apr 6 – Sun Apr 12, 2026" (week)
  let title: string;
  if (view === "day") {
    title = formatDisplayDate(currentDate.toLocaleDateString("sv"));
  } else {
    // Week: Sun–Sat of currentDate's week
    const dayOfWeek = currentDate.getDay(); // 0 = Sun
    const sunday = new Date(currentDate);
    sunday.setDate(currentDate.getDate() - dayOfWeek);
    const saturday = new Date(sunday);
    saturday.setDate(sunday.getDate() + 6);
    const startStr = sunday.toLocaleDateString("sv");
    const endStr = saturday.toLocaleDateString("sv");
    const startFormatted = formatDisplayDate(startStr);
    const endFormatted = formatDisplayDate(endStr);
    // Trim year from start label if same year to save space
    const startYear = sunday.getFullYear();
    const endYear = saturday.getFullYear();
    if (startYear === endYear) {
      // Remove ", YYYY" from the start label
      const startShort = startFormatted.replace(/, \d{4}$/, "");
      title = `${startShort} – ${endFormatted}`;
    } else {
      title = `${startFormatted} – ${endFormatted}`;
    }
  }

  return (
    <div className="flex items-center justify-between mb-4">
      {/* Navigation */}
      <div className="flex items-center gap-2">
        <button
          onClick={onPrev}
          className="rounded-md border border-gray-300 bg-white px-2.5 py-1.5 text-sm text-gray-700 hover:bg-gray-50 focus:outline-none focus:ring-2 focus:ring-blue-500"
          aria-label="Previous"
        >
          ‹
        </button>
        {onToday && (
          <button
            onClick={onToday}
            className="rounded-md border border-gray-300 bg-white px-3 py-1.5 text-sm font-medium text-gray-700 hover:bg-gray-50 focus:outline-none focus:ring-2 focus:ring-blue-500"
            aria-label="Go to today"
          >
            Today
          </button>
        )}
        <h2 className="text-base font-semibold text-gray-900 min-w-[240px] text-center">
          {title}
        </h2>
        <button
          onClick={onNext}
          className="rounded-md border border-gray-300 bg-white px-2.5 py-1.5 text-sm text-gray-700 hover:bg-gray-50 focus:outline-none focus:ring-2 focus:ring-blue-500"
          aria-label="Next"
        >
          ›
        </button>
      </div>

      {/* View toggle */}
      <div className="flex rounded-md border border-gray-300 overflow-hidden">
        <button
          onClick={() => onViewChange("day")}
          className={`px-3 py-1.5 text-sm font-medium focus:outline-none focus:ring-2 focus:ring-inset focus:ring-blue-500 ${
            view === "day"
              ? "bg-blue-600 text-white"
              : "bg-white text-gray-700 hover:bg-gray-50"
          }`}
        >
          Day
        </button>
        <button
          onClick={() => onViewChange("week")}
          className={`border-l border-gray-300 px-3 py-1.5 text-sm font-medium focus:outline-none focus:ring-2 focus:ring-inset focus:ring-blue-500 ${
            view === "week"
              ? "bg-blue-600 text-white"
              : "bg-white text-gray-700 hover:bg-gray-50"
          }`}
        >
          Week
        </button>
      </div>
    </div>
  );
}

// ─── Drag state for mouse-event-based drag-to-reschedule ──────────────────────

interface DragState {
  appointmentId: string;
  /** The Y offset within the card where the mouse grabbed (px from card top). */
  grabOffsetY: number;
  /** Current mouse Y relative to the column grid container (px). */
  currentY: number;
  /** Original card top (px). */
  originalTopPx: number;
  /** Original card height (px). */
  heightPx: number;
  /** Background color of the card. */
  bgColor: string;
  /** Display label for the card. */
  label: string;
  /** Time label for the card. */
  timeLabel: string;
  /** The date string of the column being dragged in. */
  columnDateStr: string;
}

// ─── AppointmentCard ──────────────────────────────────────────────────────────

interface AppointmentCardProps {
  appt: AppointmentRecord;
  onClick: () => void;
  patientLabel?: (patientId: string) => string;
  draggable?: boolean;
  onDragStart?: (
    apptId: string,
    grabOffsetY: number,
    topPx: number,
    heightPx: number,
    bgColor: string,
    label: string,
    timeLabel: string,
  ) => void;
  isDragging?: boolean;
}

function AppointmentCard({
  appt,
  onClick,
  patientLabel,
  draggable,
  onDragStart,
  isDragging,
}: AppointmentCardProps) {
  const display = extractAppointmentDisplay(appt.resource);

  const startStr = display.start ?? "";
  const startMin = startStr ? startMinuteOfDay(startStr) : GRID_START_MIN;
  const duration = display.durationMin ?? 30;

  // Clamp: don't render above START_HOUR or below END_HOUR
  const clampedStart = Math.max(startMin, GRID_START_MIN);
  const clampedEnd = Math.min(startMin + duration, END_HOUR * 60);
  const visibleDuration = Math.max(clampedEnd - clampedStart, 15); // 15px minimum

  const topPx = (clampedStart - GRID_START_MIN) * (HOUR_HEIGHT_PX / 60);
  const heightPx = visibleDuration * (HOUR_HEIGHT_PX / 60);

  // Use type-based auto-color; fall back to stored color, then default blue
  const bgColor =
    (display.apptType && APPT_TYPE_COLOR_MAP[display.apptType]) ??
    display.color ??
    "#3B82F6";
  const label = display.apptTypeDisplay ?? display.apptType ?? "Appointment";
  const timeLabel = startStr ? formatTime(startStr) : "";

  function handleMouseDown(e: React.MouseEvent) {
    if (!draggable || !onDragStart) return;
    e.preventDefault();

    const startX = e.clientX;
    const startY = e.clientY;
    const cardRect = e.currentTarget.getBoundingClientRect();
    const grabOffsetY = e.clientY - cardRect.top;

    function onMouseMove(moveEvent: MouseEvent) {
      const dx = moveEvent.clientX - startX;
      const dy = moveEvent.clientY - startY;
      if (Math.abs(dx) > 5 || Math.abs(dy) > 5) {
        // Exceeded threshold — initiate drag
        window.removeEventListener("mousemove", onMouseMove);
        window.removeEventListener("mouseup", onMouseUp);
        onDragStart!(appt.id, grabOffsetY, topPx, heightPx, bgColor, label, timeLabel);
      }
    }

    function onMouseUp() {
      // Did not exceed threshold — treat as click
      window.removeEventListener("mousemove", onMouseMove);
      window.removeEventListener("mouseup", onMouseUp);
      onClick();
    }

    window.addEventListener("mousemove", onMouseMove);
    window.addEventListener("mouseup", onMouseUp);
  }

  return (
    <div
      onMouseDown={handleMouseDown}
      title={`${label} at ${timeLabel}`}
      role="button"
      tabIndex={0}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onClick();
        }
      }}
      className={`absolute left-0.5 right-0.5 overflow-hidden rounded text-left text-xs font-medium text-white shadow-sm hover:brightness-90 focus:outline-none focus:ring-2 focus:ring-blue-400 focus:ring-offset-1 transition-colors select-none${draggable ? " cursor-grab active:cursor-grabbing" : ""}${isDragging ? " opacity-30" : ""}`}
      style={{
        top: `${topPx}px`,
        height: `${heightPx}px`,
        backgroundColor: bgColor,
      }}
    >
      <div className="px-1 py-0.5 truncate leading-tight">
        <span className="block truncate">
          {patientLabel ? patientLabel(appt.patientId) : label}
        </span>
        <span className="block truncate opacity-90">{timeLabel}</span>
      </div>
    </div>
  );
}

// ─── CalendarGrid ─────────────────────────────────────────────────────────────

interface CalendarGridProps {
  appointments: AppointmentRecord[];
  view: "day" | "week";
  currentDate: Date;
  onCardClick: (appt: AppointmentRecord) => void;
  onSlotClick?: (date: string, hour: number, minute: number) => void;
  patientLabel?: (patientId: string) => string;
  onReschedule?: (appointmentId: string, newStartTime: string) => void;
  canWrite?: boolean;
}

function CalendarGrid({
  appointments,
  view,
  currentDate,
  onCardClick,
  onSlotClick,
  patientLabel,
  onReschedule,
  canWrite,
}: CalendarGridProps) {
  const hours = Array.from(
    { length: END_HOUR - START_HOUR },
    (_, i) => START_HOUR + i,
  );
  const totalHeightPx = hours.length * HOUR_HEIGHT_PX;

  // Build column date strings for week view
  const columns: string[] = [];
  if (view === "day") {
    columns.push(currentDate.toLocaleDateString("sv"));
  } else {
    const dayOfWeek = currentDate.getDay();
    const sunday = new Date(currentDate);
    sunday.setDate(currentDate.getDate() - dayOfWeek);
    for (let i = 0; i < 7; i++) {
      const d = new Date(sunday);
      d.setDate(sunday.getDate() + i);
      columns.push(d.toLocaleDateString("sv"));
    }
  }

  // Map appointments to their date string (YYYY-MM-DD from start field)
  function appointmentsForDate(dateStr: string): AppointmentRecord[] {
    return appointments.filter((appt) => {
      const display = extractAppointmentDisplay(appt.resource);
      if (!display.start) return false;
      return display.start.split("T")[0] === dateStr;
    });
  }

  // ── Mouse-event-based drag state ────────────────────────────────────────
  const [dragState, setDragState] = useState<DragState | null>(null);
  const columnRefsMap = useRef<Map<string, HTMLDivElement>>(new Map());
  // Track whether we just finished a drag to prevent the click from firing
  const justFinishedDrag = useRef(false);

  const setColumnRef = useCallback(
    (dateStr: string, el: HTMLDivElement | null) => {
      if (el) {
        columnRefsMap.current.set(dateStr, el);
      } else {
        columnRefsMap.current.delete(dateStr);
      }
    },
    [],
  );

  // Handle drag start from a card
  const handleCardDragStart = useCallback(
    (
      dateStr: string,
      apptId: string,
      grabOffsetY: number,
      topPx: number,
      heightPx: number,
      bgColor: string,
      label: string,
      timeLabel: string,
    ) => {
      if (!onReschedule || !canWrite) return;
      setDragState({
        appointmentId: apptId,
        grabOffsetY,
        currentY: topPx,
        originalTopPx: topPx,
        heightPx,
        bgColor,
        label,
        timeLabel,
        columnDateStr: dateStr,
      });
    },
    [onReschedule, canWrite],
  );

  // Global mouse move/up handlers for drag
  useEffect(() => {
    if (!dragState) return;

    function handleMouseMove(e: MouseEvent) {
      // Find which column the mouse is over
      let targetDateStr = dragState!.columnDateStr;
      for (const [dateStr, el] of columnRefsMap.current.entries()) {
        const rect = el.getBoundingClientRect();
        if (e.clientX >= rect.left && e.clientX <= rect.right) {
          targetDateStr = dateStr;
          break;
        }
      }

      const columnEl = columnRefsMap.current.get(targetDateStr);
      if (!columnEl) return;

      const rect = columnEl.getBoundingClientRect();
      const yInColumn = e.clientY - rect.top - dragState!.grabOffsetY;
      // Clamp to grid bounds
      const clampedY = Math.max(
        0,
        Math.min(yInColumn, totalHeightPx - dragState!.heightPx),
      );

      setDragState((prev) =>
        prev
          ? { ...prev, currentY: clampedY, columnDateStr: targetDateStr }
          : null,
      );
    }

    function handleMouseUp(e: MouseEvent) {
      if (!dragState) return;

      // Find which column the mouse is over
      let targetDateStr = dragState.columnDateStr;
      for (const [dateStr, el] of columnRefsMap.current.entries()) {
        const rect = el.getBoundingClientRect();
        if (e.clientX >= rect.left && e.clientX <= rect.right) {
          targetDateStr = dateStr;
          break;
        }
      }

      const columnEl = columnRefsMap.current.get(targetDateStr);
      if (!columnEl) {
        setDragState(null);
        return;
      }

      const rect = columnEl.getBoundingClientRect();
      const yInColumn = e.clientY - rect.top - dragState.grabOffsetY;
      const clampedY = Math.max(
        0,
        Math.min(yInColumn, totalHeightPx - dragState.heightPx),
      );

      // Convert Y position to time
      const totalMinutes =
        (clampedY / HOUR_HEIGHT_PX) * 60 + GRID_START_MIN;
      // Round to nearest 15-minute interval
      const rawHour = Math.floor(totalMinutes / 60);
      const rawMinute = Math.round((totalMinutes % 60) / 15) * 15;
      const finalMinute = rawMinute === 60 ? 0 : rawMinute;
      const finalHour = rawMinute === 60 ? rawHour + 1 : rawHour;
      const hh = finalHour.toString().padStart(2, "0");
      const mm = finalMinute.toString().padStart(2, "0");
      const newStartTime = `${targetDateStr}T${hh}:${mm}:00`;

      onReschedule!(dragState.appointmentId, newStartTime);

      // Set flag to prevent click from firing
      justFinishedDrag.current = true;
      setTimeout(() => {
        justFinishedDrag.current = false;
      }, 100);

      setDragState(null);
    }

    window.addEventListener("mousemove", handleMouseMove);
    window.addEventListener("mouseup", handleMouseUp);
    return () => {
      window.removeEventListener("mousemove", handleMouseMove);
      window.removeEventListener("mouseup", handleMouseUp);
    };
  }, [dragState, onReschedule, totalHeightPx]);

  // Compute preview time label for the drag ghost
  let dragPreviewTimeLabel = "";
  if (dragState) {
    const totalMinutes =
      (dragState.currentY / HOUR_HEIGHT_PX) * 60 + GRID_START_MIN;
    const rawHour = Math.floor(totalMinutes / 60);
    const rawMinute = Math.round((totalMinutes % 60) / 15) * 15;
    const finalMinute = rawMinute === 60 ? 0 : rawMinute;
    const finalHour = rawMinute === 60 ? rawHour + 1 : rawHour;
    const suffix = finalHour >= 12 ? "PM" : "AM";
    const displayH = finalHour % 12 === 0 ? 12 : finalHour % 12;
    const displayM = finalMinute.toString().padStart(2, "0");
    dragPreviewTimeLabel = `${displayH}:${displayM} ${suffix}`;
  }

  return (
    <div className="flex overflow-x-auto rounded-lg border border-gray-200 bg-white">
      {/* Time gutter */}
      <div className="shrink-0 w-16 border-r border-gray-200">
        {/* Header spacer (aligns with day column headers) */}
        {view === "week" && (
          <div className="h-8 border-b border-gray-200" />
        )}
        {/* Hour labels */}
        <div className="relative" style={{ height: `${totalHeightPx}px` }}>
          {hours.map((h) => (
            <div
              key={h}
              className="absolute w-full pr-2 text-right text-xs text-gray-400"
              style={{ top: `${(h - START_HOUR) * HOUR_HEIGHT_PX - 8}px` }}
            >
              {h === 12
                ? "12 PM"
                : h > 12
                ? `${h - 12} PM`
                : `${h} AM`}
            </div>
          ))}
        </div>
      </div>

      {/* Day columns */}
      <div className={`flex flex-1 min-w-0 ${view === "week" ? "divide-x divide-gray-200" : ""}`}>
        {columns.map((dateStr, colIdx) => {
          const dayAppts = appointmentsForDate(dateStr);
          const dayLabel = view === "week" ? DAY_LABELS[colIdx] : "";
          // Parse date for numeric label in week view
          const dateParts = dateStr.split("-");
          const dayNum = dateParts[2] ? parseInt(dateParts[2], 10) : "";

          return (
            <div
              key={dateStr}
              className="flex-1 flex flex-col min-w-[80px]"
            >
              {/* Day header (week view only) */}
              {view === "week" && (
                <div className="h-8 border-b border-gray-200 flex items-center justify-center text-xs font-medium text-gray-600 shrink-0">
                  {dayLabel} {dayNum}
                </div>
              )}

              {/* Hour grid lines + appointment cards */}
              <div
                ref={(el) => setColumnRef(dateStr, el)}
                className={`relative${onSlotClick ? " cursor-pointer" : ""}`}
                style={{ height: `${totalHeightPx}px` }}
                onClick={(e) => {
                  // Suppress click if we just finished a drag
                  if (justFinishedDrag.current) return;
                  if (!onSlotClick) return;
                  if (dragState) return;
                  // Only fire if we clicked directly on the grid, not on an appointment card
                  if (e.target !== e.currentTarget) return;
                  const rect = e.currentTarget.getBoundingClientRect();
                  const yOffset = e.clientY - rect.top;
                  const totalMinutes = (yOffset / HOUR_HEIGHT_PX) * 60 + GRID_START_MIN;
                  const clickHour = Math.floor(totalMinutes / 60);
                  // Round to nearest 15-minute interval
                  const clickMinute = Math.round((totalMinutes % 60) / 15) * 15;
                  const finalMinute = clickMinute === 60 ? 0 : clickMinute;
                  const finalHour = clickMinute === 60 ? clickHour + 1 : clickHour;
                  onSlotClick(dateStr, finalHour, finalMinute);
                }}
              >
                {/* Hour grid lines */}
                {hours.map((h) => (
                  <div
                    key={h}
                    className="absolute inset-x-0 border-t border-gray-100 pointer-events-none"
                    style={{
                      top: `${(h - START_HOUR) * HOUR_HEIGHT_PX}px`,
                    }}
                  />
                ))}

                {/* Appointment cards */}
                {dayAppts.map((appt) => (
                  <AppointmentCard
                    key={appt.id}
                    appt={appt}
                    onClick={() => onCardClick(appt)}
                    patientLabel={patientLabel}
                    draggable={canWrite && !!onReschedule}
                    onDragStart={(apptId, grabY, topPx, heightPx, bg, lbl, tl) =>
                      handleCardDragStart(dateStr, apptId, grabY, topPx, heightPx, bg, lbl, tl)
                    }
                    isDragging={dragState?.appointmentId === appt.id}
                  />
                ))}

                {/* Drag ghost — rendered in the target column */}
                {dragState && dragState.columnDateStr === dateStr && (
                  <div
                    className="absolute left-0.5 right-0.5 overflow-hidden rounded text-left text-xs font-medium text-white shadow-lg pointer-events-none z-30 ring-2 ring-white/60"
                    style={{
                      top: `${dragState.currentY}px`,
                      height: `${dragState.heightPx}px`,
                      backgroundColor: dragState.bgColor,
                      opacity: 0.85,
                    }}
                  >
                    <div className="px-1 py-0.5 truncate leading-tight">
                      <span className="block truncate">{dragState.label}</span>
                      <span className="block truncate opacity-90">
                        {dragPreviewTimeLabel}
                      </span>
                    </div>
                  </div>
                )}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}

// ─── Info Popover ─────────────────────────────────────────────────────────────

interface InfoPopoverProps {
  appt: AppointmentRecord;
  onClose: () => void;
  canWrite: boolean;
  onEditAppointment: (appt: AppointmentRecord) => void;
  onCancelAppointment: (appt: AppointmentRecord) => void;
  patientLabel?: (patientId: string) => string;
  onUpdateStatus?: (appointmentId: string, status: string) => Promise<void>;
  onDeleteAppointment?: (appointmentId: string) => Promise<void>;
}

function InfoPopover({ appt, onClose, canWrite, onEditAppointment, onCancelAppointment: _onCancelAppointment, patientLabel, onUpdateStatus, onDeleteAppointment }: InfoPopoverProps) {
  void _onCancelAppointment; // Kept in interface for backward compatibility; status buttons replace it
  const { navigate } = useNav();
  const { user } = useAuth();
  const display = extractAppointmentDisplay(appt.resource);

  const startLabel = display.start ? formatTime(display.start) : "—";
  const endLabel = display.end ? formatTime(display.end) : "—";
  const typeLabel = display.apptTypeDisplay ?? display.apptType ?? "—";

  // Status buttons shown when canWrite and appointment is not already in a terminal state
  const showStatusBtns = canWrite && display.status !== "cancelled" && display.status !== "fulfilled" && display.status !== "noshow";

  // State for "Open Encounter" button
  const [openingEncounter, setOpeningEncounter] = useState(false);
  const [encounterError, setEncounterError] = useState<string | null>(null);

  // State for status update
  const [updatingStatus, setUpdatingStatus] = useState(false);
  const [statusError, setStatusError] = useState<string | null>(null);

  // State for delete confirmation
  const [confirmingDelete, setConfirmingDelete] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [deleteError, setDeleteError] = useState<string | null>(null);

  async function handleStatusUpdate(status: string) {
    if (!onUpdateStatus) return;
    setUpdatingStatus(true);
    setStatusError(null);
    try {
      await onUpdateStatus(appt.id, status);
      onClose();
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setStatusError(msg);
    } finally {
      setUpdatingStatus(false);
    }
  }

  async function handleDelete() {
    if (!onDeleteAppointment) return;
    setDeleting(true);
    setDeleteError(null);
    try {
      await onDeleteAppointment(appt.id);
      onClose();
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setDeleteError(msg);
    } finally {
      setDeleting(false);
    }
  }

  /**
   * Find or create an encounter linked to this appointment, then navigate to it.
   * Searches existing encounters for a FHIR extension with the appointment reference.
   * If none found, creates a new encounter linked to the appointment.
   */
  async function handleOpenEncounter() {
    setOpeningEncounter(true);
    setEncounterError(null);
    try {
      // List all encounters for this patient
      const encounters = await commands.listEncounters(appt.patientId);
      const appointmentRef = `Appointment/${appt.id}`;

      // Search for an encounter linked to this appointment via FHIR extension
      const linkedEncounter = encounters.find((enc) => {
        const extensions = enc.resource?.["extension"];
        if (!Array.isArray(extensions)) return false;
        return extensions.some(
          (ext: Record<string, unknown>) =>
            ext["url"] === "http://medarc.local/fhir/StructureDefinition/encounter-appointment" &&
            (ext["valueReference"] as Record<string, unknown> | undefined)?.["reference"] === appointmentRef,
        );
      });

      if (linkedEncounter) {
        // Encounter exists — navigate to it
        onClose();
        navigate({
          page: "encounter-workspace",
          patientId: appt.patientId,
          encounterId: linkedEncounter.id,
        });
      } else {
        // No encounter yet — create one linked to this appointment
        const encounterDate = display.start ?? new Date().toISOString().slice(0, 19);
        const encounterType = display.apptType ?? "office_visit";
        const created = await commands.createEncounter({
          patientId: appt.patientId,
          providerId: user?.id ?? appt.providerId,
          encounterDate,
          encounterType,
          chiefComplaint: display.reason ?? null,
          templateId: null,
          soap: null,
          appointmentId: appt.id,
        });
        onClose();
        navigate({
          page: "encounter-workspace",
          patientId: appt.patientId,
          encounterId: created.id,
        });
      }
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      console.error("[CalendarPage] handleOpenEncounter failed:", msg);
      setEncounterError(msg);
    } finally {
      setOpeningEncounter(false);
    }
  }

  return (
    /* Backdrop */
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/40"
      onClick={onClose}
    >
      {/* Panel — stop propagation so clicking inside doesn't close */}
      <div
        className="relative w-full max-w-sm rounded-xl bg-white p-6 shadow-xl"
        onClick={(e) => e.stopPropagation()}
        role="dialog"
        aria-modal="true"
        aria-label="Appointment details"
      >
        {/* Close button */}
        <button
          onClick={onClose}
          className="absolute right-4 top-4 rounded-md p-1 text-gray-400 hover:bg-gray-100 hover:text-gray-600 focus:outline-none focus:ring-2 focus:ring-blue-500"
          aria-label="Close"
        >
          ✕
        </button>

        <h3 className="text-base font-semibold text-gray-900 mb-4">
          {typeLabel}
        </h3>

        <dl className="space-y-2 text-sm">
          <div className="flex gap-2">
            <dt className="w-20 shrink-0 text-gray-500">Time</dt>
            <dd className="text-gray-900">
              {startLabel} – {endLabel}
            </dd>
          </div>
          <div className="flex gap-2">
            <dt className="w-20 shrink-0 text-gray-500">Status</dt>
            <dd className="text-gray-900 capitalize">
              {display.status ?? "—"}
            </dd>
          </div>
          {display.reason && (
            <div className="flex gap-2">
              <dt className="w-20 shrink-0 text-gray-500">Reason</dt>
              <dd className="text-gray-900">{display.reason}</dd>
            </div>
          )}
          {display.notes && (
            <div className="flex gap-2">
              <dt className="w-20 shrink-0 text-gray-500">Notes</dt>
              <dd className="text-gray-900">{display.notes}</dd>
            </div>
          )}
          <div className="flex gap-2">
            <dt className="w-20 shrink-0 text-gray-500">Patient</dt>
            <dd className="text-gray-900 text-sm break-all">
              {patientLabel ? patientLabel(appt.patientId) : appt.patientId}
            </dd>
          </div>
        </dl>

        {/* Encounter error */}
        {encounterError && (
          <p className="mt-2 text-sm text-red-600">{encounterError}</p>
        )}

        {/* Status / delete errors */}
        {statusError && (
          <p className="mt-2 text-sm text-red-600">{statusError}</p>
        )}
        {deleteError && (
          <p className="mt-2 text-sm text-red-600">{deleteError}</p>
        )}

        {/* Action buttons */}
        <div className="mt-5 flex flex-col gap-2">
          {/* Open encounter */}
          <button
            onClick={() => void handleOpenEncounter()}
            disabled={openingEncounter}
            className="w-full rounded-lg bg-blue-600 px-4 py-2 text-sm font-medium text-white hover:bg-blue-700 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:ring-offset-2 disabled:opacity-60"
          >
            {openingEncounter ? "Opening..." : "Open Encounter"}
          </button>

          {/* Edit appointment — write-gated, not cancelled/fulfilled */}
          {canWrite && display.status !== "cancelled" && display.status !== "fulfilled" && (
            <button
              onClick={() => {
                onClose();
                onEditAppointment(appt);
              }}
              className="w-full rounded-lg border border-gray-300 bg-white px-4 py-2 text-sm font-medium text-gray-700 hover:bg-gray-50 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:ring-offset-2"
            >
              Edit Appointment
            </button>
          )}

          {/* Status buttons — Attended / Canceled / No Show */}
          {showStatusBtns && onUpdateStatus && (
            <div className="flex gap-2">
              <button
                onClick={() => void handleStatusUpdate("fulfilled")}
                disabled={updatingStatus}
                className="flex-1 rounded-lg bg-green-600 px-3 py-2 text-sm font-medium text-white hover:bg-green-700 focus:outline-none focus:ring-2 focus:ring-green-500 focus:ring-offset-2 disabled:opacity-60"
              >
                Attended
              </button>
              <button
                onClick={() => void handleStatusUpdate("cancelled")}
                disabled={updatingStatus}
                className="flex-1 rounded-lg bg-red-600 px-3 py-2 text-sm font-medium text-white hover:bg-red-700 focus:outline-none focus:ring-2 focus:ring-red-500 focus:ring-offset-2 disabled:opacity-60"
              >
                Canceled
              </button>
              <button
                onClick={() => void handleStatusUpdate("noshow")}
                disabled={updatingStatus}
                className="flex-1 rounded-lg bg-orange-500 px-3 py-2 text-sm font-medium text-white hover:bg-orange-600 focus:outline-none focus:ring-2 focus:ring-orange-500 focus:ring-offset-2 disabled:opacity-60"
              >
                No Show
              </button>
            </div>
          )}

          {/* Delete appointment — with confirmation */}
          {canWrite && onDeleteAppointment && (
            <>
              {!confirmingDelete ? (
                <button
                  onClick={() => setConfirmingDelete(true)}
                  className="w-full rounded-lg border border-red-300 bg-white px-4 py-2 text-sm font-medium text-red-700 hover:bg-red-50 focus:outline-none focus:ring-2 focus:ring-red-500 focus:ring-offset-2"
                >
                  Delete Appointment
                </button>
              ) : (
                <div className="rounded-lg border border-red-300 bg-red-50 p-3">
                  <p className="text-sm text-red-800 mb-2">Are you sure? This permanently deletes the appointment.</p>
                  <div className="flex gap-2">
                    <button
                      onClick={() => void handleDelete()}
                      disabled={deleting}
                      className="flex-1 rounded-md bg-red-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-red-700 disabled:opacity-60"
                    >
                      {deleting ? "Deleting..." : "Confirm Delete"}
                    </button>
                    <button
                      onClick={() => setConfirmingDelete(false)}
                      disabled={deleting}
                      className="flex-1 rounded-md border border-gray-300 bg-white px-3 py-1.5 text-sm font-medium text-gray-700 hover:bg-gray-50 disabled:opacity-60"
                    >
                      Cancel
                    </button>
                  </div>
                </div>
              )}
            </>
          )}
        </div>
      </div>
    </div>
  );
}

// ─── CalendarPage (main export) ───────────────────────────────────────────────

export function CalendarPage({
  appointments,
  view,
  currentDate,
  onPrev,
  onNext,
  onViewChange,
  onCardClick,
  onEditAppointment,
  onCancelAppointment,
  canWrite,
  onSlotClick,
  onToday,
  patientLabel,
  onReschedule,
  onUpdateStatus,
  onDeleteAppointment,
}: CalendarPageProps) {
  const [selectedCard, setSelectedCard] = useState<AppointmentRecord | null>(null);

  function handleCardClick(appt: AppointmentRecord) {
    setSelectedCard(appt);
    onCardClick(appt);
  }

  function handleClosePopover() {
    setSelectedCard(null);
  }

  return (
    <div>
      <CalendarHeader
        view={view}
        currentDate={currentDate}
        onPrev={onPrev}
        onNext={onNext}
        onViewChange={onViewChange}
        onToday={onToday}
      />
      <CalendarGrid
        appointments={appointments}
        view={view}
        currentDate={currentDate}
        onCardClick={handleCardClick}
        onSlotClick={onSlotClick}
        patientLabel={patientLabel}
        onReschedule={onReschedule}
        canWrite={canWrite}
      />
      {selectedCard && (
        <InfoPopover
          appt={selectedCard}
          onClose={handleClosePopover}
          canWrite={canWrite}
          onEditAppointment={onEditAppointment}
          onCancelAppointment={onCancelAppointment}
          patientLabel={patientLabel}
          onUpdateStatus={onUpdateStatus}
          onDeleteAppointment={onDeleteAppointment}
        />
      )}
    </div>
  );
}

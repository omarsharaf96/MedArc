/**
 * FlowBoardPage.tsx — Patient Flow Board for today's clinic.
 *
 * Renders today's flow board entries returned by getFlowBoard. Each card shows
 * the appointment type, start time, flow status as a colored badge, a room
 * text input, and real status-transition buttons (wired in T03).
 *
 * Flow status state machine (from scheduling.rs):
 *   scheduled → checked_in → roomed → with_provider → checkout → completed
 *
 * Each card has per-entry submitting/error state so one in-flight call does
 * not block transitions on other cards.
 *
 * canWrite gates: BillingStaff sees no transition buttons.
 *
 * Observability:
 *   - Error state rendered as a red inline banner — visible without DevTools.
 *   - Per-card submitError shown inline below the transition buttons.
 *   - Structured console.error is in useSchedule; this component only renders
 *     the error strings it receives.
 */

import { useState } from "react";
import type { FlowBoardEntry, UpdateFlowStatusInput } from "../../types/scheduling";

// ─── Helpers ──────────────────────────────────────────────────────────────────

function formatTime(datetimeStr: string): string {
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

// ─── Status badge color map ───────────────────────────────────────────────────

const FLOW_STATUS_COLORS: Record<string, string> = {
  scheduled: "bg-gray-100 text-gray-700",
  checked_in: "bg-blue-100 text-blue-700",
  roomed: "bg-indigo-100 text-indigo-700",
  with_provider: "bg-purple-100 text-purple-700",
  checkout: "bg-orange-100 text-orange-700",
  completed: "bg-green-100 text-green-700",
};

const FLOW_STATUS_LABELS: Record<string, string> = {
  scheduled: "Scheduled",
  checked_in: "Checked In",
  roomed: "Roomed",
  with_provider: "With Provider",
  checkout: "Checkout",
  completed: "Completed",
};

// ─── State machine ────────────────────────────────────────────────────────────

/**
 * Returns the valid next statuses for a given flow status.
 * Terminal state "completed" has no valid transitions.
 */
function nextStatuses(current: string): string[] {
  switch (current) {
    case "scheduled":
      return ["checked_in"];
    case "checked_in":
      return ["roomed"];
    case "roomed":
      return ["with_provider"];
    case "with_provider":
      return ["checkout"];
    case "checkout":
      return ["completed"];
    case "completed":
      return [];
    default:
      return [];
  }
}

// ─── Props ────────────────────────────────────────────────────────────────────

export interface FlowBoardPageProps {
  flowBoard: FlowBoardEntry[];
  loading: boolean;
  error: string | null;
  canWrite: boolean;
  onUpdateStatus: (input: UpdateFlowStatusInput) => Promise<void>;
  patientLabel?: (patientId: string) => string;
}

// ─── FlowBoardCard ────────────────────────────────────────────────────────────

interface FlowBoardCardProps {
  entry: FlowBoardEntry;
  canWrite: boolean;
  onUpdateStatus: (input: UpdateFlowStatusInput) => Promise<void>;
  patientLabel?: (patientId: string) => string;
}

function FlowBoardCard({ entry, canWrite, onUpdateStatus, patientLabel }: FlowBoardCardProps) {
  const [submitting, setSubmitting] = useState(false);
  const [cardError, setCardError] = useState<string | null>(null);
  const [roomValue, setRoomValue] = useState(entry.room ?? "");

  const badgeClass =
    FLOW_STATUS_COLORS[entry.flowStatus] ?? "bg-gray-100 text-gray-700";
  const statusLabel =
    FLOW_STATUS_LABELS[entry.flowStatus] ?? entry.flowStatus;

  const validNextStatuses = nextStatuses(entry.flowStatus);

  async function handleTransition(nextStatus: string) {
    setSubmitting(true);
    setCardError(null);
    try {
      await onUpdateStatus({
        appointmentId: entry.appointmentId,
        flowStatus: nextStatus,
        room: roomValue.trim() || null,
        notes: null,
      });
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      setCardError(msg);
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <div className="rounded-lg border border-gray-200 bg-white px-4 py-3 shadow-sm space-y-2">
      {/* Top row: type, time, badge */}
      <div className="flex items-center gap-4">
        {/* Patient + Appointment type + time */}
        <div className="min-w-0 flex-1">
          <p className="truncate text-sm font-semibold text-gray-900">
            {patientLabel ? patientLabel(entry.patientId) : entry.patientId}
          </p>
          <p className="truncate text-sm text-gray-700">
            {entry.apptType}
          </p>
          <p className="text-xs text-gray-500">
            {formatTime(entry.startTime)}
            {entry.room ? ` · Room ${entry.room}` : ""}
          </p>
        </div>

        {/* Status badge */}
        <span
          className={`shrink-0 rounded-full px-2.5 py-0.5 text-xs font-medium ${badgeClass}`}
        >
          {statusLabel}
        </span>
      </div>

      {/* Write-path: room input + transition buttons */}
      {canWrite && validNextStatuses.length > 0 && (
        <div className="flex items-center gap-2 flex-wrap">
          {/* Room input */}
          <input
            type="text"
            value={roomValue}
            onChange={(e) => setRoomValue(e.target.value)}
            placeholder="Room (optional)"
            disabled={submitting}
            className="rounded-md border border-gray-300 px-2.5 py-1 text-xs shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500 w-32 disabled:opacity-50"
          />

          {/* Transition buttons — one per valid next status */}
          {validNextStatuses.map((next) => (
            <button
              key={next}
              type="button"
              onClick={() => handleTransition(next)}
              disabled={submitting}
              className="rounded-md bg-blue-600 px-3 py-1 text-xs font-medium text-white hover:bg-blue-700 disabled:cursor-not-allowed disabled:opacity-50 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:ring-offset-1"
            >
              {submitting
                ? "Updating…"
                : next.replace(/_/g, " ")}
            </button>
          ))}
        </div>
      )}

      {/* Per-card submit error */}
      {cardError && (
        <p className="text-xs text-red-600 mt-1">{cardError}</p>
      )}
    </div>
  );
}

// ─── FlowBoardPage (main export) ──────────────────────────────────────────────

export function FlowBoardPage({
  flowBoard,
  loading,
  error,
  canWrite,
  onUpdateStatus,
  patientLabel,
}: FlowBoardPageProps) {
  // ── Loading skeleton ──────────────────────────────────────────────────────
  if (loading) {
    return (
      <div className="space-y-2" aria-label="Loading flow board">
        {[1, 2, 3].map((i) => (
          <div
            key={i}
            className="h-16 rounded-lg bg-gray-100 animate-pulse"
          />
        ))}
      </div>
    );
  }

  // ── Error banner ──────────────────────────────────────────────────────────
  if (error) {
    return (
      <div
        role="alert"
        className="rounded-lg border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-800"
      >
        <span className="font-medium">Flow board error: </span>
        {error}
      </div>
    );
  }

  // ── Empty state ───────────────────────────────────────────────────────────
  if (flowBoard.length === 0) {
    return (
      <p className="text-sm text-gray-500 italic">
        No patients on today's flow board.
      </p>
    );
  }

  // ── Flow board cards ──────────────────────────────────────────────────────
  return (
    <div className="space-y-2">
      {flowBoard.map((entry) => (
        <FlowBoardCard
          key={entry.appointmentId}
          entry={entry}
          canWrite={canWrite}
          onUpdateStatus={onUpdateStatus}
          patientLabel={patientLabel}
        />
      ))}
    </div>
  );
}

/**
 * ActionCard.tsx — Confirmation card for assistant-proposed actions.
 *
 * Shows a description of the action with Confirm/Dismiss buttons.
 * On confirm, calls the executeAssistantAction backend command.
 *
 * For create_note actions, strips AI wrapper text (preamble/postamble)
 * and parses SOAP sections before saving to the encounter.
 */

import { useState, useEffect, useRef } from "react";
import { commands } from "../../lib/tauri";
import type { AssistantAction, ActionResult } from "../../types/assistant";

/** Prefixes for actions that only read data and can auto-execute silently. */
const READ_ONLY_PREFIXES = [
  "get_",
  "list_",
  "search_",
  "read_",
  "find_",
];

/** Check whether an action is read-only (no data mutation). */
function isReadOnly(actionName: string): boolean {
  return READ_ONLY_PREFIXES.some((p) => actionName.startsWith(p));
}

interface ActionCardProps {
  actions: AssistantAction[];
  conversationId: string;
  /** The message ID — used to clear actions_json after confirm/dismiss. */
  messageId: string;
  onActionComplete: (result: ActionResult) => void;
}

/** Human-readable labels for action types. */
const ACTION_LABELS: Record<string, string> = {
  schedule_appointment: "Schedule Appointment",
  list_appointments: "List Appointments",
  cancel_appointment: "Cancel Appointment",
  search_patients: "Search Patients",
  find_inactive_patients: "Find Inactive Patients",
  export_note_pdf: "Export Note PDF",
  export_progress_report: "Export Progress Report",
  export_chart: "Export Full Chart",
  create_note: "Create Clinical Note",
  get_patient_summary: "Get Patient Summary",
  get_patient_notes: "Get Patient Notes",
  search_documents: "Search Documents",
  get_patient_clinical_data: "Get Clinical Data",
  read_document: "Read Document",
};

/** Format action parameters for display. */
function formatParams(action: AssistantAction): string[] {
  const lines: string[] = [];
  const skip = new Set(["action"]);
  for (const [key, value] of Object.entries(action)) {
    if (skip.has(key) || value === null || value === undefined) continue;
    // camelCase to Title Case
    const label = key.replace(/([A-Z])/g, " $1").replace(/^./, (s) => s.toUpperCase());
    lines.push(`${label}: ${String(value)}`);
  }
  return lines;
}

export function ActionCard({
  actions,
  conversationId,
  messageId,
  onActionComplete,
}: ActionCardProps) {
  const [executing, setExecuting] = useState(false);
  const [dismissed, setDismissed] = useState(false);
  const [result, setResult] = useState<ActionResult | null>(null);
  const autoExecuted = useRef(false);

  /** True when every action in the batch is read-only. */
  const allReadOnly = actions.every((a) => isReadOnly(a.action));

  // Auto-execute read-only actions without confirmation
  useEffect(() => {
    if (allReadOnly && !executing && !result && !dismissed && !autoExecuted.current) {
      autoExecuted.current = true;
      handleConfirm();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  if (dismissed) return null;

  async function handleConfirm() {
    setExecuting(true);
    try {
      // Execute all actions sequentially
      for (const action of actions) {
        const { action: actionType, ...params } = action;
        const res = await commands.executeAssistantAction({
          action: actionType,
          params: params as Record<string, unknown>,
          conversationId,
        });
        setResult(res);
        onActionComplete(res);

        if (res.success) {
          window.dispatchEvent(new CustomEvent("assistant-action-completed"));
        }

        // Auto-invoke PDF export if the action returned an autoInvoke field
        if (res.success && res.data) {
          const data = res.data as Record<string, unknown>;
          if (data.autoInvoke === "generateEncounterNotePdf" && data.encounterId) {
            try {
              await commands.generateEncounterNotePdf(data.encounterId as string);
            } catch (pdfErr) {
              console.error("Auto PDF export failed:", pdfErr);
            }
          } else if (data.autoInvoke === "generateProgressReport" && data.patientId) {
            try {
              await commands.generateProgressReport(data.patientId as string);
            } catch (pdfErr) {
              console.error("Auto PDF export failed:", pdfErr);
            }
          } else if (data.autoInvoke === "generateChartExport" && data.patientId) {
            try {
              await commands.generateChartExport(data.patientId as string);
            } catch (pdfErr) {
              console.error("Auto PDF export failed:", pdfErr);
            }
          }

          // Note: The backend execute_create_note already saves the clean SOAP
          // sections from the noteContent parameter. Do NOT overwrite here.

          // Navigate to the encounter workspace when the assistant creates a note
          if (data.autoNavigate === "encounter-workspace" && data.encounterId && data.patientId) {
            setTimeout(() => {
              window.dispatchEvent(
                new CustomEvent("navigate-to-encounter", {
                  detail: {
                    patientId: data.patientId as string,
                    encounterId: data.encounterId as string,
                  },
                })
              );
            }, 300);
          }
        }
      }
    } catch (err) {
      const errorResult: ActionResult = {
        success: false,
        message: String(err),
        data: null,
      };
      setResult(errorResult);
      onActionComplete(errorResult);
    } finally {
      setExecuting(false);
      // Clear actions_json in DB so they don't re-appear on page reload
      commands.clearMessageActions(messageId).catch(() => {});
    }
  }

  if (result) {
    // Read-only actions execute silently — no result card shown on success.
    // Only show the card if a read-only action *failed* (so the user knows).
    if (allReadOnly && result.success) return null;

    return (
      <div
        className={`mb-3 rounded-lg border p-3 text-sm ${
          result.success
            ? "border-green-200 bg-green-50 text-green-800"
            : "border-red-200 bg-red-50 text-red-800"
        }`}
      >
        <div className="flex items-center gap-2 mb-1">
          <span>{result.success ? "\u2713" : "\u2717"}</span>
          <span className="font-medium">
            {result.success ? "Action completed" : "Action failed"}
          </span>
        </div>
        <div className="whitespace-pre-wrap">{result.message}</div>
      </div>
    );
  }

  return (
    <div className="mb-3 rounded-lg border border-blue-200 bg-blue-50 p-3">
      <div className="mb-2 text-xs font-semibold uppercase tracking-wide text-blue-600">
        Proposed Action{actions.length > 1 ? "s" : ""}
      </div>
      {actions.map((action, i) => (
        <div key={i} className="mb-2">
          <div className="text-sm font-medium text-gray-900">
            {ACTION_LABELS[action.action] || action.action}
          </div>
          <ul className="mt-1 space-y-0.5 text-xs text-gray-600">
            {formatParams(action).map((line, j) => (
              <li key={j}>{line}</li>
            ))}
          </ul>
        </div>
      ))}
      <div className="mt-3 flex gap-2">
        <button
          type="button"
          onClick={handleConfirm}
          disabled={executing}
          className="rounded-md bg-blue-600 px-3 py-1.5 text-xs font-medium text-white shadow-sm hover:bg-blue-700 disabled:opacity-50"
        >
          {executing ? "Executing..." : "Confirm"}
        </button>
        <button
          type="button"
          onClick={() => {
            setDismissed(true);
            commands.clearMessageActions(messageId).catch(() => {});
          }}
          disabled={executing}
          className="rounded-md border border-gray-300 bg-white px-3 py-1.5 text-xs font-medium text-gray-700 shadow-sm hover:bg-gray-50 disabled:opacity-50"
        >
          Dismiss
        </button>
      </div>
    </div>
  );
}

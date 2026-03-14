/**
 * SendFaxDialog.tsx — Reusable modal for sending a fax.
 *
 * Can be triggered from any page that has a document to fax.
 * The caller provides the file path and document name; this dialog
 * handles recipient selection (from contacts or ad-hoc number) and confirmation.
 *
 * Overlay pattern: fixed inset-0 bg-black/40 z-50 (same as all other modals).
 */
import { useState, useEffect, useCallback, type FormEvent } from "react";
import { commands } from "../../lib/tauri";
import type { FaxContact, SendFaxInput } from "../../types/fax";

// ─── Shared style constants ──────────────────────────────────────────────────

const INPUT_CLS =
  "w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500";
const LABEL_CLS = "mb-1 block text-sm font-medium text-gray-700";

// ─── Props ───────────────────────────────────────────────────────────────────

export interface SendFaxDialogProps {
  /** The name of the document being faxed (shown in the dialog). */
  documentName: string;
  /** Path to the file to fax. */
  filePath: string;
  /** Optional patient ID to associate. */
  patientId?: string | null;
  /** Called after successful send. */
  onSuccess: () => void;
  /** Called when dialog is closed/cancelled. */
  onClose: () => void;
}

// ─── Recipient mode ──────────────────────────────────────────────────────────

type RecipientMode = "contact" | "adhoc";

// ─── Component ───────────────────────────────────────────────────────────────

export function SendFaxDialog({
  documentName,
  filePath,
  patientId,
  onSuccess,
  onClose,
}: SendFaxDialogProps) {
  const [recipientMode, setRecipientMode] = useState<RecipientMode>("contact");
  const [contacts, setContacts] = useState<FaxContact[]>([]);
  const [contactsLoading, setContactsLoading] = useState(false);
  const [selectedContactId, setSelectedContactId] = useState("");
  const [adhocFaxNumber, setAdhocFaxNumber] = useState("");
  const [adhocName, setAdhocName] = useState("");
  const [sending, setSending] = useState(false);
  const [submitError, setSubmitError] = useState<string | null>(null);
  const [showConfirm, setShowConfirm] = useState(false);

  useEffect(() => {
    let mounted = true;
    setContactsLoading(true);
    commands
      .listFaxContacts(null)
      .then((list) => {
        if (mounted) setContacts(list);
      })
      .catch(() => {})
      .finally(() => {
        if (mounted) setContactsLoading(false);
      });
    return () => {
      mounted = false;
    };
  }, []);

  const selectedContact = contacts.find((c) => c.contactId === selectedContactId) ?? null;

  const recipientFaxNumber =
    recipientMode === "contact"
      ? selectedContact?.faxNumber ?? ""
      : adhocFaxNumber.trim();

  const recipientName =
    recipientMode === "contact"
      ? selectedContact?.name ?? ""
      : adhocName.trim() || "Unknown";

  const canSend = recipientFaxNumber.length >= 7;

  const handleProceedToConfirm = useCallback(
    (e: FormEvent) => {
      e.preventDefault();
      setSubmitError(null);
      if (!canSend) return;
      setShowConfirm(true);
    },
    [canSend],
  );

  const handleSend = useCallback(async () => {
    setSending(true);
    setSubmitError(null);
    try {
      const input: SendFaxInput = {
        filePath,
        recipientFax: recipientFaxNumber,
        recipientName,
        patientId: patientId ?? null,
      };
      await commands.sendFax(input);
      onSuccess();
    } catch (e) {
      setSubmitError(e instanceof Error ? e.message : String(e));
      setShowConfirm(false);
    } finally {
      setSending(false);
    }
  }, [filePath, recipientFaxNumber, recipientName, patientId, onSuccess]);

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
      <div className="w-full max-w-lg rounded-lg bg-white shadow-xl">
        <div className="flex items-center justify-between border-b border-gray-200 px-6 py-4">
          <h2 className="text-lg font-semibold text-gray-900">Send Fax</h2>
          <button
            type="button"
            onClick={onClose}
            disabled={sending}
            className="text-gray-400 transition-colors hover:text-gray-600"
            aria-label="Close"
          >
            &times;
          </button>
        </div>

        <div className="px-6 py-4">
          <div className="mb-5 rounded-md border border-gray-200 bg-gray-50 px-4 py-3">
            <p className="text-sm font-medium text-gray-900">{documentName}</p>
          </div>

          {showConfirm ? (
            <div className="space-y-4">
              <div className="rounded-md border border-blue-200 bg-blue-50 px-4 py-3 text-sm text-blue-800">
                <p className="font-medium">Confirm fax details:</p>
                <dl className="mt-2 space-y-1">
                  <div className="flex justify-between">
                    <dt className="text-blue-600">To:</dt>
                    <dd>{recipientName} ({recipientFaxNumber})</dd>
                  </div>
                  <div className="flex justify-between">
                    <dt className="text-blue-600">Document:</dt>
                    <dd>{documentName}</dd>
                  </div>
                </dl>
              </div>

              {submitError && (
                <div className="rounded-md border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700">
                  {submitError}
                </div>
              )}

              <div className="flex justify-end gap-3">
                <button
                  type="button"
                  onClick={() => setShowConfirm(false)}
                  disabled={sending}
                  className="rounded-md border border-gray-300 bg-white px-4 py-2 text-sm font-medium text-gray-700 shadow-sm transition-colors hover:bg-gray-50"
                >
                  Back
                </button>
                <button
                  type="button"
                  onClick={handleSend}
                  disabled={sending}
                  className="rounded-md bg-blue-600 px-4 py-2 text-sm font-medium text-white shadow-sm transition-colors hover:bg-blue-700 disabled:cursor-not-allowed disabled:opacity-50"
                >
                  {sending ? "Sending..." : "Send Fax"}
                </button>
              </div>
            </div>
          ) : (
            <form onSubmit={handleProceedToConfirm} className="space-y-4">
              <div className="flex gap-4">
                <label className="flex items-center gap-2 text-sm">
                  <input
                    type="radio"
                    name="recipientMode"
                    value="contact"
                    checked={recipientMode === "contact"}
                    onChange={() => setRecipientMode("contact")}
                    className="text-blue-600 focus:ring-blue-500"
                  />
                  Select from contacts
                </label>
                <label className="flex items-center gap-2 text-sm">
                  <input
                    type="radio"
                    name="recipientMode"
                    value="adhoc"
                    checked={recipientMode === "adhoc"}
                    onChange={() => setRecipientMode("adhoc")}
                    className="text-blue-600 focus:ring-blue-500"
                  />
                  Enter fax number
                </label>
              </div>

              {recipientMode === "contact" ? (
                <div>
                  <label className={LABEL_CLS}>Contact</label>
                  {contactsLoading ? (
                    <p className="text-sm text-gray-400">Loading contacts...</p>
                  ) : contacts.length === 0 ? (
                    <p className="text-sm text-gray-500">
                      No contacts found. Switch to manual entry or add contacts in the Fax page.
                    </p>
                  ) : (
                    <select
                      value={selectedContactId}
                      onChange={(e) => setSelectedContactId(e.target.value)}
                      className={INPUT_CLS}
                    >
                      <option value="">-- Select a contact --</option>
                      {contacts.map((c) => (
                        <option key={c.contactId} value={c.contactId}>
                          {c.name} {c.organization ? `(${c.organization})` : ""} - {c.faxNumber}
                        </option>
                      ))}
                    </select>
                  )}
                </div>
              ) : (
                <>
                  <div>
                    <label className={LABEL_CLS}>Recipient Name (optional)</label>
                    <input
                      type="text"
                      value={adhocName}
                      onChange={(e) => setAdhocName(e.target.value)}
                      placeholder="Dr. Smith"
                      className={INPUT_CLS}
                    />
                  </div>
                  <div>
                    <label className={LABEL_CLS}>
                      Fax Number <span className="text-red-500">*</span>
                    </label>
                    <input
                      type="tel"
                      value={adhocFaxNumber}
                      onChange={(e) => setAdhocFaxNumber(e.target.value)}
                      placeholder="+1 (555) 123-4567"
                      required
                      className={INPUT_CLS}
                    />
                  </div>
                </>
              )}

              {submitError && (
                <div className="rounded-md border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700">
                  {submitError}
                </div>
              )}

              <div className="flex justify-end gap-3 pt-2">
                <button
                  type="button"
                  onClick={onClose}
                  className="rounded-md border border-gray-300 bg-white px-4 py-2 text-sm font-medium text-gray-700 shadow-sm transition-colors hover:bg-gray-50"
                >
                  Cancel
                </button>
                <button
                  type="submit"
                  disabled={!canSend}
                  className="rounded-md bg-blue-600 px-4 py-2 text-sm font-medium text-white shadow-sm transition-colors hover:bg-blue-700 disabled:cursor-not-allowed disabled:opacity-50"
                >
                  Continue
                </button>
              </div>
            </form>
          )}
        </div>
      </div>
    </div>
  );
}

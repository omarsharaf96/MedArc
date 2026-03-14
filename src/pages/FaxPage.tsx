/**
 * FaxPage.tsx — Three-tab Fax management page: Inbox | Contacts | Log
 *
 * Inbox:     Poll and display received faxes, link to patients.
 * Contacts:  CRUD for fax contacts, filtered by type.
 * Log:       All fax activity (sent/received) with direction/status filters, retry for failed.
 *
 * Observability:
 *   - error states rendered inline in red banners
 *   - loading / submitting boolean state visible in React DevTools
 *   - console.error tagged [FaxPage] on fetch/mutation failures
 */

import { useState, useEffect, useCallback } from "react";
import { commands } from "../lib/tauri";
import type {
  FaxRecord,
  FaxContact,
  FaxContactInput,
  FaxContactType,
  FaxDirection,
  FaxStatus,
} from "../types/fax";

// ─── Shared style constants ──────────────────────────────────────────────────

const INPUT_CLS =
  "w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500";
const LABEL_CLS = "mb-1 block text-sm font-medium text-gray-700";

// ─── Tab type ────────────────────────────────────────────────────────────────

type Tab = "inbox" | "contacts" | "log";

// ─── Contact type options ────────────────────────────────────────────────────

const CONTACT_TYPE_OPTIONS: { value: FaxContactType | ""; label: string }[] = [
  { value: "", label: "All Types" },
  { value: "insurance", label: "Insurance" },
  { value: "referring_md", label: "Referring MD" },
  { value: "attorney", label: "Attorney" },
  { value: "other", label: "Other" },
];

const CONTACT_TYPE_VALUES: FaxContactType[] = ["insurance", "referring_md", "attorney", "other"];

// ─── Helpers ─────────────────────────────────────────────────────────────────

function formatDate(iso: string): string {
  try {
    return new Date(iso).toLocaleString();
  } catch {
    return iso;
  }
}

function contactTypeBadge(type: FaxContactType): React.ReactElement {
  const base = "inline-flex items-center rounded-full px-2 py-0.5 text-xs font-medium";
  switch (type) {
    case "insurance":
      return <span className={`${base} bg-blue-100 text-blue-700`}>Insurance</span>;
    case "referring_md":
      return <span className={`${base} bg-green-100 text-green-700`}>Referring MD</span>;
    case "attorney":
      return <span className={`${base} bg-purple-100 text-purple-700`}>Attorney</span>;
    case "other":
      return <span className={`${base} bg-gray-100 text-gray-600`}>Other</span>;
    default:
      return <span className={`${base} bg-gray-100 text-gray-600`}>{type}</span>;
  }
}

function faxStatusBadge(status: string): React.ReactElement {
  const base = "inline-flex items-center rounded-full px-2 py-0.5 text-xs font-medium";
  switch (status) {
    case "success":
      return <span className={`${base} bg-green-100 text-green-700`}>Success</span>;
    case "failed":
      return <span className={`${base} bg-red-100 text-red-700`}>Failed</span>;
    case "queued":
      return <span className={`${base} bg-yellow-100 text-yellow-700`}>Queued</span>;
    case "in_progress":
      return <span className={`${base} bg-blue-100 text-blue-700`}>In Progress</span>;
    default:
      return <span className={`${base} bg-gray-100 text-gray-600`}>{status}</span>;
  }
}

// ─── Contact Form Modal ──────────────────────────────────────────────────────

interface ContactFormModalProps {
  initial: FaxContact | null;
  onSave: (input: FaxContactInput) => Promise<void>;
  onClose: () => void;
}

function ContactFormModal({ initial, onSave, onClose }: ContactFormModalProps) {
  const [name, setName] = useState(initial?.name ?? "");
  const [organization, setOrganization] = useState(initial?.organization ?? "");
  const [faxNumber, setFaxNumber] = useState(initial?.faxNumber ?? "");
  const [phoneNumber, setPhoneNumber] = useState(initial?.phoneNumber ?? "");
  const [contactType, setContactType] = useState<FaxContactType>(initial?.contactType ?? "other");
  const [notes, setNotes] = useState(initial?.notes ?? "");
  const [submitting, setSubmitting] = useState(false);
  const [submitError, setSubmitError] = useState<string | null>(null);

  const handleSubmit = useCallback(
    async (e: React.FormEvent) => {
      e.preventDefault();
      setSubmitting(true);
      setSubmitError(null);
      try {
        await onSave({
          name: name.trim(),
          organization: organization.trim() || null,
          faxNumber: faxNumber.trim(),
          phoneNumber: phoneNumber.trim() || null,
          contactType,
          notes: notes.trim() || null,
        });
        onClose();
      } catch (err) {
        setSubmitError(err instanceof Error ? err.message : String(err));
      } finally {
        setSubmitting(false);
      }
    },
    [name, organization, faxNumber, phoneNumber, contactType, notes, onSave, onClose],
  );

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40" role="dialog" aria-modal="true" aria-labelledby="contact-modal-title">
      <div className="w-full max-w-md rounded-lg bg-white shadow-xl">
        <div className="flex items-center justify-between border-b border-gray-200 px-6 py-4">
          <h2 id="contact-modal-title" className="text-lg font-semibold text-gray-900">
            {initial ? "Edit Contact" : "Add Contact"}
          </h2>
          <button
            type="button"
            onClick={onClose}
            disabled={submitting}
            className="text-gray-400 transition-colors hover:text-gray-600"
            aria-label="Close"
          >
            &times;
          </button>
        </div>

        <form onSubmit={handleSubmit} className="space-y-4 px-6 py-4">
          <div>
            <label htmlFor="contact-name" className={LABEL_CLS}>
              Name <span className="text-red-500">*</span>
            </label>
            <input
              id="contact-name"
              type="text"
              value={name}
              onChange={(e) => setName(e.target.value)}
              required
              className={INPUT_CLS}
              placeholder="Dr. John Smith"
            />
          </div>

          <div>
            <label htmlFor="contact-organization" className={LABEL_CLS}>Organization</label>
            <input
              id="contact-organization"
              type="text"
              value={organization}
              onChange={(e) => setOrganization(e.target.value)}
              className={INPUT_CLS}
              placeholder="ABC Medical Group"
            />
          </div>

          <div>
            <label htmlFor="contact-fax-number" className={LABEL_CLS}>
              Fax Number <span className="text-red-500">*</span>
            </label>
            <input
              id="contact-fax-number"
              type="tel"
              value={faxNumber}
              onChange={(e) => setFaxNumber(e.target.value)}
              required
              className={INPUT_CLS}
              placeholder="+1 (555) 123-4567"
            />
          </div>

          <div>
            <label htmlFor="contact-phone-number" className={LABEL_CLS}>Phone Number</label>
            <input
              id="contact-phone-number"
              type="tel"
              value={phoneNumber}
              onChange={(e) => setPhoneNumber(e.target.value)}
              className={INPUT_CLS}
              placeholder="+1 (555) 987-6543"
            />
          </div>

          <div>
            <label htmlFor="contact-type" className={LABEL_CLS}>
              Type <span className="text-red-500">*</span>
            </label>
            <select
              id="contact-type"
              value={contactType}
              onChange={(e) => setContactType(e.target.value as FaxContactType)}
              className={INPUT_CLS}
            >
              {CONTACT_TYPE_VALUES.map((t) => (
                <option key={t} value={t}>
                  {t === "referring_md" ? "Referring MD" : t.charAt(0).toUpperCase() + t.slice(1)}
                </option>
              ))}
            </select>
          </div>

          <div>
            <label htmlFor="contact-notes" className={LABEL_CLS}>Notes</label>
            <textarea
              id="contact-notes"
              value={notes}
              onChange={(e) => setNotes(e.target.value)}
              rows={2}
              className={INPUT_CLS}
              placeholder="Optional notes..."
            />
          </div>

          {submitError && (
            <div className="rounded-md border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700">
              {submitError}
            </div>
          )}

          <div className="flex justify-end gap-3 pt-2">
            <button
              type="button"
              onClick={onClose}
              disabled={submitting}
              className="rounded-md border border-gray-300 bg-white px-4 py-2 text-sm font-medium text-gray-700 shadow-sm transition-colors hover:bg-gray-50"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={submitting || !name.trim() || !faxNumber.trim()}
              className="rounded-md bg-blue-600 px-4 py-2 text-sm font-medium text-white shadow-sm transition-colors hover:bg-blue-700 disabled:cursor-not-allowed disabled:opacity-50"
            >
              {submitting ? "Saving..." : initial ? "Update" : "Add"}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}

// ─── FaxPage ─────────────────────────────────────────────────────────────────

export function FaxPage() {
  // ── Tab state ────────────────────────────────────────────────────────────────
  const [activeTab, setActiveTab] = useState<Tab>("inbox");

  // ── Inbox state ──────────────────────────────────────────────────────────────
  const [inboxFaxes, setInboxFaxes] = useState<FaxRecord[]>([]);
  const [inboxLoading, setInboxLoading] = useState(false);
  const [inboxError, setInboxError] = useState<string | null>(null);

  // ── Contacts state ───────────────────────────────────────────────────────────
  const [contacts, setContacts] = useState<FaxContact[]>([]);
  const [contactsLoading, setContactsLoading] = useState(false);
  const [contactsError, setContactsError] = useState<string | null>(null);
  const [contactTypeFilter, setContactTypeFilter] = useState<FaxContactType | "">("");
  const [contactModal, setContactModal] = useState<{ open: boolean; editing: FaxContact | null }>({
    open: false,
    editing: null,
  });
  const [contactsReloadKey, setContactsReloadKey] = useState(0);

  // ── Log state ────────────────────────────────────────────────────────────────
  const [logEntries, setLogEntries] = useState<FaxRecord[]>([]);
  const [logLoading, setLogLoading] = useState(false);
  const [logError, setLogError] = useState<string | null>(null);
  const [logDirectionFilter, setLogDirectionFilter] = useState<FaxDirection | "">("");
  const [logStatusFilter, setLogStatusFilter] = useState<FaxStatus | "">("");
  const [retryingId, setRetryingId] = useState<string | null>(null);
  const [logReloadKey, setLogReloadKey] = useState(0);

  // ── Inbox: fetch received faxes on mount ─────────────────────────────────────
  useEffect(() => {
    let mounted = true;

    async function loadInbox() {
      setInboxLoading(true);
      setInboxError(null);
      try {
        const faxes = await commands.pollReceivedFaxes();
        if (mounted) setInboxFaxes(faxes);
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e);
        if (mounted) setInboxError(msg);
        console.error("[FaxPage] inbox fetch failed:", msg);
      } finally {
        if (mounted) setInboxLoading(false);
      }
    }

    loadInbox();
    return () => {
      mounted = false;
    };
  }, []);

  // ── Contacts: fetch on mount and after CRUD ──────────────────────────────────
  useEffect(() => {
    let mounted = true;

    async function loadContacts() {
      setContactsLoading(true);
      setContactsError(null);
      try {
        const list = await commands.listFaxContacts(contactTypeFilter || null);
        if (mounted) setContacts(list);
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e);
        if (mounted) setContactsError(msg);
        console.error("[FaxPage] contacts fetch failed:", msg);
      } finally {
        if (mounted) setContactsLoading(false);
      }
    }

    loadContacts();
    return () => {
      mounted = false;
    };
  }, [contactTypeFilter, contactsReloadKey]);

  // ── Log: fetch on mount and after filter changes ─────────────────────────────
  useEffect(() => {
    let mounted = true;

    async function loadLog() {
      setLogLoading(true);
      setLogError(null);
      try {
        const entries = await commands.listFaxLog(
          null,
          logDirectionFilter || null,
          logStatusFilter || null,
        );
        if (mounted) setLogEntries(entries);
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e);
        if (mounted) setLogError(msg);
        console.error("[FaxPage] log fetch failed:", msg);
      } finally {
        if (mounted) setLogLoading(false);
      }
    }

    loadLog();
    return () => {
      mounted = false;
    };
  }, [logDirectionFilter, logStatusFilter, logReloadKey]);

  // ── Inbox: unlinked count ────────────────────────────────────────────────────
  const unlinkedCount = inboxFaxes.filter((f) => f.patientId === null).length;

  // ── Contact handlers ─────────────────────────────────────────────────────────

  const handleSaveContact = useCallback(
    async (input: FaxContactInput) => {
      if (contactModal.editing) {
        await commands.updateFaxContact(contactModal.editing.contactId, input);
      } else {
        await commands.createFaxContact(input);
      }
      setContactsReloadKey((k) => k + 1);
    },
    [contactModal.editing],
  );

  const handleDeleteContact = useCallback(async (contactId: string) => {
    if (!window.confirm("Delete this contact? This action cannot be undone.")) return;
    try {
      await commands.deleteFaxContact(contactId);
      setContactsReloadKey((k) => k + 1);
    } catch (e) {
      console.error("[FaxPage] delete contact failed:", e instanceof Error ? e.message : String(e));
    }
  }, []);

  // ── Log handlers ─────────────────────────────────────────────────────────────

  const handleRetry = useCallback(async (faxId: string) => {
    setRetryingId(faxId);
    try {
      await commands.retryFax(faxId);
      setLogReloadKey((k) => k + 1);
    } catch (e) {
      console.error("[FaxPage] retry fax failed:", e instanceof Error ? e.message : String(e));
    } finally {
      setRetryingId(null);
    }
  }, []);

  // ── Tab definitions ──────────────────────────────────────────────────────────

  const tabs: { id: Tab; label: string; badge?: number }[] = [
    { id: "inbox", label: "Inbox", badge: unlinkedCount > 0 ? unlinkedCount : undefined },
    { id: "contacts", label: "Contacts" },
    { id: "log", label: "Log" },
  ];

  // ── Render ───────────────────────────────────────────────────────────────────

  return (
    <div className="flex h-full flex-col p-6">
      {/* Header */}
      <div className="mb-6">
        <h1 className="text-2xl font-bold text-gray-900">Fax</h1>
        <p className="mt-1 text-sm text-gray-500">
          Send and receive faxes, manage contacts, and view fax history.
        </p>
      </div>

      {/* Tab bar */}
      <div className="mb-6 border-b border-gray-200">
        <nav className="-mb-px flex gap-6" role="tablist" aria-label="Fax sections">
          {tabs.map((tab) => (
            <button
              key={tab.id}
              type="button"
              role="tab"
              aria-selected={activeTab === tab.id}
              onClick={() => setActiveTab(tab.id)}
              className={[
                "relative whitespace-nowrap border-b-2 pb-3 text-sm font-medium transition-colors",
                activeTab === tab.id
                  ? "border-blue-600 text-blue-600"
                  : "border-transparent text-gray-500 hover:border-gray-300 hover:text-gray-700",
              ].join(" ")}
            >
              {tab.label}
              {tab.badge !== undefined && (
                <span className="ml-2 inline-flex h-5 min-w-[20px] items-center justify-center rounded-full bg-red-500 px-1.5 text-xs font-semibold text-white">
                  {tab.badge}
                </span>
              )}
            </button>
          ))}
        </nav>
      </div>

      {/* Tab content */}
      <div className="flex-1 overflow-y-auto">

        {/* ─── INBOX TAB ─────────────────────────────────────────────────────── */}
        {activeTab === "inbox" && (
          <div className="space-y-4 max-w-4xl">
            {inboxError && (
              <div className="rounded-md border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700">
                {inboxError}
              </div>
            )}

            {inboxLoading ? (
              <p className="text-sm text-gray-500">Loading received faxes...</p>
            ) : inboxFaxes.length === 0 ? (
              <div className="rounded-lg border border-gray-200 bg-white p-8 text-center">
                <p className="text-sm text-gray-500">No received faxes.</p>
              </div>
            ) : (
              <div className="overflow-x-auto rounded-lg border border-gray-200 bg-white shadow-sm">
                <table className="min-w-full divide-y divide-gray-200 text-sm">
                  <thead>
                    <tr>
                      {["Recipient/Sender", "Direction", "Sent At", "Pages", "Status", "Patient"].map(
                        (h) => (
                          <th
                            key={h}
                            className="whitespace-nowrap px-4 py-3 text-left text-xs font-semibold uppercase tracking-wide text-gray-500"
                          >
                            {h}
                          </th>
                        ),
                      )}
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-gray-100">
                    {inboxFaxes.map((fax) => (
                      <tr key={fax.faxId} className="align-top">
                        <td className="px-4 py-3 font-medium text-gray-900">{fax.recipientName ?? "--"}</td>
                        <td className="px-4 py-3 text-gray-600">{fax.direction}</td>
                        <td className="whitespace-nowrap px-4 py-3 text-gray-600">
                          {formatDate(fax.sentAt)}
                        </td>
                        <td className="px-4 py-3 text-gray-600">{fax.pages ?? "--"}</td>
                        <td className="px-4 py-3">{faxStatusBadge(fax.status)}</td>
                        <td className="px-4 py-3 text-gray-600">
                          {fax.patientId ? (
                            <span className="text-green-700">Linked</span>
                          ) : (
                            <span className="text-gray-400">Unlinked</span>
                          )}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
          </div>
        )}

        {/* ─── CONTACTS TAB ──────────────────────────────────────────────────── */}
        {activeTab === "contacts" && (
          <div className="space-y-4 max-w-4xl">
            {/* Toolbar */}
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-3">
                <label className="text-sm font-medium text-gray-700">Filter:</label>
                <select
                  value={contactTypeFilter}
                  onChange={(e) =>
                    setContactTypeFilter(e.target.value as FaxContactType | "")
                  }
                  className="rounded-md border border-gray-300 px-3 py-1.5 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
                >
                  {CONTACT_TYPE_OPTIONS.map((opt) => (
                    <option key={opt.value} value={opt.value}>
                      {opt.label}
                    </option>
                  ))}
                </select>
              </div>
              <button
                type="button"
                onClick={() => setContactModal({ open: true, editing: null })}
                className="rounded-md bg-blue-600 px-4 py-2 text-sm font-medium text-white shadow-sm transition-colors hover:bg-blue-700"
              >
                Add Contact
              </button>
            </div>

            {contactsError && (
              <div className="rounded-md border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700">
                {contactsError}
              </div>
            )}

            {contactsLoading ? (
              <p className="text-sm text-gray-500">Loading contacts...</p>
            ) : contacts.length === 0 ? (
              <div className="rounded-lg border border-gray-200 bg-white p-8 text-center">
                <p className="text-sm text-gray-500">
                  {contactTypeFilter
                    ? "No contacts match this filter."
                    : "No fax contacts yet. Click \"Add Contact\" to create one."}
                </p>
              </div>
            ) : (
              <div className="overflow-x-auto rounded-lg border border-gray-200 bg-white shadow-sm">
                <table className="min-w-full divide-y divide-gray-200 text-sm">
                  <thead>
                    <tr>
                      {["Name", "Organization", "Fax #", "Phone #", "Type", "Actions"].map((h) => (
                        <th
                          key={h}
                          className="whitespace-nowrap px-4 py-3 text-left text-xs font-semibold uppercase tracking-wide text-gray-500"
                        >
                          {h}
                        </th>
                      ))}
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-gray-100">
                    {contacts.map((contact) => (
                      <tr key={contact.contactId} className="align-top">
                        <td className="px-4 py-3 font-medium text-gray-900">{contact.name}</td>
                        <td className="px-4 py-3 text-gray-600">{contact.organization ?? "--"}</td>
                        <td className="whitespace-nowrap px-4 py-3 font-mono text-gray-700">
                          {contact.faxNumber}
                        </td>
                        <td className="whitespace-nowrap px-4 py-3 text-gray-600">
                          {contact.phoneNumber ?? "--"}
                        </td>
                        <td className="px-4 py-3">{contactTypeBadge(contact.contactType)}</td>
                        <td className="px-4 py-3">
                          <div className="flex gap-2">
                            <button
                              type="button"
                              onClick={() =>
                                setContactModal({ open: true, editing: contact })
                              }
                              aria-label={`Edit contact ${contact.name}`}
                              className="text-xs font-medium text-blue-600 hover:text-blue-800"
                            >
                              Edit
                            </button>
                            <button
                              type="button"
                              onClick={() => handleDeleteContact(contact.contactId)}
                              aria-label={`Delete contact ${contact.name}`}
                              className="text-xs font-medium text-red-600 hover:text-red-800"
                            >
                              Delete
                            </button>
                          </div>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}

            {/* Contact form modal */}
            {contactModal.open && (
              <ContactFormModal
                initial={contactModal.editing}
                onSave={handleSaveContact}
                onClose={() => setContactModal({ open: false, editing: null })}
              />
            )}
          </div>
        )}

        {/* ─── LOG TAB ───────────────────────────────────────────────────────── */}
        {activeTab === "log" && (
          <div className="space-y-4 max-w-5xl">
            {/* Filters */}
            <div className="flex items-center gap-4">
              <div className="flex items-center gap-2">
                <label className="text-sm font-medium text-gray-700">Direction:</label>
                <select
                  value={logDirectionFilter}
                  onChange={(e) =>
                    setLogDirectionFilter(e.target.value as FaxDirection | "")
                  }
                  className="rounded-md border border-gray-300 px-3 py-1.5 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
                >
                  <option value="">All</option>
                  <option value="sent">Sent</option>
                  <option value="received">Received</option>
                </select>
              </div>
              <div className="flex items-center gap-2">
                <label className="text-sm font-medium text-gray-700">Status:</label>
                <select
                  value={logStatusFilter}
                  onChange={(e) =>
                    setLogStatusFilter(e.target.value as FaxStatus | "")
                  }
                  className="rounded-md border border-gray-300 px-3 py-1.5 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
                >
                  <option value="">All</option>
                  <option value="success">Success</option>
                  <option value="failed">Failed</option>
                  <option value="queued">Queued</option>
                  <option value="in_progress">In Progress</option>
                </select>
              </div>
            </div>

            {logError && (
              <div className="rounded-md border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700">
                {logError}
              </div>
            )}

            {logLoading ? (
              <p className="text-sm text-gray-500">Loading fax log...</p>
            ) : logEntries.length === 0 ? (
              <div className="rounded-lg border border-gray-200 bg-white p-8 text-center">
                <p className="text-sm text-gray-500">No fax log entries found.</p>
              </div>
            ) : (
              <div className="overflow-x-auto rounded-lg border border-gray-200 bg-white shadow-sm">
                <table className="min-w-full divide-y divide-gray-200 text-sm">
                  <thead>
                    <tr>
                      {["Direction", "Recipient", "Fax #", "Document", "Date", "Pages", "Status", "Actions"].map(
                        (h) => (
                          <th
                            key={h}
                            className="whitespace-nowrap px-4 py-3 text-left text-xs font-semibold uppercase tracking-wide text-gray-500"
                          >
                            {h}
                          </th>
                        ),
                      )}
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-gray-100">
                    {logEntries.map((entry) => (
                      <tr key={entry.faxId} className="align-top">
                        <td className="px-4 py-3">
                          <span
                            className={[
                              "inline-flex items-center rounded-full px-2 py-0.5 text-xs font-medium",
                              entry.direction === "sent"
                                ? "bg-orange-100 text-orange-700"
                                : "bg-teal-100 text-teal-700",
                            ].join(" ")}
                          >
                            {entry.direction === "sent" ? "Sent" : "Recv"}
                          </span>
                        </td>
                        <td className="px-4 py-3 font-medium text-gray-900">
                          {entry.recipientName ?? "--"}
                        </td>
                        <td className="whitespace-nowrap px-4 py-3 font-mono text-gray-600">
                          {entry.recipientFax ?? "--"}
                        </td>
                        <td className="px-4 py-3 text-gray-600">
                          {entry.documentName ?? "--"}
                        </td>
                        <td className="whitespace-nowrap px-4 py-3 text-gray-600">
                          {formatDate(entry.sentAt)}
                        </td>
                        <td className="px-4 py-3 text-gray-600">{entry.pages ?? "--"}</td>
                        <td className="px-4 py-3">{faxStatusBadge(entry.status)}</td>
                        <td className="px-4 py-3">
                          {entry.status === "failed" ? (
                            <button
                              type="button"
                              onClick={() => handleRetry(entry.faxId)}
                              disabled={retryingId === entry.faxId}
                              aria-label={`Retry fax to ${entry.recipientName ?? entry.recipientFax ?? "recipient"}`}
                              className="rounded bg-red-50 px-2 py-1 text-xs font-medium text-red-700 transition-colors hover:bg-red-100 disabled:cursor-not-allowed disabled:opacity-50"
                            >
                              {retryingId === entry.faxId ? "Retrying..." : "Retry"}
                            </button>
                          ) : (
                            <span className="text-xs text-gray-400">--</span>
                          )}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
}

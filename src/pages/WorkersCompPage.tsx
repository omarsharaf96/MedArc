/**
 * WorkersCompPage.tsx — Workers' Compensation Module UI (M003/S02)
 *
 * Case list with status filter + case detail with 4 tabs:
 *   Overview, Contacts, Communications, Impairment
 *
 * Route: { page: "workers-comp"; patientId?: string; caseId?: string }
 */
import { useState, useEffect, useCallback } from "react";
import { commands } from "../lib/tauri";
import type {
  WcCaseRecord,
  WcCaseInput,
  WcCaseStatus,
  WcContactRecord,
  WcContactInput,
  WcContactRole,
  ImpairmentRatingRecord,
  ImpairmentRatingInput,
  AmaGuidesEdition,
  WcCommunicationRecord,
  WcCommunicationInput,
  WcCommDirection,
  WcCommMethod,
  FroiResult,
  WcFeeResult,
} from "../types/workers-comp";

// ─── Props ───────────────────────────────────────────────────────────────────

interface Props {
  patientId?: string;
  caseId?: string;
  role: string;
}

// ─── Tailwind helpers ────────────────────────────────────────────────────────

const BTN_PRIMARY =
  "rounded-md bg-blue-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-blue-700 disabled:opacity-50";
const BTN_SECONDARY =
  "rounded-md border border-gray-300 bg-white px-3 py-1.5 text-sm font-medium text-gray-700 hover:bg-gray-50 disabled:opacity-50";
const INPUT_CLS =
  "w-full rounded-md border border-gray-300 px-3 py-1.5 text-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500";
const LABEL_CLS = "mb-1 block text-xs font-medium text-gray-600";
const SELECT_CLS =
  "w-full rounded-md border border-gray-300 bg-white px-3 py-1.5 text-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500";

// ─── Status badge ─────────────────────────────────────────────────────────────

const STATUS_COLORS: Record<WcCaseStatus, string> = {
  open: "bg-green-100 text-green-700",
  closed: "bg-gray-100 text-gray-600",
  settled: "bg-blue-100 text-blue-700",
  disputed: "bg-red-100 text-red-700",
};

function StatusBadge({ status }: { status: WcCaseStatus }) {
  const cls = STATUS_COLORS[status] ?? "bg-gray-100 text-gray-600";
  return (
    <span
      className={`inline-flex items-center rounded-full px-2.5 py-0.5 text-xs font-medium capitalize ${cls}`}
    >
      {status}
    </span>
  );
}

// ─── Direction icon ───────────────────────────────────────────────────────────

function DirectionIcon({ direction }: { direction: WcCommDirection }) {
  return (
    <span
      className={`text-xs font-bold ${direction === "inbound" ? "text-green-600" : "text-blue-600"}`}
    >
      {direction === "inbound" ? "IN" : "OUT"}
    </span>
  );
}

// ─── Method badge ─────────────────────────────────────────────────────────────

const METHOD_COLORS: Record<WcCommMethod, string> = {
  phone: "bg-purple-100 text-purple-700",
  email: "bg-blue-100 text-blue-700",
  fax: "bg-orange-100 text-orange-700",
  letter: "bg-yellow-100 text-yellow-700",
  in_person: "bg-green-100 text-green-700",
};

function MethodBadge({ method }: { method: WcCommMethod }) {
  const cls = METHOD_COLORS[method] ?? "bg-gray-100 text-gray-600";
  return (
    <span
      className={`inline-flex items-center rounded-full px-2 py-0.5 text-xs font-medium ${cls}`}
    >
      {method.replace("_", " ")}
    </span>
  );
}

// ─── New Case Modal ───────────────────────────────────────────────────────────

interface NewCaseModalProps {
  initialPatientId?: string;
  onClose: () => void;
  onCreated: (c: WcCaseRecord) => void;
}

function NewCaseModal({ initialPatientId, onClose, onCreated }: NewCaseModalProps) {
  const [form, setForm] = useState<WcCaseInput>({
    patientId: initialPatientId ?? "",
    employerName: "",
    employerContact: null,
    injuryDate: "",
    injuryDescription: null,
    bodyParts: null,
    claimNumber: null,
    state: "",
    status: "open",
    mmiDate: null,
  });
  const [bodyPartsText, setBodyPartsText] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    setLoading(true);
    setError(null);
    try {
      const input: WcCaseInput = {
        ...form,
        bodyParts: bodyPartsText.trim()
          ? bodyPartsText.split(",").map((s) => s.trim()).filter(Boolean)
          : null,
      };
      const created = await commands.createWcCase(input);
      onCreated(created);
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
      <div className="w-full max-w-lg rounded-xl bg-white p-6 shadow-xl">
        <h2 className="mb-4 text-lg font-semibold text-gray-900">New Workers' Comp Case</h2>
        <form onSubmit={handleSubmit} className="space-y-3">
          <div>
            <label className={LABEL_CLS}>Patient ID *</label>
            <input
              className={INPUT_CLS}
              value={form.patientId}
              onChange={(e) => setForm({ ...form, patientId: e.target.value })}
              required
              placeholder="Patient FHIR ID"
            />
          </div>
          <div className="grid grid-cols-2 gap-3">
            <div>
              <label className={LABEL_CLS}>Employer Name *</label>
              <input
                className={INPUT_CLS}
                value={form.employerName}
                onChange={(e) => setForm({ ...form, employerName: e.target.value })}
                required
                placeholder="Acme Corp"
              />
            </div>
            <div>
              <label className={LABEL_CLS}>Employer Contact</label>
              <input
                className={INPUT_CLS}
                value={form.employerContact ?? ""}
                onChange={(e) =>
                  setForm({ ...form, employerContact: e.target.value || null })
                }
                placeholder="HR name / phone"
              />
            </div>
          </div>
          <div className="grid grid-cols-2 gap-3">
            <div>
              <label className={LABEL_CLS}>Injury Date *</label>
              <input
                type="date"
                className={INPUT_CLS}
                value={form.injuryDate}
                onChange={(e) => setForm({ ...form, injuryDate: e.target.value })}
                required
              />
            </div>
            <div>
              <label className={LABEL_CLS}>State *</label>
              <input
                className={INPUT_CLS}
                value={form.state}
                onChange={(e) => setForm({ ...form, state: e.target.value.toUpperCase() })}
                required
                maxLength={2}
                placeholder="CA"
              />
            </div>
          </div>
          <div>
            <label className={LABEL_CLS}>Injury Description</label>
            <textarea
              className={`${INPUT_CLS} h-16 resize-none`}
              value={form.injuryDescription ?? ""}
              onChange={(e) =>
                setForm({ ...form, injuryDescription: e.target.value || null })
              }
              placeholder="Describe the injury..."
            />
          </div>
          <div>
            <label className={LABEL_CLS}>Body Parts (comma-separated)</label>
            <input
              className={INPUT_CLS}
              value={bodyPartsText}
              onChange={(e) => setBodyPartsText(e.target.value)}
              placeholder="lumbar_spine, left_shoulder"
            />
          </div>
          <div className="grid grid-cols-2 gap-3">
            <div>
              <label className={LABEL_CLS}>Claim Number</label>
              <input
                className={INPUT_CLS}
                value={form.claimNumber ?? ""}
                onChange={(e) =>
                  setForm({ ...form, claimNumber: e.target.value || null })
                }
                placeholder="WC-2026-001"
              />
            </div>
            <div>
              <label className={LABEL_CLS}>Status</label>
              <select
                className={SELECT_CLS}
                value={form.status ?? "open"}
                onChange={(e) =>
                  setForm({ ...form, status: e.target.value as WcCaseStatus })
                }
              >
                <option value="open">Open</option>
                <option value="closed">Closed</option>
                <option value="settled">Settled</option>
                <option value="disputed">Disputed</option>
              </select>
            </div>
          </div>
          {error && (
            <p className="rounded-md bg-red-50 px-3 py-2 text-sm text-red-700">{error}</p>
          )}
          <div className="flex justify-end gap-2 pt-2">
            <button type="button" className={BTN_SECONDARY} onClick={onClose}>
              Cancel
            </button>
            <button type="submit" className={BTN_PRIMARY} disabled={loading}>
              {loading ? "Creating..." : "Create Case"}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}

// ─── Add Contact Modal ────────────────────────────────────────────────────────

interface AddContactModalProps {
  caseId: string;
  onClose: () => void;
  onAdded: (c: WcContactRecord) => void;
}

function AddContactModal({ caseId, onClose, onAdded }: AddContactModalProps) {
  const [form, setForm] = useState<WcContactInput>({
    role: "adjuster",
    name: "",
    company: null,
    phone: null,
    email: null,
    fax: null,
    notes: null,
  });
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    setLoading(true);
    setError(null);
    try {
      const contact = await commands.addWcContact(caseId, form);
      onAdded(contact);
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
      <div className="w-full max-w-md rounded-xl bg-white p-6 shadow-xl">
        <h2 className="mb-4 text-lg font-semibold text-gray-900">Add Contact</h2>
        <form onSubmit={handleSubmit} className="space-y-3">
          <div className="grid grid-cols-2 gap-3">
            <div>
              <label className={LABEL_CLS}>Role *</label>
              <select
                className={SELECT_CLS}
                value={form.role}
                onChange={(e) => setForm({ ...form, role: e.target.value as WcContactRole })}
              >
                <option value="adjuster">Adjuster</option>
                <option value="attorney">Attorney</option>
                <option value="nurse_case_manager">Nurse Case Manager</option>
                <option value="employer_rep">Employer Rep</option>
              </select>
            </div>
            <div>
              <label className={LABEL_CLS}>Name *</label>
              <input
                className={INPUT_CLS}
                value={form.name}
                onChange={(e) => setForm({ ...form, name: e.target.value })}
                required
                placeholder="Full name"
              />
            </div>
          </div>
          <div>
            <label className={LABEL_CLS}>Company</label>
            <input
              className={INPUT_CLS}
              value={form.company ?? ""}
              onChange={(e) => setForm({ ...form, company: e.target.value || null })}
              placeholder="Company name"
            />
          </div>
          <div className="grid grid-cols-3 gap-2">
            <div>
              <label className={LABEL_CLS}>Phone</label>
              <input
                className={INPUT_CLS}
                value={form.phone ?? ""}
                onChange={(e) => setForm({ ...form, phone: e.target.value || null })}
                placeholder="555-1234"
              />
            </div>
            <div>
              <label className={LABEL_CLS}>Email</label>
              <input
                type="email"
                className={INPUT_CLS}
                value={form.email ?? ""}
                onChange={(e) => setForm({ ...form, email: e.target.value || null })}
                placeholder="email@example.com"
              />
            </div>
            <div>
              <label className={LABEL_CLS}>Fax</label>
              <input
                className={INPUT_CLS}
                value={form.fax ?? ""}
                onChange={(e) => setForm({ ...form, fax: e.target.value || null })}
                placeholder="555-9999"
              />
            </div>
          </div>
          <div>
            <label className={LABEL_CLS}>Notes</label>
            <textarea
              className={`${INPUT_CLS} h-16 resize-none`}
              value={form.notes ?? ""}
              onChange={(e) => setForm({ ...form, notes: e.target.value || null })}
            />
          </div>
          {error && (
            <p className="rounded-md bg-red-50 px-3 py-2 text-sm text-red-700">{error}</p>
          )}
          <div className="flex justify-end gap-2 pt-2">
            <button type="button" className={BTN_SECONDARY} onClick={onClose}>
              Cancel
            </button>
            <button type="submit" className={BTN_PRIMARY} disabled={loading}>
              {loading ? "Adding..." : "Add Contact"}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}

// ─── Log Communication Modal ──────────────────────────────────────────────────

interface LogCommModalProps {
  caseId: string;
  contacts: WcContactRecord[];
  onClose: () => void;
  onLogged: (c: WcCommunicationRecord) => void;
}

function LogCommModal({ caseId, contacts, onClose, onLogged }: LogCommModalProps) {
  const [form, setForm] = useState<WcCommunicationInput>({
    contactId: null,
    direction: "outbound",
    method: "phone",
    subject: null,
    content: null,
    commDate: null,
  });
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    setLoading(true);
    setError(null);
    try {
      const record = await commands.logWcCommunication(caseId, form);
      onLogged(record);
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
      <div className="w-full max-w-md rounded-xl bg-white p-6 shadow-xl">
        <h2 className="mb-4 text-lg font-semibold text-gray-900">Log Communication</h2>
        <form onSubmit={handleSubmit} className="space-y-3">
          <div className="grid grid-cols-2 gap-3">
            <div>
              <label className={LABEL_CLS}>Direction *</label>
              <select
                className={SELECT_CLS}
                value={form.direction}
                onChange={(e) =>
                  setForm({ ...form, direction: e.target.value as WcCommDirection })
                }
              >
                <option value="outbound">Outbound</option>
                <option value="inbound">Inbound</option>
              </select>
            </div>
            <div>
              <label className={LABEL_CLS}>Method *</label>
              <select
                className={SELECT_CLS}
                value={form.method}
                onChange={(e) =>
                  setForm({ ...form, method: e.target.value as WcCommMethod })
                }
              >
                <option value="phone">Phone</option>
                <option value="email">Email</option>
                <option value="fax">Fax</option>
                <option value="letter">Letter</option>
                <option value="in_person">In Person</option>
              </select>
            </div>
          </div>
          <div>
            <label className={LABEL_CLS}>Contact</label>
            <select
              className={SELECT_CLS}
              value={form.contactId ?? ""}
              onChange={(e) =>
                setForm({ ...form, contactId: e.target.value || null })
              }
            >
              <option value="">None / unknown</option>
              {contacts.map((c) => (
                <option key={c.contactId} value={c.contactId}>
                  {c.name} ({c.role.replace("_", " ")})
                </option>
              ))}
            </select>
          </div>
          <div>
            <label className={LABEL_CLS}>Subject</label>
            <input
              className={INPUT_CLS}
              value={form.subject ?? ""}
              onChange={(e) => setForm({ ...form, subject: e.target.value || null })}
              placeholder="Brief subject"
            />
          </div>
          <div>
            <label className={LABEL_CLS}>Content / Notes</label>
            <textarea
              className={`${INPUT_CLS} h-20 resize-none`}
              value={form.content ?? ""}
              onChange={(e) => setForm({ ...form, content: e.target.value || null })}
            />
          </div>
          {error && (
            <p className="rounded-md bg-red-50 px-3 py-2 text-sm text-red-700">{error}</p>
          )}
          <div className="flex justify-end gap-2 pt-2">
            <button type="button" className={BTN_SECONDARY} onClick={onClose}>
              Cancel
            </button>
            <button type="submit" className={BTN_PRIMARY} disabled={loading}>
              {loading ? "Logging..." : "Log Communication"}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}

// ─── Add Rating Modal ─────────────────────────────────────────────────────────

interface AddRatingModalProps {
  caseId: string;
  onClose: () => void;
  onAdded: (r: ImpairmentRatingRecord) => void;
}

function AddRatingModal({ caseId, onClose, onAdded }: AddRatingModalProps) {
  const [form, setForm] = useState<ImpairmentRatingInput>({
    bodyPart: "",
    amaGuidesEdition: "6th",
    impairmentClass: null,
    gradeModifier: null,
    wholePersonPct: 0,
    evaluator: null,
    evaluationDate: null,
  });
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    setLoading(true);
    setError(null);
    try {
      const record = await commands.recordImpairmentRating(caseId, form);
      onAdded(record);
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
      <div className="w-full max-w-md rounded-xl bg-white p-6 shadow-xl">
        <h2 className="mb-4 text-lg font-semibold text-gray-900">Add Impairment Rating</h2>
        <form onSubmit={handleSubmit} className="space-y-3">
          <div className="grid grid-cols-2 gap-3">
            <div>
              <label className={LABEL_CLS}>Body Part *</label>
              <input
                className={INPUT_CLS}
                value={form.bodyPart}
                onChange={(e) => setForm({ ...form, bodyPart: e.target.value })}
                required
                placeholder="lumbar_spine"
              />
            </div>
            <div>
              <label className={LABEL_CLS}>AMA Guides Edition</label>
              <select
                className={SELECT_CLS}
                value={form.amaGuidesEdition ?? "6th"}
                onChange={(e) =>
                  setForm({ ...form, amaGuidesEdition: e.target.value as AmaGuidesEdition })
                }
              >
                <option value="6th">6th</option>
                <option value="5th">5th</option>
                <option value="4th">4th</option>
                <option value="3rd_rev">3rd Rev.</option>
              </select>
            </div>
          </div>
          <div className="grid grid-cols-2 gap-3">
            <div>
              <label className={LABEL_CLS}>Impairment Class</label>
              <input
                className={INPUT_CLS}
                value={form.impairmentClass ?? ""}
                onChange={(e) =>
                  setForm({ ...form, impairmentClass: e.target.value || null })
                }
                placeholder="e.g. Class 2"
              />
            </div>
            <div>
              <label className={LABEL_CLS}>Grade Modifier</label>
              <input
                className={INPUT_CLS}
                value={form.gradeModifier ?? ""}
                onChange={(e) =>
                  setForm({ ...form, gradeModifier: e.target.value || null })
                }
                placeholder="e.g. Grade C"
              />
            </div>
          </div>
          <div>
            <label className={LABEL_CLS}>Whole Person Impairment % (0–100) *</label>
            <input
              type="number"
              min={0}
              max={100}
              step={0.5}
              className={INPUT_CLS}
              value={form.wholePersonPct}
              onChange={(e) =>
                setForm({ ...form, wholePersonPct: parseFloat(e.target.value) || 0 })
              }
              required
            />
          </div>
          <div className="grid grid-cols-2 gap-3">
            <div>
              <label className={LABEL_CLS}>Evaluator</label>
              <input
                className={INPUT_CLS}
                value={form.evaluator ?? ""}
                onChange={(e) =>
                  setForm({ ...form, evaluator: e.target.value || null })
                }
                placeholder="Dr. Smith"
              />
            </div>
            <div>
              <label className={LABEL_CLS}>Evaluation Date</label>
              <input
                type="date"
                className={INPUT_CLS}
                value={form.evaluationDate ?? ""}
                onChange={(e) =>
                  setForm({ ...form, evaluationDate: e.target.value || null })
                }
              />
            </div>
          </div>
          {error && (
            <p className="rounded-md bg-red-50 px-3 py-2 text-sm text-red-700">{error}</p>
          )}
          <div className="flex justify-end gap-2 pt-2">
            <button type="button" className={BTN_SECONDARY} onClick={onClose}>
              Cancel
            </button>
            <button type="submit" className={BTN_PRIMARY} disabled={loading}>
              {loading ? "Saving..." : "Add Rating"}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}

// ─── Case Detail ──────────────────────────────────────────────────────────────

type DetailTab = "overview" | "contacts" | "communications" | "impairment";

interface CaseDetailProps {
  wcase: WcCaseRecord;
  role: string;
  onBack: () => void;
}

function CaseDetail({ wcase, role, onBack }: CaseDetailProps) {
  const [tab, setTab] = useState<DetailTab>("overview");
  const [contacts, setContacts] = useState<WcContactRecord[]>([]);
  const [comms, setComms] = useState<WcCommunicationRecord[]>([]);
  const [ratings, setRatings] = useState<ImpairmentRatingRecord[]>([]);
  const [showAddContact, setShowAddContact] = useState(false);
  const [showLogComm, setShowLogComm] = useState(false);
  const [showAddRating, setShowAddRating] = useState(false);
  const [froi, setFroi] = useState<FroiResult | null>(null);
  const [froiLoading, setFroiLoading] = useState(false);
  const [feeState, setFeeState] = useState(wcase.state);
  const [feeCpt, setFeeCpt] = useState("97110");
  const [feeResult, setFeeResult] = useState<WcFeeResult | null>(null);
  const [feeError, setFeeError] = useState<string | null>(null);

  const canWrite = !["FrontDesk"].includes(role);

  const loadContacts = useCallback(async () => {
    try {
      const data = await commands.listWcContacts(wcase.caseId);
      setContacts(data);
    } catch {
      // ignore
    }
  }, [wcase.caseId]);

  const loadComms = useCallback(async () => {
    try {
      const data = await commands.listWcCommunications(wcase.caseId);
      setComms(data);
    } catch {
      // ignore
    }
  }, [wcase.caseId]);

  const loadRatings = useCallback(async () => {
    try {
      const data = await commands.listImpairmentRatings(wcase.caseId);
      setRatings(data);
    } catch {
      // ignore
    }
  }, [wcase.caseId]);

  useEffect(() => {
    loadContacts();
    loadComms();
    loadRatings();
  }, [loadContacts, loadComms, loadRatings]);

  async function handleGenerateFroi() {
    setFroiLoading(true);
    try {
      const result = await commands.generateFroi(wcase.caseId);
      setFroi(result);
    } catch (err) {
      alert(String(err));
    } finally {
      setFroiLoading(false);
    }
  }

  async function handleFeeLookup() {
    setFeeError(null);
    setFeeResult(null);
    try {
      const result = await commands.lookupWcFee(feeState, feeCpt);
      setFeeResult(result);
    } catch (err) {
      setFeeError(String(err));
    }
  }

  const TABS: { id: DetailTab; label: string }[] = [
    { id: "overview", label: "Overview" },
    { id: "contacts", label: `Contacts (${contacts.length})` },
    { id: "communications", label: `Communications (${comms.length})` },
    { id: "impairment", label: `Impairment (${ratings.length})` },
  ];

  return (
    <div className="flex h-full flex-col">
      {/* Header */}
      <div className="flex items-center gap-3 border-b border-gray-200 px-6 py-4">
        <button
          type="button"
          className={BTN_SECONDARY}
          onClick={onBack}
        >
          Back
        </button>
        <div className="flex-1">
          <h1 className="text-xl font-semibold text-gray-900">{wcase.employerName}</h1>
          <p className="text-sm text-gray-500">
            Claim: {wcase.claimNumber ?? "Pending"} &bull; Injured: {wcase.injuryDate} &bull; State: {wcase.state}
          </p>
        </div>
        <StatusBadge status={wcase.status} />
      </div>

      {/* Tab bar */}
      <div className="flex border-b border-gray-200 px-6">
        {TABS.map((t) => (
          <button
            key={t.id}
            type="button"
            onClick={() => setTab(t.id)}
            className={[
              "mr-6 border-b-2 pb-3 pt-3 text-sm font-medium transition-colors",
              tab === t.id
                ? "border-blue-600 text-blue-700"
                : "border-transparent text-gray-500 hover:text-gray-700",
            ].join(" ")}
          >
            {t.label}
          </button>
        ))}
      </div>

      {/* Tab content */}
      <div className="flex-1 overflow-y-auto p-6">
        {/* ── Overview ─────────────────────────────────────────────────────── */}
        {tab === "overview" && (
          <div className="space-y-6">
            <div className="grid grid-cols-2 gap-6">
              {/* Case info */}
              <div className="rounded-lg border border-gray-200 p-4">
                <h3 className="mb-3 text-sm font-semibold text-gray-700">Case Information</h3>
                <dl className="space-y-2 text-sm">
                  <div className="flex justify-between">
                    <dt className="text-gray-500">Status</dt>
                    <dd><StatusBadge status={wcase.status} /></dd>
                  </div>
                  <div className="flex justify-between">
                    <dt className="text-gray-500">Claim #</dt>
                    <dd className="font-medium">{wcase.claimNumber ?? "Pending"}</dd>
                  </div>
                  <div className="flex justify-between">
                    <dt className="text-gray-500">State</dt>
                    <dd className="font-medium">{wcase.state}</dd>
                  </div>
                  <div className="flex justify-between">
                    <dt className="text-gray-500">MMI Date</dt>
                    <dd className="font-medium">{wcase.mmiDate ?? "Not reached"}</dd>
                  </div>
                  <div className="flex justify-between">
                    <dt className="text-gray-500">Opened</dt>
                    <dd className="font-medium">{wcase.createdAt.split("T")[0]}</dd>
                  </div>
                </dl>
              </div>

              {/* Injury details */}
              <div className="rounded-lg border border-gray-200 p-4">
                <h3 className="mb-3 text-sm font-semibold text-gray-700">Injury Details</h3>
                <dl className="space-y-2 text-sm">
                  <div className="flex justify-between">
                    <dt className="text-gray-500">Date</dt>
                    <dd className="font-medium">{wcase.injuryDate}</dd>
                  </div>
                  <div>
                    <dt className="mb-1 text-gray-500">Body Parts</dt>
                    <dd className="flex flex-wrap gap-1">
                      {wcase.bodyParts?.map((bp) => (
                        <span key={bp} className="rounded-full bg-orange-100 px-2 py-0.5 text-xs text-orange-700">
                          {bp}
                        </span>
                      )) ?? <span className="text-gray-400">None recorded</span>}
                    </dd>
                  </div>
                  {wcase.injuryDescription && (
                    <div>
                      <dt className="text-gray-500">Description</dt>
                      <dd className="mt-1 rounded bg-gray-50 p-2 text-xs leading-relaxed">
                        {wcase.injuryDescription}
                      </dd>
                    </div>
                  )}
                </dl>
              </div>
            </div>

            {/* FROI button */}
            <div className="flex items-center gap-3">
              <button
                type="button"
                className={BTN_PRIMARY}
                onClick={handleGenerateFroi}
                disabled={froiLoading}
              >
                {froiLoading ? "Generating..." : "Generate FROI"}
              </button>
              <span className="text-sm text-gray-500">
                First Report of Injury document
              </span>
            </div>

            {froi && (
              <div className="rounded-lg border border-gray-200">
                <div className="flex items-center justify-between rounded-t-lg bg-gray-50 px-4 py-2">
                  <span className="text-sm font-medium text-gray-700">{froi.title}</span>
                  <button
                    type="button"
                    className={BTN_SECONDARY}
                    onClick={() => setFroi(null)}
                  >
                    Close
                  </button>
                </div>
                <pre className="max-h-64 overflow-y-auto whitespace-pre-wrap p-4 font-mono text-xs text-gray-800">
                  {froi.content}
                </pre>
              </div>
            )}

            {/* Fee schedule lookup */}
            <div className="rounded-lg border border-gray-200 p-4">
              <h3 className="mb-3 text-sm font-semibold text-gray-700">WC Fee Schedule Lookup</h3>
              <div className="flex items-end gap-3">
                <div>
                  <label className={LABEL_CLS}>State</label>
                  <input
                    className={`${INPUT_CLS} w-20`}
                    value={feeState}
                    onChange={(e) => setFeeState(e.target.value.toUpperCase())}
                    maxLength={2}
                    placeholder="CA"
                  />
                </div>
                <div>
                  <label className={LABEL_CLS}>CPT Code</label>
                  <input
                    className={`${INPUT_CLS} w-28`}
                    value={feeCpt}
                    onChange={(e) => setFeeCpt(e.target.value)}
                    placeholder="97110"
                  />
                </div>
                <button type="button" className={BTN_SECONDARY} onClick={handleFeeLookup}>
                  Lookup
                </button>
              </div>
              {feeResult && (
                <div className="mt-3 rounded-md bg-blue-50 px-4 py-2 text-sm">
                  <span className="font-medium text-blue-800">
                    {feeResult.state} — {feeResult.cptCode}:
                  </span>{" "}
                  <span className="text-blue-900">Max allowable ${feeResult.maxAllowable.toFixed(2)}</span>
                  {feeResult.effectiveDate && (
                    <span className="ml-2 text-blue-600">(eff. {feeResult.effectiveDate})</span>
                  )}
                </div>
              )}
              {feeError && (
                <p className="mt-2 text-sm text-red-600">{feeError}</p>
              )}
            </div>
          </div>
        )}

        {/* ── Contacts ─────────────────────────────────────────────────────── */}
        {tab === "contacts" && (
          <div className="space-y-4">
            {canWrite && (
              <div className="flex justify-end">
                <button
                  type="button"
                  className={BTN_PRIMARY}
                  onClick={() => setShowAddContact(true)}
                >
                  Add Contact
                </button>
              </div>
            )}
            {contacts.length === 0 ? (
              <p className="py-8 text-center text-sm text-gray-400">No contacts added yet.</p>
            ) : (
              <div className="grid grid-cols-2 gap-4">
                {contacts.map((c) => (
                  <div key={c.contactId} className="rounded-lg border border-gray-200 p-4">
                    <div className="mb-2 flex items-center justify-between">
                      <span className="text-sm font-semibold text-gray-900">{c.name}</span>
                      <span className="rounded-full bg-indigo-100 px-2 py-0.5 text-xs text-indigo-700 capitalize">
                        {c.role.replace("_", " ")}
                      </span>
                    </div>
                    {c.company && <p className="text-xs text-gray-500">{c.company}</p>}
                    <div className="mt-2 space-y-1 text-xs text-gray-600">
                      {c.phone && <div>Phone: {c.phone}</div>}
                      {c.email && <div>Email: {c.email}</div>}
                      {c.fax && <div>Fax: {c.fax}</div>}
                    </div>
                    {c.notes && (
                      <p className="mt-2 text-xs text-gray-400">{c.notes}</p>
                    )}
                  </div>
                ))}
              </div>
            )}
          </div>
        )}

        {/* ── Communications ───────────────────────────────────────────────── */}
        {tab === "communications" && (
          <div className="space-y-4">
            {canWrite && (
              <div className="flex justify-end">
                <button
                  type="button"
                  className={BTN_PRIMARY}
                  onClick={() => setShowLogComm(true)}
                >
                  Log Communication
                </button>
              </div>
            )}
            {comms.length === 0 ? (
              <p className="py-8 text-center text-sm text-gray-400">No communications logged yet.</p>
            ) : (
              <table className="w-full text-sm">
                <thead>
                  <tr className="border-b border-gray-200 text-left text-xs text-gray-500">
                    <th className="pb-2 pr-4">Dir</th>
                    <th className="pb-2 pr-4">Method</th>
                    <th className="pb-2 pr-4">Date</th>
                    <th className="pb-2 pr-4">Subject</th>
                    <th className="pb-2">Notes</th>
                  </tr>
                </thead>
                <tbody>
                  {comms.map((comm) => (
                    <tr key={comm.commId} className="border-b border-gray-100">
                      <td className="py-2 pr-4">
                        <DirectionIcon direction={comm.direction} />
                      </td>
                      <td className="py-2 pr-4">
                        <MethodBadge method={comm.method} />
                      </td>
                      <td className="py-2 pr-4 text-xs text-gray-600">
                        {comm.commDate.split("T")[0]}
                      </td>
                      <td className="py-2 pr-4 text-gray-700">
                        {comm.subject ?? <span className="text-gray-400">—</span>}
                      </td>
                      <td className="max-w-xs truncate py-2 text-xs text-gray-500">
                        {comm.content ?? "—"}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </div>
        )}

        {/* ── Impairment ───────────────────────────────────────────────────── */}
        {tab === "impairment" && (
          <div className="space-y-4">
            {canWrite && (
              <div className="flex justify-end">
                <button
                  type="button"
                  className={BTN_PRIMARY}
                  onClick={() => setShowAddRating(true)}
                >
                  Add Rating
                </button>
              </div>
            )}
            {ratings.length === 0 ? (
              <p className="py-8 text-center text-sm text-gray-400">No impairment ratings recorded yet.</p>
            ) : (
              <div className="grid grid-cols-2 gap-4">
                {ratings.map((r) => (
                  <div key={r.ratingId} className="rounded-lg border border-gray-200 p-4">
                    <div className="mb-2 flex items-center justify-between">
                      <span className="text-sm font-semibold text-gray-900 capitalize">
                        {r.bodyPart.replace("_", " ")}
                      </span>
                      <span className="text-lg font-bold text-blue-700">
                        {r.wholePersonPct}% WPI
                      </span>
                    </div>
                    <dl className="space-y-1 text-xs text-gray-600">
                      {r.amaGuidesEdition && (
                        <div>AMA Guides: {r.amaGuidesEdition} Ed.</div>
                      )}
                      {r.impairmentClass && <div>Class: {r.impairmentClass}</div>}
                      {r.gradeModifier && <div>Grade: {r.gradeModifier}</div>}
                      {r.evaluator && <div>Evaluator: {r.evaluator}</div>}
                      {r.evaluationDate && <div>Eval Date: {r.evaluationDate}</div>}
                    </dl>
                  </div>
                ))}
              </div>
            )}
          </div>
        )}
      </div>

      {/* Modals */}
      {showAddContact && (
        <AddContactModal
          caseId={wcase.caseId}
          onClose={() => setShowAddContact(false)}
          onAdded={(c) => {
            setContacts((prev) => [...prev, c]);
            setShowAddContact(false);
          }}
        />
      )}
      {showLogComm && (
        <LogCommModal
          caseId={wcase.caseId}
          contacts={contacts}
          onClose={() => setShowLogComm(false)}
          onLogged={(c) => {
            setComms((prev) => [c, ...prev]);
            setShowLogComm(false);
          }}
        />
      )}
      {showAddRating && (
        <AddRatingModal
          caseId={wcase.caseId}
          onClose={() => setShowAddRating(false)}
          onAdded={(r) => {
            setRatings((prev) => [r, ...prev]);
            setShowAddRating(false);
          }}
        />
      )}
    </div>
  );
}

// ─── Main Page ────────────────────────────────────────────────────────────────

/** Workers' Compensation Module — case list + case detail view. */
export function WorkersCompPage({ patientId, caseId: initialCaseId, role }: Props) {
  const [cases, setCases] = useState<WcCaseRecord[]>([]);
  const [loading, setLoading] = useState(true);
  const [statusFilter, setStatusFilter] = useState<WcCaseStatus | "all">("all");
  const [selectedCase, setSelectedCase] = useState<WcCaseRecord | null>(null);
  const [showNewCase, setShowNewCase] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const loadCases = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const data = await commands.listWcCases(patientId ?? null);
      setCases(data);
      // If a caseId was passed in the route, auto-select it.
      if (initialCaseId) {
        const found = data.find((c) => c.caseId === initialCaseId);
        if (found) setSelectedCase(found);
      }
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }, [patientId, initialCaseId]);

  useEffect(() => {
    loadCases();
  }, [loadCases]);

  const filtered =
    statusFilter === "all"
      ? cases
      : cases.filter((c) => c.status === statusFilter);

  if (selectedCase) {
    return (
      <CaseDetail
        wcase={selectedCase}
        role={role}
        onBack={() => setSelectedCase(null)}
      />
    );
  }

  return (
    <div className="flex h-full flex-col">
      {/* Page header */}
      <div className="flex items-center justify-between border-b border-gray-200 px-6 py-4">
        <div>
          <h1 className="text-2xl font-bold text-gray-900">Workers' Compensation</h1>
          <p className="mt-0.5 text-sm text-gray-500">
            {patientId ? "Patient cases" : "All WC cases"} — {cases.length} total
          </p>
        </div>
        <button
          type="button"
          className={BTN_PRIMARY}
          onClick={() => setShowNewCase(true)}
        >
          New Case
        </button>
      </div>

      {/* Filters */}
      <div className="flex items-center gap-3 border-b border-gray-100 px-6 py-3">
        <span className="text-xs font-medium text-gray-500">Status:</span>
        {(["all", "open", "closed", "settled", "disputed"] as const).map((s) => (
          <button
            key={s}
            type="button"
            onClick={() => setStatusFilter(s)}
            className={[
              "rounded-full px-3 py-1 text-xs font-medium transition-colors",
              statusFilter === s
                ? "bg-blue-600 text-white"
                : "bg-gray-100 text-gray-600 hover:bg-gray-200",
            ].join(" ")}
          >
            {s === "all" ? "All" : s.charAt(0).toUpperCase() + s.slice(1)}
          </button>
        ))}
      </div>

      {/* Case list */}
      <div className="flex-1 overflow-y-auto">
        {loading && (
          <div className="py-12 text-center text-sm text-gray-400">Loading cases...</div>
        )}
        {error && (
          <div className="mx-6 mt-4 rounded-md bg-red-50 px-4 py-3 text-sm text-red-700">
            {error}
          </div>
        )}
        {!loading && filtered.length === 0 && (
          <div className="py-12 text-center text-sm text-gray-400">
            {statusFilter === "all" ? "No WC cases found." : `No ${statusFilter} cases.`}
          </div>
        )}
        {!loading && filtered.length > 0 && (
          <table className="w-full text-sm">
            <thead className="sticky top-0 bg-white">
              <tr className="border-b border-gray-200 text-left text-xs text-gray-500">
                <th className="px-6 pb-2 pt-3">Patient</th>
                <th className="pb-2 pr-4 pt-3">Employer</th>
                <th className="pb-2 pr-4 pt-3">Injury Date</th>
                <th className="pb-2 pr-4 pt-3">Claim #</th>
                <th className="pb-2 pr-4 pt-3">State</th>
                <th className="pb-2 pr-4 pt-3">Status</th>
              </tr>
            </thead>
            <tbody>
              {filtered.map((c) => (
                <tr
                  key={c.caseId}
                  className="cursor-pointer border-b border-gray-100 hover:bg-blue-50"
                  onClick={() => setSelectedCase(c)}
                >
                  <td className="px-6 py-3 text-gray-700">{c.patientId}</td>
                  <td className="py-3 pr-4 font-medium text-gray-900">{c.employerName}</td>
                  <td className="py-3 pr-4 text-gray-600">{c.injuryDate}</td>
                  <td className="py-3 pr-4 text-gray-600">{c.claimNumber ?? "—"}</td>
                  <td className="py-3 pr-4 text-gray-600">{c.state}</td>
                  <td className="py-3 pr-4">
                    <StatusBadge status={c.status} />
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>

      {/* New Case modal */}
      {showNewCase && (
        <NewCaseModal
          initialPatientId={patientId}
          onClose={() => setShowNewCase(false)}
          onCreated={(newCase) => {
            setCases((prev) => [newCase, ...prev]);
            setShowNewCase(false);
            setSelectedCase(newCase);
          }}
        />
      )}
    </div>
  );
}

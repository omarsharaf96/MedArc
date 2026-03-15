/**
 * PatientDetailPage.tsx — Patient chart shell.
 *
 * Renders demographics, insurance, employer, SDOH, care team, and related
 * persons sections for a single patient.
 *
 * RBAC gates:
 *   - BillingStaff: sees demographics, insurance, and employer only.
 *     SDOH, care team, and related persons sections are hidden.
 *   - All other roles: see all sections.
 *
 * The "Edit" button opens a placeholder for T03 (PatientFormModal).
 * Sections use <section> elements with Tailwind card styling — no inline styles.
 */
import { useState, useEffect } from "react";
import { usePatient } from "../hooks/usePatient";
import { useNav } from "../contexts/RouterContext";
import {
  extractPatientDisplay,
  type PatientDisplay,
  type InsuranceDisplay,
} from "../lib/fhirExtract";
import { PatientFormModal } from "../components/patient/PatientFormModal";
import { DocumentBrowser } from "../components/clinical/DocumentBrowser";
import { AuthTrackingPanel } from "../components/clinical/AuthTrackingPanel";
import { commands } from "../lib/tauri";
import type { EncounterRecord, EncounterInput, TemplateRecord } from "../types/documentation";
import type { AppointmentRecord } from "../types/scheduling";
import { extractAppointmentDisplay } from "../lib/fhirExtract";

// ─── Props ───────────────────────────────────────────────────────────────────

interface PatientDetailPageProps {
  patientId: string;
  role: string;
  userId: string;
}

// ─── Sub-components ──────────────────────────────────────────────────────────

/** A card-styled section wrapper. */
function SectionCard({
  title,
  children,
}: {
  title: string;
  children: React.ReactNode;
}) {
  return (
    <section className="rounded-lg border border-gray-200 bg-white p-5 shadow-sm">
      <h2 className="mb-4 text-base font-semibold text-gray-800">{title}</h2>
      {children}
    </section>
  );
}

/** A two-column label/value row inside a section. */
function InfoRow({ label, value }: { label: string; value: string | null }) {
  return (
    <div className="flex gap-2 py-1 text-sm">
      <span className="w-40 shrink-0 text-gray-500">{label}</span>
      <span className="text-gray-900">{value ?? "—"}</span>
    </div>
  );
}

/** Render one insurance tier as a sub-card. */
function InsuranceTile({
  tier,
  ins,
}: {
  tier: string;
  ins: InsuranceDisplay;
}) {
  return (
    <div className="rounded-md border border-gray-100 bg-gray-50 p-3">
      <p className="mb-2 text-xs font-semibold uppercase tracking-wide text-gray-500">
        {tier} Insurance
      </p>
      <InfoRow label="Payer" value={ins.payerName} />
      <InfoRow label="Plan" value={ins.planName} />
      <InfoRow label="Member ID" value={ins.memberId} />
      <InfoRow label="Group #" value={ins.groupNumber} />
      <InfoRow label="Subscriber" value={ins.subscriberName} />
      <InfoRow label="Subscriber DOB" value={ins.subscriberDob} />
      <InfoRow
        label="Relationship"
        value={ins.relationshipToSubscriber}
      />
    </div>
  );
}

/** Loading skeleton — full-width pulsing placeholder. */
function LoadingSkeleton() {
  return (
    <div className="animate-pulse space-y-4 p-6">
      <div className="h-8 w-1/3 rounded bg-gray-200" />
      <div className="h-4 w-1/2 rounded bg-gray-200" />
      <div className="h-4 w-2/3 rounded bg-gray-200" />
      <div className="h-32 rounded bg-gray-200" />
      <div className="h-32 rounded bg-gray-200" />
    </div>
  );
}

// ─── Main component ──────────────────────────────────────────────────────────

/** Format encounter type for display: "office_visit" → "Office Visit" */
function formatEncounterType(raw: string): string {
  return raw
    .split("_")
    .map((w) => w.charAt(0).toUpperCase() + w.slice(1))
    .join(" ");
}

/** Extract status from FHIR encounter resource. */
function extractEncounterStatus(resource: Record<string, unknown>): string {
  const status = resource["status"];
  return typeof status === "string" ? status : "unknown";
}

/** Extract encounter date (YYYY-MM-DD) from FHIR encounter resource. */
function extractEncounterDateDisplay(resource: Record<string, unknown>): string {
  const period = resource["period"] as Record<string, unknown> | undefined;
  const start = period?.["start"];
  if (typeof start === "string" && start.length >= 10) return start.slice(0, 10);
  const date = resource["date"];
  if (typeof date === "string" && date.length >= 10) return date.slice(0, 10);
  return "—";
}

/** Extract encounter type label from FHIR encounter resource. */
function extractEncounterTypeLabel(resource: Record<string, unknown>): string {
  const types = resource["type"] as Array<Record<string, unknown>> | undefined;
  const typeText = types?.[0]?.["text"];
  if (typeof typeText === "string") return formatEncounterType(typeText);
  const cls = resource["class"] as Record<string, unknown> | undefined;
  const code = cls?.["code"];
  if (typeof code === "string") return formatEncounterType(code);
  return "Office Visit";
}

export function PatientDetailPage({ patientId, role, userId }: PatientDetailPageProps) {
  const { goBack, navigate } = useNav();
  const { patient, relatedPersons, loading, error, reload } =
    usePatient(patientId);

  const [editOpen, setEditOpen] = useState(false);

  // ── Encounter list state ───────────────────────────────────────────────
  const [encounters, setEncounters] = useState<EncounterRecord[]>([]);
  const [encountersLoading, setEncountersLoading] = useState(true);
  const [encountersError, setEncountersError] = useState<string | null>(null);
  const [encounterRefresh, setEncounterRefresh] = useState(0);

  // ── Start Encounter state ─────────────────────────────────────────────
  const [startingEncounter, setStartingEncounter] = useState(false);
  const [startEncounterError, setStartEncounterError] = useState<string | null>(null);

  // ── Patient appointments state ──────────────────────────────────────
  const [appointments, setAppointments] = useState<AppointmentRecord[]>([]);
  const [appointmentsLoading, setAppointmentsLoading] = useState(true);

  // ── Delete patient state ────────────────────────────────────────────
  const [showDeletePatientConfirm, setShowDeletePatientConfirm] = useState(false);
  const [deletingPatient, setDeletingPatient] = useState(false);
  const [deletePatientError, setDeletePatientError] = useState<string | null>(null);

  // ── Template picker modal state ─────────────────────────────────────
  const [showTemplatePicker, setShowTemplatePicker] = useState(false);
  const [templates, setTemplates] = useState<TemplateRecord[]>([]);
  const [templatesLoading, setTemplatesLoading] = useState(false);

  // ── Fetch encounters on mount and refresh ──────────────────────────────
  useEffect(() => {
    let mounted = true;
    setEncountersLoading(true);
    setEncountersError(null);

    commands
      .listEncounters(patientId, null, null, null)
      .then((result) => {
        if (!mounted) return;
        setEncounters(result);
      })
      .catch((e) => {
        if (!mounted) return;
        const msg = e instanceof Error ? e.message : String(e);
        console.error(`[PatientDetailPage] listEncounters failed for ${patientId}:`, msg);
        setEncountersError(msg);
        setEncounters([]);
      })
      .finally(() => {
        if (mounted) setEncountersLoading(false);
      });

    return () => {
      mounted = false;
    };
  }, [patientId, encounterRefresh]);

  // ── Fetch patient appointments (past + upcoming) ───────────────────────
  useEffect(() => {
    let mounted = true;
    setAppointmentsLoading(true);

    // Fetch a wide range: 2 years back + 1 year ahead
    const now = new Date();
    const pastDate = new Date(now);
    pastDate.setFullYear(now.getFullYear() - 2);
    const futureDate = new Date(now);
    futureDate.setFullYear(now.getFullYear() + 1);
    const startStr = pastDate.toLocaleDateString("sv");
    const endStr = futureDate.toLocaleDateString("sv");

    commands
      .listAppointments(startStr, endStr, patientId, null)
      .then((result) => {
        if (mounted) setAppointments(result);
      })
      .catch(() => {
        if (mounted) setAppointments([]);
      })
      .finally(() => {
        if (mounted) setAppointmentsLoading(false);
      });

    return () => { mounted = false; };
  }, [patientId]);

  // ── Start Encounter handler ────────────────────────────────────────────
  const canStartEncounter =
    role === "Provider" || role === "NurseMa" || role === "SystemAdmin";

  async function handleOpenTemplatePicker() {
    setShowTemplatePicker(true);
    setStartEncounterError(null);
    if (templates.length === 0) {
      setTemplatesLoading(true);
      try {
        const tpls = await commands.listTemplates(null);
        setTemplates(tpls);
      } catch (e) {
        console.error("[PatientDetailPage] listTemplates failed:", e);
      } finally {
        setTemplatesLoading(false);
      }
    }
  }

  async function handleStartEncounterWithTemplate(templateId: string | null) {
    setShowTemplatePicker(false);
    setStartingEncounter(true);
    setStartEncounterError(null);
    try {
      // If a template is selected, fetch its SOAP defaults to pre-fill
      let soap: EncounterInput["soap"] = null;
      if (templateId) {
        try {
          const tpl = await commands.getTemplate(templateId);
          soap = tpl.defaultSoap;
        } catch (e) {
          console.error("[PatientDetailPage] getTemplate failed:", e);
          // Continue without template pre-fill
        }
      }
      const input: EncounterInput = {
        patientId,
        providerId: userId,
        encounterDate: new Date().toISOString().slice(0, 19),
        encounterType: "office_visit",
        chiefComplaint: null,
        templateId,
        soap,
        appointmentId: null,
      };
      const created = await commands.createEncounter(input);
      navigate({ page: "encounter-workspace", patientId, encounterId: created.id });
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      console.error(`[PatientDetailPage] createEncounter failed for patient ${patientId}:`, msg);
      setStartEncounterError(msg);
    } finally {
      setStartingEncounter(false);
    }
  }

  // ── Delete patient handler ──────────────────────────────────────────────
  async function handleDeletePatient() {
    setDeletingPatient(true);
    setDeletePatientError(null);
    try {
      await commands.deletePatient(patientId);
      navigate({ page: "patients" });
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      console.error(`[PatientDetailPage] deletePatient failed for ${patientId}:`, msg);
      setDeletePatientError(msg);
    } finally {
      setDeletingPatient(false);
    }
  }

  // ── Loading state ──────────────────────────────────────────────────────
  if (loading) {
    return <LoadingSkeleton />;
  }

  // ── Error state ────────────────────────────────────────────────────────
  if (error) {
    return (
      <div className="p-6">
        <div className="mb-4 rounded-md border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700">
          <p className="font-semibold">Failed to load patient data</p>
          <p className="mt-1">{error}</p>
        </div>
        <button
          type="button"
          onClick={reload}
          className="rounded-md bg-red-600 px-4 py-2 text-sm font-medium text-white hover:bg-red-700 focus:outline-none focus:ring-2 focus:ring-red-500 focus:ring-offset-2"
        >
          Retry
        </button>
      </div>
    );
  }

  // ── Not found state ────────────────────────────────────────────────────
  if (patient === null) {
    return (
      <div className="p-6">
        <p className="mb-4 text-gray-600">
          Patient not found. The record may have been deleted.
        </p>
        <button
          type="button"
          onClick={goBack}
          className="rounded-md bg-gray-100 px-4 py-2 text-sm font-medium text-gray-700 hover:bg-gray-200 focus:outline-none focus:ring-2 focus:ring-gray-400 focus:ring-offset-2"
        >
          ← Back
        </button>
      </div>
    );
  }

  // ── Extract display fields from FHIR resource ──────────────────────────
  const display: PatientDisplay = extractPatientDisplay(patient.resource);

  const fullName =
    [display.givenNames.join(" "), display.familyName]
      .filter(Boolean)
      .join(" ") || "Unknown Patient";

  const isBillingStaff = role === "BillingStaff";

  // ── Data present — render chart ────────────────────────────────────────
  return (
    <div className="space-y-6 p-6">
      {/* ── Header ──────────────────────────────────────────────────────── */}
      <div className="flex items-start justify-between gap-4">
        <div className="flex items-center gap-3">
          <button
            type="button"
            onClick={goBack}
            className="rounded-md p-1.5 text-gray-500 hover:bg-gray-100 hover:text-gray-700 focus:outline-none focus:ring-2 focus:ring-gray-400 focus:ring-offset-1"
            aria-label="Go back"
          >
            ← Back
          </button>
          <div>
            <h1 className="text-xl font-bold text-gray-900">{fullName}</h1>
            {display.mrn && (
              <span className="mt-0.5 inline-block rounded-full bg-blue-100 px-2.5 py-0.5 text-xs font-medium text-blue-800">
                MRN: {display.mrn}
              </span>
            )}
          </div>
        </div>

        {/* Action buttons */}
        <div className="flex items-center gap-2">
          {/* Start Encounter — Provider, NurseMa, SystemAdmin only */}
          {canStartEncounter && (
            <div className="flex flex-col items-end gap-1">
              <button
                type="button"
                onClick={handleOpenTemplatePicker}
                disabled={startingEncounter}
                className="rounded-md bg-green-600 px-4 py-2 text-sm font-medium text-white hover:bg-green-700 focus:outline-none focus:ring-2 focus:ring-green-500 focus:ring-offset-2 disabled:opacity-60"
              >
                {startingEncounter ? "Starting…" : "Start Encounter"}
              </button>
              {startEncounterError && (
                <p className="max-w-xs text-right text-xs text-red-600">
                  {startEncounterError}
                </p>
              )}
            </div>
          )}

          {/* Edit button — hidden for BillingStaff */}
          {!isBillingStaff && (
            <button
              type="button"
              onClick={() => setEditOpen(true)}
              className="rounded-md bg-indigo-600 px-4 py-2 text-sm font-medium text-white hover:bg-indigo-700 focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:ring-offset-2"
            >
              Edit
            </button>
          )}

          {/* Delete Patient — SystemAdmin only */}
          {role === "SystemAdmin" && (
            <button
              type="button"
              onClick={() => setShowDeletePatientConfirm(true)}
              className="rounded-md bg-red-600 px-4 py-2 text-sm font-medium text-white hover:bg-red-700 focus:outline-none focus:ring-2 focus:ring-red-500 focus:ring-offset-2"
            >
              Delete Patient
            </button>
          )}
        </div>
      </div>

      {/* Edit modal */}
      {editOpen && (
        <PatientFormModal
          patientId={patientId}
          initialDisplay={display}
          onSuccess={() => {
            setEditOpen(false);
            reload();
          }}
          onClose={() => setEditOpen(false)}
        />
      )}

      {/* Delete patient confirmation dialog */}
      {showDeletePatientConfirm && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/40"
          role="dialog"
          aria-modal="true"
          aria-labelledby="delete-patient-modal-title"
        >
          <div className="w-full max-w-md rounded-lg bg-white p-6 shadow-xl">
            <h3
              id="delete-patient-modal-title"
              className="text-base font-semibold text-gray-900"
            >
              Delete Patient
            </h3>
            <p className="mt-2 text-sm text-gray-600">
              Are you sure? This cannot be undone. All patient data including encounters,
              appointments, and documents will be permanently deleted.
            </p>
            {deletePatientError && (
              <div className="mt-3 rounded-md border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-700">
                {deletePatientError}
              </div>
            )}
            <div className="mt-4 flex justify-end gap-3">
              <button
                type="button"
                onClick={() => setShowDeletePatientConfirm(false)}
                disabled={deletingPatient}
                className="rounded-md border border-gray-300 px-4 py-2 text-sm font-medium text-gray-700 hover:bg-gray-50 focus:outline-none focus:ring-2 focus:ring-gray-400 focus:ring-offset-2 disabled:opacity-60"
              >
                Cancel
              </button>
              <button
                type="button"
                onClick={() => void handleDeletePatient()}
                disabled={deletingPatient}
                className="rounded-md bg-red-600 px-4 py-2 text-sm font-medium text-white hover:bg-red-700 focus:outline-none focus:ring-2 focus:ring-red-500 focus:ring-offset-2 disabled:opacity-60"
              >
                {deletingPatient ? "Deleting..." : "Delete"}
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Template picker modal */}
      {showTemplatePicker && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/30">
          <div className="mx-4 max-h-[80vh] w-full max-w-lg overflow-y-auto rounded-lg bg-white p-6 shadow-xl">
            <h2 className="mb-1 text-lg font-bold text-gray-900">
              Start New Encounter
            </h2>
            <p className="mb-4 text-sm text-gray-500">
              Choose a note template to pre-fill the encounter, or start with a blank note.
            </p>

            {templatesLoading ? (
              <p className="py-4 text-sm text-gray-500">Loading templates...</p>
            ) : (
              <div className="space-y-2">
                {/* Blank note option */}
                <button
                  type="button"
                  onClick={() => void handleStartEncounterWithTemplate(null)}
                  className="flex w-full items-start gap-3 rounded-lg border border-gray-200 p-3 text-left hover:border-indigo-300 hover:bg-indigo-50 focus:outline-none focus:ring-2 focus:ring-indigo-400"
                >
                  <div className="mt-0.5 flex h-8 w-8 shrink-0 items-center justify-center rounded bg-gray-100 text-sm text-gray-500">
                    +
                  </div>
                  <div>
                    <p className="text-sm font-medium text-gray-900">Blank Note</p>
                    <p className="text-xs text-gray-500">Start with an empty SOAP note</p>
                  </div>
                </button>

                {/* PT-specific templates first, then others */}
                {(() => {
                  const ptTemplates = templates.filter(t => t.specialty === "physical_therapy");
                  const otherTemplates = templates.filter(t => t.specialty !== "physical_therapy");
                  return (
                    <>
                      {ptTemplates.length > 0 && (
                        <div className="pt-2">
                          <p className="mb-1.5 text-xs font-semibold uppercase tracking-wide text-indigo-600">
                            Physical Therapy Templates
                          </p>
                          {ptTemplates.map((tpl) => (
                            <button
                              key={tpl.id}
                              type="button"
                              onClick={() => void handleStartEncounterWithTemplate(tpl.id)}
                              className="flex w-full items-start gap-3 rounded-lg border border-gray-200 p-3 text-left hover:border-indigo-300 hover:bg-indigo-50 focus:outline-none focus:ring-2 focus:ring-indigo-400 mb-2"
                            >
                              <div className="mt-0.5 flex h-8 w-8 shrink-0 items-center justify-center rounded bg-indigo-100 text-sm text-indigo-600">
                                PT
                              </div>
                              <div>
                                <p className="text-sm font-medium text-gray-900">{tpl.name}</p>
                                <p className="text-xs text-gray-500">{tpl.description}</p>
                              </div>
                            </button>
                          ))}
                        </div>
                      )}
                      {otherTemplates.length > 0 && (
                        <div className="pt-2">
                          <p className="mb-1.5 text-xs font-semibold uppercase tracking-wide text-gray-500">
                            Other Templates
                          </p>
                          {otherTemplates.map((tpl) => (
                            <button
                              key={tpl.id}
                              type="button"
                              onClick={() => void handleStartEncounterWithTemplate(tpl.id)}
                              className="flex w-full items-start gap-3 rounded-lg border border-gray-200 p-3 text-left hover:border-indigo-300 hover:bg-indigo-50 focus:outline-none focus:ring-2 focus:ring-indigo-400 mb-2"
                            >
                              <div className="mt-0.5 flex h-8 w-8 shrink-0 items-center justify-center rounded bg-gray-100 text-sm text-gray-500">
                                {tpl.specialty?.charAt(0).toUpperCase() ?? "G"}
                              </div>
                              <div>
                                <p className="text-sm font-medium text-gray-900">{tpl.name}</p>
                                <p className="text-xs text-gray-500">{tpl.description}</p>
                              </div>
                            </button>
                          ))}
                        </div>
                      )}
                    </>
                  );
                })()}
              </div>
            )}

            <div className="mt-4 flex justify-end">
              <button
                type="button"
                onClick={() => setShowTemplatePicker(false)}
                className="rounded-md bg-gray-100 px-4 py-2 text-sm font-medium text-gray-700 hover:bg-gray-200 focus:outline-none focus:ring-2 focus:ring-gray-400 focus:ring-offset-2"
              >
                Cancel
              </button>
            </div>
          </div>
        </div>
      )}

      {/* ── Encounters section ───────────────────────────────────────────── */}
      {!isBillingStaff && (
        <SectionCard title="Encounters">
          {encountersLoading ? (
            <p className="text-sm text-gray-500">Loading encounters…</p>
          ) : encountersError ? (
            <div className="rounded-md border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-700">
              <p className="font-semibold">Failed to load encounters</p>
              <p className="mt-0.5">{encountersError}</p>
              <button
                type="button"
                onClick={() => setEncounterRefresh((n) => n + 1)}
                className="mt-2 rounded bg-red-600 px-3 py-1 text-xs font-medium text-white hover:bg-red-700"
              >
                Retry
              </button>
            </div>
          ) : encounters.length === 0 ? (
            <p className="text-sm text-gray-500">No encounters on file.</p>
          ) : (
            <div className="overflow-x-auto">
              <table className="w-full text-sm">
                <thead>
                  <tr className="border-b border-gray-100 text-left text-xs font-medium uppercase tracking-wide text-gray-500">
                    <th className="pb-2 pr-4">Date</th>
                    <th className="pb-2 pr-4">Type</th>
                    <th className="pb-2">Status</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-gray-50">
                  {encounters.map((enc) => {
                    const dateDisplay = extractEncounterDateDisplay(enc.resource);
                    const typeLabel = extractEncounterTypeLabel(enc.resource);
                    const status = extractEncounterStatus(enc.resource);
                    return (
                      <tr
                        key={enc.id}
                        className="group"
                      >
                        <td colSpan={3} className="p-0">
                          <button
                            type="button"
                            onClick={() =>
                              navigate({
                                page: "encounter-workspace",
                                patientId,
                                encounterId: enc.id,
                              })
                            }
                            className="flex w-full items-center gap-0 px-1 py-2 text-left hover:bg-indigo-50 focus:outline-none focus:ring-2 focus:ring-indigo-400 focus:ring-inset"
                          >
                            <span className="w-32 shrink-0 text-gray-700">
                              {dateDisplay}
                            </span>
                            <span className="flex-1 text-gray-900">
                              {typeLabel}
                            </span>
                            <span className="ml-4">
                              <span
                                className={[
                                  "inline-flex rounded-full px-2 py-0.5 text-xs font-medium",
                                  status === "finished"
                                    ? "bg-green-100 text-green-800"
                                    : status === "in-progress"
                                      ? "bg-blue-100 text-blue-800"
                                      : status === "cancelled"
                                        ? "bg-red-100 text-red-800"
                                        : "bg-gray-100 text-gray-700",
                                ].join(" ")}
                              >
                                {status.charAt(0).toUpperCase() + status.slice(1)}
                              </span>
                            </span>
                            <span className="ml-3 text-xs text-indigo-500 opacity-0 group-hover:opacity-100">
                              Open →
                            </span>
                          </button>
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
          )}
        </SectionCard>
      )}

      {/* ── Appointments section (upcoming + past) ────────────────────────── */}
      <SectionCard title="Appointments">
        {appointmentsLoading ? (
          <p className="text-sm text-gray-500">Loading appointments…</p>
        ) : appointments.length === 0 ? (
          <p className="text-sm text-gray-500">No appointments found.</p>
        ) : (() => {
          const now = new Date();
          const upcoming = appointments.filter((a) => {
            const d = extractAppointmentDisplay(a.resource);
            return d.start && new Date(d.start) >= now && d.status !== "cancelled";
          });
          const past = appointments.filter((a) => {
            const d = extractAppointmentDisplay(a.resource);
            return !d.start || new Date(d.start) < now || d.status === "cancelled";
          });

          const renderRow = (appt: AppointmentRecord) => {
            const d = extractAppointmentDisplay(appt.resource);
            const dateStr = d.start
              ? new Date(d.start).toLocaleDateString("en-US", { month: "short", day: "numeric", year: "numeric" })
              : "—";
            const timeStr = d.start
              ? new Date(d.start).toLocaleTimeString("en-US", { hour: "numeric", minute: "2-digit" })
              : "";
            const typeLabel = d.apptTypeDisplay ?? d.apptType ?? "—";
            const status = d.status ?? "booked";
            return (
              <tr key={appt.id} className="border-t border-gray-50">
                <td className="py-2 pr-3 text-gray-700">{dateStr}</td>
                <td className="py-2 pr-3 text-gray-600">{timeStr}</td>
                <td className="py-2 pr-3 text-gray-900">{typeLabel}</td>
                <td className="py-2">
                  <span className={[
                    "inline-flex rounded-full px-2 py-0.5 text-xs font-medium",
                    status === "fulfilled" ? "bg-green-100 text-green-800" :
                    status === "cancelled" ? "bg-red-100 text-red-800" :
                    status === "noshow" ? "bg-orange-100 text-orange-800" :
                    "bg-blue-100 text-blue-800",
                  ].join(" ")}>
                    {status}
                  </span>
                </td>
              </tr>
            );
          };

          return (
            <div className="space-y-4">
              {upcoming.length > 0 && (
                <div>
                  <h3 className="text-xs font-semibold uppercase tracking-wide text-gray-500 mb-2">Upcoming</h3>
                  <table className="w-full text-sm">
                    <thead>
                      <tr className="text-left text-xs font-medium uppercase tracking-wide text-gray-400">
                        <th className="pb-1 pr-3">Date</th>
                        <th className="pb-1 pr-3">Time</th>
                        <th className="pb-1 pr-3">Type</th>
                        <th className="pb-1">Status</th>
                      </tr>
                    </thead>
                    <tbody>{upcoming.map(renderRow)}</tbody>
                  </table>
                </div>
              )}
              {past.length > 0 && (
                <div>
                  <h3 className="text-xs font-semibold uppercase tracking-wide text-gray-500 mb-2">Past</h3>
                  <table className="w-full text-sm">
                    <thead>
                      <tr className="text-left text-xs font-medium uppercase tracking-wide text-gray-400">
                        <th className="pb-1 pr-3">Date</th>
                        <th className="pb-1 pr-3">Time</th>
                        <th className="pb-1 pr-3">Type</th>
                        <th className="pb-1">Status</th>
                      </tr>
                    </thead>
                    <tbody>{past.map(renderRow)}</tbody>
                  </table>
                </div>
              )}
            </div>
          );
        })()}
      </SectionCard>

      {/* ── Documents — visible to all authenticated roles ────────────────── */}
      <SectionCard title="Documents">
        <DocumentBrowser patientId={patientId} userId={userId} />
      </SectionCard>

      {/* ── Authorization Tracking — hidden for FrontDesk ────────────────── */}
      {role !== "FrontDesk" && (
        <SectionCard title="Authorization Tracking">
          <AuthTrackingPanel patientId={patientId} role={role} />
        </SectionCard>
      )}

      {/* ── Demographics section ─────────────────────────────────────────── */}
      <SectionCard title="Demographics">
        <InfoRow label="Full Name" value={fullName} />
        <InfoRow label="Date of Birth" value={display.dob} />
        <InfoRow label="Gender" value={display.gender} />
        <InfoRow label="Gender Identity" value={display.genderIdentity} />
        <InfoRow label="Phone" value={display.phone} />
        <InfoRow label="Email" value={display.email} />
        {(display.addressLine ||
          display.city ||
          display.state ||
          display.postalCode) && (
          <div className="flex gap-2 py-1 text-sm">
            <span className="w-40 shrink-0 text-gray-500">Address</span>
            <div className="text-gray-900">
              {display.addressLine && <div>{display.addressLine}</div>}
              {(display.city || display.state || display.postalCode) && (
                <div>
                  {[display.city, display.state, display.postalCode]
                    .filter(Boolean)
                    .join(", ")}
                </div>
              )}
              {display.country && <div>{display.country}</div>}
            </div>
          </div>
        )}
        <InfoRow label="MRN" value={display.mrn} />
      </SectionCard>

      {/* ── Insurance section ────────────────────────────────────────────── */}
      {(display.insurancePrimary ||
        display.insuranceSecondary ||
        display.insuranceTertiary) && (
        <SectionCard title="Insurance">
          <div className="space-y-3">
            {display.insurancePrimary && (
              <InsuranceTile tier="Primary" ins={display.insurancePrimary} />
            )}
            {display.insuranceSecondary && (
              <InsuranceTile
                tier="Secondary"
                ins={display.insuranceSecondary}
              />
            )}
            {display.insuranceTertiary && (
              <InsuranceTile tier="Tertiary" ins={display.insuranceTertiary} />
            )}
          </div>
        </SectionCard>
      )}

      {/* ── Employer section ─────────────────────────────────────────────── */}
      {display.employer?.employerName && (
        <SectionCard title="Employer">
          <InfoRow label="Employer" value={display.employer.employerName} />
          <InfoRow
            label="Occupation"
            value={display.employer.occupation ?? null}
          />
          <InfoRow
            label="Employer Phone"
            value={display.employer.employerPhone ?? null}
          />
          <InfoRow
            label="Employer Address"
            value={display.employer.employerAddress ?? null}
          />
        </SectionCard>
      )}

      {/* ── SDOH section — hidden for BillingStaff ───────────────────────── */}
      {!isBillingStaff && display.sdoh && (
        <SectionCard title="Social Determinants of Health">
          <InfoRow
            label="Housing Status"
            value={display.sdoh.housingStatus ?? null}
          />
          <InfoRow
            label="Food Security"
            value={display.sdoh.foodSecurity ?? null}
          />
          <InfoRow
            label="Transportation"
            value={display.sdoh.transportationAccess ?? null}
          />
          <InfoRow
            label="Education Level"
            value={display.sdoh.educationLevel ?? null}
          />
          <InfoRow label="Notes" value={display.sdoh.notes ?? null} />
        </SectionCard>
      )}

      {/* ── Related Persons section — hidden for BillingStaff ────────────── */}
      {!isBillingStaff && (
        <SectionCard title="Related Persons">
          {relatedPersons.length === 0 ? (
            <p className="text-sm text-gray-500">None on file.</p>
          ) : (
            <div className="space-y-3">
              {relatedPersons.map((rp) => (
                <RelatedPersonTile key={rp.id} resource={rp.resource} />
              ))}
            </div>
          )}
        </SectionCard>
      )}
    </div>
  );
}

// ─── Related Person tile ──────────────────────────────────────────────────────

function RelatedPersonTile({
  resource,
}: {
  resource: Record<string, unknown>;
}) {
  const names = resource["name"] as Array<Record<string, unknown>> | undefined;
  const firstName = names?.[0];
  const familyName =
    typeof firstName?.["family"] === "string" ? firstName["family"] : null;
  const givenRaw = firstName?.["given"] as Array<unknown> | undefined;
  const givenNames = Array.isArray(givenRaw)
    ? givenRaw.filter((g) => typeof g === "string").join(" ")
    : "";
  const fullName = [givenNames, familyName].filter(Boolean).join(" ") || "—";

  const relationship = resource["relationship"] as
    | Array<Record<string, unknown>>
    | undefined;
  const relationshipText =
    relationship?.[0]?.["text"] !== undefined
      ? String(relationship[0]["text"])
      : null;

  const telecom = resource["telecom"] as
    | Array<Record<string, unknown>>
    | undefined;
  const phone =
    telecom?.find((t) => t["system"] === "phone")?.["value"];
  const phoneStr = typeof phone === "string" ? phone : null;

  return (
    <div className="rounded-md border border-gray-100 bg-gray-50 p-3">
      <p className="text-sm font-medium text-gray-900">{fullName}</p>
      {relationshipText && (
        <p className="text-xs text-gray-500">{relationshipText}</p>
      )}
      {phoneStr && (
        <p className="mt-1 text-xs text-gray-600">📞 {phoneStr}</p>
      )}
    </div>
  );
}

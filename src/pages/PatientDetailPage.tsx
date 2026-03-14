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
import { ClinicalSidebar } from "../components/clinical/ClinicalSidebar";
import { LabResultsPanel } from "../components/clinical/LabResultsPanel";
import { DocumentBrowser } from "../components/clinical/DocumentBrowser";
import { AuthTrackingPanel } from "../components/clinical/AuthTrackingPanel";
import { commands } from "../lib/tauri";
import type { EncounterRecord, EncounterInput } from "../types/documentation";

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
  const { patient, careTeam, relatedPersons, loading, error, reload } =
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

  // ── Start Encounter handler ────────────────────────────────────────────
  const canStartEncounter =
    role === "Provider" || role === "NurseMa" || role === "SystemAdmin";

  async function handleStartEncounter() {
    setStartingEncounter(true);
    setStartEncounterError(null);
    try {
      const input: EncounterInput = {
        patientId,
        providerId: userId,
        encounterDate: new Date().toISOString().slice(0, 19),
        encounterType: "office_visit",
        chiefComplaint: null,
        templateId: null,
        soap: null,
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
                onClick={handleStartEncounter}
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

          {/* PT Notes — Provider and SystemAdmin only */}
          {(role === "Provider" || role === "SystemAdmin") && (
            <button
              type="button"
              onClick={() => navigate({ page: "pt-notes", patientId })}
              className="rounded-md bg-purple-600 px-4 py-2 text-sm font-medium text-white hover:bg-purple-700 focus:outline-none focus:ring-2 focus:ring-purple-500 focus:ring-offset-2"
            >
              PT Notes
            </button>
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

      {/* ── Clinical Data — Provider / NurseMa / SystemAdmin only ────────── */}
      {role !== "BillingStaff" && role !== "FrontDesk" && (
        <SectionCard title="Clinical Data">
          <ClinicalSidebar patientId={patientId} role={role} />
        </SectionCard>
      )}

      {/* ── Lab Results — hidden for FrontDesk ───────────────────────────── */}
      {role !== "FrontDesk" && (
        <SectionCard title="Lab Results">
          <LabResultsPanel patientId={patientId} userId={userId} role={role} />
        </SectionCard>
      )}

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

      {/* ── Care Team section — hidden for BillingStaff ──────────────────── */}
      {!isBillingStaff && (
        <SectionCard title="Care Team">
          {careTeam === null ? (
            <p className="text-sm text-gray-500">No care team assigned.</p>
          ) : (
            <CareTeamDisplay resource={careTeam.resource} />
          )}
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

// ─── Care Team display ────────────────────────────────────────────────────────

function CareTeamDisplay({
  resource,
}: {
  resource: Record<string, unknown>;
}) {
  const participants = resource["participant"] as
    | Array<Record<string, unknown>>
    | undefined;

  if (!participants || participants.length === 0) {
    return (
      <p className="text-sm text-gray-500">No care team members recorded.</p>
    );
  }

  return (
    <div className="space-y-2">
      {participants.map((p, i) => {
        const member = p["member"] as Record<string, unknown> | undefined;
        const role = p["role"] as Array<Record<string, unknown>> | undefined;
        const note = p["note"] as Array<Record<string, unknown>> | undefined;

        const memberDisplay =
          typeof member?.["display"] === "string" ? member["display"] : null;
        const roleText =
          role?.[0]?.["text"] !== undefined
            ? String(role[0]["text"])
            : null;
        const noteText =
          note?.[0]?.["text"] !== undefined
            ? String(note[0]["text"])
            : null;

        return (
          <div
            key={i}
            className="rounded-md border border-gray-100 bg-gray-50 p-3"
          >
            <p className="text-sm font-medium text-gray-900">
              {memberDisplay ?? "—"}
            </p>
            {roleText && (
              <p className="text-xs text-gray-500">{roleText}</p>
            )}
            {noteText && (
              <p className="mt-1 text-xs text-gray-600 italic">{noteText}</p>
            )}
          </div>
        );
      })}
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

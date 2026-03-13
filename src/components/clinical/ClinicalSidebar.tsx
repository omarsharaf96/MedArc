/**
 * ClinicalSidebar.tsx — Clinical data sidebar for the patient chart.
 *
 * Renders four tabs: Problems | Medications | Allergies | Immunizations.
 * Wired to useClinicalData(patientId) for live data. The DrugAllergyAlertBanner
 * renders above the tab bar when any drug-allergy CDS alerts are active.
 *
 * RBAC gate is applied in PatientDetailPage — this component renders only for
 * Provider, NurseMa, and SystemAdmin roles.
 *
 * Add/edit modal forms wired in T03:
 *   - Add [X] buttons gated to Provider | NurseMa | SystemAdmin
 *   - Edit buttons on each row gated to the same roles
 *   - Delete (allergy only) gated to Provider | SystemAdmin
 *   - All modals call reload() on success so DrugAllergyAlertBanner refreshes
 *
 * Observability:
 *   - Per-tab error banners with Retry button visible without DevTools
 *   - console.error tagged [useClinicalData] by the hook on fetch failure
 *   - console.error tagged [ClinicalSidebar] on mutation failure (from modal)
 *   - React DevTools: ClinicalSidebar → activeTab, addAllergyOpen, editAllergy,
 *     addProblemOpen, editProblem, addMedOpen, editMed, addImmunOpen
 *   - React DevTools: useClinicalData → all domain state
 */
import { useState } from "react";
import { useClinicalData } from "../../hooks/useClinicalData";
import {
  extractAllergyDisplay,
  extractProblemDisplay,
  extractMedicationDisplay,
  extractImmunizationDisplay,
} from "../../lib/fhirExtract";
import { AllergyFormModal } from "./AllergyFormModal";
import { ProblemFormModal } from "./ProblemFormModal";
import { MedicationFormModal } from "./MedicationFormModal";
import { ImmunizationFormModal } from "./ImmunizationFormModal";
import type { DrugAllergyAlert } from "../../types/documentation";
import type {
  AllergyRecord,
  ProblemRecord,
  MedicationRecord,
  ImmunizationRecord,
} from "../../types/patient";

// ─── Props ───────────────────────────────────────────────────────────────────

interface ClinicalSidebarProps {
  patientId: string;
  role: string;
}

// ─── Tab type ────────────────────────────────────────────────────────────────

type ActiveTab = "problems" | "medications" | "allergies" | "immunizations";

// ─── RBAC helper ─────────────────────────────────────────────────────────────

/** Returns true for roles that can add/edit clinical data. */
function canWrite(role: string): boolean {
  return role === "Provider" || role === "NurseMa" || role === "SystemAdmin";
}

// ─── DrugAllergyAlertBanner ───────────────────────────────────────────────────

/**
 * Inline sub-component that renders drug-allergy CDS alerts above the tab bar.
 * Returns null when alerts is empty — no empty wrapper div rendered.
 *
 * Alert severity colors:
 *   "contraindicated" → red (bg-red-50 border-red-200 text-red-800)
 *   "warning"         → amber (bg-amber-50 border-amber-200 text-amber-800)
 */
function DrugAllergyAlertBanner({ alerts }: { alerts: DrugAllergyAlert[] }) {
  if (alerts.length === 0) return null;

  return (
    <div className="mb-4 space-y-2" role="alert" aria-label="Drug-allergy alerts">
      {alerts.map((alert) => {
        const isContraindicated = alert.alertSeverity === "contraindicated";
        const containerCls = isContraindicated
          ? "rounded-md border border-red-200 bg-red-50 px-3 py-2.5 text-red-800"
          : "rounded-md border border-amber-200 bg-amber-50 px-3 py-2.5 text-amber-800";
        const badgeCls = isContraindicated
          ? "inline-flex items-center rounded-full bg-red-200 px-2 py-0.5 text-xs font-semibold uppercase text-red-900"
          : "inline-flex items-center rounded-full bg-amber-200 px-2 py-0.5 text-xs font-semibold uppercase text-amber-900";

        return (
          <div
            key={`${alert.medicationId}-${alert.allergyId}`}
            className={containerCls}
          >
            <div className="flex items-start gap-2">
              <span className="mt-0.5 text-base leading-none">⚠</span>
              <div className="flex-1">
                <p className="text-sm font-medium">{alert.message}</p>
              </div>
              <span className={badgeCls}>{alert.alertSeverity}</span>
            </div>
          </div>
        );
      })}
    </div>
  );
}

// ─── Status badge helpers ─────────────────────────────────────────────────────

/** Colored badge for problem clinical status. */
function ProblemStatusBadge({ status }: { status: string | null }) {
  if (!status) return <span className="text-xs text-gray-400">—</span>;
  const cls =
    status === "active"
      ? "bg-green-100 text-green-800"
      : status === "resolved"
        ? "bg-gray-100 text-gray-600"
        : status === "inactive"
          ? "bg-yellow-100 text-yellow-800"
          : "bg-gray-100 text-gray-600";
  return (
    <span className={`inline-flex rounded-full px-2 py-0.5 text-xs font-medium ${cls}`}>
      {status.charAt(0).toUpperCase() + status.slice(1)}
    </span>
  );
}

/** Colored badge for medication status. */
function MedStatusBadge({ status }: { status: string | null }) {
  if (!status) return <span className="text-xs text-gray-400">—</span>;
  const cls =
    status === "active"
      ? "bg-green-100 text-green-800"
      : status === "stopped" || status === "completed"
        ? "bg-gray-100 text-gray-600"
        : status === "on-hold"
          ? "bg-yellow-100 text-yellow-800"
          : status === "entered-in-error"
            ? "bg-red-100 text-red-800"
            : "bg-gray-100 text-gray-600";
  return (
    <span className={`inline-flex rounded-full px-2 py-0.5 text-xs font-medium ${cls}`}>
      {status.charAt(0).toUpperCase() + status.slice(1)}
    </span>
  );
}

/** Colored badge for allergy category. */
function AllergyCategBadge({ category }: { category: string | null }) {
  if (!category) return <span className="text-xs text-gray-400">—</span>;
  const cls =
    category === "drug"
      ? "bg-red-100 text-red-800"
      : category === "food"
        ? "bg-amber-100 text-amber-800"
        : category === "environment"
          ? "bg-green-100 text-green-800"
          : category === "biologic"
            ? "bg-purple-100 text-purple-800"
            : "bg-gray-100 text-gray-600";
  return (
    <span className={`inline-flex rounded-full px-2 py-0.5 text-xs font-medium ${cls}`}>
      {category.charAt(0).toUpperCase() + category.slice(1)}
    </span>
  );
}

/** Colored badge for allergy severity. */
function AllergySeverityBadge({ severity }: { severity: string | null }) {
  if (!severity) return <span className="text-xs text-gray-400">—</span>;
  const cls =
    severity === "severe"
      ? "bg-red-100 text-red-800"
      : severity === "moderate"
        ? "bg-amber-100 text-amber-800"
        : severity === "mild"
          ? "bg-blue-100 text-blue-800"
          : "bg-gray-100 text-gray-600";
  return (
    <span className={`inline-flex rounded-full px-2 py-0.5 text-xs font-medium ${cls}`}>
      {severity.charAt(0).toUpperCase() + severity.slice(1)}
    </span>
  );
}

// ─── Shared table shell ───────────────────────────────────────────────────────

/** Shared table wrapper for consistent tab panel layout. */
function TabTable({
  headers,
  children,
}: {
  headers: string[];
  children: React.ReactNode;
}) {
  return (
    <div className="overflow-x-auto">
      <table className="w-full text-sm">
        <thead>
          <tr className="border-b border-gray-100 text-left text-xs font-medium uppercase tracking-wide text-gray-500">
            {headers.map((h) => (
              <th key={h} className="pb-2 pr-4 last:pr-0">
                {h}
              </th>
            ))}
          </tr>
        </thead>
        <tbody className="divide-y divide-gray-50">{children}</tbody>
      </table>
    </div>
  );
}

/** Error banner shown when a tab's fetch failed. */
function TabErrorBanner({
  message,
  onRetry,
}: {
  message: string;
  onRetry: () => void;
}) {
  return (
    <div className="rounded-md border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-700">
      <p className="font-semibold">Failed to load data</p>
      <p className="mt-0.5 text-xs">{message}</p>
      <button
        type="button"
        onClick={onRetry}
        className="mt-2 rounded bg-red-600 px-3 py-1 text-xs font-medium text-white hover:bg-red-700 focus:outline-none focus:ring-2 focus:ring-red-500 focus:ring-offset-1"
      >
        Retry
      </button>
    </div>
  );
}

// ─── Tab panel header with "Add" button ──────────────────────────────────────

function TabPanelHeader({
  label,
  canAdd,
  onAdd,
}: {
  label: string;
  canAdd: boolean;
  onAdd: () => void;
}) {
  if (!canAdd) return null;
  return (
    <div className="mb-3 flex items-center justify-end">
      <button
        type="button"
        onClick={onAdd}
        className="rounded-md bg-indigo-600 px-3 py-1.5 text-xs font-medium text-white shadow-sm hover:bg-indigo-700 focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:ring-offset-1"
      >
        + Add {label}
      </button>
    </div>
  );
}

// ─── Problems tab panel ───────────────────────────────────────────────────────

function ProblemsPanel({
  problems,
  loading,
  error,
  canEdit,
  onRetry,
  onAdd,
  onEdit,
}: {
  problems: ProblemRecord[];
  loading: boolean;
  error: string | null;
  canEdit: boolean;
  onRetry: () => void;
  onAdd: () => void;
  onEdit: (record: ProblemRecord) => void;
}) {
  if (loading) {
    return <p className="text-sm text-gray-500">Loading…</p>;
  }
  if (error) {
    return <TabErrorBanner message={error} onRetry={onRetry} />;
  }

  return (
    <>
      <TabPanelHeader label="Problem" canAdd={canEdit} onAdd={onAdd} />
      {problems.length === 0 ? (
        <p className="text-sm text-gray-500">No active problems on file.</p>
      ) : (
        <TabTable headers={["ICD-10 Code", "Diagnosis", "Status", "Onset Date", ""]}>
          {problems.map((p) => {
            const d = extractProblemDisplay(p.resource);
            return (
              <tr key={p.id} className="hover:bg-gray-50">
                <td className="py-2 pr-4 font-mono text-xs text-gray-600">
                  {d.icd10Code ?? "—"}
                </td>
                <td className="py-2 pr-4 text-gray-900">{d.display ?? "—"}</td>
                <td className="py-2 pr-4">
                  <ProblemStatusBadge status={d.clinicalStatus} />
                </td>
                <td className="py-2 pr-4 text-gray-600">{d.onsetDate ?? "—"}</td>
                <td className="py-2 text-right">
                  {canEdit && (
                    <button
                      type="button"
                      onClick={() => onEdit(p)}
                      className="rounded px-2 py-0.5 text-xs font-medium text-indigo-600 hover:bg-indigo-50 focus:outline-none focus:ring-1 focus:ring-indigo-500"
                    >
                      Edit
                    </button>
                  )}
                </td>
              </tr>
            );
          })}
        </TabTable>
      )}
    </>
  );
}

// ─── Medications tab panel ────────────────────────────────────────────────────

function MedicationsPanel({
  medications,
  loading,
  error,
  canEdit,
  onRetry,
  onAdd,
  onEdit,
}: {
  medications: MedicationRecord[];
  loading: boolean;
  error: string | null;
  canEdit: boolean;
  onRetry: () => void;
  onAdd: () => void;
  onEdit: (record: MedicationRecord) => void;
}) {
  if (loading) {
    return <p className="text-sm text-gray-500">Loading…</p>;
  }
  if (error) {
    return <TabErrorBanner message={error} onRetry={onRetry} />;
  }

  return (
    <>
      <TabPanelHeader label="Medication" canAdd={canEdit} onAdd={onAdd} />
      {medications.length === 0 ? (
        <p className="text-sm text-gray-500">No medications on file.</p>
      ) : (
        <TabTable
          headers={["Drug Name", "RxNorm Code", "Status", "Dosage", "Effective Start", ""]}
        >
          {medications.map((m) => {
            const d = extractMedicationDisplay(m.resource);
            return (
              <tr key={m.id} className="hover:bg-gray-50">
                <td className="py-2 pr-4 text-gray-900">{d.drugName ?? "—"}</td>
                <td className="py-2 pr-4 font-mono text-xs text-gray-600">
                  {d.rxnormCode ?? "—"}
                </td>
                <td className="py-2 pr-4">
                  <MedStatusBadge status={d.status} />
                </td>
                <td className="py-2 pr-4 text-gray-600">{d.dosage ?? "—"}</td>
                <td className="py-2 pr-4 text-gray-600">{d.effectiveStart ?? "—"}</td>
                <td className="py-2 text-right">
                  {canEdit && (
                    <button
                      type="button"
                      onClick={() => onEdit(m)}
                      className="rounded px-2 py-0.5 text-xs font-medium text-indigo-600 hover:bg-indigo-50 focus:outline-none focus:ring-1 focus:ring-indigo-500"
                    >
                      Edit
                    </button>
                  )}
                </td>
              </tr>
            );
          })}
        </TabTable>
      )}
    </>
  );
}

// ─── Allergies tab panel ──────────────────────────────────────────────────────

function AllergiesPanel({
  allergies,
  loading,
  error,
  canEdit,
  onRetry,
  onAdd,
  onEdit,
}: {
  allergies: AllergyRecord[];
  loading: boolean;
  error: string | null;
  canEdit: boolean;
  onRetry: () => void;
  onAdd: () => void;
  onEdit: (record: AllergyRecord) => void;
}) {
  if (loading) {
    return <p className="text-sm text-gray-500">Loading…</p>;
  }
  if (error) {
    return <TabErrorBanner message={error} onRetry={onRetry} />;
  }

  return (
    <>
      <TabPanelHeader label="Allergy" canAdd={canEdit} onAdd={onAdd} />
      {allergies.length === 0 ? (
        <p className="text-sm text-gray-500">No allergies on file.</p>
      ) : (
        <TabTable
          headers={["Substance", "Category", "Severity", "Reaction", "Clinical Status", ""]}
        >
          {allergies.map((a) => {
            const d = extractAllergyDisplay(a.resource);
            return (
              <tr key={a.id} className="hover:bg-gray-50">
                <td className="py-2 pr-4 text-gray-900">{d.substance ?? "—"}</td>
                <td className="py-2 pr-4">
                  <AllergyCategBadge category={d.category} />
                </td>
                <td className="py-2 pr-4">
                  <AllergySeverityBadge severity={d.severity} />
                </td>
                <td className="py-2 pr-4 text-gray-600">{d.reaction ?? "—"}</td>
                <td className="py-2 pr-4 text-gray-600">{d.clinicalStatus ?? "—"}</td>
                <td className="py-2 text-right">
                  {canEdit && (
                    <button
                      type="button"
                      onClick={() => onEdit(a)}
                      className="rounded px-2 py-0.5 text-xs font-medium text-indigo-600 hover:bg-indigo-50 focus:outline-none focus:ring-1 focus:ring-indigo-500"
                    >
                      Edit
                    </button>
                  )}
                </td>
              </tr>
            );
          })}
        </TabTable>
      )}
    </>
  );
}

// ─── Immunizations tab panel ──────────────────────────────────────────────────

function ImmunizationsPanel({
  immunizations,
  loading,
  error,
  canEdit,
  onRetry,
  onAdd,
}: {
  immunizations: ImmunizationRecord[];
  loading: boolean;
  error: string | null;
  canEdit: boolean;
  onRetry: () => void;
  onAdd: () => void;
}) {
  if (loading) {
    return <p className="text-sm text-gray-500">Loading…</p>;
  }
  if (error) {
    return <TabErrorBanner message={error} onRetry={onRetry} />;
  }

  return (
    <>
      <TabPanelHeader label="Immunization" canAdd={canEdit} onAdd={onAdd} />
      {immunizations.length === 0 ? (
        <p className="text-sm text-gray-500">No immunizations on file.</p>
      ) : (
        <TabTable
          headers={["Vaccine Name", "CVX Code", "Date Administered", "Lot #", "Status"]}
        >
          {immunizations.map((imm) => {
            const d = extractImmunizationDisplay(imm.resource);
            return (
              <tr key={imm.id} className="hover:bg-gray-50">
                <td className="py-2 pr-4 text-gray-900">{d.vaccineName ?? "—"}</td>
                <td className="py-2 pr-4 font-mono text-xs text-gray-600">
                  {d.cvxCode ?? "—"}
                </td>
                <td className="py-2 pr-4 text-gray-600">
                  {d.occurrenceDate ?? "—"}
                </td>
                <td className="py-2 pr-4 font-mono text-xs text-gray-600">
                  {d.lotNumber ?? "—"}
                </td>
                <td className="py-2 text-gray-600">{d.status ?? "—"}</td>
              </tr>
            );
          })}
        </TabTable>
      )}
    </>
  );
}

// ─── Main component ──────────────────────────────────────────────────────────

/**
 * ClinicalSidebar — four-tab clinical data panel with add/edit/delete modals.
 *
 * Calls useClinicalData at its own top level so tab state is preserved even
 * when PatientDetailPage re-renders due to its own state changes.
 */
export function ClinicalSidebar({ patientId, role }: ClinicalSidebarProps) {
  const {
    allergies,
    problems,
    medications,
    immunizations,
    alerts,
    loading,
    errorAllergies,
    errorProblems,
    errorMedications,
    errorImmunizations,
    reload,
    addAllergy,
    updateAllergy,
    deleteAllergy,
    addProblem,
    updateProblem,
    addMedication,
    updateMedication,
    addImmunization,
  } = useClinicalData(patientId);

  const [activeTab, setActiveTab] = useState<ActiveTab>("problems");

  // ── Per-tab modal state ─────────────────────────────────────────────────
  const [addAllergyOpen, setAddAllergyOpen] = useState(false);
  const [editAllergy, setEditAllergy] = useState<AllergyRecord | null>(null);

  const [addProblemOpen, setAddProblemOpen] = useState(false);
  const [editProblem, setEditProblem] = useState<ProblemRecord | null>(null);

  const [addMedOpen, setAddMedOpen] = useState(false);
  const [editMed, setEditMed] = useState<MedicationRecord | null>(null);

  const [addImmunOpen, setAddImmunOpen] = useState(false);

  const write = canWrite(role);

  const tabs: { id: ActiveTab; label: string }[] = [
    { id: "problems", label: "Problems" },
    { id: "medications", label: "Medications" },
    { id: "allergies", label: "Allergies" },
    { id: "immunizations", label: "Immunizations" },
  ];

  // ── Modal success handlers ──────────────────────────────────────────────

  function handleAllergySuccess() {
    reload();
    setAddAllergyOpen(false);
    setEditAllergy(null);
  }

  function handleProblemSuccess() {
    reload();
    setAddProblemOpen(false);
    setEditProblem(null);
  }

  function handleMedSuccess() {
    reload();
    setAddMedOpen(false);
    setEditMed(null);
  }

  function handleImmunSuccess() {
    reload();
    setAddImmunOpen(false);
  }

  return (
    <div>
      {/* ── Drug-Allergy Alert Banner ──────────────────────────────────── */}
      <DrugAllergyAlertBanner alerts={alerts} />

      {/* ── Overall loading skeleton ───────────────────────────────────── */}
      {loading && (
        <div className="animate-pulse space-y-3 py-2">
          <div className="flex gap-2">
            <div className="h-8 w-24 rounded bg-gray-200" />
            <div className="h-8 w-24 rounded bg-gray-200" />
            <div className="h-8 w-24 rounded bg-gray-200" />
            <div className="h-8 w-24 rounded bg-gray-200" />
          </div>
          <div className="h-4 w-1/2 rounded bg-gray-200" />
          <div className="h-4 w-2/3 rounded bg-gray-200" />
          <div className="h-4 w-1/3 rounded bg-gray-200" />
        </div>
      )}

      {/* ── Tab chrome (visible once loading completes) ────────────────── */}
      {!loading && (
        <>
          {/* Tab button row — same active/inactive pattern as EncounterWorkspace */}
          <div className="flex gap-1 border-b border-gray-200 pb-0">
            {tabs.map(({ id, label }) => (
              <button
                key={id}
                type="button"
                onClick={() => setActiveTab(id)}
                className={[
                  "rounded-t-md px-5 py-2 text-sm font-medium focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:ring-offset-1",
                  activeTab === id
                    ? "border-b-2 border-indigo-600 text-indigo-700 bg-white"
                    : "text-gray-500 hover:text-gray-700 hover:bg-gray-50",
                ].join(" ")}
                aria-selected={activeTab === id}
                role="tab"
              >
                {label}
              </button>
            ))}
          </div>

          {/* Tab body */}
          <div className="mt-4 rounded-b-lg rounded-tr-lg border border-gray-200 bg-white p-4">
            {activeTab === "problems" && (
              <ProblemsPanel
                problems={problems}
                loading={false}
                error={errorProblems}
                canEdit={write}
                onRetry={reload}
                onAdd={() => setAddProblemOpen(true)}
                onEdit={(p) => setEditProblem(p)}
              />
            )}
            {activeTab === "medications" && (
              <MedicationsPanel
                medications={medications}
                loading={false}
                error={errorMedications}
                canEdit={write}
                onRetry={reload}
                onAdd={() => setAddMedOpen(true)}
                onEdit={(m) => setEditMed(m)}
              />
            )}
            {activeTab === "allergies" && (
              <AllergiesPanel
                allergies={allergies}
                loading={false}
                error={errorAllergies}
                canEdit={write}
                onRetry={reload}
                onAdd={() => setAddAllergyOpen(true)}
                onEdit={(a) => setEditAllergy(a)}
              />
            )}
            {activeTab === "immunizations" && (
              <ImmunizationsPanel
                immunizations={immunizations}
                loading={false}
                error={errorImmunizations}
                canEdit={write}
                onRetry={reload}
                onAdd={() => setAddImmunOpen(true)}
              />
            )}
          </div>
        </>
      )}

      {/* ── Modals ────────────────────────────────────────────────────── */}

      {/* Allergy modal — add or edit */}
      {(addAllergyOpen || editAllergy !== null) && (
        <AllergyFormModal
          patientId={patientId}
          initial={editAllergy}
          role={role}
          onAdd={addAllergy}
          onUpdate={updateAllergy}
          onDelete={deleteAllergy}
          onSuccess={handleAllergySuccess}
          onClose={() => {
            setAddAllergyOpen(false);
            setEditAllergy(null);
          }}
        />
      )}

      {/* Problem modal — add or edit */}
      {(addProblemOpen || editProblem !== null) && (
        <ProblemFormModal
          patientId={patientId}
          initial={editProblem}
          onAdd={addProblem}
          onUpdate={updateProblem}
          onSuccess={handleProblemSuccess}
          onClose={() => {
            setAddProblemOpen(false);
            setEditProblem(null);
          }}
        />
      )}

      {/* Medication modal — add or edit */}
      {(addMedOpen || editMed !== null) && (
        <MedicationFormModal
          patientId={patientId}
          initial={editMed}
          onAdd={addMedication}
          onUpdate={updateMedication}
          onSuccess={handleMedSuccess}
          onClose={() => {
            setAddMedOpen(false);
            setEditMed(null);
          }}
        />
      )}

      {/* Immunization modal — add only */}
      {addImmunOpen && (
        <ImmunizationFormModal
          patientId={patientId}
          onAdd={addImmunization}
          onSuccess={handleImmunSuccess}
          onClose={() => setAddImmunOpen(false)}
        />
      )}
    </div>
  );
}

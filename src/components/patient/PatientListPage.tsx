/**
 * PatientListPage.tsx — Searchable patient roster with RBAC-gated "New Patient" button.
 *
 * Calls commands.searchPatients on mount (empty query → up to 50 results) and
 * re-fires on debounced (300 ms) text input changes. Rows show MRN, full name,
 * DOB, gender, and phone. Clicking a row navigates to patient-detail.
 *
 * The "New Patient" button is rendered only for roles with Create permission on
 * Patients: Provider, NurseMa, FrontDesk, SystemAdmin.
 *
 * PatientFormModal (built in T03) will be imported here; for T01 the button
 * opens a placeholder message instead of the real modal.
 *
 * Table + useEffect fetch pattern mirrors AuditLog.tsx.
 * Input styling mirrors LoginForm.tsx.
 */
import { useState, useEffect, useCallback } from "react";
import { commands } from "../../lib/tauri";
import { useNav } from "../../contexts/RouterContext";
import type { PatientSummary } from "../../types/patient";
import { PatientFormModal } from "./PatientFormModal";

export interface PatientListPageProps {
  /** Role string from parent (passed in from PatientsPage via useAuth). */
  role: string;
}

/** Roles that are allowed to create new patient records. */
const CREATE_ROLES = ["Provider", "NurseMa", "FrontDesk", "SystemAdmin"];

/** Format a PatientSummary's given + family name for display. */
function formatName(row: PatientSummary): string {
  return `${row.givenNames.join(" ")} ${row.familyName}`.trim();
}

/** Format an ISO date string (YYYY-MM-DD) as a more readable form, or "—" if null. */
function formatDate(iso: string | null): string {
  if (!iso) return "—";
  try {
    // Parse as local date to avoid timezone shift on YYYY-MM-DD strings
    const [year, month, day] = iso.split("-");
    return `${month}/${day}/${year}`;
  } catch {
    return iso;
  }
}

export function PatientListPage({ role }: PatientListPageProps) {
  const { navigate } = useNav();

  const [patients, setPatients] = useState<PatientSummary[]>([]);
  const [query, setQuery] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [showCreateModal, setShowCreateModal] = useState(false);

  const canCreate = CREATE_ROLES.includes(role);

  /** Fetch patients matching the current query. */
  const fetchPatients = useCallback(async (searchQuery: string) => {
    setLoading(true);
    setError(null);
    try {
      const results = await commands.searchPatients({
        name: searchQuery || null,
        mrn: null,
        birthDate: null,
        limit: null,
      });
      setPatients(results);
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      setError(msg);
      console.error("[PatientListPage] searchPatients failed:", msg);
    } finally {
      setLoading(false);
    }
  }, []);

  // On mount: load initial patient list (empty query → up to 50 results).
  useEffect(() => {
    fetchPatients("");
  }, [fetchPatients]);

  // Debounce query input: re-fire search 300 ms after the user stops typing.
  useEffect(() => {
    const timer = setTimeout(() => {
      fetchPatients(query);
    }, 300);
    return () => clearTimeout(timer);
  }, [query, fetchPatients]);

  return (
    <div className="p-6">
      {/* ── Page header ─────────────────────────────────────────────── */}
      <div className="mb-5 flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold text-gray-900">Patients</h1>
          <p className="mt-0.5 text-sm text-gray-500">
            Search and manage patient records
          </p>
        </div>

        {/* RBAC-gated New Patient button */}
        {canCreate && (
          <button
            onClick={() => setShowCreateModal(true)}
            className="rounded-md bg-blue-600 px-4 py-2 text-sm font-medium text-white hover:bg-blue-700 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:ring-offset-2"
          >
            + New Patient
          </button>
        )}
      </div>

      {/* Create patient modal */}
      {showCreateModal && (
        <PatientFormModal
          patientId={null}
          initialDisplay={null}
          onSuccess={(id) => {
            setShowCreateModal(false);
            navigate({ page: "patient-detail", patientId: id });
          }}
          onClose={() => setShowCreateModal(false)}
        />
      )}

      {/* ── Search input ─────────────────────────────────────────────── */}
      <div className="mb-4">
        <label
          htmlFor="patient-search"
          className="mb-1 block text-sm font-medium text-gray-700"
        >
          Search patients
        </label>
        <input
          id="patient-search"
          type="search"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Name or MRN…"
          className="w-full max-w-md rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
        />
      </div>

      {/* ── "Showing first N results" hint ──────────────────────────── */}
      {patients.length >= 50 && (
        <p className="mb-3 text-xs text-amber-700 bg-amber-50 border border-amber-200 rounded px-3 py-2">
          Showing first {patients.length} results — refine your search to narrow results.
        </p>
      )}

      {/* ── Error state ──────────────────────────────────────────────── */}
      {error && (
        <div className="mb-4 rounded-md border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700">
          Failed to load patients: {error}
        </div>
      )}

      {/* ── Patient table ─────────────────────────────────────────────── */}
      <div className="overflow-x-auto rounded-lg border border-gray-200 bg-white shadow-sm">
        <table className="w-full text-sm">
          <thead>
            <tr className="border-b border-gray-100 bg-gray-50 text-left text-xs font-medium uppercase tracking-wide text-gray-500">
              <th className="px-4 py-3">MRN</th>
              <th className="px-4 py-3">Name</th>
              <th className="px-4 py-3">Date of Birth</th>
              <th className="px-4 py-3">Gender</th>
              <th className="px-4 py-3">Phone</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-gray-50">
            {/* Loading state */}
            {loading && (
              <tr>
                <td colSpan={5} className="px-4 py-8 text-center text-gray-400">
                  Loading…
                </td>
              </tr>
            )}

            {/* Empty state (not loading, no error, no results) */}
            {!loading && !error && patients.length === 0 && (
              <tr>
                <td colSpan={5} className="px-4 py-8 text-center text-gray-400">
                  No patients found — try a different search
                </td>
              </tr>
            )}

            {/* Patient rows */}
            {!loading &&
              patients.map((row) => (
                <tr
                  key={row.id}
                  onClick={() =>
                    navigate({ page: "patient-detail", patientId: row.id })
                  }
                  className="cursor-pointer transition-colors hover:bg-gray-50"
                  role="button"
                  tabIndex={0}
                  onKeyDown={(e) => {
                    if (e.key === "Enter" || e.key === " ") {
                      navigate({ page: "patient-detail", patientId: row.id });
                    }
                  }}
                >
                  <td className="whitespace-nowrap px-4 py-2.5 font-mono text-xs text-gray-600">
                    {row.mrn}
                  </td>
                  <td className="px-4 py-2.5 font-medium text-gray-900">
                    {formatName(row)}
                  </td>
                  <td className="whitespace-nowrap px-4 py-2.5 text-gray-700">
                    {formatDate(row.birthDate)}
                  </td>
                  <td className="px-4 py-2.5 text-gray-700">
                    {row.gender ?? "—"}
                  </td>
                  <td className="whitespace-nowrap px-4 py-2.5 text-gray-700">
                    {row.phone ?? "—"}
                  </td>
                </tr>
              ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}

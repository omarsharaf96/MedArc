/**
 * PatientsPage.tsx — Thin wrapper that resolves the authenticated user's role
 * and renders PatientListPage.
 *
 * This component is the navigation shell's render target for the "patients"
 * route. All page logic lives in PatientListPage; this wrapper keeps the hook
 * boundary separate so PatientListPage remains testable with an injected role.
 */
import { useAuth } from "../hooks/useAuth";
import { PatientListPage } from "../components/patient/PatientListPage";

export function PatientsPage() {
  const auth = useAuth();

  if (auth.loading) {
    return (
      <div className="p-6 text-sm text-gray-500">Loading…</div>
    );
  }

  return <PatientListPage role={auth.user?.role ?? "unknown"} />;
}

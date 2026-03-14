/**
 * ContentArea.tsx — Route-to-page dispatcher.
 *
 * Reads `currentRoute` from `useNav()` and renders the matching page component.
 * Includes an exhaustive fallback for unrecognized routes so the UI never
 * renders a blank screen — a visible "Unknown page" message acts as an
 * observable failure signal for missing route handlers.
 *
 * Adding a new page: add a `case` here and import the page component.
 */

import { useNav } from "../../contexts/RouterContext";
import { PatientsPage } from "../../pages/PatientsPage";
import { PatientDetailPage } from "../../pages/PatientDetailPage";
import { EncounterWorkspace } from "../../pages/EncounterWorkspace";
import { SchedulePage } from "../../pages/SchedulePage";
import { SettingsPage } from "../../pages/SettingsPage";
import { AuditLogPage } from "../../pages/AuditLogPage";
import { PTNotesPage } from "../../pages/PTNotesPage";
import { PTNoteFormPage } from "../../pages/PTNoteFormPage";
import { ObjectiveMeasuresPage } from "../../pages/ObjectiveMeasuresPage";
import { DocumentCenterPage } from "../../pages/DocumentCenterPage";
import { SurveyBuilderPage } from "../../pages/SurveyBuilderPage";
import { SurveyKioskPage } from "../../pages/SurveyKioskPage";
import { VoiceToNotePage } from "../../pages/VoiceToNotePage";
import { ExportPage } from "../../pages/ExportPage";
import { FaxPage } from "../../pages/FaxPage";
import { HEPBuilderPage } from "../../pages/HEPBuilderPage";
import { BillingPage } from "../../pages/BillingPage";
import { useAuth } from "../../hooks/useAuth";

// ─── Unknown route fallback ─────────────────────────────────────────────────

/** Rendered when `currentRoute.page` doesn't match any known route. */
function UnknownPage({ page }: { page: string }) {
  return (
    <div className="p-6">
      <h1 className="text-2xl font-bold text-red-600">Unknown page</h1>
      <p className="mt-2 text-gray-500">
        No page component is registered for route: <code className="rounded bg-gray-100 px-1 py-0.5 font-mono text-sm">{page}</code>
      </p>
      <p className="mt-1 text-xs text-gray-400">
        This is a developer error — add a case to ContentArea.tsx for this route.
      </p>
    </div>
  );
}

// ─── Component ──────────────────────────────────────────────────────────────

/**
 * Renders the page component for the current route.
 *
 * The `patient-detail` route renders PatientDetailPage with the patientId
 * from the route and the user's role from useAuth.
 */
export function ContentArea() {
  const { currentRoute } = useNav();
  const { user } = useAuth();

  switch (currentRoute.page) {
    case "patients":
      return <PatientsPage />;
    case "patient-detail":
      return (
        <PatientDetailPage
          patientId={currentRoute.patientId}
          role={user?.role ?? ""}
          userId={user?.id ?? ""}
        />
      );
    case "encounter-workspace":
      return (
        <EncounterWorkspace
          patientId={currentRoute.patientId}
          encounterId={currentRoute.encounterId}
          role={user?.role ?? ""}
          userId={user?.id ?? ""}
        />
      );
    case "schedule":
      return <SchedulePage />;
    case "settings":
      return <SettingsPage />;
    case "audit-log":
      return <AuditLogPage />;
    case "pt-notes":
      return (
        <PTNotesPage
          patientId={currentRoute.patientId}
          role={user?.role ?? ""}
        />
      );
    case "pt-note-detail":
      return (
        <PTNoteFormPage
          patientId={currentRoute.patientId}
          noteType={currentRoute.noteType}
          ptNoteId={currentRoute.ptNoteId}
          role={user?.role ?? ""}
        />
      );
    case "outcome-measures":
      return (
        <ObjectiveMeasuresPage
          patientId={currentRoute.patientId}
          role={user?.role ?? ""}
          userId={user?.id ?? ""}
        />
      );
    case "document-center":
      return (
        <DocumentCenterPage
          patientId={currentRoute.patientId}
          role={user?.role ?? ""}
          userId={user?.id ?? ""}
        />
      );
    case "survey-builder":
      return (
        <SurveyBuilderPage
          role={user?.role ?? ""}
          userId={user?.id ?? ""}
        />
      );
    case "survey-kiosk":
      return (
        <SurveyKioskPage
          patientId={currentRoute.patientId}
          templateId={currentRoute.templateId}
        />
      );
    case "voice-to-note":
      return (
        <VoiceToNotePage
          patientId={currentRoute.patientId}
          noteType={currentRoute.noteType}
          role={user?.role ?? ""}
          userId={user?.id ?? ""}
        />
      );
    case "export":
      return (
        <ExportPage
          patientId={currentRoute.patientId}
          role={user?.role ?? ""}
          userId={user?.id ?? ""}
        />
      );
    case "fax":
      return <FaxPage />;
    case "hep-builder":
      return (
        <HEPBuilderPage
          patientId={currentRoute.patientId}
          encounterId={currentRoute.encounterId}
          role={user?.role ?? ""}
          userId={user?.id ?? ""}
        />
      );
    case "billing":
      return (
        <BillingPage
          patientId={currentRoute.patientId}
          encounterId={currentRoute.encounterId}
          role={user?.role ?? ""}
        />
      );
    default: {
      // Exhaustiveness guard: TypeScript will warn if a Route variant is unhandled.
      // Cast to string so we can display the unknown page value at runtime.
      const unhandled: never = currentRoute;
      return <UnknownPage page={(unhandled as { page: string }).page} />;
    }
  }
}

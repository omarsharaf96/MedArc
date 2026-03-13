/**
 * AuditLogPage.tsx — Placeholder for the Audit Log section.
 *
 * Full implementation in S07. This component exists so the navigation shell
 * has a real render target for the "audit-log" route.
 * Accessible to SystemAdmin only (enforced by Sidebar nav item visibility).
 */

export function AuditLogPage() {
  return (
    <div className="p-6">
      <h1 className="text-2xl font-bold text-gray-900">Audit Log</h1>
      <p className="mt-2 text-gray-500">Audit log viewer coming in S07.</p>
    </div>
  );
}

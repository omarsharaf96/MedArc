import { useEffect, useState, useCallback } from "react";
import { commands } from "../lib/tauri";
import type { FhirResource, FhirResourceList } from "../types/fhir";

export default function FhirExplorer() {
  const [resourceList, setResourceList] = useState<FhirResourceList | null>(
    null,
  );
  const [creating, setCreating] = useState(false);
  const [deletingId, setDeletingId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const loadResources = useCallback(async () => {
    try {
      const list = await commands.listResources();
      setResourceList(list);
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    loadResources();
  }, [loadResources]);

  const handleCreatePatient = async () => {
    setCreating(true);
    setError(null);
    try {
      await commands.createResource({
        resourceType: "Patient",
        resource: {
          resourceType: "Patient",
          name: [{ family: "Smith", given: ["John"] }],
          birthDate: "1985-03-15",
          gender: "male",
          telecom: [{ system: "phone", value: "555-0123" }],
        },
      });
      await loadResources();
    } catch (e) {
      setError(String(e));
    } finally {
      setCreating(false);
    }
  };

  const handleDelete = async (id: string) => {
    setDeletingId(id);
    setError(null);
    try {
      await commands.deleteResource(id);
      await loadResources();
    } catch (e) {
      setError(String(e));
    } finally {
      setDeletingId(null);
    }
  };

  const formatTimestamp = (iso: string): string => {
    try {
      const date = new Date(iso);
      return date.toLocaleString();
    } catch {
      return iso;
    }
  };

  const total = resourceList?.total ?? 0;
  const resources: FhirResource[] = resourceList?.resources ?? [];

  return (
    <div className="rounded-lg border border-gray-200 bg-white p-6 shadow-sm">
      {/* Header */}
      <div className="mb-4 flex items-center justify-between">
        <h2 className="text-lg font-semibold text-gray-800">FHIR Resources</h2>
        <span className="rounded-full bg-blue-100 px-3 py-1 text-sm font-medium text-blue-800">
          {total} total
        </span>
      </div>

      {/* Error */}
      {error && (
        <div className="mb-4 rounded-md border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-700">
          {error}
        </div>
      )}

      {/* Resource list */}
      {resources.length > 0 ? (
        <div className="mb-4 max-h-80 space-y-2 overflow-y-auto">
          {resources.map((r) => (
            <div
              key={r.id}
              className="flex items-center justify-between rounded-md border border-gray-100 bg-gray-50 px-4 py-3"
            >
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-2">
                  <span className="rounded bg-blue-50 px-2 py-0.5 text-xs font-semibold text-blue-700">
                    {r.resourceType}
                  </span>
                  <span className="truncate font-mono text-xs text-gray-400">
                    {r.id.slice(0, 8)}...
                  </span>
                </div>
                <p className="mt-1 text-xs text-gray-500">
                  Updated: {formatTimestamp(r.lastUpdated)}
                </p>
              </div>
              <button
                onClick={() => handleDelete(r.id)}
                disabled={deletingId === r.id}
                className="ml-3 rounded px-2 py-1 text-xs font-medium text-red-600 transition-colors hover:bg-red-50 hover:text-red-700 disabled:cursor-not-allowed disabled:opacity-50"
              >
                {deletingId === r.id ? "Deleting..." : "Delete"}
              </button>
            </div>
          ))}
        </div>
      ) : (
        <p className="mb-4 text-sm text-gray-500">
          No resources yet. Create a test patient to verify the database.
        </p>
      )}

      {/* Create button */}
      <button
        onClick={handleCreatePatient}
        disabled={creating}
        className="rounded-md bg-blue-600 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-blue-700 disabled:cursor-not-allowed disabled:opacity-50"
      >
        {creating ? "Creating..." : "Create Test Patient"}
      </button>
    </div>
  );
}

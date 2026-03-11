import { useEffect, useState, useCallback } from "react";
import { commands } from "./lib/tauri";
import type { DbStatus, FhirResourceList } from "./types/fhir";

function App() {
  const [dbStatus, setDbStatus] = useState<DbStatus | null>(null);
  const [resourceList, setResourceList] = useState<FhirResourceList | null>(
    null,
  );
  const [error, setError] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);

  const loadData = useCallback(async () => {
    try {
      const [status, resources] = await Promise.all([
        commands.checkDb(),
        commands.listResources(),
      ]);
      setDbStatus(status);
      setResourceList(resources);
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    loadData();
  }, [loadData]);

  const handleCreateTestResource = async () => {
    setCreating(true);
    try {
      await commands.createResource({
        resourceType: "Patient",
        resource: {
          resourceType: "Patient",
          name: [{ family: "Test", given: ["User"] }],
          birthDate: "1990-01-01",
        },
      });
      await loadData();
    } catch (e) {
      setError(String(e));
    } finally {
      setCreating(false);
    }
  };

  return (
    <div className="min-h-screen bg-gray-50 p-8">
      <div className="mx-auto max-w-2xl">
        {/* Header */}
        <div className="mb-8 text-center">
          <h1 className="text-4xl font-bold text-gray-900">MedArc</h1>
          <p className="mt-1 text-lg text-gray-600">AI-Powered Desktop EMR</p>
        </div>

        {/* Error Banner */}
        {error && (
          <div className="mb-6 rounded-lg border border-red-200 bg-red-50 p-4 text-sm text-red-700">
            {error}
          </div>
        )}

        {/* Database Status Card */}
        <div className="mb-6 rounded-lg border border-gray-200 bg-white p-6 shadow-sm">
          <h2 className="mb-4 text-lg font-semibold text-gray-800">
            Database Status
          </h2>
          {dbStatus ? (
            <div className="space-y-3">
              <div className="flex items-center gap-2">
                <span className="inline-block h-3 w-3 rounded-full bg-green-500" />
                <span className="font-medium text-gray-700">
                  Database: Encrypted
                </span>
              </div>
              <div className="grid grid-cols-2 gap-4 text-sm text-gray-600">
                <div>
                  <span className="font-medium">Cipher Version:</span>{" "}
                  {dbStatus.cipher_version}
                </div>
                <div>
                  <span className="font-medium">Page Count:</span>{" "}
                  {dbStatus.page_count}
                </div>
              </div>
            </div>
          ) : (
            <div className="flex items-center gap-2">
              <span className="inline-block h-3 w-3 rounded-full bg-gray-400" />
              <span className="text-sm text-gray-500">Connecting...</span>
            </div>
          )}
        </div>

        {/* FHIR Resources Card */}
        <div className="mb-6 rounded-lg border border-gray-200 bg-white p-6 shadow-sm">
          <div className="mb-4 flex items-center justify-between">
            <h2 className="text-lg font-semibold text-gray-800">
              FHIR Resources
            </h2>
            <span className="rounded-full bg-blue-100 px-3 py-1 text-sm font-medium text-blue-800">
              {resourceList?.total ?? 0} total
            </span>
          </div>

          {/* Resource List */}
          {resourceList && resourceList.resources.length > 0 ? (
            <div className="mb-4 max-h-64 space-y-2 overflow-y-auto">
              {resourceList.resources.map((r) => (
                <div
                  key={r.id}
                  className="flex items-center justify-between rounded border border-gray-100 bg-gray-50 px-3 py-2 text-sm"
                >
                  <div>
                    <span className="font-medium text-gray-700">
                      {r.resourceType}
                    </span>
                    <span className="ml-2 text-gray-400">
                      v{r.versionId}
                    </span>
                  </div>
                  <span className="font-mono text-xs text-gray-400">
                    {r.id.slice(0, 8)}...
                  </span>
                </div>
              ))}
            </div>
          ) : (
            <p className="mb-4 text-sm text-gray-500">
              No resources yet. Create one to get started.
            </p>
          )}

          {/* Create Test Resource Button */}
          <button
            onClick={handleCreateTestResource}
            disabled={creating}
            className="rounded-md bg-blue-600 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-blue-700 disabled:cursor-not-allowed disabled:opacity-50"
          >
            {creating ? "Creating..." : "Create Test Resource"}
          </button>
        </div>
      </div>
    </div>
  );
}

export default App;

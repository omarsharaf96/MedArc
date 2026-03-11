import { useEffect, useState } from "react";
import { commands } from "../lib/tauri";
import type { DbStatus, AppInfo } from "../types/fhir";

interface DatabaseStatusState {
  status: "loading" | "connected" | "error";
  dbStatus: DbStatus | null;
  appInfo: AppInfo | null;
  error: string | null;
}

export default function DatabaseStatus() {
  const [state, setState] = useState<DatabaseStatusState>({
    status: "loading",
    dbStatus: null,
    appInfo: null,
    error: null,
  });

  useEffect(() => {
    async function fetchStatus() {
      try {
        const [dbStatus, appInfo] = await Promise.all([
          commands.checkDb(),
          commands.getAppInfo(),
        ]);
        setState({
          status: "connected",
          dbStatus,
          appInfo,
          error: null,
        });
      } catch (e) {
        setState({
          status: "error",
          dbStatus: null,
          appInfo: null,
          error: String(e),
        });
      }
    }
    fetchStatus();
  }, []);

  if (state.status === "loading") {
    return (
      <div className="rounded-lg border border-gray-200 bg-white p-6 shadow-sm">
        <h2 className="mb-4 text-lg font-semibold text-gray-800">
          Database Status
        </h2>
        <div className="flex items-center gap-2">
          <span className="inline-block h-3 w-3 animate-pulse rounded-full bg-gray-400" />
          <span className="text-sm text-gray-500">Connecting...</span>
        </div>
      </div>
    );
  }

  if (state.status === "error") {
    return (
      <div className="rounded-lg border border-red-200 bg-white p-6 shadow-sm">
        <h2 className="mb-4 text-lg font-semibold text-gray-800">
          Database Status
        </h2>
        <div className="flex items-center gap-2">
          <span className="inline-block h-3 w-3 rounded-full bg-red-500" />
          <span className="font-medium text-red-700">Connection Error</span>
        </div>
        <p className="mt-2 text-sm text-red-600">{state.error}</p>
      </div>
    );
  }

  const { dbStatus, appInfo } = state;

  return (
    <div className="rounded-lg border border-gray-200 bg-white p-6 shadow-sm">
      <h2 className="mb-4 text-lg font-semibold text-gray-800">
        Database Status
      </h2>

      {/* Encryption indicator */}
      <div className="mb-4 flex items-center gap-2">
        <span className="inline-block h-3 w-3 rounded-full bg-green-500" />
        <span className="font-medium text-gray-700">Encrypted</span>
      </div>

      {/* Details grid */}
      <div className="grid grid-cols-2 gap-x-6 gap-y-3 text-sm">
        <div>
          <span className="text-gray-500">SQLCipher Version</span>
          <p className="font-mono font-medium text-gray-800">
            {dbStatus?.cipher_version}
          </p>
        </div>
        <div>
          <span className="text-gray-500">Page Count</span>
          <p className="font-mono font-medium text-gray-800">
            {dbStatus?.page_count}
          </p>
        </div>
        <div>
          <span className="text-gray-500">App Version</span>
          <p className="font-mono font-medium text-gray-800">
            {appInfo?.version}
          </p>
        </div>
        <div>
          <span className="text-gray-500">Database Path</span>
          <p
            className="truncate font-mono text-xs font-medium text-gray-800"
            title={appInfo?.db_path}
          >
            {appInfo?.db_path}
          </p>
        </div>
      </div>
    </div>
  );
}

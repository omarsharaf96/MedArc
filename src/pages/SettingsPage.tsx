/**
 * SettingsPage.tsx — Four-tab Settings panel: Backup | Security | Fax | Account
 *
 * Replaces the S06 placeholder. Wired to real backend commands via tauri.ts.
 *
 * Tabs:
 *   - Backup:   folder picker, create backup, history table, restore (SystemAdmin only)
 *   - Security: TOTP setup/disable, Touch ID enable/disable
 *   - Fax:      Phaxio API key/secret, practice fax number, test connection
 *   - Account:  session info (read-only), sign-out
 *
 * Observability:
 *   - backupError / mfaError / biometricError / faxError: inline red banners
 *   - lastResult: inline success toast with file path and SHA-256 prefix
 *   - history table surfaces status / errorMessage per log entry
 */

import { useState, useEffect, useCallback } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { commands } from "../lib/tauri";
import { useAuth } from "../hooks/useAuth";
import MfaSetup from "../components/auth/MfaSetup";
import type { BackupResult, BackupLogEntry } from "../types/backup";
import type { BiometricStatus } from "../types/auth";
import type { UserListEntry } from "../types/mips";
import type { ReminderConfigRecord, ReminderConfigInput } from "../types/reminders";

// ─── Helpers ──────────────────────────────────────────────────────────────────

function formatFileSize(bytes: number | null): string {
  if (bytes === null) return "—";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
}

function formatSha(sha: string | null): string {
  if (!sha) return "—";
  return `${sha.slice(0, 12)}…`;
}

function statusBadge(status: string): React.ReactElement {
  const base = "inline-flex items-center rounded-full px-2 py-0.5 text-xs font-medium";
  if (status === "completed") {
    return <span className={`${base} bg-green-100 text-green-700`}>completed</span>;
  }
  if (status === "failed") {
    return <span className={`${base} bg-red-100 text-red-700`}>failed</span>;
  }
  if (status === "in_progress") {
    return <span className={`${base} bg-yellow-100 text-yellow-700`}>in progress</span>;
  }
  return <span className={`${base} bg-gray-100 text-gray-600`}>{status}</span>;
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

const ROLE_OPTIONS = [
  "SystemAdmin",
  "Provider",
  "NurseMa",
  "BillingStaff",
  "FrontDesk",
] as const;

function roleBadge(role: string): React.ReactElement {
  const base = "inline-flex items-center rounded-full px-2 py-0.5 text-xs font-medium";
  const map: Record<string, string> = {
    SystemAdmin: "bg-purple-100 text-purple-800",
    Provider: "bg-blue-100 text-blue-800",
    NurseMa: "bg-green-100 text-green-800",
    BillingStaff: "bg-yellow-100 text-yellow-800",
    FrontDesk: "bg-gray-100 text-gray-700",
  };
  const cls = map[role] ?? "bg-gray-100 text-gray-600";
  return <span className={`${base} ${cls}`}>{role}</span>;
}

// ─── Tab type ─────────────────────────────────────────────────────────────────

type Tab = "backup" | "security" | "fax" | "reminders" | "account" | "users";

// ─── SettingsPage ─────────────────────────────────────────────────────────────

export function SettingsPage() {
  const { user, session, logout } = useAuth();

  // ── Tab state ───────────────────────────────────────────────────────────────
  const [activeTab, setActiveTab] = useState<Tab>("backup");

  // ── Backup tab state ────────────────────────────────────────────────────────
  const [backupDir, setBackupDir] = useState<string | null>(null);
  const [backupEntries, setBackupEntries] = useState<BackupLogEntry[]>([]);
  const [backupLoading, setBackupLoading] = useState(false);
  const [backupError, setBackupError] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);
  const [createError, setCreateError] = useState<string | null>(null);
  const [lastResult, setLastResult] = useState<BackupResult | null>(null);
  const [reloadKey, setReloadKey] = useState(0);

  // Restore (SystemAdmin only)
  const [restorePath, setRestorePath] = useState("");
  const [restoring, setRestoring] = useState(false);
  const [restoreError, setRestoreError] = useState<string | null>(null);
  const [restoreSuccess, setRestoreSuccess] = useState<string | null>(null);

  // ── Security tab state ──────────────────────────────────────────────────────
  const [showMfaSetup, setShowMfaSetup] = useState(false);
  const [disablingTotp, setDisablingTotp] = useState(false);
  const [totpPassword, setTotpPassword] = useState("");
  const [mfaError, setMfaError] = useState<string | null>(null);
  const [mfaSuccess, setMfaSuccess] = useState<string | null>(null);

  // Touch ID
  const [biometric, setBiometric] = useState<BiometricStatus | null>(null);
  const [biometricLoading, setBiometricLoading] = useState(false);
  const [biometricError, setBiometricError] = useState<string | null>(null);
  const [touchIdPassword, setTouchIdPassword] = useState("");

  // ── Fax tab state ──────────────────────────────────────────────────────────
  const [faxApiKey, setFaxApiKey] = useState("");
  const [faxApiSecret, setFaxApiSecret] = useState("");
  const [faxNumber, setFaxNumber] = useState("");
  const [faxSaving, setFaxSaving] = useState(false);
  const [faxError, setFaxError] = useState<string | null>(null);
  const [faxSuccess, setFaxSuccess] = useState<string | null>(null);
  const [faxTesting, setFaxTesting] = useState(false);
  const [faxTestResult, setFaxTestResult] = useState<string | null>(null);

  // ── Reminders tab state ──────────────────────────────────────────────────────
  const [reminderConfig, setReminderConfig] = useState<ReminderConfigRecord | null>(null);
  const [reminderSmsEnabled, setReminderSmsEnabled] = useState(false);
  const [reminderEmailEnabled, setReminderEmailEnabled] = useState(false);
  const [reminder24hr, setReminder24hr] = useState(true);
  const [reminder2hr, setReminder2hr] = useState(true);
  const [reminderPracticeName, setReminderPracticeName] = useState("");
  const [reminderPracticePhone, setReminderPracticePhone] = useState("");
  const [twilioSid, setTwilioSid] = useState("");
  const [twilioToken, setTwilioToken] = useState("");
  const [twilioFrom, setTwilioFrom] = useState("");
  const [sgApiKey, setSgApiKey] = useState("");
  const [sgFromEmail, setSgFromEmail] = useState("");
  const [sgFromName, setSgFromName] = useState("");
  const [reminderSaving, setReminderSaving] = useState(false);
  const [reminderError, setReminderError] = useState<string | null>(null);
  const [reminderSuccess, setReminderSuccess] = useState<string | null>(null);
  const [reminderTesting, setReminderTesting] = useState(false);
  const [reminderTestResult, setReminderTestResult] = useState<string | null>(null);

  // ── Account tab state ───────────────────────────────────────────────────────
  const [signingOut, setSigningOut] = useState(false);

  // ── Users tab state (SystemAdmin only) ──────────────────────────────────────
  const [users, setUsers] = useState<UserListEntry[]>([]);
  const [usersLoading, setUsersLoading] = useState(false);
  const [usersError, setUsersError] = useState<string | null>(null);
  const [showAddUser, setShowAddUser] = useState(false);
  const [newUsername, setNewUsername] = useState("");
  const [newDisplayName, setNewDisplayName] = useState("");
  const [newRole, setNewRole] = useState<string>(ROLE_OPTIONS[1]);
  const [newPassword, setNewPassword] = useState("");
  const [addUserError, setAddUserError] = useState<string | null>(null);
  const [addUserLoading, setAddUserLoading] = useState(false);
  const [addUserSuccess, setAddUserSuccess] = useState<string | null>(null);

  // ── Users: load when SystemAdmin opens the Users tab ────────────────────────
  useEffect(() => {
    if (user?.role !== "SystemAdmin") return;
    if (activeTab !== "users") return;
    let mounted = true;
    setUsersLoading(true);
    setUsersError(null);
    commands.listUsers()
      .then((list) => { if (mounted) setUsers(list); })
      .catch((e) => { if (mounted) setUsersError(e instanceof Error ? e.message : String(e)); })
      .finally(() => { if (mounted) setUsersLoading(false); });
    return () => { mounted = false; };
  }, [activeTab, user?.role]);

  // ── Backup: fetch history on mount and after create ─────────────────────────
  useEffect(() => {
    let mounted = true;

    async function loadBackups() {
      setBackupLoading(true);
      setBackupError(null);
      try {
        const entries = await commands.listBackups();
        if (mounted) setBackupEntries(entries);
      } catch (e) {
        if (mounted) setBackupError(e instanceof Error ? e.message : String(e));
      } finally {
        if (mounted) setBackupLoading(false);
      }
    }

    loadBackups();
    return () => { mounted = false; };
  }, [reloadKey]);

  // ── Security: check biometric status on mount ────────────────────────────────
  useEffect(() => {
    let mounted = true;
    commands.checkBiometric()
      .then((status) => { if (mounted) setBiometric(status); })
      .catch(() => { /* unavailable on this platform — silently ignore */ });
    return () => { mounted = false; };
  }, []);

  // ── Reminders: load config when tab becomes active ────────────────────────────
  useEffect(() => {
    if (activeTab !== "reminders") return;
    let mounted = true;
    commands.getReminderConfig()
      .then((cfg) => {
        if (!mounted) return;
        setReminderConfig(cfg);
        setReminderSmsEnabled(cfg.smsEnabled);
        setReminderEmailEnabled(cfg.emailEnabled);
        setReminder24hr(cfg.reminder24hr);
        setReminder2hr(cfg.reminder2hr);
        setReminderPracticeName(cfg.practiceName ?? "");
        setReminderPracticePhone(cfg.practicePhone ?? "");
        setTwilioFrom(cfg.twilioFromNumber ?? "");
        setSgFromEmail(cfg.sendgridFromEmail ?? "");
      })
      .catch(() => { /* not yet configured — silently ignore */ });
    return () => { mounted = false; };
  }, [activeTab]);

  // ── Handlers ─────────────────────────────────────────────────────────────────

  const handleChooseFolder = useCallback(async () => {
    try {
      const dir = await open({ directory: true });
      if (dir !== null) {
        setBackupDir(dir as string);
      }
      // null means user cancelled — handle silently, no error
    } catch {
      // dialog API error — ignore
    }
  }, []);

  const handleCreateBackup = useCallback(async () => {
    if (!backupDir) return;
    setCreating(true);
    setCreateError(null);
    setLastResult(null);
    try {
      const result = await commands.createBackup(backupDir);
      setLastResult(result);
      setReloadKey((k) => k + 1);
    } catch (e) {
      setCreateError(e instanceof Error ? e.message : String(e));
    } finally {
      setCreating(false);
    }
  }, [backupDir]);

  const handleRestore = useCallback(async () => {
    if (!restorePath.trim()) return;
    setRestoring(true);
    setRestoreError(null);
    setRestoreSuccess(null);
    try {
      const result = await commands.restoreBackup(restorePath.trim(), null);
      setRestoreSuccess(
        `Restore completed. ID: ${result.restoreId}. Integrity: ${result.integrityVerified ? "✓ verified" : "⚠ not verified"}`
      );
      setRestorePath("");
    } catch (e) {
      setRestoreError(e instanceof Error ? e.message : String(e));
    } finally {
      setRestoring(false);
    }
  }, [restorePath]);

  const handleDisableTotp = useCallback(async () => {
    if (!totpPassword) return;
    setDisablingTotp(true);
    setMfaError(null);
    setMfaSuccess(null);
    try {
      await commands.disableTotp(totpPassword);
      setMfaSuccess("TOTP disabled successfully.");
      setTotpPassword("");
    } catch (e) {
      setMfaError(e instanceof Error ? e.message : String(e));
    } finally {
      setDisablingTotp(false);
    }
  }, [totpPassword]);

  const handleEnableTouchId = useCallback(async () => {
    if (!touchIdPassword) return;
    setBiometricLoading(true);
    setBiometricError(null);
    try {
      await commands.enableTouchId(touchIdPassword);
      const status = await commands.checkBiometric();
      setBiometric(status);
      setTouchIdPassword("");
    } catch (e) {
      setBiometricError(e instanceof Error ? e.message : String(e));
    } finally {
      setBiometricLoading(false);
    }
  }, [touchIdPassword]);

  const handleDisableTouchId = useCallback(async () => {
    setBiometricLoading(true);
    setBiometricError(null);
    try {
      await commands.disableTouchId();
      const status = await commands.checkBiometric();
      setBiometric(status);
    } catch (e) {
      setBiometricError(e instanceof Error ? e.message : String(e));
    } finally {
      setBiometricLoading(false);
    }
  }, []);

  const handleSaveFaxConfig = useCallback(async () => {
    if (!faxApiKey.trim() || !faxApiSecret.trim() || !faxNumber.trim()) return;
    setFaxSaving(true);
    setFaxError(null);
    setFaxSuccess(null);
    try {
      await commands.configurePhaxio({
        apiKey: faxApiKey.trim(),
        apiSecret: faxApiSecret.trim(),
        faxNumber: faxNumber.trim(),
      });
      setFaxSuccess("Fax configuration saved successfully.");
    } catch (e) {
      setFaxError(e instanceof Error ? e.message : String(e));
    } finally {
      setFaxSaving(false);
    }
  }, [faxApiKey, faxApiSecret, faxNumber]);

  const handleTestFaxConnection = useCallback(async () => {
    setFaxTesting(true);
    setFaxError(null);
    setFaxTestResult(null);
    try {
      const result = await commands.testPhaxioConnection();
      setFaxTestResult(
        result.success
          ? `Connection successful: ${result.message}`
          : `Connection failed: ${result.message}`
      );
    } catch (e) {
      setFaxError(e instanceof Error ? e.message : String(e));
    } finally {
      setFaxTesting(false);
    }
  }, []);

  const handleSaveReminderConfig = useCallback(async () => {
    setReminderSaving(true);
    setReminderError(null);
    setReminderSuccess(null);
    try {
      const input: ReminderConfigInput = {
        smsEnabled: reminderSmsEnabled,
        emailEnabled: reminderEmailEnabled,
        reminder24hr,
        reminder2hr,
        practiceName: reminderPracticeName.trim() || null,
        practicePhone: reminderPracticePhone.trim() || null,
        twilio: (twilioSid.trim() || twilioToken.trim() || twilioFrom.trim())
          ? { accountSid: twilioSid.trim(), authToken: twilioToken.trim(), fromNumber: twilioFrom.trim() }
          : null,
        sendgrid: (sgApiKey.trim() || sgFromEmail.trim())
          ? { apiKey: sgApiKey.trim(), fromEmail: sgFromEmail.trim(), fromName: sgFromName.trim() || null }
          : null,
      };
      const cfg = await commands.configureReminders(input);
      setReminderConfig(cfg);
      setReminderSuccess("Reminder settings saved.");
    } catch (e) {
      setReminderError(e instanceof Error ? e.message : String(e));
    } finally {
      setReminderSaving(false);
    }
  }, [
    reminderSmsEnabled, reminderEmailEnabled, reminder24hr, reminder2hr,
    reminderPracticeName, reminderPracticePhone,
    twilioSid, twilioToken, twilioFrom,
    sgApiKey, sgFromEmail, sgFromName,
  ]);

  const handleTestReminders = useCallback(async () => {
    setReminderTesting(true);
    setReminderTestResult(null);
    try {
      const result = await commands.processPendingReminders();
      setReminderTestResult(
        `Processed: ${result.sentCount} sent, ${result.skippedCount} skipped, ${result.failedCount} failed.`
      );
    } catch (e) {
      setReminderTestResult(`Error: ${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setReminderTesting(false);
    }
  }, []);

  const handleSignOut = useCallback(async () => {
    setSigningOut(true);
    try {
      await logout();
      // useAuth / App router will navigate to login automatically
    } finally {
      setSigningOut(false);
    }
  }, [logout]);

  const handleAddUser = useCallback(async () => {
    if (!newUsername.trim() || !newDisplayName.trim() || !newPassword) return;
    setAddUserLoading(true);
    setAddUserError(null);
    setAddUserSuccess(null);
    try {
      await commands.registerUser({
        username: newUsername.trim(),
        password: newPassword,
        displayName: newDisplayName.trim(),
        role: newRole,
      });
      setAddUserSuccess(`User '${newUsername.trim()}' created successfully.`);
      setNewUsername("");
      setNewDisplayName("");
      setNewPassword("");
      setNewRole(ROLE_OPTIONS[1]);
      setShowAddUser(false);
      // Refresh user list
      const list = await commands.listUsers();
      setUsers(list);
    } catch (e) {
      setAddUserError(e instanceof Error ? e.message : String(e));
    } finally {
      setAddUserLoading(false);
    }
  }, [newUsername, newDisplayName, newPassword, newRole]);

  const handleDeactivateUser = useCallback(async (userId: string) => {
    try {
      await commands.deactivateUser(userId);
      const list = await commands.listUsers();
      setUsers(list);
    } catch (e) {
      setUsersError(e instanceof Error ? e.message : String(e));
    }
  }, []);

  // ── Render ────────────────────────────────────────────────────────────────────

  const tabs: { id: Tab; label: string; adminOnly?: boolean }[] = [
    { id: "backup", label: "Backup" },
    { id: "security", label: "Security" },
    { id: "fax", label: "Fax" },
    { id: "reminders", label: "Reminders", adminOnly: true },
    { id: "account", label: "Account" },
    { id: "users", label: "Users", adminOnly: true },
  ];

  return (
    <div className="flex h-full flex-col p-6">
      {/* Header */}
      <div className="mb-6">
        <h1 className="text-2xl font-bold text-gray-900">Settings</h1>
        <p className="mt-1 text-sm text-gray-500">
          Manage backups, security, fax integration, and your account.
        </p>
      </div>

      {/* Tab bar */}
      <div className="mb-6 border-b border-gray-200">
        <nav className="-mb-px flex gap-6">
          {tabs
            .filter((tab) => !tab.adminOnly || user?.role === "SystemAdmin")
            .map((tab) => (
              <button
                key={tab.id}
                type="button"
                onClick={() => setActiveTab(tab.id)}
                className={[
                  "whitespace-nowrap border-b-2 pb-3 text-sm font-medium transition-colors",
                  activeTab === tab.id
                    ? "border-blue-600 text-blue-600"
                    : "border-transparent text-gray-500 hover:border-gray-300 hover:text-gray-700",
                ].join(" ")}
              >
                {tab.label}
              </button>
            ))}
        </nav>
      </div>

      {/* Tab content */}
      <div className="flex-1 overflow-y-auto">

        {/* ─── BACKUP TAB ─────────────────────────────────────────────────────── */}
        {activeTab === "backup" && (
          <div className="space-y-6 max-w-3xl">

            {/* Destination picker */}
            <section className="rounded-lg border border-gray-200 bg-white p-5 shadow-sm">
              <h2 className="mb-4 text-base font-semibold text-gray-900">
                Backup Destination
              </h2>
              <div className="flex items-center gap-3">
                <input
                  type="text"
                  readOnly
                  value={backupDir ?? "No folder selected"}
                  className="flex-1 rounded-md border border-gray-300 bg-gray-50 px-3 py-2 text-sm text-gray-700 focus:outline-none"
                />
                <button
                  type="button"
                  onClick={handleChooseFolder}
                  className="whitespace-nowrap rounded-md border border-gray-300 bg-white px-4 py-2 text-sm font-medium text-gray-700 shadow-sm transition-colors hover:bg-gray-50"
                >
                  Choose Folder
                </button>
              </div>
            </section>

            {/* Create backup */}
            <section className="rounded-lg border border-gray-200 bg-white p-5 shadow-sm">
              <h2 className="mb-4 text-base font-semibold text-gray-900">
                Create Backup
              </h2>

              {/* Success toast */}
              {lastResult && (
                <div className="mb-4 rounded-md border border-green-200 bg-green-50 px-4 py-3 text-sm text-green-800">
                  <p className="font-medium">Backup created successfully.</p>
                  <p className="mt-1 break-all font-mono text-xs text-green-700">
                    {lastResult.filePath}
                  </p>
                  <p className="mt-1 font-mono text-xs text-green-600">
                    SHA-256: {formatSha(lastResult.sha256Digest)}
                  </p>
                </div>
              )}

              {/* Create error */}
              {createError && (
                <div className="mb-4 rounded-md border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700">
                  {createError}
                </div>
              )}

              <button
                type="button"
                onClick={handleCreateBackup}
                disabled={!backupDir || creating}
                className="rounded-md bg-blue-600 px-4 py-2 text-sm font-medium text-white shadow-sm transition-colors hover:bg-blue-700 disabled:cursor-not-allowed disabled:opacity-50"
              >
                {creating ? "Creating…" : "Create Backup Now"}
              </button>
            </section>

            {/* Restore (SystemAdmin only) */}
            {user?.role === "SystemAdmin" && (
              <section className="rounded-lg border border-gray-200 bg-white p-5 shadow-sm">
                <h2 className="mb-4 text-base font-semibold text-gray-900">
                  Restore Backup
                </h2>

                {restoreSuccess && (
                  <div className="mb-4 rounded-md border border-green-200 bg-green-50 px-4 py-3 text-sm text-green-800">
                    {restoreSuccess}
                  </div>
                )}
                {restoreError && (
                  <div className="mb-4 rounded-md border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700">
                    {restoreError}
                  </div>
                )}

                <div className="flex items-center gap-3">
                  <input
                    type="text"
                    value={restorePath}
                    onChange={(e) => setRestorePath(e.target.value)}
                    placeholder="Absolute path to backup file…"
                    className="flex-1 rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
                  />
                  <button
                    type="button"
                    onClick={handleRestore}
                    disabled={!restorePath.trim() || restoring}
                    className="whitespace-nowrap rounded-md bg-amber-600 px-4 py-2 text-sm font-medium text-white shadow-sm transition-colors hover:bg-amber-700 disabled:cursor-not-allowed disabled:opacity-50"
                  >
                    {restoring ? "Restoring…" : "Restore"}
                  </button>
                </div>
              </section>
            )}

            {/* History table */}
            <section className="rounded-lg border border-gray-200 bg-white p-5 shadow-sm">
              <h2 className="mb-4 text-base font-semibold text-gray-900">
                Backup History
              </h2>

              {backupError && (
                <div className="mb-4 rounded-md border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700">
                  {backupError}
                </div>
              )}

              {backupLoading ? (
                <p className="text-sm text-gray-500">Loading…</p>
              ) : backupEntries.length === 0 ? (
                <p className="text-sm text-gray-500">No backup history yet.</p>
              ) : (
                <div className="overflow-x-auto">
                  <table className="min-w-full divide-y divide-gray-200 text-sm">
                    <thead>
                      <tr>
                        {["Operation", "Started", "Status", "File Size", "SHA-256"].map((h) => (
                          <th
                            key={h}
                            className="whitespace-nowrap px-3 py-2 text-left text-xs font-semibold uppercase tracking-wide text-gray-500"
                          >
                            {h}
                          </th>
                        ))}
                      </tr>
                    </thead>
                    <tbody className="divide-y divide-gray-100">
                      {backupEntries.map((entry) => (
                        <>
                          <tr key={entry.id} className="align-top">
                            <td className="px-3 py-2 font-medium text-gray-900">
                              {entry.operation}
                            </td>
                            <td className="whitespace-nowrap px-3 py-2 text-gray-600">
                              {new Date(entry.startedAt).toLocaleString()}
                            </td>
                            <td className="px-3 py-2">
                              {statusBadge(entry.status)}
                            </td>
                            <td className="px-3 py-2 text-gray-600">
                              {formatFileSize(entry.fileSizeBytes)}
                            </td>
                            <td className="px-3 py-2 font-mono text-gray-500">
                              {formatSha(entry.sha256Digest)}
                            </td>
                          </tr>
                          {entry.status === "failed" && entry.errorMessage && (
                            <tr key={`${entry.id}-err`} className="bg-red-50">
                              <td
                                colSpan={5}
                                className="px-3 py-1.5 text-xs text-red-700"
                              >
                                ⚠ {entry.errorMessage}
                              </td>
                            </tr>
                          )}
                        </>
                      ))}
                    </tbody>
                  </table>
                </div>
              )}
            </section>
          </div>
        )}

        {/* ─── SECURITY TAB ───────────────────────────────────────────────────── */}
        {activeTab === "security" && (
          <div className="space-y-6 max-w-2xl">

            {/* Shared MFA error / success */}
            {mfaError && (
              <div className="rounded-md border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700">
                {mfaError}
              </div>
            )}
            {mfaSuccess && (
              <div className="rounded-md border border-green-200 bg-green-50 px-4 py-3 text-sm text-green-800">
                {mfaSuccess}
              </div>
            )}

            {/* TOTP Setup */}
            <section className="rounded-lg border border-gray-200 bg-white p-5 shadow-sm">
              <h2 className="mb-1 text-base font-semibold text-gray-900">
                Authenticator App (TOTP)
              </h2>
              <p className="mb-4 text-sm text-gray-500">
                Use Google Authenticator, Authy, or any TOTP app to add a
                second factor.
              </p>

              {showMfaSetup ? (
                <MfaSetup
                  onComplete={() => {
                    setShowMfaSetup(false);
                    setMfaSuccess("TOTP enabled successfully.");
                  }}
                  onCancel={() => setShowMfaSetup(false)}
                />
              ) : (
                <button
                  type="button"
                  onClick={() => {
                    setMfaError(null);
                    setMfaSuccess(null);
                    setShowMfaSetup(true);
                  }}
                  className="rounded-md bg-blue-600 px-4 py-2 text-sm font-medium text-white shadow-sm transition-colors hover:bg-blue-700"
                >
                  Set up TOTP
                </button>
              )}
            </section>

            {/* Disable TOTP */}
            <section className="rounded-lg border border-gray-200 bg-white p-5 shadow-sm">
              <h2 className="mb-1 text-base font-semibold text-gray-900">
                Disable TOTP
              </h2>
              <p className="mb-4 text-sm text-gray-500">
                Confirm your password to remove the TOTP second factor.
              </p>
              <div className="flex items-center gap-3">
                <input
                  type="password"
                  value={totpPassword}
                  onChange={(e) => setTotpPassword(e.target.value)}
                  placeholder="Current password…"
                  className="flex-1 rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
                />
                <button
                  type="button"
                  onClick={handleDisableTotp}
                  disabled={!totpPassword || disablingTotp}
                  className="whitespace-nowrap rounded-md border border-red-300 bg-red-50 px-4 py-2 text-sm font-medium text-red-700 shadow-sm transition-colors hover:bg-red-100 disabled:cursor-not-allowed disabled:opacity-50"
                >
                  {disablingTotp ? "Disabling…" : "Disable"}
                </button>
              </div>
            </section>

            {/* Touch ID */}
            <section className="rounded-lg border border-gray-200 bg-white p-5 shadow-sm">
              <h2 className="mb-1 text-base font-semibold text-gray-900">
                Touch ID / Biometric
              </h2>

              {biometricError && (
                <div className="mb-3 rounded-md border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-700">
                  {biometricError}
                </div>
              )}

              {biometric === null ? (
                <p className="text-sm text-gray-400">
                  Checking biometric availability…
                </p>
              ) : !biometric.available ? (
                <p className="text-sm text-gray-500">
                  Touch ID is not available on this device.
                </p>
              ) : (
                <div className="space-y-4">
                  <p className="text-sm text-gray-600">
                    Status:{" "}
                    <span
                      className={
                        biometric.enabled
                          ? "font-medium text-green-700"
                          : "font-medium text-gray-500"
                      }
                    >
                      {biometric.enabled ? "Enabled" : "Disabled"}
                    </span>
                  </p>

                  {!biometric.enabled ? (
                    <div className="flex items-center gap-3">
                      <input
                        type="password"
                        value={touchIdPassword}
                        onChange={(e) => setTouchIdPassword(e.target.value)}
                        placeholder="Confirm password to enable…"
                        className="flex-1 rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
                      />
                      <button
                        type="button"
                        onClick={handleEnableTouchId}
                        disabled={!touchIdPassword || biometricLoading}
                        className="whitespace-nowrap rounded-md bg-blue-600 px-4 py-2 text-sm font-medium text-white shadow-sm transition-colors hover:bg-blue-700 disabled:cursor-not-allowed disabled:opacity-50"
                      >
                        {biometricLoading ? "Enabling…" : "Enable Touch ID"}
                      </button>
                    </div>
                  ) : (
                    <button
                      type="button"
                      onClick={handleDisableTouchId}
                      disabled={biometricLoading}
                      className="rounded-md border border-red-300 bg-red-50 px-4 py-2 text-sm font-medium text-red-700 shadow-sm transition-colors hover:bg-red-100 disabled:cursor-not-allowed disabled:opacity-50"
                    >
                      {biometricLoading ? "Disabling…" : "Disable Touch ID"}
                    </button>
                  )}
                </div>
              )}
            </section>
          </div>
        )}

        {/* ─── FAX TAB ────────────────────────────────────────────────────────── */}
        {activeTab === "fax" && (
          <div className="space-y-6 max-w-2xl">

            {faxError && (
              <div className="rounded-md border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700">
                {faxError}
              </div>
            )}
            {faxSuccess && (
              <div className="rounded-md border border-green-200 bg-green-50 px-4 py-3 text-sm text-green-800">
                {faxSuccess}
              </div>
            )}

            {/* Phaxio Configuration */}
            <section className="rounded-lg border border-gray-200 bg-white p-5 shadow-sm">
              <h2 className="mb-1 text-base font-semibold text-gray-900">
                Phaxio Integration
              </h2>
              <p className="mb-4 text-sm text-gray-500">
                Enter your Phaxio API credentials to enable fax sending and receiving.
              </p>

              <div className="space-y-4">
                <div>
                  <label className="mb-1 block text-sm font-medium text-gray-700">
                    API Key
                  </label>
                  <input
                    type="password"
                    value={faxApiKey}
                    onChange={(e) => setFaxApiKey(e.target.value)}
                    placeholder="Enter Phaxio API key..."
                    className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
                  />
                </div>

                <div>
                  <label className="mb-1 block text-sm font-medium text-gray-700">
                    API Secret
                  </label>
                  <input
                    type="password"
                    value={faxApiSecret}
                    onChange={(e) => setFaxApiSecret(e.target.value)}
                    placeholder="Enter Phaxio API secret..."
                    className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
                  />
                </div>

                <div>
                  <label className="mb-1 block text-sm font-medium text-gray-700">
                    Practice Fax Number
                  </label>
                  <input
                    type="tel"
                    value={faxNumber}
                    onChange={(e) => setFaxNumber(e.target.value)}
                    placeholder="+1 (555) 000-0000"
                    className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
                  />
                </div>

                <div className="flex gap-3">
                  <button
                    type="button"
                    onClick={handleSaveFaxConfig}
                    disabled={faxSaving || !faxApiKey.trim() || !faxApiSecret.trim() || !faxNumber.trim()}
                    className="rounded-md bg-blue-600 px-4 py-2 text-sm font-medium text-white shadow-sm transition-colors hover:bg-blue-700 disabled:cursor-not-allowed disabled:opacity-50"
                  >
                    {faxSaving ? "Saving..." : "Save"}
                  </button>
                  <button
                    type="button"
                    onClick={handleTestFaxConnection}
                    disabled={faxTesting}
                    className="rounded-md border border-gray-300 bg-white px-4 py-2 text-sm font-medium text-gray-700 shadow-sm transition-colors hover:bg-gray-50 disabled:cursor-not-allowed disabled:opacity-50"
                  >
                    {faxTesting ? "Testing..." : "Test Connection"}
                  </button>
                </div>

                {faxTestResult && (
                  <div
                    className={[
                      "rounded-md border px-4 py-3 text-sm",
                      faxTestResult.startsWith("Connection successful")
                        ? "border-green-200 bg-green-50 text-green-800"
                        : "border-red-200 bg-red-50 text-red-700",
                    ].join(" ")}
                  >
                    {faxTestResult}
                  </div>
                )}
              </div>
            </section>
          </div>
        )}

        {/* ─── USERS TAB (SystemAdmin only) ───────────────────────────────────── */}
        {activeTab === "users" && user?.role === "SystemAdmin" && (
          <div className="space-y-6 max-w-4xl">

            {usersError && (
              <div className="rounded-md border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700">
                {usersError}
              </div>
            )}

            {addUserSuccess && (
              <div className="rounded-md border border-green-200 bg-green-50 px-4 py-3 text-sm text-green-800">
                {addUserSuccess}
              </div>
            )}

            {/* Add User */}
            <section className="rounded-lg border border-gray-200 bg-white p-5 shadow-sm">
              <div className="mb-4 flex items-center justify-between">
                <h2 className="text-base font-semibold text-gray-900">
                  System Users
                </h2>
                <button
                  type="button"
                  onClick={() => {
                    setShowAddUser((v) => !v);
                    setAddUserError(null);
                  }}
                  className="rounded-md bg-blue-600 px-4 py-2 text-sm font-medium text-white shadow-sm transition-colors hover:bg-blue-700"
                >
                  {showAddUser ? "Cancel" : "Add User"}
                </button>
              </div>

              {showAddUser && (
                <div className="mb-6 rounded-md border border-blue-100 bg-blue-50 p-4 space-y-3">
                  <h3 className="text-sm font-semibold text-blue-900">
                    New User
                  </h3>

                  {addUserError && (
                    <div className="rounded-md border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-700">
                      {addUserError}
                    </div>
                  )}

                  <div className="grid gap-3 sm:grid-cols-2">
                    <div>
                      <label className="mb-1 block text-sm font-medium text-gray-700">
                        Username
                      </label>
                      <input
                        type="text"
                        value={newUsername}
                        onChange={(e) => setNewUsername(e.target.value)}
                        placeholder="john.doe"
                        className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
                      />
                    </div>

                    <div>
                      <label className="mb-1 block text-sm font-medium text-gray-700">
                        Display Name
                      </label>
                      <input
                        type="text"
                        value={newDisplayName}
                        onChange={(e) => setNewDisplayName(e.target.value)}
                        placeholder="John Doe, PT"
                        className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
                      />
                    </div>

                    <div>
                      <label className="mb-1 block text-sm font-medium text-gray-700">
                        Role
                      </label>
                      <select
                        value={newRole}
                        onChange={(e) => setNewRole(e.target.value)}
                        className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
                      >
                        {ROLE_OPTIONS.map((r) => (
                          <option key={r} value={r}>
                            {r}
                          </option>
                        ))}
                      </select>
                    </div>

                    <div>
                      <label className="mb-1 block text-sm font-medium text-gray-700">
                        Password (min 12 chars)
                      </label>
                      <input
                        type="password"
                        value={newPassword}
                        onChange={(e) => setNewPassword(e.target.value)}
                        placeholder="••••••••••••"
                        className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
                      />
                    </div>
                  </div>

                  <button
                    type="button"
                    onClick={handleAddUser}
                    disabled={
                      addUserLoading ||
                      !newUsername.trim() ||
                      !newDisplayName.trim() ||
                      newPassword.length < 12
                    }
                    className="rounded-md bg-blue-600 px-4 py-2 text-sm font-medium text-white shadow-sm transition-colors hover:bg-blue-700 disabled:cursor-not-allowed disabled:opacity-50"
                  >
                    {addUserLoading ? "Creating…" : "Create User"}
                  </button>
                </div>
              )}

              {/* User list */}
              {usersLoading ? (
                <p className="text-sm text-gray-500">Loading users…</p>
              ) : users.length === 0 ? (
                <p className="text-sm text-gray-500">No users found.</p>
              ) : (
                <div className="overflow-x-auto">
                  <table className="min-w-full divide-y divide-gray-200 text-sm">
                    <thead>
                      <tr>
                        {["Username", "Display Name", "Role", "Status", "Created", "Actions"].map(
                          (h) => (
                            <th
                              key={h}
                              className="whitespace-nowrap px-3 py-2 text-left text-xs font-semibold uppercase tracking-wide text-gray-500"
                            >
                              {h}
                            </th>
                          )
                        )}
                      </tr>
                    </thead>
                    <tbody className="divide-y divide-gray-100">
                      {users.map((u) => (
                        <tr
                          key={u.id}
                          className={u.isActive ? "" : "bg-gray-50 opacity-60"}
                        >
                          <td className="px-3 py-2 font-mono text-xs text-gray-700">
                            {u.username}
                          </td>
                          <td className="px-3 py-2 text-gray-900">
                            {u.displayName}
                          </td>
                          <td className="px-3 py-2">{roleBadge(u.role)}</td>
                          <td className="px-3 py-2">
                            {u.isActive ? (
                              <span className="inline-flex items-center rounded-full bg-green-100 px-2 py-0.5 text-xs font-medium text-green-700">
                                Active
                              </span>
                            ) : (
                              <span className="inline-flex items-center rounded-full bg-gray-200 px-2 py-0.5 text-xs font-medium text-gray-500">
                                Inactive
                              </span>
                            )}
                          </td>
                          <td className="whitespace-nowrap px-3 py-2 text-xs text-gray-500">
                            {new Date(u.createdAt).toLocaleDateString()}
                          </td>
                          <td className="px-3 py-2">
                            {u.isActive && u.id !== user.id && (
                              <button
                                type="button"
                                onClick={() => handleDeactivateUser(u.id)}
                                className="rounded-md border border-red-200 bg-red-50 px-2 py-1 text-xs font-medium text-red-700 transition-colors hover:bg-red-100"
                              >
                                Deactivate
                              </button>
                            )}
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              )}
            </section>
          </div>
        )}

        {/* ─── REMINDERS TAB ──────────────────────────────────────────────────── */}
        {activeTab === "reminders" && (
          <div className="space-y-6 max-w-3xl">

            {/* Status badges */}
            {reminderConfig && (
              <div className="flex gap-3 flex-wrap">
                <span className={`inline-flex items-center rounded-full px-3 py-1 text-xs font-medium ${reminderConfig.twilioConfigured ? "bg-green-100 text-green-700" : "bg-gray-100 text-gray-500"}`}>
                  SMS {reminderConfig.twilioConfigured ? "Configured" : "Not Configured"}
                </span>
                <span className={`inline-flex items-center rounded-full px-3 py-1 text-xs font-medium ${reminderConfig.sendgridConfigured ? "bg-green-100 text-green-700" : "bg-gray-100 text-gray-500"}`}>
                  Email {reminderConfig.sendgridConfigured ? "Configured" : "Not Configured"}
                </span>
              </div>
            )}

            {reminderError && (
              <div className="rounded-md bg-red-50 border border-red-200 px-4 py-3 text-sm text-red-700">
                {reminderError}
              </div>
            )}
            {reminderSuccess && (
              <div className="rounded-md bg-green-50 border border-green-200 px-4 py-3 text-sm text-green-700">
                {reminderSuccess}
              </div>
            )}

            {/* Channel toggles */}
            <section className="rounded-lg border border-gray-200 bg-white p-5 shadow-sm">
              <h2 className="mb-4 text-base font-semibold text-gray-900">Channels</h2>
              <div className="space-y-3">
                <label className="flex items-center gap-3 cursor-pointer">
                  <input
                    type="checkbox"
                    checked={reminderSmsEnabled}
                    onChange={(e) => setReminderSmsEnabled(e.target.checked)}
                    className="h-4 w-4 rounded border-gray-300 text-blue-600"
                  />
                  <span className="text-sm font-medium text-gray-700">Enable SMS reminders (Twilio)</span>
                </label>
                <label className="flex items-center gap-3 cursor-pointer">
                  <input
                    type="checkbox"
                    checked={reminderEmailEnabled}
                    onChange={(e) => setReminderEmailEnabled(e.target.checked)}
                    className="h-4 w-4 rounded border-gray-300 text-blue-600"
                  />
                  <span className="text-sm font-medium text-gray-700">Enable email reminders (SendGrid)</span>
                </label>
              </div>
            </section>

            {/* Reminder intervals */}
            <section className="rounded-lg border border-gray-200 bg-white p-5 shadow-sm">
              <h2 className="mb-4 text-base font-semibold text-gray-900">Reminder Intervals</h2>
              <div className="space-y-3">
                <label className="flex items-center gap-3 cursor-pointer">
                  <input
                    type="checkbox"
                    checked={reminder24hr}
                    onChange={(e) => setReminder24hr(e.target.checked)}
                    className="h-4 w-4 rounded border-gray-300 text-blue-600"
                  />
                  <span className="text-sm font-medium text-gray-700">24 hours before appointment</span>
                </label>
                <label className="flex items-center gap-3 cursor-pointer">
                  <input
                    type="checkbox"
                    checked={reminder2hr}
                    onChange={(e) => setReminder2hr(e.target.checked)}
                    className="h-4 w-4 rounded border-gray-300 text-blue-600"
                  />
                  <span className="text-sm font-medium text-gray-700">2 hours before appointment</span>
                </label>
              </div>
            </section>

            {/* Practice info */}
            <section className="rounded-lg border border-gray-200 bg-white p-5 shadow-sm">
              <h2 className="mb-4 text-base font-semibold text-gray-900">Practice Information</h2>
              <div className="space-y-4">
                <div>
                  <label className="block text-sm font-medium text-gray-700 mb-1">Practice Name</label>
                  <input
                    type="text"
                    value={reminderPracticeName}
                    onChange={(e) => setReminderPracticeName(e.target.value)}
                    placeholder="e.g. MedArc Physical Therapy"
                    className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500"
                  />
                </div>
                <div>
                  <label className="block text-sm font-medium text-gray-700 mb-1">Practice Phone</label>
                  <input
                    type="tel"
                    value={reminderPracticePhone}
                    onChange={(e) => setReminderPracticePhone(e.target.value)}
                    placeholder="e.g. 555-123-4567"
                    className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500"
                  />
                </div>
              </div>
            </section>

            {/* Twilio credentials */}
            <section className="rounded-lg border border-gray-200 bg-white p-5 shadow-sm">
              <h2 className="mb-1 text-base font-semibold text-gray-900">Twilio SMS Credentials</h2>
              <p className="mb-4 text-xs text-gray-500">
                Required for SMS reminders. Stored encrypted at rest.{" "}
                {reminderConfig?.twilioConfigured && (
                  <span className="text-green-600 font-medium">Currently configured.</span>
                )}
              </p>
              <div className="space-y-4">
                <div>
                  <label className="block text-sm font-medium text-gray-700 mb-1">Account SID</label>
                  <input
                    type="password"
                    value={twilioSid}
                    onChange={(e) => setTwilioSid(e.target.value)}
                    placeholder={reminderConfig?.twilioConfigured ? "Leave blank to keep existing" : "ACxxxxxxxxxxxxxxxx"}
                    className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500"
                  />
                </div>
                <div>
                  <label className="block text-sm font-medium text-gray-700 mb-1">Auth Token</label>
                  <input
                    type="password"
                    value={twilioToken}
                    onChange={(e) => setTwilioToken(e.target.value)}
                    placeholder={reminderConfig?.twilioConfigured ? "Leave blank to keep existing" : "Auth token"}
                    className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500"
                  />
                </div>
                <div>
                  <label className="block text-sm font-medium text-gray-700 mb-1">From Number (E.164)</label>
                  <input
                    type="text"
                    value={twilioFrom}
                    onChange={(e) => setTwilioFrom(e.target.value)}
                    placeholder="+15551234567"
                    className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500"
                  />
                </div>
              </div>
            </section>

            {/* SendGrid credentials */}
            <section className="rounded-lg border border-gray-200 bg-white p-5 shadow-sm">
              <h2 className="mb-1 text-base font-semibold text-gray-900">SendGrid Email Credentials</h2>
              <p className="mb-4 text-xs text-gray-500">
                Required for email reminders. Stored encrypted at rest.{" "}
                {reminderConfig?.sendgridConfigured && (
                  <span className="text-green-600 font-medium">Currently configured.</span>
                )}
              </p>
              <div className="space-y-4">
                <div>
                  <label className="block text-sm font-medium text-gray-700 mb-1">API Key</label>
                  <input
                    type="password"
                    value={sgApiKey}
                    onChange={(e) => setSgApiKey(e.target.value)}
                    placeholder={reminderConfig?.sendgridConfigured ? "Leave blank to keep existing" : "SG.xxxxxx"}
                    className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500"
                  />
                </div>
                <div>
                  <label className="block text-sm font-medium text-gray-700 mb-1">From Email</label>
                  <input
                    type="email"
                    value={sgFromEmail}
                    onChange={(e) => setSgFromEmail(e.target.value)}
                    placeholder="noreply@yourpractice.com"
                    className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500"
                  />
                </div>
                <div>
                  <label className="block text-sm font-medium text-gray-700 mb-1">From Name</label>
                  <input
                    type="text"
                    value={sgFromName}
                    onChange={(e) => setSgFromName(e.target.value)}
                    placeholder="MedArc Physical Therapy"
                    className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500"
                  />
                </div>
              </div>
            </section>

            {/* Template preview */}
            <section className="rounded-lg border border-gray-200 bg-white p-5 shadow-sm">
              <h2 className="mb-2 text-base font-semibold text-gray-900">Default Templates</h2>
              <p className="mb-4 text-xs text-gray-500">
                Placeholders: {"{patient_name}"}, {"{appointment_date}"}, {"{appointment_time}"},{" "}
                {"{provider_name}"}, {"{practice_name}"}, {"{practice_phone}"}
              </p>
              <div className="space-y-3">
                {[
                  { label: "24hr Reminder", text: "Hi {patient_name}, this is a reminder of your PT appointment tomorrow at {appointment_time} with {provider_name}. Reply C to confirm or call {practice_phone} to reschedule." },
                  { label: "2hr Reminder", text: "Reminder: Your PT appointment is in 2 hours at {appointment_time}. See you soon!" },
                  { label: "No-Show Follow-up", text: "We missed you at your appointment today. Please call {practice_phone} to reschedule." },
                  { label: "Waitlist Offer", text: "Hi {patient_name}, a slot has opened at {appointment_time} on {appointment_date}. Reply Y to book or call {practice_phone}." },
                ].map((tmpl) => (
                  <div key={tmpl.label} className="rounded-md bg-gray-50 p-3">
                    <div className="text-xs font-semibold text-gray-600 mb-1">{tmpl.label}</div>
                    <div className="text-sm text-gray-700 font-mono whitespace-pre-wrap">{tmpl.text}</div>
                  </div>
                ))}
              </div>
            </section>

            {/* Action buttons */}
            <div className="flex gap-3 flex-wrap">
              <button
                type="button"
                onClick={handleSaveReminderConfig}
                disabled={reminderSaving}
                className="rounded-md bg-blue-600 px-4 py-2 text-sm font-medium text-white shadow-sm transition-colors hover:bg-blue-700 disabled:cursor-not-allowed disabled:opacity-50"
              >
                {reminderSaving ? "Saving…" : "Save Settings"}
              </button>
              <button
                type="button"
                onClick={handleTestReminders}
                disabled={reminderTesting}
                className="rounded-md border border-gray-300 bg-white px-4 py-2 text-sm font-medium text-gray-700 shadow-sm transition-colors hover:bg-gray-50 disabled:cursor-not-allowed disabled:opacity-50"
              >
                {reminderTesting ? "Processing…" : "Process Pending Reminders"}
              </button>
            </div>
            {reminderTestResult && (
              <div className="rounded-md bg-blue-50 border border-blue-200 px-4 py-3 text-sm text-blue-700">
                {reminderTestResult}
              </div>
            )}
          </div>
        )}

        {/* ─── ACCOUNT TAB ────────────────────────────────────────────────────── */}
        {activeTab === "account" && (
          <div className="space-y-6 max-w-xl">
            <section className="rounded-lg border border-gray-200 bg-white p-5 shadow-sm">
              <h2 className="mb-4 text-base font-semibold text-gray-900">
                Account Information
              </h2>

              <dl className="divide-y divide-gray-100">
                {[
                  { label: "Display Name", value: user?.displayName ?? "—" },
                  { label: "Role", value: user?.role ?? "—" },
                  { label: "Session State", value: session?.state ?? "—" },
                  {
                    label: "Last Activity",
                    value: session?.lastActivity
                      ? new Date(session.lastActivity).toLocaleString()
                      : "Never",
                  },
                  { label: "Session ID", value: session?.sessionId ?? "—" },
                ].map(({ label, value }) => (
                  <div key={label} className="flex justify-between py-3">
                    <dt className="text-sm font-medium text-gray-500">{label}</dt>
                    <dd className="text-sm text-gray-900">{value}</dd>
                  </div>
                ))}
              </dl>
            </section>

            {/* Sign Out */}
            <section className="rounded-lg border border-gray-200 bg-white p-5 shadow-sm">
              <h2 className="mb-2 text-base font-semibold text-gray-900">
                Sign Out
              </h2>
              <p className="mb-4 text-sm text-gray-500">
                You will be returned to the login screen.
              </p>
              <button
                type="button"
                onClick={handleSignOut}
                disabled={signingOut}
                className="w-full rounded-md bg-red-600 px-4 py-2 text-sm font-medium text-white shadow-sm transition-colors hover:bg-red-700 disabled:cursor-not-allowed disabled:opacity-50"
              >
                {signingOut ? "Signing out…" : "Sign Out"}
              </button>
            </section>
          </div>
        )}
      </div>
    </div>
  );
}

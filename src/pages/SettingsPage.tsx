/**
 * SettingsPage.tsx — Multi-tab Settings panel: Backup | Security | Fax | Reminders | Export | Account | Users
 *
 * Replaces the S06 placeholder. Wired to real backend commands via tauri.ts.
 *
 * Tabs:
 *   - Backup:    folder picker, create backup, history table, restore (SystemAdmin only)
 *   - Security:  TOTP setup/disable, Touch ID enable/disable
 *   - Fax:       Phaxio API key/secret, practice fax number, test connection
 *   - Reminders: Twilio/SendGrid config, reminder intervals (SystemAdmin only)
 *   - Export:    letterhead (name, address, phone, logo) + signature (image, credentials, license)
 *   - Account:   session info (read-only), sign-out
 *   - Users:     user management (SystemAdmin only)
 *
 * Observability:
 *   - backupError / mfaError / biometricError / faxError: inline red banners
 *   - lastResult: inline success toast with file path and SHA-256 prefix
 *   - history table surfaces status / errorMessage per log entry
 */

import { useState, useEffect, useCallback } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { readFile } from "@tauri-apps/plugin-fs";
import { commands } from "../lib/tauri";
import { useAuth } from "../hooks/useAuth";
import MfaSetup from "../components/auth/MfaSetup";
import type { BackupResult, BackupLogEntry } from "../types/backup";
import type { BiometricStatus } from "../types/auth";
import type { UserListEntry } from "../types/mips";
import type { ReminderConfigRecord, ReminderConfigInput } from "../types/reminders";
import type { ExportSettings } from "../types/export";
// SchedulePrintSettings type is inferred from commands.saveSchedulePrintSettings

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

type Tab = "backup" | "security" | "fax" | "reminders" | "export" | "calendar" | "ai" | "account" | "users";

// ─── Provider Appointment Types Section ────────────────────────────────────────

/**
 * Inline section within the Users tab for managing per-provider appointment types.
 * Shows only providers (role === "Provider") and allows adding/removing
 * appointment type strings per provider.
 */
function ProviderAppointmentTypesSection({ users }: { users: UserListEntry[] }) {
  const [typesMap, setTypesMap] = useState<Record<string, string[]>>({});
  const [loadingTypes, setLoadingTypes] = useState(false);
  const [savingTypes, setSavingTypes] = useState(false);
  const [typesError, setTypesError] = useState<string | null>(null);
  const [typesSuccess, setTypesSuccess] = useState<string | null>(null);
  const [newTypeInputs, setNewTypeInputs] = useState<Record<string, string>>({});

  const providerUsers = users.filter((u) => u.role === "Provider" && u.isActive);

  // Load on mount
  useEffect(() => {
    let mounted = true;
    setLoadingTypes(true);
    commands.getProviderAppointmentTypes()
      .then((result) => {
        if (mounted) setTypesMap(result.types);
      })
      .catch((e) => {
        if (mounted) setTypesError(e instanceof Error ? e.message : String(e));
      })
      .finally(() => {
        if (mounted) setLoadingTypes(false);
      });
    return () => { mounted = false; };
  }, []);

  async function handleSave() {
    setSavingTypes(true);
    setTypesError(null);
    setTypesSuccess(null);
    try {
      await commands.setProviderAppointmentTypes(typesMap);
      setTypesSuccess("Provider appointment types saved.");
    } catch (e) {
      setTypesError(e instanceof Error ? e.message : String(e));
    } finally {
      setSavingTypes(false);
    }
  }

  function handleAddType(providerId: string) {
    const newType = (newTypeInputs[providerId] ?? "").trim();
    if (!newType) return;
    setTypesMap((prev) => {
      const existing = prev[providerId] ?? [];
      if (existing.includes(newType)) return prev;
      return { ...prev, [providerId]: [...existing, newType] };
    });
    setNewTypeInputs((prev) => ({ ...prev, [providerId]: "" }));
  }

  function handleRemoveType(providerId: string, typeToRemove: string) {
    setTypesMap((prev) => {
      const existing = prev[providerId] ?? [];
      return { ...prev, [providerId]: existing.filter((t) => t !== typeToRemove) };
    });
  }

  if (providerUsers.length === 0) return null;

  return (
    <section className="rounded-lg border border-gray-200 bg-white p-5 shadow-sm">
      <div className="mb-4 flex items-center justify-between">
        <h2 className="text-base font-semibold text-gray-900">
          Provider Appointment Types
        </h2>
        <button
          type="button"
          onClick={handleSave}
          disabled={savingTypes}
          className="rounded-md bg-blue-600 px-4 py-2 text-sm font-medium text-white shadow-sm transition-colors hover:bg-blue-700 disabled:cursor-not-allowed disabled:opacity-50"
        >
          {savingTypes ? "Saving..." : "Save"}
        </button>
      </div>

      <p className="text-sm text-gray-500 mb-4">
        Configure which appointment types each provider can accept. When scheduling,
        only the selected provider's types will appear in the appointment type dropdown.
      </p>

      {typesError && (
        <div className="rounded-md border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700 mb-4">
          {typesError}
        </div>
      )}

      {typesSuccess && (
        <div className="rounded-md border border-green-200 bg-green-50 px-4 py-3 text-sm text-green-800 mb-4">
          {typesSuccess}
        </div>
      )}

      {loadingTypes ? (
        <p className="text-sm text-gray-500">Loading...</p>
      ) : (
        <div className="space-y-4">
          {providerUsers.map((prov) => {
            const provTypes = typesMap[prov.id] ?? [];
            const newTypeValue = newTypeInputs[prov.id] ?? "";

            return (
              <div key={prov.id} className="rounded-md border border-gray-100 bg-gray-50 p-4">
                <h3 className="text-sm font-semibold text-gray-800 mb-2">
                  {prov.displayName}
                </h3>

                {/* Existing types as removable tags */}
                <div className="flex flex-wrap gap-2 mb-2">
                  {provTypes.length === 0 && (
                    <span className="text-xs text-gray-400 italic">No types configured (will show generic options)</span>
                  )}
                  {provTypes.map((t) => (
                    <span
                      key={t}
                      className="inline-flex items-center gap-1 rounded-full bg-blue-100 px-3 py-1 text-xs font-medium text-blue-800"
                    >
                      {t}
                      <button
                        type="button"
                        onClick={() => handleRemoveType(prov.id, t)}
                        className="ml-0.5 rounded-full p-0.5 text-blue-600 hover:bg-blue-200 hover:text-blue-900 focus:outline-none"
                        aria-label={`Remove ${t}`}
                      >
                        x
                      </button>
                    </span>
                  ))}
                </div>

                {/* Add new type */}
                <div className="flex gap-2">
                  <input
                    type="text"
                    value={newTypeValue}
                    onChange={(e) => setNewTypeInputs((prev) => ({ ...prev, [prov.id]: e.target.value }))}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") {
                        e.preventDefault();
                        handleAddType(prov.id);
                      }
                    }}
                    placeholder="New appointment type..."
                    className="flex-1 rounded-md border border-gray-300 px-3 py-1.5 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
                  />
                  <button
                    type="button"
                    onClick={() => handleAddType(prov.id)}
                    className="rounded-md border border-gray-300 bg-white px-3 py-1.5 text-sm font-medium text-gray-700 shadow-sm hover:bg-gray-50"
                  >
                    Add
                  </button>
                </div>
              </div>
            );
          })}
        </div>
      )}
    </section>
  );
}

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

  // Profile editing state
  const [profileDisplayName, setProfileDisplayName] = useState(user?.displayName ?? "");
  const [profileUsername, setProfileUsername] = useState(user?.username ?? "");
  const [profileSaving, setProfileSaving] = useState(false);
  const [profileError, setProfileError] = useState<string | null>(null);
  const [profileSuccess, setProfileSuccess] = useState<string | null>(null);

  // Password change state
  const [pwdCurrent, setPwdCurrent] = useState("");
  const [pwdNew, setPwdNew] = useState("");
  const [pwdConfirm, setPwdConfirm] = useState("");
  const [pwdSaving, setPwdSaving] = useState(false);
  const [pwdError, setPwdError] = useState<string | null>(null);
  const [pwdSuccess, setPwdSuccess] = useState<string | null>(null);

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

  // ── Export tab state ────────────────────────────────────────────────────────
  const [exportPracticeName, setExportPracticeName] = useState("");
  const [exportPracticeAddress, setExportPracticeAddress] = useState("");
  const [exportPracticePhone, setExportPracticePhone] = useState("");
  const [exportPracticeLogoBase64, setExportPracticeLogoBase64] = useState<string | null>(null);
  const [exportLogoWidthPx, setExportLogoWidthPx] = useState(200);
  const [exportSignatureBase64, setExportSignatureBase64] = useState<string | null>(null);
  const [exportProviderCredentials, setExportProviderCredentials] = useState("");
  const [exportLicenseNumber, setExportLicenseNumber] = useState("");
  const [exportSaving, setExportSaving] = useState(false);
  const [exportError, setExportError] = useState<string | null>(null);
  const [exportSuccess, setExportSuccess] = useState<string | null>(null);
  const [exportLoading, setExportLoading] = useState(false);

  // ── Calendar tab state ─────────────────────────────────────────────────────
  const [calShowSaturday, setCalShowSaturday] = useState(false);
  const [calShowSunday, setCalShowSunday] = useState(false);
  const [calStartHour, setCalStartHour] = useState(6);
  const [calEndHour, setCalEndHour] = useState(20);
  const [calDefaultDuration, setCalDefaultDuration] = useState(60);
  const [calDefaultView, setCalDefaultView] = useState<string>("week");
  const [calHourHeight, setCalHourHeight] = useState(60);
  const [calShowHalfHourLines, setCalShowHalfHourLines] = useState(true);
  const [calSaving, setCalSaving] = useState(false);
  const [calError, setCalError] = useState<string | null>(null);
  const [calSuccess, setCalSuccess] = useState<string | null>(null);
  const [calLoading, setCalLoading] = useState(false);

  // ── AI tab state ────────────────────────────────────────────────────────────
  const [aiProvider, setAiProvider] = useState<"ollama" | "bedrock" | "claude">("ollama");
  const [aiOllamaUrl, setAiOllamaUrl] = useState("http://localhost:11434");
  const [aiModel, setAiModel] = useState("deepseek-r1:14b");
  const [aiBedrockAccessKey, setAiBedrockAccessKey] = useState("");
  const [aiBedrockSecretKey, setAiBedrockSecretKey] = useState("");
  const [aiBedrockRegion, setAiBedrockRegion] = useState("us-east-1");
  const [aiClaudeApiKey, setAiClaudeApiKey] = useState("");
  // Masked placeholders for secret fields (display-only, never saved back)
  const [aiBedrockModel, setAiBedrockModel] = useState("us.anthropic.claude-sonnet-4-6");
  const [aiClaudeModel, setAiClaudeModel] = useState("claude-sonnet-4-6");
  const [aiClaudeApiKeyPlaceholder, setAiClaudeApiKeyPlaceholder] = useState("Enter Claude API key...");
  const [aiBedrockAccessKeyPlaceholder, setAiBedrockAccessKeyPlaceholder] = useState("Enter AWS access key...");
  const [aiBedrockSecretKeyPlaceholder, setAiBedrockSecretKeyPlaceholder] = useState("Enter AWS secret key...");
  const [aiSaving, setAiSaving] = useState(false);
  const [aiError, setAiError] = useState<string | null>(null);
  const [aiSuccess, setAiSuccess] = useState<string | null>(null);
  const [aiLoading, setAiLoading] = useState(false);
  const [ollamaModels, setOllamaModels] = useState<string[]>([]);
  const [ollamaFetching, setOllamaFetching] = useState(false);
  const [ollamaFetchError, setOllamaFetchError] = useState<string | null>(null);

  // ── Schedule print settings state ──────────────────────────────────────────
  const [spIncludeCancelled, setSpIncludeCancelled] = useState(false);
  const [spDateFormat, setSpDateFormat] = useState("MM/DD/YYYY");
  const [spShowPatientDob, setSpShowPatientDob] = useState(false);
  const [spShowAppointmentType, setSpShowAppointmentType] = useState(true);
  const [spShowAppointmentStatus, setSpShowAppointmentStatus] = useState(true);
  const [spShowProviderName, setSpShowProviderName] = useState(true);
  const [spIncludeClinicLogo, setSpIncludeClinicLogo] = useState(true);
  const [spDocumentFormat, setSpDocumentFormat] = useState("letter");
  const [spOrientation, setSpOrientation] = useState("portrait");
  const [spSaving, setSpSaving] = useState(false);
  const [spError, setSpError] = useState<string | null>(null);
  const [spSuccess, setSpSuccess] = useState<string | null>(null);
  const [spLoading, setSpLoading] = useState(false);

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

  // ── Export: load settings when tab becomes active ────────────────────────────
  useEffect(() => {
    if (activeTab !== "export") return;
    let mounted = true;
    setExportLoading(true);
    commands.getExportSettings()
      .then((settings) => {
        if (!mounted) return;
        setExportPracticeName(settings.practiceName ?? "");
        setExportPracticeAddress(settings.practiceAddress ?? "");
        setExportPracticePhone(settings.practicePhone ?? "");
        setExportPracticeLogoBase64(settings.practiceLogoBase64 ?? null);
        setExportLogoWidthPx(settings.logoWidthPx ?? 200);
        setExportSignatureBase64(settings.signatureImageBase64 ?? null);
        setExportProviderCredentials(settings.providerNameCredentials ?? "");
        setExportLicenseNumber(settings.licenseNumber ?? "");
      })
      .catch(() => { /* not yet configured — silently ignore */ })
      .finally(() => { if (mounted) setExportLoading(false); });
    return () => { mounted = false; };
  }, [activeTab]);

  // ── Calendar: load settings when tab becomes active ────────────────────────
  useEffect(() => {
    if (activeTab !== "calendar") return;
    let mounted = true;
    setCalLoading(true);
    commands.getCalendarSettings()
      .then((settings) => {
        if (!mounted) return;
        setCalShowSaturday(settings.showSaturday);
        setCalShowSunday(settings.showSunday);
        setCalStartHour(settings.startHour);
        setCalEndHour(settings.endHour);
        setCalDefaultDuration(settings.defaultDurationMinutes);
        setCalDefaultView(settings.defaultView);
        setCalHourHeight(settings.hourHeightPx);
        setCalShowHalfHourLines(settings.showHalfHourLines);
      })
      .catch(() => { /* not yet configured — silently ignore */ })
      .finally(() => { if (mounted) setCalLoading(false); });
    return () => { mounted = false; };
  }, [activeTab]);

  // ── Schedule print: load settings when calendar tab becomes active ─────────
  useEffect(() => {
    if (activeTab !== "calendar") return;
    let mounted = true;
    setSpLoading(true);
    commands.getSchedulePrintSettings()
      .then((settings) => {
        if (!mounted) return;
        setSpIncludeCancelled(settings.includeCancelled ?? false);
        setSpDateFormat(settings.dateFormat ?? "MM/DD/YYYY");
        setSpShowPatientDob(settings.showPatientDob ?? false);
        setSpShowAppointmentType(settings.showAppointmentType ?? true);
        setSpShowAppointmentStatus(settings.showAppointmentStatus ?? true);
        setSpShowProviderName(settings.showProviderName ?? true);
        setSpIncludeClinicLogo(settings.includeClinicLogo ?? true);
        setSpDocumentFormat(settings.documentFormat ?? "letter");
        setSpOrientation(settings.orientation ?? "portrait");
      })
      .catch(() => { /* not yet configured — use defaults */ })
      .finally(() => { if (mounted) setSpLoading(false); });
    return () => { mounted = false; };
  }, [activeTab]);

  // ── AI: load settings when tab becomes active ──────────────────────────────
  useEffect(() => {
    if (activeTab !== "ai") return;
    let mounted = true;
    setAiLoading(true);
    commands.getLlmSettings()
      .then((settings) => {
        if (!mounted) return;
        const p = settings.provider as "ollama" | "bedrock" | "claude";
        if (p === "ollama" || p === "bedrock" || p === "claude") setAiProvider(p);
        if (settings.model) setAiModel(settings.model);
        if (settings.ollamaUrl) setAiOllamaUrl(settings.ollamaUrl);
        if (settings.bedrockRegion) setAiBedrockRegion(settings.bedrockRegion);
        if (settings.bedrockModel) setAiBedrockModel(settings.bedrockModel);
        if (settings.claudeModel) setAiClaudeModel(settings.claudeModel);
        if (settings.claudeApiKey) setAiClaudeApiKeyPlaceholder(`Configured (${settings.claudeApiKey})`);
        if (settings.bedrockAccessKey) setAiBedrockAccessKeyPlaceholder(`Configured (${settings.bedrockAccessKey})`);
        if (settings.bedrockSecretKey) setAiBedrockSecretKeyPlaceholder(`Configured (${settings.bedrockSecretKey})`);
        setAiClaudeApiKey("");
        setAiBedrockAccessKey("");
        setAiBedrockSecretKey("");
      })
      .catch(() => { /* not yet configured — use defaults */ })
      .finally(() => { if (mounted) setAiLoading(false); });
    return () => { mounted = false; };
  }, [activeTab]);

  const fetchOllamaModels = useCallback(async () => {
    setOllamaFetching(true);
    setOllamaFetchError(null);
    try {
      const status = await commands.checkOllamaStatus();
      if (!status.available) {
        setOllamaFetchError(status.error ?? "Ollama is not reachable");
        setOllamaModels([]);
        return;
      }
      setOllamaModels(status.models);
      if (status.models.length > 0) {
        const preferred = "deepseek-r1:14b";
        const hasPreferred = status.models.includes(preferred);
        setAiModel((prev) => {
          if (status.models.includes(prev)) return prev;
          return hasPreferred ? preferred : status.models[0];
        });
      }
    } catch (e) {
      setOllamaFetchError(e instanceof Error ? e.message : String(e));
      setOllamaModels([]);
    } finally {
      setOllamaFetching(false);
    }
  }, []);

  useEffect(() => {
    if (activeTab !== "ai" || aiProvider !== "ollama") return;
    fetchOllamaModels();
  }, [activeTab, aiProvider, fetchOllamaModels]);

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

  const handleSaveAiSettings = useCallback(async () => {
    setAiSaving(true);
    setAiError(null);
    setAiSuccess(null);
    try {
      await commands.configureLlmSettings({
        provider: aiProvider,
        model: aiProvider === "ollama" ? aiModel.trim() || null : null,
        ollamaUrl: aiProvider === "ollama" ? aiOllamaUrl.trim() || null : null,
        apiKey: aiProvider === "bedrock"
          ? aiBedrockAccessKey.trim() || null
          : aiProvider === "claude"
            ? aiClaudeApiKey.trim() || null
            : null,
        apiSecret: aiProvider === "bedrock"
          ? aiBedrockSecretKey.trim() || null
          : null,
        bedrockRegion: aiProvider === "bedrock" ? aiBedrockRegion.trim() || null : null,
        bedrockModel: aiProvider === "bedrock" ? aiBedrockModel : null,
        claudeModel: aiProvider === "claude" ? aiClaudeModel : null,
      });
      setAiSuccess("AI settings saved successfully.");
    } catch (e) {
      setAiError(e instanceof Error ? e.message : String(e));
    } finally {
      setAiSaving(false);
    }
  }, [aiProvider, aiModel, aiOllamaUrl, aiBedrockAccessKey, aiBedrockSecretKey, aiBedrockRegion, aiBedrockModel, aiClaudeApiKey, aiClaudeModel]);

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

  const handleSaveProfile = useCallback(async () => {
    setProfileSaving(true);
    setProfileError(null);
    setProfileSuccess(null);
    try {
      await commands.updateUserProfile(profileDisplayName, profileUsername);
      setProfileSuccess("Profile updated successfully.");
    } catch (e) {
      setProfileError(e instanceof Error ? e.message : String(e));
    } finally {
      setProfileSaving(false);
    }
  }, [profileDisplayName, profileUsername]);

  const handleChangePassword = useCallback(async () => {
    setPwdError(null);
    setPwdSuccess(null);

    if (!pwdCurrent || !pwdNew || !pwdConfirm) {
      setPwdError("All password fields are required.");
      return;
    }
    if (pwdNew.length < 12) {
      setPwdError("New password must be at least 12 characters.");
      return;
    }
    if (pwdNew !== pwdConfirm) {
      setPwdError("New password and confirmation do not match.");
      return;
    }

    setPwdSaving(true);
    try {
      await commands.changePassword(pwdCurrent, pwdNew);
      setPwdSuccess("Password changed successfully.");
      setPwdCurrent("");
      setPwdNew("");
      setPwdConfirm("");
    } catch (e) {
      setPwdError(e instanceof Error ? e.message : String(e));
    } finally {
      setPwdSaving(false);
    }
  }, [pwdCurrent, pwdNew, pwdConfirm]);

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

  // ── Export settings handlers ─────────────────────────────────────────────────

  /**
   * Base64-encode a Uint8Array in 8 KB chunks using btoa.
   * Chunked to avoid stack overflow from String.fromCharCode spread on large arrays.
   * We build the full binary string first, then call btoa once so intermediate
   * padding characters don't corrupt the output.
   */
  function bytesToBase64(bytes: Uint8Array): string {
    const CHUNK = 8192;
    let binaryStr = "";
    for (let i = 0; i < bytes.length; i += CHUNK) {
      binaryStr += String.fromCharCode(...bytes.subarray(i, i + CHUNK));
    }
    return btoa(binaryStr);
  }

  const handleSaveExportSettings = useCallback(async () => {
    setExportSaving(true);
    setExportError(null);
    setExportSuccess(null);
    try {
      const settings: ExportSettings = {
        practiceName: exportPracticeName.trim() || null,
        practiceAddress: exportPracticeAddress.trim() || null,
        practicePhone: exportPracticePhone.trim() || null,
        practiceLogoBase64: exportPracticeLogoBase64,
        logoWidthPx: exportLogoWidthPx,
        signatureImageBase64: exportSignatureBase64,
        providerNameCredentials: exportProviderCredentials.trim() || null,
        licenseNumber: exportLicenseNumber.trim() || null,
      };
      await commands.setExportSettings(settings);
      setExportSuccess("Export settings saved successfully.");
    } catch (e) {
      setExportError(e instanceof Error ? e.message : String(e));
    } finally {
      setExportSaving(false);
    }
  }, [
    exportPracticeName, exportPracticeAddress, exportPracticePhone,
    exportPracticeLogoBase64, exportLogoWidthPx, exportSignatureBase64,
    exportProviderCredentials, exportLicenseNumber,
  ]);

  const handlePickLogo = useCallback(async () => {
    try {
      const result = await open({
        multiple: false,
        filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "gif", "webp"] }],
      });
      if (result === null) return;
      const selectedPath = Array.isArray(result) ? result[0] : result;
      if (!selectedPath) return;
      const bytes = await readFile(selectedPath);
      // Validate file size (max 1MB)
      if (bytes.length > 1024 * 1024) {
        setExportError("Logo image must be under 1 MB.");
        return;
      }
      const ext = selectedPath.split(".").pop()?.toLowerCase() ?? "png";
      const mime = ext === "jpg" || ext === "jpeg" ? "image/jpeg"
        : ext === "png" ? "image/png"
        : ext === "gif" ? "image/gif"
        : ext === "webp" ? "image/webp"
        : "image/png";
      const base64 = `data:${mime};base64,${bytesToBase64(bytes)}`;
      setExportPracticeLogoBase64(base64);
    } catch (e) {
      setExportError(e instanceof Error ? e.message : String(e));
    }
  }, []);

  const handlePickSignature = useCallback(async () => {
    try {
      const result = await open({
        multiple: false,
        filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "gif", "webp"] }],
      });
      if (result === null) return;
      const selectedPath = Array.isArray(result) ? result[0] : result;
      if (!selectedPath) return;
      const bytes = await readFile(selectedPath);
      // Validate file size (max 1MB)
      if (bytes.length > 1024 * 1024) {
        setExportError("Signature image must be under 1 MB.");
        return;
      }
      const ext = selectedPath.split(".").pop()?.toLowerCase() ?? "png";
      const mime = ext === "jpg" || ext === "jpeg" ? "image/jpeg"
        : ext === "png" ? "image/png"
        : ext === "gif" ? "image/gif"
        : ext === "webp" ? "image/webp"
        : "image/png";
      const base64 = `data:${mime};base64,${bytesToBase64(bytes)}`;
      setExportSignatureBase64(base64);
    } catch (e) {
      setExportError(e instanceof Error ? e.message : String(e));
    }
  }, []);

  const handleSaveCalendarSettings = useCallback(async () => {
    setCalSaving(true);
    setCalError(null);
    setCalSuccess(null);
    try {
      await commands.saveCalendarSettings({
        showSaturday: calShowSaturday,
        showSunday: calShowSunday,
        startHour: calStartHour,
        endHour: calEndHour,
        defaultDurationMinutes: calDefaultDuration,
        defaultView: calDefaultView,
        hourHeightPx: calHourHeight,
        showHalfHourLines: calShowHalfHourLines,
      });
      setCalSuccess("Calendar settings saved successfully.");
    } catch (e) {
      setCalError(e instanceof Error ? e.message : String(e));
    } finally {
      setCalSaving(false);
    }
  }, [
    calShowSaturday, calShowSunday, calStartHour, calEndHour,
    calDefaultDuration, calDefaultView, calHourHeight, calShowHalfHourLines,
  ]);

  const handleSaveSchedulePrintSettings = useCallback(async () => {
    setSpSaving(true);
    setSpError(null);
    setSpSuccess(null);
    try {
      await commands.saveSchedulePrintSettings({
        includeCalendarEvents: null,
        includeCancelled: spIncludeCancelled,
        dateFormat: spDateFormat,
        showPatientDob: spShowPatientDob,
        showAppointmentType: spShowAppointmentType,
        showAppointmentStatus: spShowAppointmentStatus,
        clinicName: null,
        clinicAddress: null,
        clinicPhone: null,
        includeClinicLogo: spIncludeClinicLogo,
        documentFormat: spDocumentFormat,
        orientation: spOrientation,
        showProviderName: spShowProviderName,
      });
      setSpSuccess("Schedule print settings saved successfully.");
    } catch (e) {
      setSpError(e instanceof Error ? e.message : String(e));
    } finally {
      setSpSaving(false);
    }
  }, [
    spIncludeCancelled, spDateFormat, spShowPatientDob, spShowAppointmentType,
    spShowAppointmentStatus, spShowProviderName, spIncludeClinicLogo,
    spDocumentFormat, spOrientation,
  ]);

  // ── Render ────────────────────────────────────────────────────────────────────

  const tabs: { id: Tab; label: string; adminOnly?: boolean }[] = [
    { id: "backup", label: "Backup" },
    { id: "security", label: "Security" },
    { id: "fax", label: "Fax" },
    { id: "reminders", label: "Reminders", adminOnly: true },
    { id: "export", label: "Export" },
    { id: "calendar", label: "Calendar" },
    { id: "ai", label: "AI" },
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

            {/* ── Provider Appointment Types Configuration ──────────────────────── */}
            <ProviderAppointmentTypesSection users={users} />
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
                    placeholder="e.g. PanaceaEMR Physical Therapy"
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
                    placeholder="PanaceaEMR Physical Therapy"
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

        {/* ─── EXPORT TAB ──────────────────────────────────────────────────────── */}
        {activeTab === "export" && (
          <div className="space-y-6 max-w-3xl">

            {exportError && (
              <div className="rounded-md border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700">
                {exportError}
              </div>
            )}
            {exportSuccess && (
              <div className="rounded-md border border-green-200 bg-green-50 px-4 py-3 text-sm text-green-800">
                {exportSuccess}
              </div>
            )}

            {exportLoading ? (
              <p className="text-sm text-gray-500">Loading export settings...</p>
            ) : (
              <>
                {/* Letterhead Settings */}
                <section className="rounded-lg border border-gray-200 bg-white p-5 shadow-sm">
                  <h2 className="mb-1 text-base font-semibold text-gray-900">
                    Letterhead Settings
                  </h2>
                  <p className="mb-4 text-sm text-gray-500">
                    Configure your practice letterhead for PDF exports and reports.
                  </p>

                  <div className="space-y-4">
                    <div>
                      <label className="mb-1 block text-sm font-medium text-gray-700">
                        Practice Name
                      </label>
                      <input
                        type="text"
                        value={exportPracticeName}
                        onChange={(e) => setExportPracticeName(e.target.value)}
                        placeholder="e.g. PanaceaEMR Physical Therapy"
                        className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
                      />
                    </div>

                    <div>
                      <label className="mb-1 block text-sm font-medium text-gray-700">
                        Practice Address
                      </label>
                      <input
                        type="text"
                        value={exportPracticeAddress}
                        onChange={(e) => setExportPracticeAddress(e.target.value)}
                        placeholder="e.g. 123 Main St, Suite 100, City, ST 12345"
                        className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
                      />
                    </div>

                    <div>
                      <label className="mb-1 block text-sm font-medium text-gray-700">
                        Practice Phone
                      </label>
                      <input
                        type="tel"
                        value={exportPracticePhone}
                        onChange={(e) => setExportPracticePhone(e.target.value)}
                        placeholder="e.g. (555) 123-4567"
                        className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
                      />
                    </div>

                    <div>
                      <label className="mb-1 block text-sm font-medium text-gray-700">
                        Practice Logo
                      </label>
                      <div className="flex items-center gap-3">
                        <button
                          type="button"
                          onClick={handlePickLogo}
                          className="rounded-md border border-gray-300 bg-white px-4 py-2 text-sm font-medium text-gray-700 shadow-sm transition-colors hover:bg-gray-50"
                        >
                          {exportPracticeLogoBase64 ? "Change Logo" : "Upload Logo"}
                        </button>
                        {exportPracticeLogoBase64 && (
                          <button
                            type="button"
                            onClick={() => setExportPracticeLogoBase64(null)}
                            className="rounded-md border border-red-200 bg-red-50 px-3 py-2 text-sm font-medium text-red-700 hover:bg-red-100"
                          >
                            Remove
                          </button>
                        )}
                      </div>
                      {exportPracticeLogoBase64 && (
                        <div className="mt-3 space-y-3">
                          <div className="rounded-md border border-gray-200 bg-gray-50 p-3 inline-block">
                            <img
                              src={exportPracticeLogoBase64}
                              alt="Practice logo"
                              style={{ width: `${exportLogoWidthPx}px`, height: "auto" }}
                              className="object-contain"
                            />
                          </div>
                          <div>
                            <label className="mb-1 block text-xs font-medium text-gray-600">
                              Logo Width: {exportLogoWidthPx}px
                            </label>
                            <input
                              type="range"
                              min={50}
                              max={500}
                              value={exportLogoWidthPx}
                              onChange={(e) => setExportLogoWidthPx(Number(e.target.value))}
                              className="w-64 accent-blue-600"
                            />
                            <div className="flex justify-between text-xs text-gray-400 w-64">
                              <span>50px</span>
                              <span>500px</span>
                            </div>
                          </div>
                        </div>
                      )}
                      <p className="mt-1 text-xs text-gray-400">
                        Accepted formats: PNG, JPG, GIF, WebP. Recommended: 300px wide, transparent background.
                      </p>
                    </div>
                  </div>
                </section>

                {/* Signature Settings */}
                <section className="rounded-lg border border-gray-200 bg-white p-5 shadow-sm">
                  <h2 className="mb-1 text-base font-semibold text-gray-900">
                    Signature Settings
                  </h2>
                  <p className="mb-4 text-sm text-gray-500">
                    Configure provider signature details for PDF exports and clinical documents.
                  </p>

                  <div className="space-y-4">
                    <div>
                      <label className="mb-1 block text-sm font-medium text-gray-700">
                        Provider Name &amp; Credentials
                      </label>
                      <input
                        type="text"
                        value={exportProviderCredentials}
                        onChange={(e) => setExportProviderCredentials(e.target.value)}
                        placeholder="e.g. Omar Safwat Sharaf, PT, DPT"
                        className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
                      />
                    </div>

                    <div>
                      <label className="mb-1 block text-sm font-medium text-gray-700">
                        License Number
                      </label>
                      <input
                        type="text"
                        value={exportLicenseNumber}
                        onChange={(e) => setExportLicenseNumber(e.target.value)}
                        placeholder="e.g. PT-12345"
                        className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
                      />
                    </div>

                    <div>
                      <label className="mb-1 block text-sm font-medium text-gray-700">
                        Signature Image
                      </label>
                      <div className="flex items-center gap-3">
                        <button
                          type="button"
                          onClick={handlePickSignature}
                          className="rounded-md border border-gray-300 bg-white px-4 py-2 text-sm font-medium text-gray-700 shadow-sm transition-colors hover:bg-gray-50"
                        >
                          {exportSignatureBase64 ? "Change Signature" : "Upload Signature"}
                        </button>
                        {exportSignatureBase64 && (
                          <button
                            type="button"
                            onClick={() => setExportSignatureBase64(null)}
                            className="rounded-md border border-red-200 bg-red-50 px-3 py-2 text-sm font-medium text-red-700 hover:bg-red-100"
                          >
                            Remove
                          </button>
                        )}
                      </div>
                      {exportSignatureBase64 && (
                        <div className="mt-3 rounded-md border border-gray-200 bg-gray-50 p-3 inline-block">
                          <img
                            src={exportSignatureBase64}
                            alt="Provider signature"
                            className="max-h-16 max-w-[200px] object-contain"
                          />
                        </div>
                      )}
                      <p className="mt-1 text-xs text-gray-400">
                        Upload a scanned or digital signature image. PNG with transparent background recommended.
                      </p>
                    </div>
                  </div>
                </section>

                {/* Letterhead Preview */}
                <section className="rounded-lg border border-gray-200 bg-white p-5 shadow-sm">
                  <h2 className="mb-4 text-base font-semibold text-gray-900">
                    Letterhead Preview
                  </h2>
                  <div className="rounded-lg border border-gray-300 bg-white p-6 shadow-inner" style={{ minHeight: "200px" }}>
                    {/* Letterhead area */}
                    <div className="border-b-2 border-gray-800 pb-4 mb-4">
                      <div className="flex items-start gap-4">
                        {exportPracticeLogoBase64 && (
                          <img
                            src={exportPracticeLogoBase64}
                            alt="Logo preview"
                            style={{ width: `${Math.min(exportLogoWidthPx, 200)}px`, height: "auto" }}
                            className="object-contain"
                          />
                        )}
                        <div className="flex-1">
                          <div className="text-lg font-bold text-gray-900">
                            {exportPracticeName || "Practice Name"}
                          </div>
                          <div className="text-sm text-gray-600">
                            {exportPracticeAddress || "Practice Address"}
                          </div>
                          <div className="text-sm text-gray-600">
                            {exportPracticePhone || "Phone Number"}
                          </div>
                        </div>
                      </div>
                    </div>

                    {/* Body placeholder */}
                    <div className="space-y-2 mb-8">
                      <div className="h-3 w-full rounded bg-gray-100" />
                      <div className="h-3 w-5/6 rounded bg-gray-100" />
                      <div className="h-3 w-4/5 rounded bg-gray-100" />
                      <div className="h-3 w-full rounded bg-gray-100" />
                      <div className="h-3 w-3/4 rounded bg-gray-100" />
                    </div>

                    {/* Signature area */}
                    <div className="border-t border-gray-200 pt-4">
                      {exportSignatureBase64 ? (
                        <img
                          src={exportSignatureBase64}
                          alt="Signature preview"
                          className="h-10 w-auto object-contain mb-1"
                        />
                      ) : (
                        <div className="mb-1 w-48 border-b border-gray-400" style={{ height: "40px" }} />
                      )}
                      <div className="text-sm font-medium text-gray-900">
                        {exportProviderCredentials || "Provider Name, Credentials"}
                      </div>
                      {exportLicenseNumber && (
                        <div className="text-xs text-gray-500">
                          License: {exportLicenseNumber}
                        </div>
                      )}
                    </div>
                  </div>
                </section>

                {/* Save button */}
                <div className="flex gap-3">
                  <button
                    type="button"
                    onClick={handleSaveExportSettings}
                    disabled={exportSaving}
                    className="rounded-md bg-blue-600 px-4 py-2 text-sm font-medium text-white shadow-sm transition-colors hover:bg-blue-700 disabled:cursor-not-allowed disabled:opacity-50"
                  >
                    {exportSaving ? "Saving..." : "Save Export Settings"}
                  </button>
                </div>
              </>
            )}
          </div>
        )}

        {/* ─── CALENDAR TAB ───────────────────────────────────────────────────── */}
        {activeTab === "calendar" && (
          <div className="space-y-6 max-w-3xl">

            {calError && (
              <div className="rounded-md border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700">
                {calError}
              </div>
            )}
            {calSuccess && (
              <div className="rounded-md border border-green-200 bg-green-50 px-4 py-3 text-sm text-green-800">
                {calSuccess}
              </div>
            )}

            {calLoading ? (
              <p className="text-sm text-gray-500">Loading calendar settings...</p>
            ) : (
              <>
                {/* Week View Settings */}
                <section className="rounded-lg border border-gray-200 bg-white p-5 shadow-sm">
                  <h2 className="mb-1 text-base font-semibold text-gray-900">
                    Week View Settings
                  </h2>
                  <p className="mb-4 text-sm text-gray-500">
                    Configure which days and hours are visible in the calendar week view.
                  </p>

                  <div className="space-y-4">
                    {/* Show Saturday */}
                    <div className="flex items-center justify-between">
                      <label className="text-sm font-medium text-gray-700">Show Saturday</label>
                      <button
                        type="button"
                        onClick={() => setCalShowSaturday((v) => !v)}
                        className={`relative inline-flex h-6 w-11 items-center rounded-full transition-colors focus:outline-none focus:ring-2 focus:ring-blue-500 focus:ring-offset-2 ${
                          calShowSaturday ? "bg-blue-600" : "bg-gray-200"
                        }`}
                      >
                        <span
                          className={`inline-block h-4 w-4 transform rounded-full bg-white transition-transform ${
                            calShowSaturday ? "translate-x-6" : "translate-x-1"
                          }`}
                        />
                      </button>
                    </div>

                    {/* Show Sunday */}
                    <div className="flex items-center justify-between">
                      <label className="text-sm font-medium text-gray-700">Show Sunday</label>
                      <button
                        type="button"
                        onClick={() => setCalShowSunday((v) => !v)}
                        className={`relative inline-flex h-6 w-11 items-center rounded-full transition-colors focus:outline-none focus:ring-2 focus:ring-blue-500 focus:ring-offset-2 ${
                          calShowSunday ? "bg-blue-600" : "bg-gray-200"
                        }`}
                      >
                        <span
                          className={`inline-block h-4 w-4 transform rounded-full bg-white transition-transform ${
                            calShowSunday ? "translate-x-6" : "translate-x-1"
                          }`}
                        />
                      </button>
                    </div>

                    {/* Start Hour */}
                    <div>
                      <label className="mb-1 block text-sm font-medium text-gray-700">
                        Start Hour
                      </label>
                      <select
                        value={calStartHour}
                        onChange={(e) => setCalStartHour(Number(e.target.value))}
                        className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
                      >
                        {[5, 6, 7, 8, 9, 10].map((h) => (
                          <option key={h} value={h}>
                            {h <= 11 ? `${h}:00 AM` : `${h - 12 || 12}:00 PM`}
                          </option>
                        ))}
                      </select>
                    </div>

                    {/* End Hour */}
                    <div>
                      <label className="mb-1 block text-sm font-medium text-gray-700">
                        End Hour
                      </label>
                      <select
                        value={calEndHour}
                        onChange={(e) => setCalEndHour(Number(e.target.value))}
                        className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
                      >
                        {[17, 18, 19, 20, 21, 22].map((h) => (
                          <option key={h} value={h}>
                            {h <= 11 ? `${h}:00 AM` : `${h - 12 || 12}:00 PM`}
                          </option>
                        ))}
                      </select>
                    </div>

                    {/* Default Appointment Duration */}
                    <div>
                      <label className="mb-1 block text-sm font-medium text-gray-700">
                        Default Appointment Duration
                      </label>
                      <select
                        value={calDefaultDuration}
                        onChange={(e) => setCalDefaultDuration(Number(e.target.value))}
                        className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
                      >
                        <option value={15}>15 minutes</option>
                        <option value={30}>30 minutes</option>
                        <option value={45}>45 minutes</option>
                        <option value={60}>60 minutes</option>
                      </select>
                    </div>

                    {/* Default View */}
                    <div>
                      <label className="mb-1 block text-sm font-medium text-gray-700">
                        Default View
                      </label>
                      <select
                        value={calDefaultView}
                        onChange={(e) => setCalDefaultView(e.target.value)}
                        className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
                      >
                        <option value="day">Day</option>
                        <option value="week">Week</option>
                      </select>
                    </div>
                  </div>
                </section>

                {/* Appearance */}
                <section className="rounded-lg border border-gray-200 bg-white p-5 shadow-sm">
                  <h2 className="mb-1 text-base font-semibold text-gray-900">
                    Appearance
                  </h2>
                  <p className="mb-4 text-sm text-gray-500">
                    Customize the visual appearance of the calendar grid.
                  </p>

                  <div className="space-y-4">
                    {/* Hour Height */}
                    <div>
                      <label className="mb-1 block text-sm font-medium text-gray-700">
                        Hour Height
                      </label>
                      <select
                        value={calHourHeight}
                        onChange={(e) => setCalHourHeight(Number(e.target.value))}
                        className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
                      >
                        <option value={40}>Small (40px)</option>
                        <option value={60}>Medium (60px)</option>
                        <option value={80}>Large (80px)</option>
                      </select>
                    </div>

                    {/* Show Half-Hour Lines */}
                    <div className="flex items-center justify-between">
                      <label className="text-sm font-medium text-gray-700">Show Half-Hour Lines</label>
                      <button
                        type="button"
                        onClick={() => setCalShowHalfHourLines((v) => !v)}
                        className={`relative inline-flex h-6 w-11 items-center rounded-full transition-colors focus:outline-none focus:ring-2 focus:ring-blue-500 focus:ring-offset-2 ${
                          calShowHalfHourLines ? "bg-blue-600" : "bg-gray-200"
                        }`}
                      >
                        <span
                          className={`inline-block h-4 w-4 transform rounded-full bg-white transition-transform ${
                            calShowHalfHourLines ? "translate-x-6" : "translate-x-1"
                          }`}
                        />
                      </button>
                    </div>
                  </div>
                </section>

                {/* Save button */}
                <div className="flex gap-3">
                  <button
                    type="button"
                    onClick={handleSaveCalendarSettings}
                    disabled={calSaving}
                    className="rounded-md bg-blue-600 px-4 py-2 text-sm font-medium text-white shadow-sm transition-colors hover:bg-blue-700 disabled:cursor-not-allowed disabled:opacity-50"
                  >
                    {calSaving ? "Saving..." : "Save Calendar Settings"}
                  </button>
                </div>

                {/* ── Schedule Print Settings ────────────────────────────── */}
                {spError && (
                  <div className="rounded-md border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700">
                    {spError}
                  </div>
                )}
                {spSuccess && (
                  <div className="rounded-md border border-green-200 bg-green-50 px-4 py-3 text-sm text-green-800">
                    {spSuccess}
                  </div>
                )}

                {spLoading ? (
                  <p className="text-sm text-gray-500">Loading print settings...</p>
                ) : (
                  <>
                    <section className="rounded-lg border border-gray-200 bg-white p-5 shadow-sm">
                      <h2 className="mb-1 text-base font-semibold text-gray-900">
                        Schedule Print Settings
                      </h2>
                      <p className="mb-4 text-sm text-gray-500">
                        Configure how schedule PDFs are generated when printing appointments.
                      </p>

                      <div className="space-y-4">
                        {/* Include Cancelled */}
                        <div className="flex items-center justify-between">
                          <label className="text-sm font-medium text-gray-700">Include Cancelled Appointments</label>
                          <button
                            type="button"
                            onClick={() => setSpIncludeCancelled((v) => !v)}
                            className={`relative inline-flex h-6 w-11 items-center rounded-full transition-colors focus:outline-none focus:ring-2 focus:ring-blue-500 focus:ring-offset-2 ${
                              spIncludeCancelled ? "bg-blue-600" : "bg-gray-200"
                            }`}
                          >
                            <span
                              className={`inline-block h-4 w-4 transform rounded-full bg-white transition-transform ${
                                spIncludeCancelled ? "translate-x-6" : "translate-x-1"
                              }`}
                            />
                          </button>
                        </div>

                        {/* Show Patient DOB */}
                        <div className="flex items-center justify-between">
                          <label className="text-sm font-medium text-gray-700">Show Patient Date of Birth</label>
                          <button
                            type="button"
                            onClick={() => setSpShowPatientDob((v) => !v)}
                            className={`relative inline-flex h-6 w-11 items-center rounded-full transition-colors focus:outline-none focus:ring-2 focus:ring-blue-500 focus:ring-offset-2 ${
                              spShowPatientDob ? "bg-blue-600" : "bg-gray-200"
                            }`}
                          >
                            <span
                              className={`inline-block h-4 w-4 transform rounded-full bg-white transition-transform ${
                                spShowPatientDob ? "translate-x-6" : "translate-x-1"
                              }`}
                            />
                          </button>
                        </div>

                        {/* Show Appointment Type */}
                        <div className="flex items-center justify-between">
                          <label className="text-sm font-medium text-gray-700">Show Appointment Type</label>
                          <button
                            type="button"
                            onClick={() => setSpShowAppointmentType((v) => !v)}
                            className={`relative inline-flex h-6 w-11 items-center rounded-full transition-colors focus:outline-none focus:ring-2 focus:ring-blue-500 focus:ring-offset-2 ${
                              spShowAppointmentType ? "bg-blue-600" : "bg-gray-200"
                            }`}
                          >
                            <span
                              className={`inline-block h-4 w-4 transform rounded-full bg-white transition-transform ${
                                spShowAppointmentType ? "translate-x-6" : "translate-x-1"
                              }`}
                            />
                          </button>
                        </div>

                        {/* Show Appointment Status */}
                        <div className="flex items-center justify-between">
                          <label className="text-sm font-medium text-gray-700">Show Appointment Status</label>
                          <button
                            type="button"
                            onClick={() => setSpShowAppointmentStatus((v) => !v)}
                            className={`relative inline-flex h-6 w-11 items-center rounded-full transition-colors focus:outline-none focus:ring-2 focus:ring-blue-500 focus:ring-offset-2 ${
                              spShowAppointmentStatus ? "bg-blue-600" : "bg-gray-200"
                            }`}
                          >
                            <span
                              className={`inline-block h-4 w-4 transform rounded-full bg-white transition-transform ${
                                spShowAppointmentStatus ? "translate-x-6" : "translate-x-1"
                              }`}
                            />
                          </button>
                        </div>

                        {/* Show Provider Name */}
                        <div className="flex items-center justify-between">
                          <label className="text-sm font-medium text-gray-700">Show Provider Name</label>
                          <button
                            type="button"
                            onClick={() => setSpShowProviderName((v) => !v)}
                            className={`relative inline-flex h-6 w-11 items-center rounded-full transition-colors focus:outline-none focus:ring-2 focus:ring-blue-500 focus:ring-offset-2 ${
                              spShowProviderName ? "bg-blue-600" : "bg-gray-200"
                            }`}
                          >
                            <span
                              className={`inline-block h-4 w-4 transform rounded-full bg-white transition-transform ${
                                spShowProviderName ? "translate-x-6" : "translate-x-1"
                              }`}
                            />
                          </button>
                        </div>

                        {/* Include Clinic Letterhead */}
                        <div className="flex items-center justify-between">
                          <label className="text-sm font-medium text-gray-700">Include Clinic Letterhead</label>
                          <button
                            type="button"
                            onClick={() => setSpIncludeClinicLogo((v) => !v)}
                            className={`relative inline-flex h-6 w-11 items-center rounded-full transition-colors focus:outline-none focus:ring-2 focus:ring-blue-500 focus:ring-offset-2 ${
                              spIncludeClinicLogo ? "bg-blue-600" : "bg-gray-200"
                            }`}
                          >
                            <span
                              className={`inline-block h-4 w-4 transform rounded-full bg-white transition-transform ${
                                spIncludeClinicLogo ? "translate-x-6" : "translate-x-1"
                              }`}
                            />
                          </button>
                        </div>

                        {/* Date Format */}
                        <div>
                          <label className="mb-1 block text-sm font-medium text-gray-700">
                            Date Format
                          </label>
                          <select
                            value={spDateFormat}
                            onChange={(e) => setSpDateFormat(e.target.value)}
                            className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
                          >
                            <option value="MM/DD/YYYY">MM/DD/YYYY</option>
                            <option value="DD/MM/YYYY">DD/MM/YYYY</option>
                            <option value="YYYY-MM-DD">YYYY-MM-DD</option>
                          </select>
                        </div>

                        {/* Document Format */}
                        <div>
                          <label className="mb-1 block text-sm font-medium text-gray-700">
                            Paper Size
                          </label>
                          <select
                            value={spDocumentFormat}
                            onChange={(e) => setSpDocumentFormat(e.target.value)}
                            className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
                          >
                            <option value="letter">Letter (8.5 x 11)</option>
                            <option value="a4">A4 (210 x 297 mm)</option>
                          </select>
                        </div>

                        {/* Orientation */}
                        <div>
                          <label className="mb-1 block text-sm font-medium text-gray-700">
                            Orientation
                          </label>
                          <select
                            value={spOrientation}
                            onChange={(e) => setSpOrientation(e.target.value)}
                            className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
                          >
                            <option value="portrait">Portrait</option>
                            <option value="landscape">Landscape</option>
                          </select>
                        </div>
                      </div>
                    </section>

                    {/* Save Print Settings button */}
                    <div className="flex gap-3">
                      <button
                        type="button"
                        onClick={handleSaveSchedulePrintSettings}
                        disabled={spSaving}
                        className="rounded-md bg-blue-600 px-4 py-2 text-sm font-medium text-white shadow-sm transition-colors hover:bg-blue-700 disabled:cursor-not-allowed disabled:opacity-50"
                      >
                        {spSaving ? "Saving..." : "Save Print Settings"}
                      </button>
                    </div>
                  </>
                )}
              </>
            )}
          </div>
        )}

        {/* ─── AI TAB ──────────────────────────────────────────────────────────── */}
        {activeTab === "ai" && (
          <div className="space-y-6 max-w-2xl">

            {aiError && (
              <div className="rounded-md border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700">
                {aiError}
              </div>
            )}
            {aiSuccess && (
              <div className="rounded-md border border-green-200 bg-green-50 px-4 py-3 text-sm text-green-800">
                {aiSuccess}
              </div>
            )}

            {aiLoading ? (
              <p className="text-sm text-gray-500">Loading AI settings...</p>
            ) : (
              <>
                <section className="rounded-lg border border-gray-200 bg-white p-5 shadow-sm">
                  <h2 className="mb-1 text-base font-semibold text-gray-900">
                    LLM Provider
                  </h2>
                  <p className="mb-4 text-sm text-gray-500">
                    Choose between a local model (Ollama) or a cloud provider for AI-assisted note generation.
                  </p>

                  <div className="flex gap-4">
                    {([
                      { value: "ollama" as const, label: "Local (Ollama)" },
                      { value: "bedrock" as const, label: "Cloud (AWS Bedrock)" },
                      { value: "claude" as const, label: "Claude API" },
                    ]).map((opt) => (
                      <label key={opt.value} className="flex items-center gap-2 text-sm text-gray-700 cursor-pointer">
                        <input
                          type="radio"
                          name="ai-provider"
                          value={opt.value}
                          checked={aiProvider === opt.value}
                          onChange={() => setAiProvider(opt.value)}
                          className="h-4 w-4 border-gray-300 text-blue-600 focus:ring-blue-500"
                        />
                        {opt.label}
                      </label>
                    ))}
                  </div>
                </section>

                {aiProvider === "ollama" && (
                  <section className="rounded-lg border border-gray-200 bg-white p-5 shadow-sm">
                    <h2 className="mb-1 text-base font-semibold text-gray-900">
                      Ollama Settings
                    </h2>
                    <p className="mb-4 text-sm text-gray-500">
                      Configure the local Ollama instance for on-device AI processing.
                    </p>

                    <div className="space-y-4">
                      <div>
                        <label className="mb-1 block text-sm font-medium text-gray-700">
                          Ollama URL
                        </label>
                        <input
                          type="text"
                          value={aiOllamaUrl}
                          onChange={(e) => setAiOllamaUrl(e.target.value)}
                          placeholder="http://localhost:11434"
                          className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
                        />
                      </div>

                      <div>
                        <label className="mb-1 block text-sm font-medium text-gray-700">
                          Model
                        </label>
                        {ollamaFetchError ? (
                          <>
                            <input
                              type="text"
                              value={aiModel}
                              onChange={(e) => setAiModel(e.target.value)}
                              placeholder="deepseek-r1:14b"
                              className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
                            />
                            <p className="mt-1 text-xs text-red-600">{ollamaFetchError}</p>
                          </>
                        ) : (
                          <div className="flex gap-2">
                            <select
                              value={aiModel}
                              onChange={(e) => setAiModel(e.target.value)}
                              disabled={ollamaFetching || ollamaModels.length === 0}
                              className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
                            >
                              {ollamaModels.length === 0 && (
                                <option value="">
                                  {ollamaFetching ? "Loading models..." : "No models found"}
                                </option>
                              )}
                              {ollamaModels.map((m) => (
                                <option key={m} value={m}>{m}</option>
                              ))}
                            </select>
                            <button
                              type="button"
                              onClick={fetchOllamaModels}
                              disabled={ollamaFetching}
                              className="shrink-0 rounded-md border border-gray-300 bg-white px-3 py-2 text-sm font-medium text-gray-700 shadow-sm transition-colors hover:bg-gray-50 disabled:cursor-not-allowed disabled:opacity-50"
                            >
                              {ollamaFetching ? "..." : "Refresh"}
                            </button>
                          </div>
                        )}
                      </div>
                    </div>
                  </section>
                )}

                {aiProvider === "bedrock" && (
                  <section className="rounded-lg border border-gray-200 bg-white p-5 shadow-sm">
                    <h2 className="mb-1 text-base font-semibold text-gray-900">
                      AWS Bedrock Settings
                    </h2>
                    <p className="mb-4 text-sm text-gray-500">
                      Enter your AWS credentials for Bedrock AI access. A BAA is required for PHI.
                    </p>

                    <div className="space-y-4">
                      <div>
                        <label className="mb-1 block text-sm font-medium text-gray-700">
                          Access Key
                        </label>
                        <input
                          type="text"
                          value={aiBedrockAccessKey}
                          onChange={(e) => setAiBedrockAccessKey(e.target.value)}
                          placeholder={aiBedrockAccessKeyPlaceholder}
                          className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
                        />
                      </div>

                      <div>
                        <label className="mb-1 block text-sm font-medium text-gray-700">
                          Secret Key
                        </label>
                        <input
                          type="password"
                          value={aiBedrockSecretKey}
                          onChange={(e) => setAiBedrockSecretKey(e.target.value)}
                          placeholder={aiBedrockSecretKeyPlaceholder}
                          className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
                        />
                      </div>

                      <div>
                        <label className="mb-1 block text-sm font-medium text-gray-700">
                          Region
                        </label>
                        <input
                          type="text"
                          value={aiBedrockRegion}
                          onChange={(e) => setAiBedrockRegion(e.target.value)}
                          placeholder="us-east-1"
                          className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
                        />
                      </div>

                      <div>
                        <label className="mb-1 block text-sm font-medium text-gray-700">
                          Model
                        </label>
                        <select
                          value={aiBedrockModel}
                          onChange={(e) => setAiBedrockModel(e.target.value)}
                          className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
                        >
                          <option value="us.anthropic.claude-sonnet-4-6">Claude Sonnet 4 (us.anthropic.claude-sonnet-4-6)</option>
                          <option value="us.anthropic.claude-haiku-4-5-20251001-v1:0">Claude Haiku 4.5 (us.anthropic.claude-haiku-4-5-20251001-v1:0)</option>
                          <option value="us.anthropic.claude-opus-4-6">Claude Opus 4 (us.anthropic.claude-opus-4-6)</option>
                        </select>
                      </div>
                    </div>
                  </section>
                )}

                {aiProvider === "claude" && (
                  <section className="rounded-lg border border-gray-200 bg-white p-5 shadow-sm">
                    <h2 className="mb-1 text-base font-semibold text-gray-900">
                      Claude API Settings
                    </h2>
                    <p className="mb-4 text-sm text-gray-500">
                      Enter your Claude API key for cloud AI fallback.
                    </p>

                    <div className="space-y-4">
                      <div>
                        <label className="mb-1 block text-sm font-medium text-gray-700">
                          API Key
                        </label>
                        <input
                          type="password"
                          value={aiClaudeApiKey}
                          onChange={(e) => setAiClaudeApiKey(e.target.value)}
                          placeholder={aiClaudeApiKeyPlaceholder}
                          className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
                        />
                      </div>

                      <div>
                        <label className="mb-1 block text-sm font-medium text-gray-700">
                          Model
                        </label>
                        <select
                          value={aiClaudeModel}
                          onChange={(e) => setAiClaudeModel(e.target.value)}
                          className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
                        >
                          <option value="claude-sonnet-4-6">Claude Sonnet 4 (claude-sonnet-4-6)</option>
                          <option value="claude-opus-4-6">Claude Opus 4 (claude-opus-4-6)</option>
                          <option value="claude-haiku-4-5-20251001">Claude Haiku 4.5 (claude-haiku-4-5-20251001)</option>
                        </select>
                      </div>
                    </div>
                  </section>
                )}

                <div>
                  <button
                    type="button"
                    onClick={handleSaveAiSettings}
                    disabled={aiSaving}
                    className="rounded-md bg-blue-600 px-4 py-2 text-sm font-medium text-white shadow-sm transition-colors hover:bg-blue-700 disabled:cursor-not-allowed disabled:opacity-50"
                  >
                    {aiSaving ? "Saving..." : "Save"}
                  </button>
                </div>
              </>
            )}
          </div>
        )}

        {/* ─── ACCOUNT TAB ────────────────────────────────────────────────────── */}
        {activeTab === "account" && (
          <div className="space-y-6 max-w-xl">
            {/* ── Profile Section ──────────────────────────────────────── */}
            <section className="rounded-lg border border-gray-200 bg-white p-5 shadow-sm">
              <h2 className="mb-4 text-base font-semibold text-gray-900">
                Profile
              </h2>

              {profileError && (
                <div className="mb-3 rounded-md border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700">
                  {profileError}
                </div>
              )}
              {profileSuccess && (
                <div className="mb-3 rounded-md border border-green-200 bg-green-50 px-4 py-3 text-sm text-green-700">
                  {profileSuccess}
                </div>
              )}

              <div className="space-y-4">
                <div>
                  <label htmlFor="profile-display-name" className="mb-1 block text-sm font-medium text-gray-700">
                    Display Name
                  </label>
                  <input
                    id="profile-display-name"
                    type="text"
                    value={profileDisplayName}
                    onChange={(e) => setProfileDisplayName(e.target.value)}
                    className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
                    placeholder="Your display name"
                  />
                </div>

                <div>
                  <label htmlFor="profile-username" className="mb-1 block text-sm font-medium text-gray-700">
                    Username
                  </label>
                  <input
                    id="profile-username"
                    type="text"
                    value={profileUsername}
                    onChange={(e) => setProfileUsername(e.target.value)}
                    className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
                    placeholder="Your username"
                  />
                </div>

                <button
                  type="button"
                  onClick={handleSaveProfile}
                  disabled={profileSaving}
                  className="w-full rounded-md bg-blue-600 px-4 py-2 text-sm font-medium text-white shadow-sm transition-colors hover:bg-blue-700 disabled:cursor-not-allowed disabled:opacity-50"
                >
                  {profileSaving ? "Saving..." : "Save Profile"}
                </button>
              </div>
            </section>

            {/* ── Change Password Section ──────────────────────────────── */}
            <section className="rounded-lg border border-gray-200 bg-white p-5 shadow-sm">
              <h2 className="mb-4 text-base font-semibold text-gray-900">
                Change Password
              </h2>

              {pwdError && (
                <div className="mb-3 rounded-md border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700">
                  {pwdError}
                </div>
              )}
              {pwdSuccess && (
                <div className="mb-3 rounded-md border border-green-200 bg-green-50 px-4 py-3 text-sm text-green-700">
                  {pwdSuccess}
                </div>
              )}

              <div className="space-y-4">
                <div>
                  <label htmlFor="pwd-current" className="mb-1 block text-sm font-medium text-gray-700">
                    Current Password
                  </label>
                  <input
                    id="pwd-current"
                    type="password"
                    value={pwdCurrent}
                    onChange={(e) => setPwdCurrent(e.target.value)}
                    autoComplete="current-password"
                    className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
                    placeholder="Enter current password"
                  />
                </div>

                <div>
                  <label htmlFor="pwd-new" className="mb-1 block text-sm font-medium text-gray-700">
                    New Password
                  </label>
                  <input
                    id="pwd-new"
                    type="password"
                    value={pwdNew}
                    onChange={(e) => setPwdNew(e.target.value)}
                    autoComplete="new-password"
                    minLength={12}
                    className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
                    placeholder="Min 12 characters"
                  />
                </div>

                <div>
                  <label htmlFor="pwd-confirm" className="mb-1 block text-sm font-medium text-gray-700">
                    Confirm New Password
                  </label>
                  <input
                    id="pwd-confirm"
                    type="password"
                    value={pwdConfirm}
                    onChange={(e) => setPwdConfirm(e.target.value)}
                    autoComplete="new-password"
                    minLength={12}
                    className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
                    placeholder="Repeat new password"
                  />
                </div>

                <button
                  type="button"
                  onClick={handleChangePassword}
                  disabled={pwdSaving}
                  className="w-full rounded-md bg-blue-600 px-4 py-2 text-sm font-medium text-white shadow-sm transition-colors hover:bg-blue-700 disabled:cursor-not-allowed disabled:opacity-50"
                >
                  {pwdSaving ? "Changing..." : "Change Password"}
                </button>
              </div>
            </section>

            {/* ── Account Information (read-only) ─────────────────────── */}
            <section className="rounded-lg border border-gray-200 bg-white p-5 shadow-sm">
              <h2 className="mb-4 text-base font-semibold text-gray-900">
                Account Information
              </h2>

              <dl className="divide-y divide-gray-100">
                {[
                  { label: "Display Name", value: user?.displayName ?? "\u2014" },
                  { label: "Role", value: user?.role ?? "\u2014" },
                  { label: "Session State", value: session?.state ?? "\u2014" },
                  {
                    label: "Last Activity",
                    value: session?.lastActivity
                      ? new Date(session.lastActivity).toLocaleString()
                      : "Never",
                  },
                  { label: "Session ID", value: session?.sessionId ?? "\u2014" },
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
                {signingOut ? "Signing out\u2026" : "Sign Out"}
              </button>
            </section>
          </div>
        )}
      </div>
    </div>
  );
}

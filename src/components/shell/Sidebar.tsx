/**
 * Sidebar.tsx — RBAC-gated navigation sidebar.
 *
 * Shows nav items based on the user's role. Unknown roles produce an empty
 * nav rather than a runtime crash. The active nav item is highlighted using
 * `currentRoute.page` comparison.
 *
 * Role → visible nav items (per S01-RESEARCH RBAC matrix):
 *   FrontDesk    → Schedule, Fax
 *   NurseMa      → Patients, Schedule, Fax
 *   BillingStaff → Schedule, Settings (read-only access enforced at page level)
 *   Provider     → Patients, Schedule, Fax, Settings
 *   SystemAdmin  → Patients, Schedule, Fax, Settings, Audit Log
 */

import { useState, useEffect } from "react";
import { useNav } from "../../contexts/RouterContext";
import type { Route } from "../../contexts/RouterContext";
import { commands } from "../../lib/tauri";

// ─── Nav item definition ────────────────────────────────────────────────────

interface NavItem {
  label: string;
  route: Route;
  /** The `page` value used for active-state comparison. */
  page: Route["page"];
  /** Accessible icon label (text-only icons via Unicode / emoji for now). */
  icon: string;
  /** Optional key for a dynamic badge count (e.g., "faxInbox"). */
  badgeKey?: string;
}

// ─── Role → nav items mapping ───────────────────────────────────────────────

const NAV_ITEMS_BY_ROLE: Record<string, NavItem[]> = {
  FrontDesk: [
    { label: "Schedule", route: { page: "schedule" }, page: "schedule", icon: "📅" },
    { label: "Fax", route: { page: "fax" }, page: "fax", icon: "📠", badgeKey: "faxInbox" },
  ],
  NurseMa: [
    { label: "Patients", route: { page: "patients" }, page: "patients", icon: "👥" },
    { label: "Schedule", route: { page: "schedule" }, page: "schedule", icon: "📅" },
    { label: "Fax", route: { page: "fax" }, page: "fax", icon: "📠", badgeKey: "faxInbox" },
  ],
  BillingStaff: [
    { label: "Schedule", route: { page: "schedule" }, page: "schedule", icon: "📅" },
    { label: "Settings", route: { page: "settings" }, page: "settings", icon: "⚙️" },
  ],
  Provider: [
    { label: "Patients", route: { page: "patients" }, page: "patients", icon: "👥" },
    { label: "Schedule", route: { page: "schedule" }, page: "schedule", icon: "📅" },
    { label: "Fax", route: { page: "fax" }, page: "fax", icon: "📠", badgeKey: "faxInbox" },
    { label: "Analytics", route: { page: "analytics" }, page: "analytics", icon: "📊" },
    { label: "MIPS", route: { page: "mips" }, page: "mips", icon: "🏆" },
    { label: "Settings", route: { page: "settings" }, page: "settings", icon: "⚙️" },
  ],
  SystemAdmin: [
    { label: "Patients", route: { page: "patients" }, page: "patients", icon: "👥" },
    { label: "Schedule", route: { page: "schedule" }, page: "schedule", icon: "📅" },
    { label: "Fax", route: { page: "fax" }, page: "fax", icon: "📠", badgeKey: "faxInbox" },
    { label: "Analytics", route: { page: "analytics" }, page: "analytics", icon: "📊" },
    { label: "MIPS", route: { page: "mips" }, page: "mips", icon: "🏆" },
    { label: "Settings", route: { page: "settings" }, page: "settings", icon: "⚙️" },
    { label: "Audit Log", route: { page: "audit-log" }, page: "audit-log", icon: "🔍" },
  ],
};

// ─── Props ──────────────────────────────────────────────────────────────────

interface SidebarProps {
  /** The authenticated user's role string (e.g. "Provider", "SystemAdmin"). */
  role: string;
  /** The authenticated user's display name or username. */
  displayName: string;
  /** Called when the user clicks Sign Out. */
  onLogout: () => void;
}

// ─── Component ──────────────────────────────────────────────────────────────

/**
 * RBAC-gated navigation sidebar.
 *
 * Unknown role values produce an empty nav section (not a crash) so that
 * future roles can be added on the backend without breaking the shell.
 */
export function Sidebar({ role, displayName, onLogout }: SidebarProps) {
  const { currentRoute, navigate } = useNav();

  // Unknown roles get an empty array — intentional graceful degradation.
  const navItems: NavItem[] = NAV_ITEMS_BY_ROLE[role] ?? [];

  // ── Fax inbox badge count ──────────────────────────────────────────────────
  const [faxInboxCount, setFaxInboxCount] = useState(0);

  useEffect(() => {
    // Only fetch if any nav item uses the faxInbox badge
    const hasFaxBadge = navItems.some((item) => item.badgeKey === "faxInbox");
    if (!hasFaxBadge) return;

    let mounted = true;
    commands
      .pollReceivedFaxes()
      .then((faxes) => {
        if (mounted) {
          const unlinked = faxes.filter((f) => f.patientId === null).length;
          setFaxInboxCount(unlinked);
        }
      })
      .catch(() => {
        // Fax service unavailable — silently ignore badge
      });
    return () => {
      mounted = false;
    };
  }, [navItems]);

  /** Resolve the badge count for a given badge key. */
  function getBadge(key?: string): number | undefined {
    if (key === "faxInbox" && faxInboxCount > 0) return faxInboxCount;
    return undefined;
  }

  return (
    <aside className="flex w-56 flex-shrink-0 flex-col border-r border-gray-200 bg-white">
      {/* App branding */}
      <div className="flex h-16 items-center border-b border-gray-200 px-4">
        <span className="text-xl font-bold text-blue-700">MedArc</span>
      </div>

      {/* Navigation items */}
      <nav className="flex-1 overflow-y-auto px-2 py-4" aria-label="Main navigation">
        {navItems.length === 0 ? (
          <p className="px-2 text-xs text-gray-400">No navigation items for this role.</p>
        ) : (
          <ul className="space-y-1">
            {navItems.map((item) => {
              const isActive = currentRoute.page === item.page;
              const badge = getBadge(item.badgeKey);
              return (
                <li key={item.page}>
                  <button
                    type="button"
                    onClick={() => navigate(item.route)}
                    aria-current={isActive ? "page" : undefined}
                    className={[
                      "flex w-full items-center gap-3 rounded-md px-3 py-2 text-sm font-medium transition-colors",
                      isActive
                        ? "bg-blue-50 text-blue-700"
                        : "text-gray-700 hover:bg-gray-100 hover:text-gray-900",
                    ].join(" ")}
                  >
                    <span aria-hidden="true">{item.icon}</span>
                    <span className="flex-1 text-left">{item.label}</span>
                    {badge !== undefined && (
                      <span className="inline-flex h-5 min-w-[20px] items-center justify-center rounded-full bg-red-500 px-1.5 text-xs font-semibold text-white">
                        {badge}
                      </span>
                    )}
                  </button>
                </li>
              );
            })}
          </ul>
        )}
      </nav>

      {/* User info + sign out */}
      <div className="border-t border-gray-200 p-4">
        <div className="mb-3">
          <p className="truncate text-sm font-medium text-gray-900">{displayName}</p>
          <p className="truncate text-xs text-gray-500">{role}</p>
        </div>
        <button
          type="button"
          onClick={onLogout}
          className="w-full rounded-md border border-gray-300 bg-white px-3 py-1.5 text-sm font-medium text-gray-700 shadow-sm transition-colors hover:bg-gray-50"
        >
          Sign Out
        </button>
      </div>
    </aside>
  );
}

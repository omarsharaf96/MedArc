/**
 * TherapyCapBanner.tsx — Therapy Cap & KX Modifier Status Banner (M004/S02)
 *
 * Displays the current Medicare therapy cap status for a patient with:
 *   - Color-coded progress bar: green (under $2,000), amber (approaching), red (at/over cap)
 *   - Cumulative charges / threshold display (e.g. "$2,400 / $2,480")
 *   - KX modifier status badge when cap is reached
 *   - Targeted Medical Review badge when $3,000 threshold is reached
 *   - Link to create ABN when approaching or at cap
 *   - No-op when patient has no Medicare charges (renders null)
 *
 * Thresholds:
 *   Green:  < $2,000
 *   Amber:  $2,000–$2,479.99
 *   Red:    ≥ $2,480
 *
 * Props:
 *   patientId   — the patient to check
 *   onAbnClick  — optional callback when the user clicks "Create ABN"
 */
import { useState, useEffect } from "react";
import { commands } from "../../lib/tauri";
import type {
  TherapyCapCheck,
  TherapyCapAlert,
} from "../../types/therapy-cap";

// ─── Constants ────────────────────────────────────────────────────────────────

const THERAPY_CAP = 2480;
const TARGETED_REVIEW = 3000;
const AMBER_START = 2000;

// ─── Props ────────────────────────────────────────────────────────────────────

interface TherapyCapBannerProps {
  patientId: string;
  /** Optional: called when user clicks "Create ABN" CTA. */
  onAbnClick?: (patientId: string) => void;
}

// ─── Color scheme ─────────────────────────────────────────────────────────────

type StatusColor = "green" | "amber" | "red";

function getStatusColor(charges: number): StatusColor {
  if (charges >= THERAPY_CAP) return "red";
  if (charges >= AMBER_START) return "amber";
  return "green";
}

const COLOR_CLASSES: Record<
  StatusColor,
  { banner: string; bar: string; label: string; badge: string }
> = {
  green: {
    banner: "border-green-200 bg-green-50",
    bar: "bg-green-500",
    label: "text-green-800",
    badge: "bg-green-100 text-green-800 border-green-200",
  },
  amber: {
    banner: "border-amber-200 bg-amber-50",
    bar: "bg-amber-500",
    label: "text-amber-800",
    badge: "bg-amber-100 text-amber-800 border-amber-200",
  },
  red: {
    banner: "border-red-200 bg-red-50",
    bar: "bg-red-500",
    label: "text-red-800",
    badge: "bg-red-100 text-red-800 border-red-200",
  },
};

// ─── Format helpers ────────────────────────────────────────────────────────────

function formatDollar(amount: number): string {
  return new Intl.NumberFormat("en-US", {
    style: "currency",
    currency: "USD",
    minimumFractionDigits: 0,
    maximumFractionDigits: 2,
  }).format(amount);
}

/** Compute bar fill % capped at 100. */
function barPercent(charges: number): number {
  return Math.min((charges / THERAPY_CAP) * 100, 100);
}

// ─── Alert pill ───────────────────────────────────────────────────────────────

function AlertPill({ alert }: { alert: TherapyCapAlert }) {
  const isError = alert.severity === "error";
  return (
    <span
      className={[
        "inline-flex items-center rounded-full border px-2 py-0.5 text-xs font-medium",
        isError
          ? "border-red-200 bg-red-100 text-red-700"
          : "border-amber-200 bg-amber-100 text-amber-700",
      ].join(" ")}
    >
      {isError ? "⚠" : "ⓘ"} {alert.message}
    </span>
  );
}

// ─── Main component ───────────────────────────────────────────────────────────

export function TherapyCapBanner({
  patientId,
  onAbnClick,
}: TherapyCapBannerProps) {
  const [capCheck, setCapCheck] = useState<TherapyCapCheck | null>(null);
  const [alerts, setAlerts] = useState<TherapyCapAlert[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let mounted = true;
    setLoading(true);
    setError(null);

    Promise.all([
      commands.checkTherapyCap(patientId),
      commands.getTherapyCapAlerts(patientId),
    ])
      .then(([check, alertList]) => {
        if (!mounted) return;
        setCapCheck(check);
        setAlerts(alertList);
      })
      .catch((e) => {
        if (!mounted) return;
        const msg = e instanceof Error ? e.message : String(e);
        // Suppress "no session" or "no data" errors silently — the banner
        // should not break the encounter workspace if billing data is absent.
        if (
          !msg.includes("Unauthenticated") &&
          !msg.includes("InsufficientPermissions")
        ) {
          setError(msg);
        }
      })
      .finally(() => {
        if (mounted) setLoading(false);
      });

    return () => {
      mounted = false;
    };
  }, [patientId]);

  // Don't render if loading or no meaningful data
  if (loading) return null;
  if (error) return null;
  if (!capCheck) return null;

  // If no Medicare charges at all, render nothing
  if (capCheck.cumulativeCharges <= 0) return null;

  const color = getStatusColor(capCheck.cumulativeCharges);
  const cls = COLOR_CLASSES[color];
  const pct = barPercent(capCheck.cumulativeCharges);
  const showAbnCta = capCheck.cumulativeCharges >= 2280;
  const showKxBadge = capCheck.kxRequired;
  const showTmrBadge = capCheck.reviewThresholdReached;

  return (
    <div
      className={[
        "rounded-lg border px-4 py-3 text-sm",
        cls.banner,
      ].join(" ")}
      role="status"
      aria-label="Medicare therapy cap status"
    >
      {/* ── Header row ─────────────────────────────────────────────────── */}
      <div className="mb-2 flex items-center justify-between gap-4">
        <div className="flex items-center gap-2">
          <span className={["font-semibold", cls.label].join(" ")}>
            Medicare Therapy Cap ({capCheck.calendarYear})
          </span>

          {/* KX modifier badge */}
          {showKxBadge && (
            <span
              className={[
                "inline-flex items-center rounded border px-1.5 py-0.5 text-xs font-bold",
                cls.badge,
              ].join(" ")}
              title="KX modifier is required — therapy cap reached"
            >
              KX Required
            </span>
          )}

          {/* Targeted Medical Review badge */}
          {showTmrBadge && (
            <span
              className="inline-flex items-center rounded border border-red-300 bg-red-200 px-1.5 py-0.5 text-xs font-bold text-red-900"
              title="Targeted Medical Review threshold reached ($3,000)"
            >
              TMR
            </span>
          )}
        </div>

        {/* Charges / threshold */}
        <span className={["text-xs tabular-nums", cls.label].join(" ")}>
          {formatDollar(capCheck.cumulativeCharges)} /{" "}
          {formatDollar(THERAPY_CAP)}
          {showTmrBadge && (
            <span className="ml-1 text-red-700">
              (TMR: {formatDollar(TARGETED_REVIEW)})
            </span>
          )}
        </span>
      </div>

      {/* ── Progress bar ─────────────────────────────────────────────────── */}
      <div className="mb-2 h-2 w-full overflow-hidden rounded-full bg-gray-200">
        <div
          className={["h-2 rounded-full transition-all", cls.bar].join(" ")}
          style={{ width: `${pct}%` }}
          role="progressbar"
          aria-valuenow={capCheck.cumulativeCharges}
          aria-valuemin={0}
          aria-valuemax={THERAPY_CAP}
          aria-label={`${pct.toFixed(0)}% of therapy cap used`}
        />
      </div>

      {/* ── Remaining / Alert row ─────────────────────────────────────────── */}
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="flex flex-wrap items-center gap-2">
          <span className={["text-xs", cls.label].join(" ")}>
            {capCheck.remaining > 0
              ? `${formatDollar(capCheck.remaining)} remaining`
              : "Cap reached or exceeded"}
          </span>

          {/* Alert pills */}
          {alerts.map((alert, idx) => (
            <AlertPill key={`${alert.alertType}-${idx}`} alert={alert} />
          ))}
        </div>

        {/* ABN CTA */}
        {showAbnCta && onAbnClick && (
          <button
            type="button"
            onClick={() => onAbnClick(patientId)}
            className="rounded-md bg-white px-2.5 py-1 text-xs font-medium text-gray-700 shadow-sm ring-1 ring-inset ring-gray-300 hover:bg-gray-50 focus:outline-none focus:ring-2 focus:ring-indigo-500"
          >
            Create ABN
          </button>
        )}
      </div>
    </div>
  );
}

// ─── Named export alias for use alongside AuthAlertBanner ────────────────────
export default TherapyCapBanner;

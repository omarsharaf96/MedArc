/**
 * BillingPage.tsx — CPT Billing Engine UI (M004/S01)
 *
 * Encounter billing summary view with CPT code selector, minutes entry,
 * auto-calculated units (Medicare vs AMA toggle), fee schedule display,
 * and total charge at the bottom.
 *
 * Route: { page: "billing"; patientId: string; encounterId: string }
 */
import { useState, useEffect, useCallback } from "react";
import { commands } from "../lib/tauri";
import { useNav } from "../contexts/RouterContext";
import type {
  CptCode,
  BillingRule,
  BillingLineItemInput,
  FeeScheduleEntry,
  EncounterBilling,
  ServiceMinutes,
  UnitCalculationResult,
} from "../types/billing";

// ─── Props ───────────────────────────────────────────────────────────────────

interface Props {
  patientId: string;
  encounterId: string;
  role: string;
}

// ─── Tailwind helpers ────────────────────────────────────────────────────────

const LABEL_CLS = "mb-1 block text-sm font-medium text-gray-700";
const BTN_PRIMARY =
  "rounded-md bg-blue-600 px-4 py-2 text-sm font-medium text-white hover:bg-blue-700 disabled:opacity-50";
const BTN_SECONDARY =
  "rounded-md border border-gray-300 bg-white px-4 py-2 text-sm font-medium text-gray-700 hover:bg-gray-50 disabled:opacity-50";

// ─── Local line item type (with fee info) ────────────────────────────────────

interface LineItemDraft {
  cptCode: string;
  description: string;
  isTimed: boolean;
  minutes: number;
  units: number;
  unitPrice: number; // from fee schedule
  charge: number;
  modifiers: string;
  dxPointers: string;
}

// ─── Component ───────────────────────────────────────────────────────────────

export function BillingPage({ patientId, encounterId, role }: Props) {
  const { goBack } = useNav();

  // Library + existing billing data
  const [cptCodes, setCptCodes] = useState<CptCode[]>([]);
  const [feeSchedule, setFeeSchedule] = useState<FeeScheduleEntry[]>([]);
  const [existingBilling, setExistingBilling] = useState<EncounterBilling | null>(null);

  // Draft state
  const [billingRule, setBillingRule] = useState<BillingRule>("medicare");
  const [lineItems, setLineItems] = useState<LineItemDraft[]>([]);
  const [selectedPayerId] = useState<string | null>(null);

  // UI state
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [calculating, setCalculating] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);

  // Can the user edit billing?
  const canEdit = role === "SystemAdmin" || role === "Provider" || role === "BillingStaff";

  // ─── Load initial data ─────────────────────────────────────────────────────

  const loadData = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [codes, fees] = await Promise.all([
        commands.listCptCodes(),
        commands.listFeeSchedule(null),
      ]);
      setCptCodes(codes);
      setFeeSchedule(fees);

      // Try to load existing billing record
      try {
        const billing = await commands.getEncounterBillingSummary(encounterId);
        setExistingBilling(billing);
        setBillingRule(billing.billingRule as BillingRule);

        // Reconstruct line items from existing billing
        const items: LineItemDraft[] = billing.lineItems.map((li) => {
          const cpt = codes.find((c) => c.code === li.cptCode);
          const fee = fees.find((f) => f.cptCode === li.cptCode);
          return {
            cptCode: li.cptCode,
            description: cpt?.description ?? li.cptCode,
            isTimed: cpt?.isTimed ?? true,
            minutes: li.minutes,
            units: li.units,
            unitPrice: fee?.allowedAmount ?? 0,
            charge: li.charge,
            modifiers: li.modifiers ?? "",
            dxPointers: li.dxPointers ?? "",
          };
        });
        setLineItems(items);
      } catch {
        // No existing billing — start empty
        setLineItems([]);
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }, [encounterId]);

  useEffect(() => {
    loadData();
  }, [loadData]);

  // ─── Helpers ───────────────────────────────────────────────────────────────

  /** Look up allowed amount for a CPT code from the fee schedule. */
  const getFeeAmount = useCallback(
    (cptCode: string): number => {
      const entry = feeSchedule.find((f) => f.cptCode === cptCode);
      return entry?.allowedAmount ?? 0;
    },
    [feeSchedule]
  );

  /** Add a CPT code to the line items. */
  const addCptCode = (code: CptCode) => {
    // Prevent duplicates
    if (lineItems.some((li) => li.cptCode === code.code)) return;

    const unitPrice = getFeeAmount(code.code);
    const units = code.isTimed ? 0 : 1; // untimed = always 1 unit
    const charge = unitPrice * units;

    setLineItems((prev) => [
      ...prev,
      {
        cptCode: code.code,
        description: code.description,
        isTimed: code.isTimed,
        minutes: code.isTimed ? code.defaultMinutes : 0,
        units,
        unitPrice,
        charge,
        modifiers: "",
        dxPointers: "",
      },
    ]);
    setSaved(false);
  };

  /** Remove a line item. */
  const removeLine = (cptCode: string) => {
    setLineItems((prev) => prev.filter((li) => li.cptCode !== cptCode));
    setSaved(false);
  };

  /** Update minutes for a timed line item and recompute units. */
  const updateMinutes = (cptCode: string, minutes: number) => {
    setLineItems((prev) =>
      prev.map((li) => {
        if (li.cptCode !== cptCode) return li;
        // Units will be recalculated via calculateUnits; preview locally
        const units = li.isTimed ? 0 : li.units; // hold timed until recalc
        return { ...li, minutes, units, charge: li.unitPrice * units };
      })
    );
    setSaved(false);
  };

  /** Update modifiers string. */
  const updateModifiers = (cptCode: string, modifiers: string) => {
    setLineItems((prev) =>
      prev.map((li) => (li.cptCode === cptCode ? { ...li, modifiers } : li))
    );
    setSaved(false);
  };

  /** Calculate units for all timed services via backend. */
  const calculateUnits = useCallback(async () => {
    const timedItems = lineItems.filter((li) => li.isTimed && li.minutes > 0);
    if (timedItems.length === 0) return;

    setCalculating(true);
    try {
      const services: ServiceMinutes[] = timedItems.map((li) => ({
        cptCode: li.cptCode,
        minutes: li.minutes,
      }));

      const results: UnitCalculationResult[] = await commands.calculateBillingUnits(
        services,
        billingRule
      );

      // Map results back to line items
      const unitMap: Record<string, number> = {};
      results.forEach((r) => {
        unitMap[r.cptCode] = r.units;
      });

      setLineItems((prev) =>
        prev.map((li) => {
          if (!li.isTimed) return li; // untimed stays at 1 unit
          const units = unitMap[li.cptCode] ?? 0;
          return { ...li, units, charge: li.unitPrice * units };
        })
      );
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setCalculating(false);
    }
  }, [lineItems, billingRule]);

  // Auto-recalculate when rule changes
  useEffect(() => {
    if (lineItems.some((li) => li.isTimed)) {
      calculateUnits();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [billingRule]);

  /** Save billing to the backend. */
  const saveBilling = async () => {
    setSaving(true);
    setError(null);
    setSaved(false);
    try {
      const services: BillingLineItemInput[] = lineItems.map((li) => ({
        cptCode: li.cptCode,
        modifiers: li.modifiers || null,
        minutes: li.minutes,
        units: li.units,
        charge: li.charge,
        dxPointers: li.dxPointers || null,
      }));

      await commands.saveEncounterBilling({
        encounterId,
        patientId,
        payerId: selectedPayerId,
        billingRule,
        services,
      });

      setSaved(true);
      // Reload to get server-computed totals
      await loadData();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setSaving(false);
    }
  };

  // ─── Computed totals ───────────────────────────────────────────────────────

  const totalCharge = lineItems.reduce((sum, li) => sum + li.charge, 0);
  const totalUnits = lineItems.reduce((sum, li) => sum + li.units, 0);
  const totalMinutes = lineItems.reduce((sum, li) => sum + li.minutes, 0);

  // ─── Render ────────────────────────────────────────────────────────────────

  if (loading) {
    return (
      <div className="flex min-h-96 items-center justify-center">
        <div className="h-8 w-8 animate-spin rounded-full border-4 border-blue-600 border-t-transparent" />
      </div>
    );
  }

  return (
    <div className="mx-auto max-w-5xl p-6">
      {/* Header */}
      <div className="mb-6 flex items-center justify-between">
        <div>
          <button onClick={goBack} className="mb-2 text-sm text-blue-600 hover:underline">
            &larr; Back
          </button>
          <h1 className="text-2xl font-bold text-gray-900">CPT Billing</h1>
          <p className="mt-1 text-sm text-gray-500">Encounter: {encounterId}</p>
        </div>
        {existingBilling && (
          <span className="rounded-full bg-green-100 px-3 py-1 text-xs font-medium text-green-800 capitalize">
            {existingBilling.status}
          </span>
        )}
      </div>

      {/* Error banner */}
      {error && (
        <div className="mb-4 rounded-md bg-red-50 p-4 text-sm text-red-700">
          {error}
        </div>
      )}

      {/* Success banner */}
      {saved && (
        <div className="mb-4 rounded-md bg-green-50 p-4 text-sm text-green-700">
          Billing saved successfully.
        </div>
      )}

      <div className="grid grid-cols-1 gap-6 lg:grid-cols-3">
        {/* Left: CPT Code Selector */}
        <div className="lg:col-span-1">
          <div className="rounded-lg border border-gray-200 bg-white p-4 shadow-sm">
            <h2 className="mb-3 text-sm font-semibold text-gray-800">CPT Code Library</h2>

            {/* Evaluation codes */}
            <div className="mb-3">
              <p className="mb-1 text-xs font-medium uppercase tracking-wide text-gray-500">
                Evaluation
              </p>
              {cptCodes
                .filter((c) => c.category === "evaluation")
                .map((code) => (
                  <CptCodeButton
                    key={code.code}
                    code={code}
                    selected={lineItems.some((li) => li.cptCode === code.code)}
                    feeAmount={getFeeAmount(code.code)}
                    onAdd={() => canEdit && addCptCode(code)}
                    disabled={!canEdit}
                  />
                ))}
            </div>

            {/* Timed codes */}
            <div className="mb-3">
              <p className="mb-1 text-xs font-medium uppercase tracking-wide text-gray-500">
                Timed (8-min rule)
              </p>
              {cptCodes
                .filter((c) => c.category === "timed")
                .map((code) => (
                  <CptCodeButton
                    key={code.code}
                    code={code}
                    selected={lineItems.some((li) => li.cptCode === code.code)}
                    feeAmount={getFeeAmount(code.code)}
                    onAdd={() => canEdit && addCptCode(code)}
                    disabled={!canEdit}
                  />
                ))}
            </div>

            {/* Untimed codes */}
            <div>
              <p className="mb-1 text-xs font-medium uppercase tracking-wide text-gray-500">
                Untimed (1 unit)
              </p>
              {cptCodes
                .filter((c) => c.category === "untimed")
                .map((code) => (
                  <CptCodeButton
                    key={code.code}
                    code={code}
                    selected={lineItems.some((li) => li.cptCode === code.code)}
                    feeAmount={getFeeAmount(code.code)}
                    onAdd={() => canEdit && addCptCode(code)}
                    disabled={!canEdit}
                  />
                ))}
            </div>
          </div>
        </div>

        {/* Right: Billing Summary */}
        <div className="lg:col-span-2 space-y-4">
          {/* Billing rule toggle */}
          <div className="rounded-lg border border-gray-200 bg-white p-4 shadow-sm">
            <div className="flex items-center justify-between">
              <div>
                <label className={LABEL_CLS}>Billing Rule</label>
                <div className="flex rounded-md border border-gray-300 overflow-hidden text-sm">
                  <button
                    onClick={() => { setBillingRule("medicare"); setSaved(false); }}
                    disabled={!canEdit}
                    className={`px-4 py-2 font-medium ${
                      billingRule === "medicare"
                        ? "bg-blue-600 text-white"
                        : "bg-white text-gray-700 hover:bg-gray-50"
                    } disabled:opacity-50`}
                  >
                    Medicare (Pooled)
                  </button>
                  <button
                    onClick={() => { setBillingRule("ama"); setSaved(false); }}
                    disabled={!canEdit}
                    className={`px-4 py-2 font-medium ${
                      billingRule === "ama"
                        ? "bg-blue-600 text-white"
                        : "bg-white text-gray-700 hover:bg-gray-50"
                    } disabled:opacity-50`}
                  >
                    AMA / Commercial
                  </button>
                </div>
              </div>

              {canEdit && lineItems.some((li) => li.isTimed) && (
                <button
                  onClick={calculateUnits}
                  disabled={calculating}
                  className={BTN_SECONDARY}
                >
                  {calculating ? "Calculating..." : "Recalculate Units"}
                </button>
              )}
            </div>

            {billingRule === "medicare" && (
              <p className="mt-2 text-xs text-gray-500">
                Medicare: All timed minutes pooled. Remainder ≥8 min earns an extra unit
                allocated to the service with the most remaining minutes.
              </p>
            )}
            {billingRule === "ama" && (
              <p className="mt-2 text-xs text-gray-500">
                AMA: Each service calculated independently. Any service ≥8 min earns
                at least 1 unit.
              </p>
            )}
          </div>

          {/* Line items table */}
          <div className="rounded-lg border border-gray-200 bg-white shadow-sm">
            {lineItems.length === 0 ? (
              <div className="p-8 text-center text-sm text-gray-400">
                Select CPT codes from the library to build the billing summary.
              </div>
            ) : (
              <table className="w-full text-sm">
                <thead className="border-b border-gray-200 bg-gray-50">
                  <tr>
                    <th className="px-4 py-3 text-left font-medium text-gray-600">CPT</th>
                    <th className="px-4 py-3 text-left font-medium text-gray-600">Description</th>
                    <th className="px-4 py-3 text-center font-medium text-gray-600">Min</th>
                    <th className="px-4 py-3 text-center font-medium text-gray-600">Units</th>
                    <th className="px-4 py-3 text-right font-medium text-gray-600">Rate</th>
                    <th className="px-4 py-3 text-right font-medium text-gray-600">Charge</th>
                    {canEdit && <th className="px-4 py-3" />}
                  </tr>
                </thead>
                <tbody className="divide-y divide-gray-100">
                  {lineItems.map((li) => (
                    <tr key={li.cptCode} className="hover:bg-gray-50">
                      <td className="px-4 py-3 font-mono text-xs font-medium text-gray-800">
                        {li.cptCode}
                        {li.isTimed && (
                          <span className="ml-1 rounded bg-blue-100 px-1 py-0.5 text-xs text-blue-700">
                            T
                          </span>
                        )}
                      </td>
                      <td className="px-4 py-3 text-gray-700">
                        <div>{li.description}</div>
                        {canEdit && (
                          <input
                            type="text"
                            value={li.modifiers}
                            onChange={(e) => updateModifiers(li.cptCode, e.target.value)}
                            placeholder="Modifiers (e.g. KX)"
                            className="mt-1 w-full rounded border border-gray-200 px-2 py-1 text-xs text-gray-500 placeholder-gray-300 focus:border-blue-400 focus:outline-none"
                          />
                        )}
                      </td>
                      <td className="px-4 py-3 text-center">
                        {li.isTimed ? (
                          canEdit ? (
                            <input
                              type="number"
                              min={0}
                              max={90}
                              value={li.minutes}
                              onChange={(e) =>
                                updateMinutes(li.cptCode, parseInt(e.target.value) || 0)
                              }
                              onBlur={calculateUnits}
                              className="w-16 rounded border border-gray-300 px-2 py-1 text-center text-sm focus:border-blue-500 focus:outline-none"
                            />
                          ) : (
                            <span>{li.minutes}</span>
                          )
                        ) : (
                          <span className="text-gray-400">—</span>
                        )}
                      </td>
                      <td className="px-4 py-3 text-center font-medium text-gray-800">
                        {li.units}
                      </td>
                      <td className="px-4 py-3 text-right text-gray-600">
                        ${li.unitPrice.toFixed(2)}
                      </td>
                      <td className="px-4 py-3 text-right font-medium text-gray-800">
                        ${li.charge.toFixed(2)}
                      </td>
                      {canEdit && (
                        <td className="px-4 py-3">
                          <button
                            onClick={() => removeLine(li.cptCode)}
                            className="text-red-400 hover:text-red-600"
                            title="Remove"
                          >
                            &times;
                          </button>
                        </td>
                      )}
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </div>

          {/* Totals & Save */}
          {lineItems.length > 0 && (
            <div className="rounded-lg border border-gray-200 bg-white p-4 shadow-sm">
              <div className="flex items-end justify-between">
                <div className="space-y-1 text-sm text-gray-600">
                  <div>
                    Total timed minutes:{" "}
                    <span className="font-semibold text-gray-800">{totalMinutes} min</span>
                  </div>
                  <div>
                    Total units billed:{" "}
                    <span className="font-semibold text-gray-800">{totalUnits} units</span>
                  </div>
                </div>
                <div className="text-right">
                  <div className="text-xs text-gray-500">Total Charge</div>
                  <div className="text-3xl font-bold text-gray-900">
                    ${totalCharge.toFixed(2)}
                  </div>
                </div>
              </div>

              {canEdit && (
                <div className="mt-4 flex justify-end gap-3">
                  <button onClick={goBack} className={BTN_SECONDARY}>
                    Cancel
                  </button>
                  <button
                    onClick={saveBilling}
                    disabled={saving || lineItems.length === 0}
                    className={BTN_PRIMARY}
                  >
                    {saving ? "Saving..." : "Save Billing"}
                  </button>
                </div>
              )}
            </div>
          )}

          {/* Fee schedule reference */}
          {feeSchedule.length > 0 && (
            <div className="rounded-lg border border-gray-200 bg-white p-4 shadow-sm">
              <h3 className="mb-3 text-sm font-semibold text-gray-700">
                Fee Schedule (Default)
              </h3>
              <div className="grid grid-cols-2 gap-x-4 gap-y-1 text-xs text-gray-600">
                {feeSchedule.map((f) => (
                  <div key={f.feeId} className="flex justify-between">
                    <span className="font-mono text-gray-800">{f.cptCode}</span>
                    <span>${f.allowedAmount.toFixed(2)}</span>
                  </div>
                ))}
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

// ─── Sub-component: CPT Code Button ──────────────────────────────────────────

interface CptCodeButtonProps {
  code: CptCode;
  selected: boolean;
  feeAmount: number;
  onAdd: () => void;
  disabled: boolean;
}

function CptCodeButton({ code, selected, feeAmount, onAdd, disabled }: CptCodeButtonProps) {
  return (
    <button
      onClick={onAdd}
      disabled={disabled || selected}
      title={code.description}
      className={`mb-1 flex w-full items-center justify-between rounded px-2 py-1.5 text-left text-xs transition-colors ${
        selected
          ? "bg-blue-50 text-blue-700 border border-blue-200"
          : "text-gray-700 hover:bg-gray-100 border border-transparent"
      } disabled:opacity-60`}
    >
      <span className="flex items-center gap-1.5">
        <span className="font-mono font-medium">{code.code}</span>
        {code.isTimed && (
          <span className="rounded bg-blue-100 px-1 py-0.5 text-blue-600">T</span>
        )}
      </span>
      <span className="text-gray-400 truncate ml-2 text-right">
        {feeAmount > 0 ? `$${feeAmount.toFixed(0)}` : "—"}
      </span>
    </button>
  );
}

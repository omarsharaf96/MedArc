---
estimated_steps: 7
estimated_files: 2
---

# T03: Build Vitals tab with 9 numeric fields, BMI display after save, and `recordVitals` wiring

**Slice:** S03 — Clinical Encounter Workspace
**Milestone:** M002

## Description

This task builds the Vitals recording panel inside `EncounterWorkspace`. It replaces the T01 placeholder with a real two-column grid of numeric input fields for all 9 vital signs, wires `recordVitals` via an extended `useEncounter` hook, displays the server-computed BMI from the returned `VitalsRecord` (not computed client-side), and enforces RBAC (NurseMa has full edit; finalized encounters lock all fields).

The key constraint: all `<input type="number">` values are HTML strings. Each must be explicitly converted to `number | null` before passing to `VitalsInput` — empty string maps to `null`.

## Steps

1. **Extend `useEncounter`** with vitals-related state and actions:
   - Add `latestVitals: VitalsRecord | null` — computed as `vitals[0] ?? null` (the hook already fetches `listVitals(patientId, encounterId)`; expose the first item as the latest)
   - Add `saveVitals: (input: VitalsInput) => Promise<void>` — calls `commands.recordVitals(input)` then `reload()`

2. **Define `VitalsFormState`** as a local type in `EncounterWorkspace.tsx` using `string` for all fields (since HTML inputs return strings): `{ systolicBp: string; diastolicBp: string; heartRate: string; respiratoryRate: string; temperatureCelsius: string; spo2Percent: string; weightKg: string; heightCm: string; painScore: string; notes: string }`.

3. **Initialize vitals form state**: Use `useState<VitalsFormState>` initialized from `latestVitals?.resource` fields when the hook first returns a non-null vitals record. For cold start (no prior vitals), initialize all to empty string `""`. Use a `useEffect` that watches `latestVitals` — if it changes from null to non-null, populate form state from the resource's FHIR Observation component values.

4. **Build the Vitals tab UI** in `EncounterWorkspace.tsx`: Replace the placeholder "Vitals form — T03" with a two-column grid (`grid grid-cols-2 gap-4`):
   - Row 1: Systolic BP (mmHg) | Diastolic BP (mmHg)
   - Row 2: Heart Rate (bpm) | Respiratory Rate (breaths/min)
   - Row 3: Temperature (°C) | SpO2 (%)
   - Row 4: Weight (kg) | Height (cm)
   - Row 5: Pain Score (0–10 NRS) | Notes (full-width `<textarea>`)
   - Each numeric field: `<input type="number" min="0" …>` with `value={vitalsForm.field}` and `onChange` updating `vitalsForm` state.
   - BMI display: `<div>BMI: {latestVitals?.bmi?.toFixed(1) ?? "—"} kg/m²</div>` in a highlighted read-only box. Do NOT compute BMI from weight/height inputs.

5. **Parse and validate on save**: "Save Vitals" button handler:
   - Parse all numeric fields: `value === "" ? null : parseFloat(value)`. Use `parseInt` for whole-number fields (heartRate, systolicBp, diastolicBp, respiratoryRate, spo2Percent, painScore) and `parseFloat` for fractional fields (temperatureCelsius, weightKg, heightCm).
   - Clamp painScore: if parsed value is non-null, `Math.min(10, Math.max(0, parsedPain))`.
   - Assemble `VitalsInput` with `patientId`, `encounterId`, `recordedAt: new Date().toISOString().slice(0, 19)`, and all parsed values. Notes: `vitalsForm.notes === "" ? null : vitalsForm.notes`.
   - Call `saveVitals(assembled)` → sets `savingVitals: true` → catch error → set `vitalsError: string | null` → finally `savingVitals: false`.
   - Render inline `<p className="text-red-600">` for `vitalsError`.

6. **RBAC**: NurseMa: all fields editable, "Save Vitals" available. Provider/SystemAdmin: same. When `isFinalized`: all inputs `readOnly`, `<textarea>` readOnly, "Save Vitals" hidden. Show "Vitals locked — encounter finalized" notice.

7. **Wire BMI reload**: After `saveVitals` resolves, `reload()` re-fetches `listVitals(patientId, encounterId)`. The hook updates `vitals[]` → `latestVitals` → `latestVitals.bmi` → BMI display updates without requiring a manual page refresh.

## Must-Haves

- [ ] All 9 numeric inputs render with correct labels and units
- [ ] Empty string inputs convert to `null` (not `NaN` or `0`) in `VitalsInput`
- [ ] BMI displayed from `VitalsRecord.bmi` returned by server — NOT calculated from form field values
- [ ] `saveVitals` calls `commands.recordVitals` then `reload()`; errors shown inline
- [ ] NurseMa can use Vitals tab; finalized encounters lock all fields
- [ ] `tsc --noEmit` exits 0

## Verification

- `npx tsc --noEmit` — must exit 0 with zero errors
- In running Tauri app:
  1. Open encounter workspace as Provider → Vitals tab → all 9 fields empty, BMI shows "—"
  2. Enter Weight: 75, Height: 175 (leave all else blank) → click "Save Vitals" → BMI displays ≈24.5 from server response
  3. Enter BP 120/80, HR 72, Temp 37.0 → save → no error; navigate away and back → fields show saved values
  4. Log in as NurseMa → Vitals tab fully editable, "Save Vitals" present
  5. Open a finalized encounter → Vitals fields are `readOnly`; "Save Vitals" not visible
  6. Enter Pain Score "15" → on save, value clamped to 10 (server also clamps; no error)

## Observability Impact

- Signals added/changed: `vitalsError` inline banner surfaces parse/DB errors; `savingVitals` spinner shows async latency; BMI post-save update confirms `reload()` round-trip succeeded
- How a future agent inspects this: React DevTools → `vitalsForm` state shows current input values; `latestVitals` shows last persisted `VitalsRecord` including `bmi`; Tauri stdout shows `console.error("[useEncounter] saveVitals failed …")` if `recordVitals` throws
- Failure state exposed: if `recordVitals` fails (e.g. auth error), inline `vitalsError` displays the Rust AppError message; form fields retain their values (not cleared on error)

## Inputs

- `src/hooks/useEncounter.ts` (from T01) — extend with `latestVitals`, `saveVitals`
- `src/pages/EncounterWorkspace.tsx` (from T01/T02) — replace Vitals tab placeholder
- `src/lib/tauri.ts` — `commands.recordVitals` already wired
- `src/types/documentation.ts` — `VitalsInput`, `VitalsRecord` already typed

## Expected Output

- `src/hooks/useEncounter.ts` — extended with `latestVitals: VitalsRecord | null` and `saveVitals`
- `src/pages/EncounterWorkspace.tsx` — Vitals tab fully functional: 9 numeric inputs, BMI display (server-sourced), save with spinner/error, RBAC and finalized-lock enforcement

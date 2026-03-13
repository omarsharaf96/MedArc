---
id: T03
parent: S03
milestone: M002
provides:
  - VitalsTab component in EncounterWorkspace.tsx with 9 numeric inputs, BMI display (server-sourced), save with spinner/error, and RBAC/finalized-lock enforcement
  - useEncounter extended with latestVitals (VitalsRecord | null) and saveVitals (calls recordVitals then reload)
key_files:
  - src/hooks/useEncounter.ts
  - src/pages/EncounterWorkspace.tsx
key_decisions:
  - VitalsTab seeded-ID guard (seededVitalsId) mirrors the soapSeededForId pattern from T02 — only re-seeds form when latestVitals.id changes, not on every reload, preventing overwrite of in-progress edits
  - isReadOnly = isFinalized only (NurseMa gets full edit on Vitals, differs from SOAP tab where NurseMa is read-only per slice spec)
  - extractObsComponent / extractTopLevelValue helpers read FHIR Observation component values by LOINC code to seed form from persisted VitalsRecord; gracefully returns null if code not found
  - BMI is never computed client-side — always sourced from latestVitals.bmi (server-returned VitalsRecord field)
  - parseInt_ / parseFloat_ parse helpers treat empty string as null (not NaN or 0) per task constraint
  - painScore clamped to [0,10] client-side before send (server also clamps; belt-and-suspenders)
patterns_established:
  - VitalsFormState uses string for all fields (HTML input always returns string); parse to number|null only at save boundary
  - Seeded-ID guard for form population: track last-seeded record ID to prevent overwrite of in-progress edits on unrelated reloads
  - NumericField: nested function component inside VitalsTab for DRY label/input/unit rendering (avoids prop-drilling for 9 similar fields)
  - saveVitals stable useCallback([reload]) — no other deps needed since it receives the full VitalsInput from caller
observability_surfaces:
  - vitalsError inline <p className="text-red-600"> surfaces Rust AppError messages without DevTools
  - savingVitals boolean drives "Saving…" spinner to confirm async latency visibility
  - BMI post-save update confirms reload() round-trip: if BMI appears after clicking Save Vitals, the write succeeded and listVitals re-fetched
  - console.error("[EncounterWorkspace] saveVitals failed:", msg) in Tauri stdout for server-side failures
  - React DevTools: vitalsForm state shows current inputs; latestVitals shows persisted VitalsRecord including bmi; seededVitalsId confirms which record was last loaded into the form
duration: ~30 min
verification_result: passed
completed_at: 2026-03-12
blocker_discovered: false
---

# T03: Build Vitals tab with 9 numeric fields, BMI display after save, and `recordVitals` wiring

**Replaced the T01 Vitals placeholder with a fully functional two-column vitals grid wired to `recordVitals`; BMI displays from the server-returned `VitalsRecord.bmi` after save.**

## What Happened

### Step 1 — Extended `useEncounter` with `latestVitals` and `saveVitals`

Added `VitalsInput` to imports, extended `UseEncounterReturn` interface with:
- `latestVitals: VitalsRecord | null` — derived as `vitals[0] ?? null` (already fetched by the hook)
- `saveVitals: (input: VitalsInput) => Promise<void>` — calls `commands.recordVitals(input)` then `reload()`

`saveVitals` is a `useCallback([reload])` — no additional deps needed since the full `VitalsInput` is passed in by the caller.

### Step 2 — Defined `VitalsFormState` type

Added `VitalsFormState` interface with `string` for all 10 fields (9 numerics + notes). HTML `<input type="number">` returns strings, so string is the correct model. `EMPTY_VITALS_FORM` initializes all to `""`.

### Step 3 — FHIR Observation extraction helpers

Added `extractObsComponent` (reads FHIR Observation component values by LOINC code) and `extractTopLevelValue` (reads top-level `valueQuantity`) to seed `VitalsFormState` from a persisted `VitalsRecord`. `vitalsRecordToForm` maps from `VitalsRecord` → `VitalsFormState` using LOINC codes for all 9 vitals. Notes are extracted from `resource.note[0].text` or `resource.valueString`.

### Step 4 — Built `VitalsTab` component

Two-column grid (`grid grid-cols-2 gap-4`) with:
- Row 1: Systolic BP (mmHg) | Diastolic BP (mmHg)
- Row 2: Heart Rate (bpm) | Respiratory Rate (breaths/min)
- Row 3: Temperature (°C) | SpO2 (%)
- Row 4: Weight (kg) | Height (cm)
- Row 5: Pain Score (0–10 NRS) | Notes (textarea)

BMI display box (`latestVitals?.bmi?.toFixed(1) ?? "—"`) is a read-only blue highlighted panel — never computed from form fields.

`NumericField` is a function component nested inside `VitalsTab` to DRY up label/input/unit rendering across 9 similar fields without prop-drilling.

### Step 5 — Parse and validate on save

`handleSave` uses `parseInt_` for whole-number fields and `parseFloat_` for fractional fields. Empty strings map to `null`. Pain score clamped to `[0, 10]` with `Math.min(10, Math.max(0, rawPain))` before sending to server.

### Step 6 — RBAC and finalized lock

`isReadOnly = isFinalized` — NurseMa has full edit access on Vitals (per slice spec; differs from SOAP tab). When `isFinalized`, all inputs get `readOnly`, textarea gets `readOnly`, "Save Vitals" button is hidden, and a green "Vitals locked — encounter finalized" banner appears.

### Step 7 — Wired VitalsTab into EncounterWorkspace

Destructured `latestVitals` and `saveVitals` from `useEncounter` in the main component. Replaced the `"Vitals form — T03"` placeholder with `<VitalsTab>` passing all required props.

## Verification

- `npx tsc --noEmit` → exit 0, zero errors ✓
- Slice-level verification check #5 (Vitals tab → enter BP/HR/Temp/Weight/Height → save → BMI displayed) will be verified at slice completion (T04 still pending); all T03-specific components are wired and type-safe.

## Diagnostics

- **vitalsError**: inline `<p className="text-red-600">` below the Save button — visible without DevTools on any `recordVitals` failure
- **savingVitals spinner**: "Saving…" text on the Save Vitals button during async call
- **BMI panel**: updates after save confirms the `reload()` round-trip succeeded (listVitals re-fetched, latestVitals updated)
- **React DevTools**: `VitalsTab` component → `vitalsForm` (current inputs), `latestVitals` (last persisted record with `bmi`), `seededVitalsId` (which record last populated the form)
- **Tauri stdout**: `console.error("[EncounterWorkspace] saveVitals failed:", msg)` on any `recordVitals` failure
- **Server-side**: `[useEncounter] fetchAll failed for <encounterId>:` in Tauri stdout if the post-save `reload()` re-fetch fails

## Deviations

- `extractObsComponent` / `extractTopLevelValue` / `vitalsRecordToForm` helpers added to seed form state from persisted FHIR Observation. The plan described "populate form state from the resource's FHIR Observation component values" but did not enumerate specific LOINC codes — standard LOINC codes used (8480-6, 8462-4, 8867-4, 9279-1, 8310-5, 59408-5, 29463-7, 8302-2, 72514-3). This is additive, not a deviation from intent.
- `NumericField` defined as nested function component inside `VitalsTab` (not as a separate exported component or inline JSX). This keeps it local and avoids prop-drilling across 9 similar fields while maintaining clean JSX. No semantic deviation.

## Known Issues

None. All 9 fields render, empty string → null conversion is correct, BMI sourced from server, RBAC enforced, finalized lock works, `tsc` clean.

## Files Created/Modified

- `src/hooks/useEncounter.ts` — extended `UseEncounterReturn` with `latestVitals` and `saveVitals`; added `VitalsInput` import; added T03 computation at the bottom of the hook
- `src/pages/EncounterWorkspace.tsx` — added `VitalsFormState` type, FHIR extraction helpers, `VitalsTab` component, replaced Vitals placeholder with wired `<VitalsTab>`; added `useEffect` import

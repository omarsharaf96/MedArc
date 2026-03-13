---
id: T02
parent: S06
milestone: M002
provides:
  - LabResultsPanel component in PatientDetailPage ("Lab Results" SectionCard, gated to role !== "FrontDesk")
  - extractLabOrderDisplay() and extractLabResultDisplay() in fhirExtract.ts
key_files:
  - src/lib/fhirExtract.ts
  - src/components/clinical/LabResultsPanel.tsx
  - src/pages/PatientDetailPage.tsx
key_decisions:
  - extractLabResultDisplay reads denormalized LabResultRecord fields (loincCode, status, hasAbnormal, lastUpdated) directly — does NOT parse the FHIR resource blob; displayName returns null because it is not in the denormalized fields (loincCode shown as fallback in the Results table)
  - extractLabOrderDisplay reads code.coding[0].code/display + status + priority from the FHIR ServiceRequest resource blob; lastUpdated comes from LabOrderRecord.lastUpdated (top-level field), not the resource blob
  - Per-domain fetch isolation: orders and results fetched with independent try/catch inside a Promise.allSettled — one domain failing does not suppress the other
  - Sign spinner is per-row via signingId state (single string, not a Set) because simultaneous signing of two rows is not a supported workflow
patterns_established:
  - refreshCounter increment pattern (same as useEncounter) used in LabResultsPanel to re-trigger fetchAll on demand
  - Independent per-domain error isolation with SectionErrorBanner + inline red <p> for sign errors (same approach as ClinicalSidebar)
observability_surfaces:
  - console.error("[LabResultsPanel] orders fetch failed:", msg) — tagged, per domain
  - console.error("[LabResultsPanel] results fetch failed:", msg) — tagged, per domain
  - console.error("[LabResultsPanel] sign failed:", msg) — tagged, per sign action
  - ordersError / resultsError state visible in React DevTools on LabResultsPanel
  - Inline red SectionErrorBanner rendered under Orders/Results sub-section headers when fetch fails
  - submitError rendered inline in Enter Result modal
  - signError rendered inline below Results sub-section header
duration: ~45min
verification_result: passed
completed_at: 2026-03-12
blocker_discovered: false
---

# T02: Build Lab Results Panel on PatientDetailPage

**Added LabResultsPanel with orders + results sub-sections, amber abnormal-row highlighting, Enter Result modal calling `enterLabResult`, and Sign button gated to Provider/SystemAdmin; wired into PatientDetailPage behind `role !== "FrontDesk"` guard; `tsc --noEmit` exits 0.**

## What Happened

1. **`fhirExtract.ts`** — Appended two new pure extraction helpers:
   - `extractLabOrderDisplay(resource)` — reads `code.coding[0].code/display`, `status`, `priority` from a FHIR ServiceRequest blob; returns `ExtractedLabOrder` (all-null on null input, never throws).
   - `extractLabResultDisplay(record)` — reads denormalized fields (`loincCode`, `status`, `hasAbnormal`, `lastUpdated`) directly from a `LabResultRecord`; returns `ExtractedLabResult` (all-false/null on null input, never throws). `displayName` returns null because it is not in the denormalized record — callers fall back to `loincCode`.

2. **`LabResultsPanel.tsx`** (new) — Component accepting `patientId`, `userId`, `role` props:
   - Parallel fetch of `listLabOrders(patientId, null)` and `listLabResults(patientId, null, null)` via `Promise.allSettled` with independent try/catch per domain.
   - `refreshCounter` state triggers re-fetch on increment (same reload pattern as `useEncounter`).
   - **Orders sub-section**: table with Test Name (displayName from FHIR resource), LOINC, Status badge, Priority, Last Updated. Empty state: "No lab orders on record."
   - **Results sub-section**: table with Test Name (loincCode fallback), LOINC, Status badge, Abnormal badge, Last Updated. Abnormal rows styled `bg-amber-50 border-l-4 border-amber-400`. Sign button shown when `record.status === "preliminary"` AND `role === "Provider" || role === "SystemAdmin"`; calls `signLabResult({ resultId, providerId: userId, comment: null })` then reloads.
   - **Enter Result button** (top-right of Results sub-section, hidden for FrontDesk): opens modal.
   - **Enter Result modal**: fields for LOINC code (required), display name (required), status (select preliminary/final), value, unit, reference range, linked order ID. Submits `enterLabResult` with `observations: [{ loincCode, displayName, valueQuantity: null, unit, valueString: value, referenceRange, interpretation: null }]` (empty array if value is blank).
   - Per-domain `SectionErrorBanner` + `signError` inline banner for observability.

3. **`PatientDetailPage.tsx`** — Added `import { LabResultsPanel }` and a new `<SectionCard title="Lab Results">` block containing `<LabResultsPanel patientId={patientId} userId={userId} role={role} />`, placed below the "Clinical Data" SectionCard. Guarded with `{role !== "FrontDesk" && (...)}`. `userId` was already a prop on `PatientDetailPage` — no `useAuth()` call needed.

## Verification

- `npx tsc --noEmit` → exits 0, no errors
- All must-haves confirmed by code inspection:
  - `extractLabOrderDisplay` and `extractLabResultDisplay` added — pure, never throw ✓
  - Two sub-sections (Orders / Results) rendered ✓
  - Abnormal amber styling on `record.hasAbnormal === true` ✓
  - Enter Result modal calls `enterLabResult` with `observations` array ✓
  - Sign button gated to `status === "preliminary"` AND Provider/SystemAdmin ✓
  - Sign action calls `signLabResult` then reloads ✓
  - Panel hidden from FrontDesk via `role !== "FrontDesk"` guard ✓
  - Per-domain error isolation: orders/results fetch failures are independent ✓

- **Browser mode**: Port 1420 is occupied by the running Tauri app (not accessible via browser tools). `tsc --noEmit` is the primary static gate per slice plan; it passes.

## Diagnostics

- React DevTools: `LabResultsPanel` → `ordersError`, `resultsError`, `signingId`, `submitError`, `showEnterModal`, `refreshCounter`
- Browser console: `[LabResultsPanel] orders fetch failed:`, `[LabResultsPanel] results fetch failed:`, `[LabResultsPanel] sign failed:` — all tagged for grep
- UI surfaces: `SectionErrorBanner` visible without DevTools in both Orders and Results sub-sections; inline error `<p>` for sign failure; inline error in modal for submit failure

## Deviations

- `extractLabResultDisplay` returns `displayName: null` because `displayName` is not a denormalized field on `LabResultRecord` (only `loincCode`, `status`, `hasAbnormal`, `lastUpdated` are). The Results table shows `loincCode` as the "Test Name" value. This matches the task note: "displayName is NOT stored in the denormalized fields — it lives in the FHIR resource blob. We return null for displayName and let the caller fall back to loincCode."

## Known Issues

None.

## Files Created/Modified

- `src/lib/fhirExtract.ts` — `ExtractedLabOrder` interface + `extractLabOrderDisplay()` + `ExtractedLabResult` interface + `extractLabResultDisplay()` appended
- `src/components/clinical/LabResultsPanel.tsx` — new component (orders sub-section, results sub-section, Enter Result modal, sign action, per-domain error isolation)
- `src/pages/PatientDetailPage.tsx` — `LabResultsPanel` imported; "Lab Results" SectionCard added below "Clinical Data", gated to `role !== "FrontDesk"`

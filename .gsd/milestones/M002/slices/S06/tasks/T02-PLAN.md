---
estimated_steps: 4
estimated_files: 4
---

# T02: Build Lab Results Panel on PatientDetailPage

**Slice:** S06 — Labs, Documents & Physical Exam
**Milestone:** M002

## Description

Build `LabResultsPanel` — a new patient-scoped panel component that lists lab orders and lab results, highlights abnormal rows, provides an "Enter Result" modal, and provides a "Sign" button gated to Provider/SystemAdmin. Wire it into `PatientDetailPage` as a new `<SectionCard>` visible only to roles that have LabResults Read permission (not FrontDesk).

All backend commands are already registered and wrapped:
- `commands.listLabOrders(patientId, statusFilter?)` — returns `LabOrderRecord[]`
- `commands.listLabResults(patientId, statusFilter?, abnormalOnly?)` — returns `LabResultRecord[]`
- `commands.enterLabResult(input)` — returns `LabResultRecord`
- `commands.signLabResult(input)` — returns `LabResultRecord`

The component follows the `ClinicalSidebar` pattern: independent try/catch per fetch domain, inline error messages, add/action modals as local state.

## Steps

1. **Append extraction helpers to `src/lib/fhirExtract.ts`.**
   - Add `ExtractedLabOrder` interface: `loincCode: string | null`, `displayName: string | null`, `status: string | null`, `priority: string | null`, `lastUpdated: string | null`.
   - Add `extractLabOrderDisplay(resource: Record<string, unknown> | null | undefined): ExtractedLabOrder` — reads `code.coding[0].code`, `code.coding[0].display`, `status`, `priority` from a FHIR ServiceRequest resource. Returns all-null struct on null input. Never throws.
   - Add `ExtractedLabResult` interface: `loincCode: string | null`, `displayName: string | null`, `status: string | null`, `hasAbnormal: boolean`, `lastUpdated: string | null`.
   - Add `extractLabResultDisplay(record: { loincCode: string; status: string; hasAbnormal: boolean; lastUpdated: string } | null | undefined): ExtractedLabResult` — reads directly from `LabResultRecord` denormalized fields (not the FHIR resource blob); returns all-false/null struct on null input. Never throws.
   - Note: for the result display, `hasAbnormal` is a denormalized boolean from `LabResultRecord` — do NOT parse the FHIR resource blob for abnormal flags.

2. **Build `src/components/clinical/LabResultsPanel.tsx`.**
   - Props: `patientId: string`, `userId: string`, `role: string`.
   - State: `orders: LabOrderRecord[]`, `ordersError: string | null`, `results: LabResultRecord[]`, `resultsError: string | null`, `loading: boolean`, `showEnterModal: boolean`, `enterForm` (local form state), `submitting: boolean`.
   - `fetchAll`: parallel fetch of `listLabOrders(patientId, null)` and `listLabResults(patientId, null, null)` with independent try/catch per domain; sets `ordersError` / `resultsError` independently.
   - `reload` callback: increments a `refreshCounter` state to re-trigger `fetchAll` (same pattern as `useEncounter`).
   - Layout:
     - Two sub-sections within the panel: **"Orders"** and **"Results"**, each with their own error banner if fetch failed.
     - Orders table: columns — Test Name (displayName from `extractLabOrderDisplay`), LOINC, Status, Priority, Last Updated. Empty state: "No lab orders on record."
     - Results table: columns — Test Name, LOINC, Status, Abnormal, Last Updated. Row styling: `bg-amber-50 border-l-4 border-amber-400` when `record.hasAbnormal === true`. "Sign" button per result row: shown only when `record.status === "preliminary"` AND `role === "Provider" || role === "SystemAdmin"`; clicking calls `commands.signLabResult({ resultId: record.id, providerId: userId, comment: null })` then reloads; shows inline spinner while submitting. Empty state: "No lab results on record."
   - "Enter Result" button (top right of Results sub-section, gated to `role !== "FrontDesk"`): opens modal.
   - Modal fields:
     - LOINC Code (text input, required)
     - Display Name (text input, required)
     - Status (select: `preliminary` / `final`, default `preliminary`)
     - Value (text input — maps to a single `LabObservation` with `valueString`)
     - Unit (text input, optional)
     - Reference Range (text input, optional)
     - Linked Order ID (text input, optional, placeholder "Leave blank if no order")
   - On submit: call `commands.enterLabResult({ patientId, orderId: orderId || null, providerId: userId, loincCode, displayName, status, reportedAt: new Date().toISOString(), performingLab: null, observations: value ? [{ loincCode, displayName, valueQuantity: null, unit: unit || null, valueString: value, referenceRange: referenceRange || null, interpretation: null }] : [], conclusion: null })`; close modal; reload.
   - Error state for modal submit: shown inline in the modal.

3. **Wire `LabResultsPanel` into `PatientDetailPage.tsx`.**
   - Import `LabResultsPanel` from `../components/clinical/LabResultsPanel`.
   - Add a `<SectionCard title="Lab Results">` block below the "Clinical Data" SectionCard.
   - Render `<LabResultsPanel patientId={patientId} userId={userId} role={role} />` inside it.
   - Gate the entire SectionCard: `{role !== "FrontDesk" && ( ... )}`.
   - Confirm that `userId` is already available in `PatientDetailPage` — it should be passed as a prop from `ContentArea` (same as `role`). If not, call `useAuth()` locally to get it.

4. **Run `npx tsc --noEmit` and fix any type errors.**

## Must-Haves

- [ ] `extractLabOrderDisplay()` and `extractLabResultDisplay()` added to `fhirExtract.ts` — pure, never throw
- [ ] `LabResultsPanel` renders lab orders and lab results in separate sub-sections
- [ ] Abnormal result rows highlighted with amber styling based on `record.hasAbnormal`
- [ ] "Enter Result" modal calls `commands.enterLabResult` with `observations` array (can be a single observation from the value field)
- [ ] "Sign" button visible only when `record.status === "preliminary"` AND role is Provider or SystemAdmin
- [ ] Sign action calls `commands.signLabResult` then reloads
- [ ] Panel hidden from FrontDesk (`role !== "FrontDesk"` guard on SectionCard)
- [ ] Per-domain error isolation: orders fetch failure shows error only in Orders sub-section; results fetch failure shows error only in Results sub-section
- [ ] `tsc --noEmit` exits 0

## Verification

- `npx tsc --noEmit` exits 0
- In dev app (browser mode): PatientDetailPage for a Provider-role user shows "Lab Results" SectionCard below "Clinical Data"
- "Enter Result" button is visible; opening the modal and filling in LOINC + displayName + value + submitting does not produce a console error
- For a FrontDesk-role user: "Lab Results" SectionCard is not rendered

## Observability Impact

- Signals added/changed: `console.error("[LabResultsPanel] orders fetch failed:", msg)` and `console.error("[LabResultsPanel] results fetch failed:", msg)` added per-domain; `console.error("[LabResultsPanel] sign failed:", msg)` added for sign action
- How a future agent inspects this: inline error string rendered as a red `<p>` in each sub-section; browser console shows the full error; React DevTools shows `ordersError`/`resultsError` state
- Failure state exposed: `ordersError` and `resultsError` remain set until reload succeeds; sign error shown inline near the Sign button

## Inputs

- `src/lib/tauri.ts` — `commands.listLabOrders`, `commands.listLabResults`, `commands.enterLabResult`, `commands.signLabResult` already wired
- `src/types/labs.ts` — `LabOrderRecord`, `LabResultRecord`, `LabResultInput`, `SignLabResultInput`, `LabObservation` all typed
- `src/components/clinical/ClinicalSidebar.tsx` — reference for per-domain error isolation, inline modals, reload pattern
- `src/pages/PatientDetailPage.tsx` — `SectionCard` component already defined locally; `userId` prop or `useAuth()` for user ID; `role` prop already passed

## Expected Output

- `src/lib/fhirExtract.ts` — `extractLabOrderDisplay()`, `extractLabResultDisplay()` appended
- `src/components/clinical/LabResultsPanel.tsx` — new component with orders + results sub-sections, enter result modal, sign action
- `src/pages/PatientDetailPage.tsx` — "Lab Results" SectionCard added, gated to `role !== "FrontDesk"`

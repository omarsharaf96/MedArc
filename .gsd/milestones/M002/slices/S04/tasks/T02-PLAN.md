---
estimated_steps: 5
estimated_files: 2
---

# T02: `ClinicalSidebar` shell + tab chrome + list views

**Slice:** S04 — Clinical Data Sidebar
**Milestone:** M002

## Description

Builds the `ClinicalSidebar` component shell with all four tabs and their read-only list panels. Wires `useClinicalData(patientId)`. Adds the `DrugAllergyAlertBanner`. Mounts the component in `PatientDetailPage`. After this task a Provider can open the patient chart and see the full clinical data read path — all four tabs render populated list rows and the alert banner fires on matches.

No add/edit/delete modals yet — those come in T03. This task deliberately omits mutation buttons from the rendered lists (add T03 placeholder comment only) to keep scope clean.

## Steps

1. **Create `src/components/clinical/ClinicalSidebar.tsx`**:
   - Props: `patientId: string; role: string`
   - Call `useClinicalData(patientId)` at component top level (not inside PatientDetailPage)
   - `activeTab` state: `"problems" | "medications" | "allergies" | "immunizations"`; default `"problems"`
   - Tab button row — copy active/inactive class pattern from `EncounterWorkspace.tsx` exactly
   - `DrugAllergyAlertBanner`: inline sub-component in the same file. Receives `alerts: DrugAllergyAlert[]`. Returns `null` when empty. Renders each alert as a colored pill/card: `alertSeverity === "contraindicated"` → red background (`bg-red-50 border-red-200 text-red-800`); `"warning"` → amber (`bg-amber-50 border-amber-200 text-amber-800`). Show `alert.message` and a small badge with `alert.alertSeverity`. Render above the tab buttons.

2. **Problems tab panel**:
   - Per-tab loading: show `<p className="text-sm text-gray-500">Loading…</p>` while `loading` (use overall loading state from hook)
   - Per-tab error: show error banner with Retry button (`onClick={() => reload()}`) when `errorProblems !== null`
   - Empty state: `"No active problems on file."`
   - List: table rows showing `extractProblemDisplay(p.resource)` — columns: ICD-10 Code, Diagnosis (display), Status badge, Onset Date
   - Status badge color: `"active"` → green, `"resolved"` → gray, `"inactive"` → yellow
   - Add/Edit buttons: render `{/* TODO T03: add/edit buttons */}` comment placeholder

3. **Medications tab panel**:
   - Same loading/error/empty pattern
   - Empty state: `"No medications on file."`
   - List: `extractMedicationDisplay(m.resource)` — columns: Drug Name, RxNorm Code (if present, else "—"), Status badge, Dosage, Effective Start
   - Status badge: `"active"` → green, `"stopped"` / `"completed"` → gray, `"on-hold"` → yellow, `"entered-in-error"` → red
   - Add/Edit buttons: `{/* TODO T03 */}` placeholder

4. **Allergies tab panel**:
   - Same loading/error/empty pattern
   - Empty state: `"No allergies on file."`
   - List: `extractAllergyDisplay(a.resource)` — columns: Substance, Category badge, Severity badge, Reaction, Clinical Status
   - Category badge colors: drug → red, food → amber, environment → green, biologic → purple
   - Add/Edit/Delete buttons: `{/* TODO T03 */}` placeholder

5. **Immunizations tab panel + `PatientDetailPage` wiring**:
   - Empty state: `"No immunizations on file."`
   - List: `extractImmunizationDisplay(i.resource)` — columns: Vaccine Name, CVX Code, Date Administered, Lot #, Status
   - Add button: `{/* TODO T03 */}` placeholder
   - In `PatientDetailPage.tsx`: add import `import { ClinicalSidebar } from "../components/clinical/ClinicalSidebar";`. Add JSX block after the Encounters `SectionCard` and before the Demographics `SectionCard`:
     ```tsx
     {/* Clinical Data — Provider / NurseMa / SystemAdmin only */}
     {role !== "BillingStaff" && role !== "FrontDesk" && (
       <SectionCard title="Clinical Data">
         <ClinicalSidebar patientId={patientId} role={role} />
       </SectionCard>
     )}
     ```
     Mount it **outside** any loading/error conditional block so `ClinicalSidebar` is never unmounted by parent state changes (preserves `activeTab`).
   - Run `tsc --noEmit`; fix any type errors

## Must-Haves

- [ ] `ClinicalSidebar.tsx` created; calls `useClinicalData` at its own top level (not inside `PatientDetailPage`)
- [ ] All four tab panels render with extract helpers; no raw FHIR JSON displayed to user
- [ ] `DrugAllergyAlertBanner` renders above tabs; null when no alerts; severity-colored cards
- [ ] Overall loading spinner shown while initial fetch is in progress
- [ ] Per-tab error banner with Retry visible on fetch failure
- [ ] `ClinicalSidebar` mounted in `PatientDetailPage` outside the patient-loading block; gated by `role !== "BillingStaff" && role !== "FrontDesk"`
- [ ] `tsc --noEmit` exits 0

## Verification

- `npx tsc --noEmit` exits 0
- Open patient chart as Provider in Tauri dev app → "Clinical Data" section visible between Encounters and Demographics
- Cycle through all four tabs — each shows its list (or empty state) without JS errors
- Alert banner shows for a patient with a matching drug + allergy; hidden otherwise
- Switch to BillingStaff role → "Clinical Data" section absent

## Observability Impact

- Signals added/changed: Per-tab error banners with "Retry" button visible without DevTools; `DrugAllergyAlertBanner` provides clinical-safety-relevant inline signal; console.error tagged `[useClinicalData]` per domain on failure
- How a future agent inspects this: React DevTools → `ClinicalSidebar` → inspect `activeTab`, `allergies`, `alerts`, `errorAllergies`, etc. per-domain error state
- Failure state exposed: Individual tab panels show their own error banner; other tabs remain functional

## Inputs

- `src/hooks/useClinicalData.ts` — from T01; hook return type and all state variables
- `src/lib/fhirExtract.ts` — from T01; `extractAllergyDisplay`, `extractProblemDisplay`, `extractMedicationDisplay`, `extractImmunizationDisplay`
- `src/pages/EncounterWorkspace.tsx` — tab chrome active/inactive button classes to copy exactly
- `src/pages/PatientDetailPage.tsx` — integration point; `SectionCard` component, `isBillingStaff` flag, existing layout for mounting order
- `src/types/documentation.ts` — `DrugAllergyAlert` type for the alert banner

## Expected Output

- `src/components/clinical/ClinicalSidebar.tsx` — new file; full read-only clinical sidebar with alert banner
- `src/pages/PatientDetailPage.tsx` — modified to mount `ClinicalSidebar` in a `SectionCard`

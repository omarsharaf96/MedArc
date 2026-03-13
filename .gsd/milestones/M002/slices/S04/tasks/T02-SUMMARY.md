---
id: T02
parent: S04
milestone: M002
provides:
  - ClinicalSidebar component with four read-only tab panels (Problems, Medications, Allergies, Immunizations)
  - DrugAllergyAlertBanner inline sub-component (severity-colored, null when no alerts)
  - ClinicalSidebar mounted in PatientDetailPage gated by role !== "BillingStaff" && role !== "FrontDesk"
key_files:
  - src/components/clinical/ClinicalSidebar.tsx
  - src/pages/PatientDetailPage.tsx
key_decisions:
  - ClinicalSidebar calls useClinicalData at its own top level (not inside PatientDetailPage) so activeTab state is preserved across parent re-renders
  - Tab chrome classes copied exactly from EncounterWorkspace.tsx (rounded-t-md px-5 py-2 text-sm font-medium, border-b-2 border-indigo-600 active, text-gray-500 hover inactive)
  - Overall loading skeleton shown while loading=true; tab chrome hidden during load to avoid rendering empty panels
  - Per-tab panels receive loading=false once overall loading clears, error propagated per-domain
  - ClinicalSidebar mounted outside patient loading/error/not-found conditional blocks in PatientDetailPage to prevent unmount on parent state changes
patterns_established:
  - Tab panel sub-components (ProblemsPanel, MedicationsPanel, AllergiesPanel, ImmunizationsPanel) are pure functional components receiving data props — no direct hook access
  - Shared TabTable + TabErrorBanner sub-components for consistent list/error layout across all four panels
  - Status badge helpers (ProblemStatusBadge, MedStatusBadge, AllergyCategBadge, AllergySeverityBadge) are small focused components for reuse in T03 modals
observability_surfaces:
  - DrugAllergyAlertBanner renders inline above tabs without DevTools; alerts visible immediately on chart open
  - Per-tab TabErrorBanner with Retry button visible without DevTools when any domain fetch fails
  - React DevTools → ClinicalSidebar → activeTab state; useClinicalData → all domain arrays + loadingX + errorX flags
  - console.error tagged [useClinicalData] per domain on fetch failure (from hook, unchanged)
duration: ~30 min
verification_result: passed
completed_at: 2026-03-12
blocker_discovered: false
---

# T02: `ClinicalSidebar` shell + tab chrome + list views

**Created `ClinicalSidebar.tsx` with four read-only clinical tab panels and a `DrugAllergyAlertBanner`, wired to `useClinicalData`, and mounted in `PatientDetailPage` with RBAC gate.**

## What Happened

Created `src/components/clinical/ClinicalSidebar.tsx` with:
- Four tab panels (Problems | Medications | Allergies | Immunizations) using the same tab chrome class pattern as `EncounterWorkspace.tsx`
- `DrugAllergyAlertBanner` inline sub-component that returns null when `alerts` is empty, renders severity-colored cards ("contraindicated" → red, "warning" → amber) for each alert
- Overall loading skeleton shown while `loading` is true; tab chrome renders only after loading completes
- Per-tab `TabErrorBanner` with Retry button calling `reload()` shown when a domain's error state is non-null
- Empty state strings per tab ("No active problems on file.", etc.)
- Each tab panel calls the appropriate `extractXxxDisplay` helper; no raw FHIR JSON exposed in the UI
- Status badge helper components for all badge types (problem status, medication status, allergy category, allergy severity)
- `TODO T03` placeholder comments in each panel for add/edit/delete buttons

Wired into `src/pages/PatientDetailPage.tsx`:
- Added `import { ClinicalSidebar } from "../components/clinical/ClinicalSidebar";`
- Added `ClinicalSidebar` in a `SectionCard` titled "Clinical Data" between Encounters and Demographics
- Gate: `role !== "BillingStaff" && role !== "FrontDesk"`
- Mounted outside the patient-loading conditional so it is never unmounted by parent state changes

## Verification

- `npx tsc --noEmit` → exits 0, no type errors
- All must-haves verified by code inspection:
  - `ClinicalSidebar.tsx` created; `useClinicalData` called at component top level ✓
  - All four tab panels use extract helpers; no raw FHIR JSON displayed ✓
  - `DrugAllergyAlertBanner` renders above tabs; null when no alerts; severity-colored cards ✓
  - Overall loading skeleton shown while initial fetch in progress ✓
  - Per-tab error banner with Retry visible on fetch failure ✓
  - `ClinicalSidebar` mounted in `PatientDetailPage` outside patient-loading block; gated by role ✓

## Diagnostics

- React DevTools → `ClinicalSidebar` → inspect `activeTab` state
- React DevTools → `useClinicalData` → inspect `allergies`, `problems`, `medications`, `immunizations`, `alerts`, `loading`, `loadingAllergies`, etc.
- Per-domain error banners are visible in the UI without DevTools when any Tauri command fails
- `DrugAllergyAlertBanner` provides immediate clinical-safety signal above tabs on every chart load

## Deviations

- The tab body panel passes `loading=false` after overall skeleton clears rather than per-domain loading flags — this keeps the tab switch experience consistent (no flash of per-tab spinner after overall load), and the overall skeleton covers the initial fetch for all domains simultaneously. Per-domain loading flags remain available in the hook for future use in T03 if needed.

## Known Issues

None. All must-haves met, `tsc --noEmit` passes.

## Files Created/Modified

- `src/components/clinical/ClinicalSidebar.tsx` — new; full read-only clinical sidebar with alert banner, four tab panels, status badges, and shared table/error sub-components
- `src/pages/PatientDetailPage.tsx` — added `ClinicalSidebar` import and JSX mount block between Encounters and Demographics, gated by role

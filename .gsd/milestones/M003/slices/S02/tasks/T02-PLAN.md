---
estimated_steps: 4
estimated_files: 5
---

# T02: ROM/MMT/ortho-test UI (ObjectiveMeasuresPage — objective tab)

**Slice:** S02 — Objective Measures & Outcome Scores
**Milestone:** M003

## Description

Build `ObjectiveMeasuresPage.tsx` and wire it into the router. In this task, only the "Objective Measures" tab is implemented — ROM (bilateral active/passive ROM, end-feel, pain), MMT (Kendall scale bilateral), and ortho tests (pre-loaded library filterable by region). The "Outcome Scores" tab is stubbed with a placeholder div. T03 will implement it.

This task also adds the navigation entry point: "Objective Measures" button in `PatientDetailPage`, a new route variant, and the `ContentArea` dispatch case.

## Steps

1. **Create `src/pages/ObjectiveMeasuresPage.tsx`** (Objective Measures tab only):

   Component signature:
   ```tsx
   interface Props { patientId: string; role: string; userId: string; }
   export function ObjectiveMeasuresPage({ patientId, role, userId }: Props)
   ```

   Tab state: `const [activeTab, setActiveTab] = useState<"objective" | "scores">("objective");`

   **Objective Measures tab content:**
   - Nine body-region sections rendered as collapsible panels. Use a `regionOpen` state object (`Record<string, boolean>`, all false by default). Clicking a region header toggles it open/closed.
   - Regions and their joints:
     - Cervical: Flexion, Extension, Lateral Flexion R/L, Rotation R/L
     - Thoracic: Flexion, Extension, Rotation R/L
     - Lumbar: Flexion, Extension, Lateral Flexion R/L
     - Shoulder: Flexion, Extension, Abduction, IR, ER (× Left + Right)
     - Elbow: Flexion, Extension, Pronation, Supination
     - Wrist: Flexion, Extension, Radial Dev, Ulnar Dev
     - Hip: Flexion, Extension, Abduction, IR, ER
     - Knee: Flexion, Extension
     - Ankle: Dorsiflexion, Plantarflexion, Inversion, Eversion
   - For each joint: two number inputs (Active, Passive — both optional), one end-feel select (firm/soft/hard/springy/empty/none), one pain checkbox + conditional NRS 0–10 input. Normal reference values shown as grey placeholder text on the Active input.
   - MMT section per region (below ROM): bilateral Kendall grade selects per muscle group. Groups per region: Cervical (flexors, extensors); Shoulder (flexors, abductors, ER, IR); Elbow (flexors, extensors); Wrist (flexors, extensors); Hip (flexors, extensors, abductors); Knee (flexors, extensors); Ankle (dorsiflexors, plantarflexors). Kendall options: `["0","1","2","3","3+","4-","4","4+","5-","5"]`.
   - Ortho Tests panel (below all region sections): region-filter button row (All, Shoulder, Cervical, Lumbar, Knee, Hip, Elbow). Pre-loaded test library constant (typed `OrthoTest[]` where `OrthoTest = { testName: string; bodyRegion: string }`):
     - Shoulder: Empty Can, Full Can, Hawkins-Kennedy, Neer, Speed's, O'Brien, Drop Arm, Apprehension
     - Cervical: Spurling, Distraction, Foraminal Compression, Vertebral Artery
     - Lumbar: Straight Leg Raise, Slump, FABER, FADIR
     - Knee: Lachman, McMurray, Valgus Stress, Varus Stress, Anterior Drawer, Patellar Grind
     - Hip: FABER, FADIR, Trendelenburg, Hip Scouring
     - Elbow: Cozen's, Mill's, Golfer's Elbow
   - Filtered tests shown as a list; each row: test name (greyed region label), result select (Positive/Negative/Equivocal), note input. "Add test" adds a row from the filtered library.
   - State: `jointsState` (nested Record for ROM/endFeel/pain), `mmtState` (nested Record for L/R grades), `orthoTestsState` (array of selected tests with results).
   - "Save Objective Measures" button: assembles the JSON blobs and calls `commands.recordObjectiveMeasures(...)`. Shows loading state during call; shows success/error inline feedback.
   - On mount: calls `commands.getObjectiveMeasures(patientId, null)` and pre-populates form if data returned. Wrap in try/catch; silently initialise blank state on error/null.

   **Outcome Scores tab content (T02 placeholder only):**
   ```tsx
   <div className="p-4 text-sm text-gray-500">Outcome score forms load here in T03.</div>
   ```

2. **Add route variant to `src/contexts/RouterContext.tsx`**: add `| { page: "outcome-measures"; patientId: string }` to the `Route` discriminated union type.

3. **Add dispatch case to `src/components/shell/ContentArea.tsx`**: add a `case "outcome-measures":` branch in the `renderPage()` switch that returns `<ObjectiveMeasuresPage patientId={route.patientId} role={role} userId={userId} />`. Import `ObjectiveMeasuresPage` at the top of the file. Ensure `role` and `userId` are available in scope (they already are in ContentArea per existing pattern).

4. **Add navigation button to `src/pages/PatientDetailPage.tsx`**: immediately after the existing "PT Notes" button, add:
   ```tsx
   {(role === "Provider" || role === "SystemAdmin") && (
     <button onClick={() => navigate({ page: "outcome-measures", patientId })}>
       Objective Measures
     </button>
   )}
   ```
   Use the same Tailwind button classes as the "PT Notes" button for visual consistency.

## Must-Haves

- [ ] All nine body regions are present with correct joint lists
- [ ] ROM fields include: active (number), passive (number), end-feel (select with 6 options), pain checkbox, pain NRS (conditionally shown)
- [ ] MMT fields use Kendall scale with all 10 grade options for correct bilateral muscle groups
- [ ] Ortho test library has ≥20 pre-loaded tests covering at least 6 regions
- [ ] Region filter buttons correctly hide/show tests from the library
- [ ] "Save Objective Measures" button calls `recordObjectiveMeasures` from `src/lib/tauri.ts`
- [ ] On-mount fetch calls `getObjectiveMeasures` and pre-fills form (or silently initialises blank on null)
- [ ] New route `{ page: "outcome-measures"; patientId: string }` added to `RouterContext.tsx`
- [ ] `ContentArea.tsx` dispatches to `ObjectiveMeasuresPage` for the `"outcome-measures"` case
- [ ] "Objective Measures" button in `PatientDetailPage` visible only to Provider and SystemAdmin
- [ ] `tsc --noEmit` exits 0 after all changes

## Verification

- `npx tsc --noEmit` — must exit 0
- Check that `RouterContext.tsx` `Route` type now includes the `outcome-measures` variant (grep for it)
- Check that `ContentArea.tsx` has the `case "outcome-measures":` dispatch (grep for it)

## Observability Impact

- Signals added/changed: UI calls `recordObjectiveMeasures` (which writes `objective_measures.record` audit row) and `getObjectiveMeasures` (which writes `objective_measures.get` audit row) — observability flows through T01's backend
- How a future agent inspects this: `tsc --noEmit` confirms UI contract; `fhir_resources WHERE resource_type = 'PTObjectiveMeasures'` confirms saves reached the DB
- Failure state exposed: inline error state in the component (save error surfaced as a red message below the Save button); null/error on mount load silently initialises blank form (not a crash)

## Inputs

- `src/lib/tauri.ts` — `recordObjectiveMeasures`, `getObjectiveMeasures` wrappers (from T01)
- `src/types/pt.ts` — `ObjectiveMeasuresInput`, `ObjectiveMeasuresRecord` types (from T01)
- `src/contexts/RouterContext.tsx` — existing `Route` union to extend
- `src/components/shell/ContentArea.tsx` — existing `renderPage()` switch to extend
- `src/pages/PatientDetailPage.tsx` — existing "PT Notes" button as style reference
- S02-RESEARCH.md — tabular body-region approach decision, ortho test library list

## Expected Output

- `src/pages/ObjectiveMeasuresPage.tsx` — new page (~300–400 lines) with two tabs; only Objective Measures tab implemented; Outcome Scores tab is placeholder
- `src/contexts/RouterContext.tsx` — one union variant added
- `src/components/shell/ContentArea.tsx` — one case + import added
- `src/pages/PatientDetailPage.tsx` — one button added

---
estimated_steps: 5
estimated_files: 2
---

# T03: Outcome score entry forms, trend chart, and Discharge Summary integration

**Slice:** S02 — Objective Measures & Outcome Scores
**Milestone:** M003

## Description

Implement the Outcome Scores tab in `ObjectiveMeasuresPage.tsx` (replacing the T02 placeholder) and replace the amber outcome-comparison banner in `DischargeSummaryForm` (`PTNoteFormPage.tsx`) with a live comparison table fed by `getOutcomeComparison`.

This is the final task in S02. When complete, the slice demo is fully realised: a provider can record outcome scores, see them auto-scored and graphed, and the Discharge Summary shows a real pre/post comparison table.

## Steps

1. **Implement the Outcome Scores tab in `ObjectiveMeasuresPage.tsx`**:

   Replace the T02 placeholder with the full implementation:

   - Measure type selector: render 6 buttons/tabs (LEFS, DASH, NDI, Oswestry, PSFS, FABQ). `selectedMeasure` state: `MeasureType | null`, initially null (shows "select a measure" prompt).
   - Episode phase radio group: "Initial", "Mid", "Discharge". `episodePhase` state: `"initial" | "mid" | "discharge"`, default `"mid"`.
   - Per-measure item entry forms — item labels are hard-coded constants (extract as a `MEASURE_ITEMS` record outside the component to avoid re-creation on render):
     - LEFS: 20 items, each 0–4 (select or number). Labels: standard LEFS activity items (e.g. "Any of your usual work, housework, or school activities", "Your usual hobbies, recreational or sporting activities", etc. — use published item text).
     - DASH: 30 items, each 1–5. Include a note "≥27 items required for valid score."
     - NDI: 10 items, each 0–5 (6 sub-options per item; use select dropdowns with descriptive option labels from the published NDI form).
     - Oswestry: 10 items, each 0–5 (select dropdowns with published option text).
     - PSFS: Dynamic — item count selector (3, 4, or 5); each item has a text input for "Activity name" and a 0–10 numeric input. Start with 3 items; "Add activity" button adds up to 5.
     - FABQ: 16 items, each 0–6. Items 1, 8, 16 (1-indexed) marked "(not scored)" as grey labels so the clinician still sees them but understands they don't count.
   - "Score & Save" button: collects items as `number[]` (NaN for blank fields → treat blank as 0 or flag as incomplete), calls `commands.recordOutcomeScore({ patientId, encounterId: null, measureType: selectedMeasure, items, episodePhase })`. On success:
     - Display returned `OutcomeScoreRecord` in an inline result card showing: score, severity class (badge), MCID note. For FABQ: show both `score` (work subscale) and `scoreSecondary` (PA subscale) with labels.
     - Call `commands.listOutcomeScores(patientId, selectedMeasure)` to refresh the score history list.
   - Score history section: on mount and after each save, call `commands.listOutcomeScores(patientId, selectedMeasure ?? null)` filtered to `selectedMeasure`. Display as a table: Date | Phase | Score | Severity. Empty state: "No scores recorded yet for this measure."
   - `ScoreChart` component (inline in the same file or extracted as a sibling helper component in the same file — no new file):
     ```tsx
     interface ScoreChartProps {
       scores: { date: string; score: number }[];
       maxScore: number;
       label: string;
     }
     function ScoreChart({ scores, maxScore, label }: ScoreChartProps)
     ```
     - SVG viewBox: `0 0 400 120`. Padding: 20px on each side.
     - Plot area: x from 20 to 380, y from 10 to 110 (height 100).
     - Y coordinate: `y = 110 - Math.max(0, Math.min(1, score / maxScore)) * 100`. Clamp ensures 0 maps to y=110 (bottom) and maxScore maps to y=10 (top).
     - X coordinate: evenly spaced `x = 20 + (i / Math.max(1, scores.length - 1)) * 360`. For single point: `x = 200` (centred).
     - 0 points: render `<text x="200" y="65" textAnchor="middle" className="text-xs fill-gray-400">No data</text>`.
     - 1 point: render a `<circle cx={x} cy={y} r="4">` with a label `<text>` below it showing the score.
     - ≥2 points: render a `<polyline points={pointsStr}>` (stroke, no fill) plus `<circle>` dots at each point.
     - Y-axis: two reference lines and labels at y=10 (maxScore) and y=110 (0) in grey.
     - Title text `<text>` at the top showing `label`.
   - MCID constants (inline object in the file):
     ```ts
     const MEASURE_MCID: Record<MeasureType, number> = {
       lefs: 9, dash: 10.8, ndi: 7.5, oswestry: 10, psfs: 2, fabq: 5
     };
     const MEASURE_MAX_SCORE: Record<MeasureType, number> = {
       lefs: 80, dash: 100, ndi: 100, oswestry: 100, psfs: 10, fabq: 42
     };
     ```

2. **Replace the amber banner in `DischargeSummaryForm` in `src/pages/PTNoteFormPage.tsx`**:

   The `DischargeSummaryForm` component currently accepts `{ fields, readOnly, onChange }`. Add `patientId: string` as a new required prop (update interface `DischargeSummaryFormProps`). Update the single call site in `PTNoteFormPage.tsx` to pass `patientId={patientId}` (patientId is already available in the page's route context).

   Inside `DischargeSummaryForm`:
   - Add state: `const [comparison, setComparison] = useState<OutcomeComparison | null>(null);`
   - `useEffect(() => { commands.getOutcomeComparison(patientId).then(setComparison).catch(() => {}); }, [patientId]);`
   - Replace the existing amber banner (lines ~387-395 in PTNoteFormPage.tsx — the block starting with `{/* Outcome comparison — reserved for S02 */}`) with:
     ```tsx
     {comparison && comparison.measures.length > 0 ? (
       <OutcomeComparisonTable comparison={comparison} />
     ) : (
       <div className="rounded-md border border-amber-200 bg-amber-50 px-4 py-3 text-sm text-amber-700">
         <p className="font-medium">Outcome Comparison</p>
         <p className="mt-0.5">No outcome scores recorded yet. Record scores in Objective Measures to see pre/post comparison.</p>
       </div>
     )}
     ```
   - `OutcomeComparisonTable` is a small helper component in the same file:
     ```tsx
     function OutcomeComparisonTable({ comparison }: { comparison: OutcomeComparison }) {
       return (
         <div>
           <p className="font-medium text-sm mb-2">Outcome Comparison</p>
           <table className="w-full text-sm border-collapse">
             <thead><tr><th>Measure</th><th>Initial</th><th>Discharge</th><th>Change</th><th>MCID Met</th></tr></thead>
             <tbody>
               {comparison.measures.map((m) => (
                 <tr key={m.measureType}>
                   <td>{m.displayName}</td>
                   <td>{m.initialScore} ({m.initialSeverity})</td>
                   <td>{m.dischargeScore} ({m.dischargeSeverity})</td>
                   <td>{m.change >= 0 ? "+" : ""}{m.change}</td>
                   <td>{m.achievedMcid ? "✓" : "–"}</td>
                 </tr>
               ))}
             </tbody>
           </table>
         </div>
       );
     }
     ```
   - Also update the outcome comparison persistence: in `handleSave` (or wherever `update_pt_note` is called for the Discharge Summary), populate `outcomeComparisonPlaceholder` with `comparison ? JSON.stringify(comparison) : null` so the comparison is persisted with the note.

3. **Import types** in `PTNoteFormPage.tsx`: add `OutcomeComparison`, `OutcomeComparisonMeasure` to the import from `src/types/pt.ts`; add `commands` reference for `getOutcomeComparison` (already imported if using the `commands` pattern).

4. **Import types** in `ObjectiveMeasuresPage.tsx`: add `MeasureType`, `OutcomeScoreRecord`, `OutcomeComparison` from `src/types/pt.ts`; add `commands.recordOutcomeScore`, `commands.listOutcomeScores`, `commands.getOutcomeComparison` from `src/lib/tauri.ts`.

5. **Final TypeScript check**: run `npx tsc --noEmit` and fix any errors before marking done.

## Must-Haves

- [ ] All six measure entry forms are implemented with correct item counts (LEFS 20, DASH 30, NDI 10, Oswestry 10, PSFS 3–5, FABQ 16)
- [ ] PSFS "Add activity" button adds items up to a maximum of 5
- [ ] "Score & Save" calls `recordOutcomeScore` with correct `items: number[]` and `episodePhase`
- [ ] Result card shows score, severity, MCID threshold note after save
- [ ] FABQ result card shows both work subscale (`score`) and PA subscale (`scoreSecondary`) with clear labels
- [ ] `ScoreChart` renders correctly for 0, 1, and ≥2 data points without TypeScript errors
- [ ] `ScoreChart` Y-axis uses clamped formula: `y = 110 - Math.max(0, Math.min(1, score / maxScore)) * 100`
- [ ] `DischargeSummaryFormProps` now includes `patientId: string`
- [ ] `DischargeSummaryForm` fetches `getOutcomeComparison` on mount
- [ ] Comparison table renders correctly when `comparison.measures.length > 0`
- [ ] Amber informational message renders when comparison is null or has no measures
- [ ] `outcomeComparisonPlaceholder` populated with `JSON.stringify(comparison)` before calling `updatePtNote`
- [ ] `tsc --noEmit` exits 0

## Verification

- `npx tsc --noEmit` — must exit 0
- Inspect `ScoreChart` clamp logic: for `score = 0, maxScore = 80`, y should equal 110; for `score = 80, maxScore = 80`, y should equal 10
- Inspect `DischargeSummaryForm` — amber banner line replaced; `patientId` prop added to interface and call site
- Grep `getOutcomeComparison` in `PTNoteFormPage.tsx` — must appear

## Observability Impact

- Signals added/changed: `get_outcome_comparison` audit row (`"outcome_comparison.get"`) written on every Discharge Summary form mount — provides a retrievable audit trail of which providers viewed the comparison
- How a future agent inspects this: `tsc --noEmit` to confirm wiring; `SELECT * FROM outcome_score_index WHERE patient_id = ?` to verify data exists for comparison; `SELECT * FROM audit_log WHERE action = 'outcome_comparison.get'` to verify the audit signal fires
- Failure state exposed: `getOutcomeComparison` error swallowed (catch(() => {})) — renders amber fallback, no crash; this is deliberate since the form must remain usable when no scores exist yet

## Inputs

- `src/pages/ObjectiveMeasuresPage.tsx` — T02 implementation with placeholder Outcome Scores tab (replace the placeholder)
- `src/pages/PTNoteFormPage.tsx` — lines ~387-395 amber banner (replace); existing `DischargeSummaryFormProps` interface (extend with `patientId`)
- `src/lib/tauri.ts` — `recordOutcomeScore`, `listOutcomeScores`, `getOutcomeComparison` wrappers (from T01)
- `src/types/pt.ts` — `MeasureType`, `OutcomeScoreRecord`, `OutcomeComparison`, `OutcomeComparisonMeasure` (from T01)
- S02-RESEARCH.md — SVG coordinate math, MCID values, item counts per measure, FABQ subscale item mapping, comparison blob format

## Expected Output

- `src/pages/ObjectiveMeasuresPage.tsx` — extended with full Outcome Scores tab including 6 measure forms, `ScoreChart` SVG component, score history table (~250 additional lines)
- `src/pages/PTNoteFormPage.tsx` — amber banner replaced with `OutcomeComparisonTable` + fallback; `patientId` prop added to `DischargeSummaryForm`; `outcomeComparisonPlaceholder` populated on save

---
estimated_steps: 2
estimated_files: 2
---

# T04: Add src/types/pt.ts and tauri.ts command wrappers

**Slice:** S01 — Touch ID Fix + PT Note Templates
**Milestone:** M003

## Description

T03 produced the backend data model. This task produces the TypeScript mirror: `src/types/pt.ts` with full type definitions for all three PT note shapes, and 6 new wrappers in `src/lib/tauri.ts`. This is a pure contract layer — no UI, no React components. T05 builds on these types.

The convention is `T | null` for all optional fields (never `T | undefined`), camelCase field names matching Rust's `#[serde(rename_all = "camelCase")]`, and `?? null` fallbacks on all optional invoke params.

## Steps

1. **Create `src/types/pt.ts`** with the following types (all fields camelCase, `T | null` for optionals):
   ```ts
   export type PtNoteType = "initial_eval" | "progress_note" | "discharge_summary";
   export type PtNoteStatus = "draft" | "signed" | "locked";

   export interface InitialEvalFields {
     chiefComplaint: string | null;
     mechanismOfInjury: string | null;
     priorLevelOfFunction: string | null;
     painNrs: string | null;           // stored as string "0"–"10" for flexibility
     functionalLimitations: string | null;
     icd10Codes: string | null;
     physicalExamFindings: string | null;
     shortTermGoals: string | null;
     longTermGoals: string | null;
     planOfCare: string | null;
     frequencyDuration: string | null;
     cptCodes: string | null;
     referringPhysician: string | null;
     referralDocumentId: string | null;
   }

   export interface ProgressNoteFields {
     subjective: string | null;
     patientReportPainNrs: string | null;
     hepCompliance: string | null;     // "yes" | "no" | "partial"
     barriers: string | null;
     treatments: string | null;
     exercises: string | null;
     assessment: string | null;
     progressTowardGoals: string | null;
     plan: string | null;
     hepUpdates: string | null;
     totalTreatmentMinutes: string | null;
   }

   export interface DischargeSummaryFields {
     totalVisitsAttended: string | null;
     totalVisitsAuthorized: string | null;
     treatmentSummary: string | null;
     goalAchievement: string | null;
     outcomeComparisonPlaceholder: string | null;  // S02 fills this
     dischargeRecommendations: string | null;
     hepNarrative: string | null;
     returnToCare: string | null;
   }

   export type PtNoteFields =
     | InitialEvalFields
     | ProgressNoteFields
     | DischargeSummaryFields;

   export interface PtNoteInput {
     patientId: string;
     encounterId: string | null;
     noteType: PtNoteType;
     fields: PtNoteFields | null;
     addendumOf: string | null;
   }

   export interface PtNoteRecord {
     id: string;
     patientId: string;
     encounterId: string | null;
     noteType: PtNoteType;
     status: PtNoteStatus;
     providerId: string;
     resource: Record<string, unknown>;
     createdAt: string;
     updatedAt: string;
     addendumOf: string | null;
   }
   ```
   Add a file header comment explaining the convention (matches Rust `#[serde(rename_all = "camelCase")]`, `Option<T>` → `T | null`).

2. **Add imports and 6 wrappers to `src/lib/tauri.ts`**: Import the new types at the top of the file:
   ```ts
   import type { PtNoteInput, PtNoteRecord, PtNoteType } from "../types/pt";
   ```
   Then append a `// ─── PT Note commands ───` section to the `commands` object:
   ```ts
   createPtNote: (input: PtNoteInput) =>
     invoke<PtNoteRecord>("create_pt_note", { input }),

   getPtNote: (ptNoteId: string) =>
     invoke<PtNoteRecord>("get_pt_note", { ptNoteId }),

   listPtNotes: (patientId: string, noteType?: PtNoteType | null) =>
     invoke<PtNoteRecord[]>("list_pt_notes", {
       patientId,
       noteType: noteType ?? null,
     }),

   updatePtNote: (ptNoteId: string, input: PtNoteInput) =>
     invoke<PtNoteRecord>("update_pt_note", { ptNoteId, input }),

   cosignPtNote: (ptNoteId: string) =>
     invoke<PtNoteRecord>("cosign_pt_note", { ptNoteId }),

   lockPtNote: (ptNoteId: string) =>
     invoke<PtNoteRecord>("lock_pt_note", { ptNoteId }),
   ```

## Must-Haves

- [ ] `src/types/pt.ts` is a new file (not appended to an existing types file)
- [ ] `PtNoteType` is a string literal union — NOT a numeric enum
- [ ] `PtNoteStatus` is a string literal union
- [ ] All optional fields use `T | null` — zero uses of `T | undefined`
- [ ] `outcomeComparisonPlaceholder` is present in `DischargeSummaryFields` and typed `string | null`
- [ ] `addendumOf` is present in `PtNoteInput` and `PtNoteRecord`
- [ ] `resource` field in `PtNoteRecord` is typed `Record<string, unknown>` (not `any`)
- [ ] All 6 wrappers in `tauri.ts` use `?? null` for optional params
- [ ] Import added to `tauri.ts` — `PtNoteInput`, `PtNoteRecord`, `PtNoteType` from `../types/pt`
- [ ] No `any` types anywhere in either file
- [ ] `tsc --noEmit` exits 0

## Verification

```bash
npx tsc --noEmit 2>&1 | tail -5
```
Expected: no output (clean exit 0).

```bash
grep -n "any" src/types/pt.ts src/lib/tauri.ts
```
Expected: no matches in the new PT types or the new wrappers.

```bash
grep -n "| undefined" src/types/pt.ts
```
Expected: no matches — all optionals are `| null`.

## Observability Impact

- Signals added/changed: None — this is a pure type layer with no runtime behavior.
- How a future agent inspects this: `tsc --noEmit` immediately reveals any type contract violations between frontend and backend.
- Failure state exposed: TypeScript compile errors are the failure signal. The strict no-`any` requirement means type mismatches surface at compile time rather than at runtime.

## Inputs

- `src-tauri/src/commands/pt_notes.rs` — Rust types from T03 (field names, optionality, enum variants)
- `src/types/documentation.ts` — reference for TypeScript type conventions (`T | null`, `Record<string, unknown>`)
- `src/lib/tauri.ts` — existing `commands` object to extend

## Expected Output

- `src/types/pt.ts` (new) — complete TypeScript types for all PT note shapes
- `src/lib/tauri.ts` — 6 new PT note command wrappers + import

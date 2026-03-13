---
id: T04
parent: S01
milestone: M003
provides:
  - src/types/pt.ts — complete TypeScript type layer for all three PT note shapes
  - src/lib/tauri.ts — 6 new PT note command wrappers + import
key_files:
  - src/types/pt.ts
  - src/lib/tauri.ts
key_decisions:
  - No new decisions — all type conventions (T | null, Record<string, unknown>, string literal unions) are already established; this task simply extends them to the PT note domain
patterns_established:
  - PtNoteType and PtNoteStatus are string literal unions matching Rust #[serde(rename_all = "snake_case")] — never numeric enums
  - All optional fields use T | null (never T | undefined), matching the project-wide convention
  - listPtNotes wrapper passes noteType ?? null so TypeScript optional param coerces to null at the invoke boundary
observability_surfaces:
  - none — pure type/contract layer with no runtime behaviour
duration: ~10 min
verification_result: passed
completed_at: 2026-03-13
blocker_discovered: false
---

# T04: Add src/types/pt.ts and tauri.ts command wrappers

**Created the TypeScript contract layer for PT notes: `src/types/pt.ts` (all three note shapes + record/input types) and 6 new command wrappers in `src/lib/tauri.ts`.**

## What Happened

Created `src/types/pt.ts` as a new file (not appended to an existing types file) with:
- `PtNoteType` string literal union: `"initial_eval" | "progress_note" | "discharge_summary"`
- `PtNoteStatus` string literal union: `"draft" | "signed" | "locked"`
- `InitialEvalFields` (14 fields, all `string | null`)
- `ProgressNoteFields` (11 fields, all `string | null`)
- `DischargeSummaryFields` (8 fields including `outcomeComparisonPlaceholder`, all `string | null`)
- `PtNoteFields` union type
- `PtNoteInput` and `PtNoteRecord` (with `resource: Record<string, unknown>` and `addendumOf: string | null`)

Added to `src/lib/tauri.ts`:
- Import: `PtNoteInput, PtNoteRecord, PtNoteType` from `../types/pt`
- 6 wrappers in a `// ─── PT Note commands ───` section: `createPtNote`, `getPtNote`, `listPtNotes`, `updatePtNote`, `cosignPtNote`, `lockPtNote`
- All optional params use `?? null` at the invoke boundary

## Verification

```bash
# TypeScript contract — clean compile
npx tsc --noEmit 2>&1 | tail -5
# → (no output, exit 0)

# No `any` types in the new files
grep -n "any" src/types/pt.ts src/lib/tauri.ts
# → (no matches in new PT sections)

# No `| undefined` in type declarations
grep -n ": .*| undefined\|?: " src/types/pt.ts
# → (no matches)

# Rust tests still pass (slice-level check)
cd src-tauri && cargo test --lib 2>&1 | tail -5
# → test result: ok. 272 passed; 0 failed; 0 ignored
```

## Diagnostics

This task is a pure type/contract layer — no runtime behaviour, no new observability surfaces. Type contract violations surface immediately as `tsc --noEmit` errors, which serve as the sole diagnostic signal for this layer.

## Deviations

None.

## Known Issues

None.

## Files Created/Modified

- `src/types/pt.ts` (new) — Full TypeScript type definitions for PtNoteType, PtNoteStatus, InitialEvalFields, ProgressNoteFields, DischargeSummaryFields, PtNoteFields, PtNoteInput, PtNoteRecord
- `src/lib/tauri.ts` (modified) — Added `import type { PtNoteInput, PtNoteRecord, PtNoteType }` and 6 PT note command wrappers (createPtNote, getPtNote, listPtNotes, updatePtNote, cosignPtNote, lockPtNote)

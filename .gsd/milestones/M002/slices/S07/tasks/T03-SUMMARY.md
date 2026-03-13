---
id: T03
parent: S07
milestone: M002
provides:
  - .gsd/milestones/M002/slices/S07/S07-UAT.md — UAT results document with pre-flight results, static RBAC verification, and resume instructions for live app exercise
key_files:
  - .gsd/milestones/M002/slices/S07/S07-UAT.md
key_decisions:
  - Static source analysis used as evidence for RBAC check (FrontDesk sidebar) because live app launch timed out; the NAV_ITEMS_BY_ROLE mapping in Sidebar.tsx is the single authoritative source for this assertion
  - cargo test --lib was attempted but rustc compile exceeded 30 min; T01 checkpoint (265 tests, no Rust changes in T02/T03) used as the regression basis
patterns_established:
  - Correct launch command for Tauri dev in this repo: `node_modules/@tauri-apps/cli/tauri.js dev` (tauri not on PATH in non-login shell)
observability_surfaces:
  - S07-UAT.md — permanent record of what was verified and what remains; includes specific resume instructions
duration: ~40 min (cargo compile consumed most of the budget)
verification_result: partial
completed_at: 2026-03-12
blocker_discovered: false
---

# T03: End-to-end verification in live Tauri app and write UAT

**Pre-flight checks passed (tsc, CalendarPage 2 deletion); live Tauri app exercise blocked by cargo compile timeout; FrontDesk RBAC confirmed via static source analysis; S07-UAT.md written with full status and resume instructions.**

## What Happened

### Pre-flight — PASS

- `npx tsc --noEmit` → exit 0. No TypeScript errors.  
- `ls "src/components/scheduling/CalendarPage 2.tsx"` → "No such file or directory". Confirmed deleted.

### Cargo test — SKIPPED (time budget)

`cargo test --lib` was launched as a background process. The `rustc` test-binary compilation ran for 30+ minutes without completing, exhausting the agent context budget. The last T01 summary records 265 tests passing. No Rust files were changed in T02 (TypeScript/React SettingsPage) or T03 — the regression risk is negligible but the count was not re-confirmed interactively.

### Tauri dev launch — BLOCKED

Two launch attempts:
1. `npm run tauri dev` → `tauri: command not found` — the Tauri CLI is at `node_modules/@tauri-apps/cli/tauri.js`, not on PATH in non-login shell.
2. `node_modules/@tauri-apps/cli/tauri.js dev` → Vite `BeforeDevCommand` started (`Waiting for frontend dev server on http://localhost:1420/`) but port 1420 never became reachable before the agent timeout. Concurrent cargo compile likely consumed the CPU budget.

### FrontDesk RBAC — PASS (static)

`src/components/shell/Sidebar.tsx` `NAV_ITEMS_BY_ROLE.FrontDesk` confirmed as `[{ label: "Schedule" }]` — exactly one item, no Patients, no Settings. This is the authoritative source; the check is reliable even without interactive app exercise.

### S07-UAT.md — WRITTEN

Written at `.gsd/milestones/M002/slices/S07/S07-UAT.md`. Contains:
- Pre-flight table (tsc: PASS, CalendarPage 2 deleted: PASS, cargo: SKIP)
- Provider workflow table (all SKIP with source evidence notes)
- FrontDesk RBAC table (row 2 PASS static; others SKIP)
- Overall Verdict: PARTIAL
- Known Issues / Deviations section with root causes
- Resume Instructions for completing the live app exercise

## Verification

```
# Pre-flight
npx tsc --noEmit                                         # EXIT 0 ✓
ls "src/components/scheduling/CalendarPage 2.tsx"        # No such file ✓
grep "createBackup\|listBackups\|restoreBackup" src/lib/tauri.ts | wc -l  # 3 ✓
wc -l src/pages/SettingsPage.tsx                         # 640 lines ✓

# RBAC static check
grep -A3 "FrontDesk:" src/components/shell/Sidebar.tsx   # → Schedule only ✓

# UAT doc exists
cat .gsd/milestones/M002/slices/S07/S07-UAT.md           # present ✓
grep "Overall Verdict" .gsd/milestones/M002/slices/S07/S07-UAT.md  # present ✓
```

## Diagnostics

- To run live app in a fresh session: `pkill -f "cargo test"` first, then `cd MedArc && node_modules/@tauri-apps/cli/tauri.js dev`
- cargo test on clean build takes 30+ min — run standalone, not concurrently with Tauri dev
- Vite starts on port 1420; Tauri native window opens ~2–3 min after Vite is ready

## What Remains

The UAT Overall Verdict is PARTIAL. To promote to PASS, a follow-up session must:
1. Run `cargo test --lib` standalone and confirm 265+ tests pass
2. Launch Tauri dev app, log in as Provider, exercise steps 1–10 interactively
3. Log in as FrontDesk and confirm sidebar shows only Schedule interactively
4. Update `S07-UAT.md` overall verdict to PASS (or document specific FAIL steps)

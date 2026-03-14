# S03: ERA/835 Remittance Processing

**Goal:** BillingStaff can import an 835 ERA file; payments auto-post to patient accounts; adjustments and denials are flagged for manual review; the A/R aging table reflects the new balances in real time. Proven by parsing two real ERA samples (Medicare and commercial) with correct payment amounts and zero panics.

**Demo:** BillingStaff clicks "Import ERA" and selects an 835 file. The import processes in under 5 seconds and displays a summary: "14 claims processed: 11 posted, 2 pending review (CO-97 denial), 1 unmatched." The pending review queue shows the denied claims with CARC/RARC codes and a "Resolve Denial" button. The A/R aging table immediately updates with new bucket totals.

## Must-Haves

- Migrations 27 (`era_batch_index`) and 28 (`payment_posting_index`) applied without errors
- `import_era_835(file_bytes)` parses a complete 835 file, posts all payments, routes CO-97/CO-109/CO-119 and zero-payment SVC lines to `pending_review` status, returns `EraImportResult`
- SHA-256 hash deduplication: importing the same file twice produces no second posting; second import returns `AppError::Validation("Duplicate ERA file")`
- `list_era_batches`, `get_era_batch`, `list_payment_postings`, `list_pending_denials`, `resolve_denial`, `get_ar_aging` Tauri commands registered
- `update_claim_status` from S02 is called by ERA processor to advance claims to `paid`, `denied`, or `adjusted`
- Contractual adjustments (CO group) auto-posted without manual review
- Patient responsibility (PR group) creates patient balance entry in future milestone (placeholder column in `payment_posting_index` for now)
- A/R aging query returns correct bucket totals using `julianday` arithmetic on `billed_date`
- Denial review queue: BillingStaff sees CARC code, RARC code (if present), CPT code, denied amount, and can enter resolution note via `resolve_denial(posting_id, resolution_note)` → transitions status to `resolved`
- All ERA commands write audit rows; `import_era_835` writes one row per batch plus one per posting
- `src/types/era.ts` — TypeScript types for ERA shapes
- `EraPage.tsx` — ERA batch list, import button, pending denials tab, A/R aging widget
- `cargo test --lib` passes with ≥5 new ERA unit tests (duplicate detection, partial payment, CAS multi-pair, unmatched claim handling)
- `tsc --noEmit` exits 0

## Proof Level

- This slice proves: **contract + integration**
- Real runtime required: yes — two real ERA sample files (Medicare 835 and one commercial payer 835) must parse without panics
- Human/UAT required: yes — BillingStaff imports an ERA and confirms payment amounts match the paper EOB

## Verification

```bash
# 1. Contract
cd src-tauri && cargo test --lib 2>&1 | tail -5

# 2. TypeScript contract
cd .. && npx tsc --noEmit 2>&1 | tail -5

# 3. ERA parsing unit tests (embedded in cargo test --lib):
#    - Duplicate hash: second import of same file returns Err(Validation("Duplicate ERA file"))
#    - Partial payment: claim with 3 SVC lines, one denied — 2 posted, 1 pending_review
#    - CAS multi-pair: CAS segment with 3 CARC/amount pairs all correctly parsed
#    - Unmatched CLP01: claim_id not in claim_index → posting_index row with status 'unmatched'
#    - A/R aging correctness: claim billed 45 days ago with partial payment → '31-60' bucket

# 4. Real ERA parsing integration test:
#    Parse a real Medicare 835 file → no panics, EraImportResult.denial_count matches
#    manually reviewed file
#    Parse a real commercial payer 835 file → same criteria
```

## Observability / Diagnostics

- Runtime signals: `write_audit_entry` for `era.import`, `era.post_payment`, `era.flag_denial`, `era.resolve_denial`
- Inspection surfaces:
  - `era_batch_index`: `SELECT * FROM era_batch_index ORDER BY imported_at DESC` — batch history
  - `payment_posting_index WHERE status = 'pending_review'` — denial queue
  - `payment_posting_index WHERE status = 'unmatched'` — unmatched claim queue
  - A/R aging: `get_ar_aging(null)` Tauri command returns live bucket data
- Failure state: `import_era_835` returns `AppError::Parse` with segment location on parse failure; returns `AppError::Validation` for duplicate; all errors propagate to frontend error banner; import never partially commits (transaction rollback on any error)

## Integration Closure

- Upstream surfaces consumed:
  - `claim_index` (S02) — CLP01 matching; `update_claim_status` called on era resolution
  - `billing_index` (S01) — joined for A/R aging calculation
  - `Claims` RBAC resource (S02) — ERA import is a billing operation
  - Office Ally SFTP background task (S02) — ERA download reuses the same SFTP connection pattern
- New wiring introduced:
  - `commands/era.rs` registered in `commands/mod.rs` and `lib.rs`
  - Six Tauri commands in `invoke_handler!`
  - `EraPage.tsx` as new route target
  - Background SFTP polling extended to download ERA files from `/edi/era/` path
- What remains: S06 analytics reads `payment_posting_index` for net collection rate and A/R KPIs

## Tasks

- [ ] **T01: Backend — ERA module, Migrations 27–28, 835 parser, auto-posting** `est:4h`
  - Why: The ERA parser and auto-posting logic is the highest-risk item. Getting CAS group parsing and claim matching right requires careful parsing and comprehensive unit tests. Migrations must be in place before the UI can display real data.
  - Files: `src-tauri/src/commands/era.rs` (new), `src-tauri/src/commands/mod.rs`, `src-tauri/src/db/migrations.rs`, `src-tauri/src/lib.rs`, `src/types/era.ts` (new), `src/lib/tauri.ts`
  - Do:
    1. Create `src-tauri/src/commands/era.rs` with: (a) `EraParser` struct that walks `edi::parse` token stream and builds `EraRecord`, `Vec<ClaimPayment>`, `Vec<ServicePayment>`; (b) auto-posting logic (tx per batch, rollback on error); (c) SHA-256 hash check via `sha2` crate (add to Cargo.toml if not present); (d) A/R aging query as a SQL string executed via `conn.query`; (e) six Tauri commands; (f) `#[cfg(test)]` module with ≥5 unit tests
    2. Append Migrations 27 and 28 to `MIGRATIONS` vector
    3. Add `pub mod era;` to `commands/mod.rs`; register commands in `lib.rs`
    4. Create `src/types/era.ts` with `EraRecord`, `PaymentPosting`, `ArAgingBucket`, `EraImportResult`, `DenialRecord`
    5. Append era wrappers to `src/lib/tauri.ts` under `// M004/S03`
  - Verify: `cargo test --lib` passes with ≥5 new ERA tests; parse two real ERA samples without panics

- [ ] **T02: Frontend — EraPage with denial review and A/R aging** `est:2h`
  - Why: Delivers ERA-01 through ERA-04 (import, auto-post, denial queue, A/R aging). Makes remittance processing actionable for BillingStaff.
  - Files: `src/pages/EraPage.tsx` (new), `src/contexts/RouterContext.tsx`, `src/components/shell/ContentArea.tsx`
  - Do:
    1. Create `EraPage.tsx` with three tabs: "ERA Batches" (list with import button and batch summary stats), "Pending Denials" (denial queue table with CARC/RARC explanation text, Resolve button, resolution note textarea), "A/R Aging" (bucket table: 0-30, 31-60, 61-90, 91-120, 120+; refreshes on tab focus)
    2. "Import ERA" button opens a file dialog (Tauri `open` dialog filtered to `.835` and `.txt`); passes file bytes to `importEra835`
    3. Denial resolution modal: shows full claim context, denial reason, allows resolution note entry
    4. Add route and ContentArea dispatch; add "ERA" navigation item to BillingStaff sidebar
  - Verify: `tsc --noEmit` exits 0; ERA import flow works end-to-end in Tauri dev app; denial queue shows correct records after import

## Files Likely Touched

- `src-tauri/src/commands/era.rs` — new module (T01)
- `src-tauri/src/commands/mod.rs` — `pub mod era` (T01)
- `src-tauri/src/db/migrations.rs` — Migrations 27, 28 appended (T01)
- `src-tauri/src/lib.rs` — 6 commands registered (T01)
- `src-tauri/Cargo.toml` — `sha2` added if not present (T01)
- `src/types/era.ts` — new file (T01)
- `src/lib/tauri.ts` — ERA wrappers appended (T01)
- `src/pages/EraPage.tsx` — new page (T02)
- `src/contexts/RouterContext.tsx` — new route (T02)
- `src/components/shell/ContentArea.tsx` — dispatch case (T02)

# S02: Electronic Claims Submission (837P)

**Goal:** Any encounter with a completed billing summary can be wrapped in a standards-compliant 837P EDI file, validated locally, and transmitted to Office Ally via SFTP. The claim lifecycle (Draft → Validated → Submitted → Accepted → Paid → Denied) is tracked in the database and visible to BillingStaff. Proven by WEDI validator accepting the generated file and Office Ally confirming receipt of a test claim.

**Demo:** BillingStaff selects an encounter with a `ready_to_bill` billing record, clicks "Generate Claim", reviews the claim summary (patient, payer, CPT codes, charges), clicks "Validate" — status transitions to Validated with a green checkmark. Clicking "Submit to Office Ally" uploads the 837P file; status transitions to Submitted with timestamp. Within 2 hours, the 999 acknowledgement is polled and status updates to Accepted.

## Must-Haves

- Migrations 25 (`claim_index`) and 26 (`payer_config` with Office Ally as default entry) applied without errors
- `generate_837p(billing_id)` produces a valid X12N 5010A1 837P file with all required loops and segments for a PT outpatient professional claim
- `validate_claim(claim_id)` performs local structural validation: required segments present, control numbers valid, ICD-10 format valid, CPT codes in library, GP modifier on all Medicare timed codes — returns a `Vec<ValidationError>` or Ok(())
- `submit_claim_sftp(claim_id)` uses `ssh2` to upload to Office Ally SFTP; stores private key path from macOS Keychain
- `list_claims`, `get_claim`, `update_claim_status`, `get_payer_config`, `set_payer_config` Tauri commands registered
- `ClaimStatus` state transitions enforced: Draft → Validated → Submitted → Accepted/Rejected → Paid/Denied → Appealed; invalid transitions return `AppError::Validation`
- GP modifier auto-applied on all Medicare timed CPT codes; REF*9F populated from M003 `auth_index` when auth number exists
- PT taxonomy code `225100000X` in PRV segment
- New RBAC `Claims` resource: BillingStaff = CRUD; Provider = CR; NurseMa = R; FrontDesk = none
- ISA interchange control numbers stored and incremented atomically per payer in `payer_config`
- 997/999 acknowledgement background polling task runs every 30 minutes via Tauri background thread
- All claim commands write audit rows; `AppError` propagates to frontend error banner
- `src/types/claims.ts` — TypeScript types for all claim shapes
- Claims UI: `ClaimsPage.tsx` with claim list, status badges, generate/validate/submit actions
- `cargo test --lib` passes with ≥5 new claim unit tests (segment generation correctness, state machine transitions)
- `tsc --noEmit` exits 0

## Proof Level

- This slice proves: **contract + integration**
- Real runtime required: yes — SFTP upload to Office Ally sandbox must succeed end-to-end
- Human/UAT required: yes — BillingStaff generates, validates, and submits a test claim; confirms receipt in Office Ally portal

## Verification

```bash
# 1. Contract
cd src-tauri && cargo test --lib 2>&1 | tail -5

# 2. TypeScript contract
cd .. && npx tsc --noEmit 2>&1 | tail -5

# 3. 837P structural validation (embedded in unit tests):
#    - Generated file contains ISA, GS, ST, BPR?, Loop 1000A, Loop 1000B,
#      Loop 2000A, Loop 2000B, Loop 2010BA, Loop 2010BB, Loop 2300, Loop 2400, SE, GE, IEA
#    - SE segment count matches actual segment count
#    - SV1 for timed codes contains GP modifier in first modifier position
#    - HI segment uses ABK qualifier for primary diagnosis
#    - PRV segment contains taxonomy 225100000X

# 4. WEDI validator — run generated file through wedi.org/resources validator tool
#    Must return: "Transaction accepted" with 0 errors

# 5. Office Ally integration — upload test claim to Office Ally sandbox,
#    confirm file appears in /edi/claims/ and 999 AA received within 2 hours
```

## Observability / Diagnostics

- Runtime signals: `write_audit_entry` on every `claim.generate`, `claim.validate`, `claim.submit`, `claim.status_update`; audit action strings `"claim.generate"`, `"claim.validate"`, `"claim.submit"`, `"claim.ack_received"`
- Inspection surfaces:
  - `claim_index` table: `SELECT * FROM claim_index WHERE patient_id = ?` for claim lifecycle
  - `fhir_resources` WHERE `resource_type = 'ClaimEDI'` for raw 837P content (stored as text for audit trail)
  - Background poll log: Tauri background thread logs to `tracing::info!` on each poll cycle
- Failure state: `validate_claim` returns `Vec<ValidationError>` with human-readable descriptions; `submit_claim_sftp` returns `AppError::Network` with SFTP error detail; `AppError::Validation` for state transition violations

## Integration Closure

- Upstream surfaces consumed:
  - `billing_index` — `generate_837p` reads CPT entries, charges, payer, diagnoses from `BillingRecord`
  - `get_encounter_billing(encounter_id)` — resolves billing record for the claim
  - `auth_index` (M003/S07) — reads prior auth number for REF*9F segment
  - `rbac/roles.rs` — `Claims` resource added
- New wiring introduced:
  - `commands/claims.rs` registered in `commands/mod.rs` and `lib.rs`
  - `Claims` RBAC resource added alongside existing resources
  - Eight Tauri commands in `invoke_handler!`
  - `ClaimsPage.tsx` as a new route target
  - Tauri background polling task started in `setup` hook in `lib.rs`
- What remains: S03 (ERA posting advances claims to Paid/Denied), S04 (therapy cap reads claim data), S06 (analytics reads financial KPIs from claim lifecycle)

## Tasks

- [ ] **T01: Backend — claims module, Migrations 25–26, 837P generator** `est:4h`
  - Why: 837P generation is the highest-risk item. Building and validating the segment output is the primary proof point for this slice. State machine and RBAC are prerequisite for the UI task.
  - Files: `src-tauri/src/commands/claims.rs` (new), `src-tauri/src/commands/mod.rs`, `src-tauri/src/db/migrations.rs`, `src-tauri/src/rbac/roles.rs`, `src-tauri/src/lib.rs`, `src/types/claims.ts` (new), `src/lib/tauri.ts`
  - Do:
    1. Create `src-tauri/src/commands/claims.rs` with: (a) `ClaimBuilder` struct implementing all required 837P loops/segments as methods; (b) `validate_claim_segments(edi_content: &str) -> Vec<ValidationError>` pure function; (c) Eight Tauri commands; (d) `#[cfg(test)]` module with ≥5 tests: segment structure test (ISA/GS/ST/SE present and SE count correct), SV1 modifier ordering test (GP first for Medicare), HI qualifier test (ABK for primary), state transition validity tests
    2. Append Migrations 25 (`claim_index`) and 26 (`payer_config`) to `MIGRATIONS` vector
    3. Add `Claims` resource to `Resource` enum in `rbac/roles.rs`
    4. Add `pub mod claims;` to `commands/mod.rs`; register commands in `lib.rs`
    5. Create `src/types/claims.ts` with `ClaimStatus`, `ClaimRecord`, `ClaimInput`, `PayerConfig`, `ValidationError`
    6. Append claim wrappers to `src/lib/tauri.ts` under `// M004/S02`
    7. Add background SFTP polling task in `lib.rs` `setup` hook (Tauri async background thread, 30-minute interval)
  - Verify: `cargo test --lib` passes; generated 837P passes WEDI validator

- [ ] **T02: SFTP integration + acknowledgement polling** `est:2h`
  - Why: Retiring the SFTP connectivity risk (per M004 ROADMAP proof strategy). `submit_claim_sftp` must prove it can actually reach Office Ally.
  - Files: `src-tauri/src/commands/claims.rs` (sftp module), `src-tauri/Cargo.toml`
  - Do:
    1. Add `ssh2 = "0.9"` to `Cargo.toml` dependencies
    2. Implement `upload_claim_sftp` function using `ssh2::Session`, `TcpStream` connection to `ftp.officeally.com:22`, pubkey auth from keychain key path
    3. Implement `poll_acknowledgements` function that opens SFTP, lists `/edi/acks/`, downloads new 997/999 files, parses AK/AA/AE status, calls `update_claim_status`
    4. Wire background task to call `poll_acknowledgements` on 30-minute interval
    5. Test: connect to Office Ally sandbox SFTP with test credentials; upload a test file; verify it appears in the remote directory; download acknowledgement
  - Verify: Real SFTP connection to Office Ally sandbox succeeds; 999 AA received for a syntactically valid test claim

- [ ] **T03: Frontend — ClaimsPage** `est:2h`
  - Why: Makes the claim workflow visible and interactive. BillingStaff needs the UI to generate, validate, and submit claims without using a command line.
  - Files: `src/pages/ClaimsPage.tsx` (new), `src/contexts/RouterContext.tsx`, `src/components/shell/ContentArea.tsx`, `src/pages/BillingPage.tsx`
  - Do:
    1. Create `ClaimsPage.tsx` with claim list table (columns: Claim ID, Patient, Payer, Date, Total, Status); status badges with colour coding; row actions: View, Validate, Submit, Cancel
    2. Claim detail panel: shows payer config, CPT line items, diagnosis codes, modifiers, EDI content preview (read-only text area)
    3. "Generate Claim" button on `BillingPage.tsx` for `ready_to_bill` records; navigates to claim detail
    4. Payer configuration form in Settings (accessible from ClaimsPage)
    5. Add route and ContentArea dispatch
  - Verify: `tsc --noEmit` exits 0; claim list renders with status badges; generate/validate/submit button flow works in Tauri dev app

## Files Likely Touched

- `src-tauri/src/commands/claims.rs` — new module (T01, T02)
- `src-tauri/src/commands/mod.rs` — `pub mod claims` (T01)
- `src-tauri/src/db/migrations.rs` — Migrations 25, 26 appended (T01)
- `src-tauri/src/rbac/roles.rs` — `Claims` resource added (T01)
- `src-tauri/src/lib.rs` — 8 commands + background task (T01, T02)
- `src-tauri/Cargo.toml` — `ssh2` added (T02)
- `src/types/claims.ts` — new file (T01)
- `src/lib/tauri.ts` — claim wrappers appended (T01)
- `src/pages/ClaimsPage.tsx` — new page (T03)
- `src/contexts/RouterContext.tsx` — new route (T03)
- `src/components/shell/ContentArea.tsx` — dispatch case (T03)
- `src/pages/BillingPage.tsx` — "Generate Claim" button (T03)

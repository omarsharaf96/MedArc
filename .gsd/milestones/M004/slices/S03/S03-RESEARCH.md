# S03: ERA/835 Remittance Processing — Research

**Date:** 2026-03-14

## Summary

S03 implements Electronic Remittance Advice (ERA) processing — importing ANSI X12N 835 files from Office Ally, auto-posting payments to patient accounts, and surfacing denials in a review queue. The 835 is the electronic equivalent of a paper Explanation of Benefits (EOB) from payers to providers.

The `edi` 0.4 crate handles 835 parsing. ERA processing is fundamentally a matching problem: 835 CLP segments contain claim numbers that must be matched to `claim_index` records from S02. Payment amounts from SVC segments are posted to `payment_posting_index`. CARC/RARC codes in CAS segments determine whether a line item is a contractual adjustment (no action needed), a patient responsibility (update patient balance), or a denial (route to review queue).

After an ERA batch is processed, `billing_index` and `claim_index` statuses are updated, and the A/R aging table reflects the new balances. This is the data source that S06 analytics reads for net collection rate and days-in-A/R KPIs.

**Confidence: HIGH** for the parsing and posting logic — ANSI X12N 835 is well-specified and the `edi` crate handles tokenisation. **MEDIUM** for CARC/RARC handling — while the codes are standardised by CMS/X12, different payers use different combinations and some use non-standard supplemental codes. Covering the most common PT-relevant denial codes is sufficient for V1.

## Recommendation

- Use `edi` crate for 835 tokenisation; write a domain-specific `EraParser` that walks the token stream and builds typed structs
- Auto-post contractual adjustments and payments; route denials to a manual review queue
- Store the raw 835 file content in `fhir_resources` (as a `ERADocument` resource type) for audit trail
- Implement the A/R aging query as a pure SQL view over `claim_index` and `payment_posting_index`

## Don't Hand-Roll

| Problem | Existing Solution | Why Use It |
|---------|------------------|------------|
| X12 tokenisation | `edi` 0.4 crate | Handles ISA/GS/ST envelopes and segment splitting correctly |
| RBAC | `Claims` resource (from S02) | ERA import is a billing operation — BillingStaff owns it |
| Audit log | `write_audit_entry` | ERA processing touches financial ePHI |
| Status updates | `update_claim_status(claim_id, status)` from S02 | ERA processor calls this to advance claim lifecycle |
| Background poll | Tauri background task pattern from S02 | Poll Office Ally `/edi/era/` directory on same 30-minute schedule |

## 835 File Structure

### Envelope
```
ISA — Interchange Control Header
  GS — Functional Group Header
    ST*835 — Transaction Set Header (835 = Health Care Claim Payment)
    BPR — Financial Information (payment amount, check/EFT number, payment date)
    TRN — Reassociation Trace Number (check/EFT trace number)
    DTM*405 — Production Date

    Loop 1000A — Payer Identification
      N1*PR — Payer Name
      N3 — Payer Address
      N4 — Payer City/State/ZIP
      REF*2U — Payer ID

    Loop 1000B — Payee Identification
      N1*PE — Payee (Provider/Practice)
      REF*PQ — Payee NPI

    Loop 2000 — Header Number (one per patient/claim group)
      LX — Header Number

      Loop 2100 — Claim Payment Information
        CLP — Claim Payment Information
              CLP01 = Patient control number (matches claim_index.claim_id)
              CLP02 = Claim status code (1=paid, 2=adjusted, 3=denied, 4=accepted as secondary)
              CLP03 = Total charge submitted
              CLP04 = Amount paid
              CLP05 = Patient responsibility
              CLP06 = Claim filing indicator (MB=Medicare Part B, CI=commercial)
              CLP07 = Payer claim control number
        CAS — Claim Adjustment (contractual, patient resp, other)
              CAS01 = Adjustment group code (CO, PR, OA, PI, CR)
              CAS02, CAS03 = CARC code, amount
              (up to 6 CARC/amount pairs per CAS segment)
        NM1*QC — Patient Name
        NM1*IL — Insured Name
        DTM*232 — Service Date
        AMT*AU — Claim supplemental amount
        REF*EA — Medical Record Number (optional)

        Loop 2110 — Service Payment Information
          SVC — Service Payment
                SVC01 = Composite of adjudicated procedure (HC:<CPT_code>:<modifier>)
                SVC02 = Amount submitted
                SVC03 = Amount paid (0 if denied)
                SVC04 = Revenue code (blank for PT)
                SVC05 = Units paid
          DTM*472 — Date of Service
          CAS — Service Adjustment
          REF*6R — Line Item Control Number (matches billing line)
          AMT*B6 — Allowed Amount
          LSVC — Service Level Remark (optional)
          LQ — Health Care Remark Code (RARC codes)
              LQ01 = Qualifier (HE = HIPAA remark code = RARC)
              LQ02 = RARC code

    SE — Transaction Set Trailer
  GE — Functional Group Trailer
IEA — Interchange Control Trailer
```

## Auto-Posting Workflow

```
1. Download 835 file from Office Ally /edi/era/ via SFTP (background task or manual import)
2. Hash file content (SHA-256) — if hash already in era_batch_index, skip (duplicate detection)
3. Parse 835 using EraParser:
   a. Extract BPR payment amount and TRN trace number → create EraRecord
   b. For each CLP segment:
      i.  Look up claim_index by claim_id = CLP01 → find ClaimRecord
      ii. If no match: flag as "unmatched" in era_batch_index, create PaymentPosting with status "unmatched"
      iii. Parse CAS segments for claim-level adjustments
      iv. CAS01="CO" → contractual adjustment (write-off, no patient action)
      v.  CAS01="PR" → patient responsibility (update patient balance)
      vi. CAS01="OA" or "PI" → other payer or payer-initiated adjustment
      vii. For each SVC segment: create PaymentPosting row
           - amount_paid = SVC03
           - adjustment = SVC03 - SVC02 (negative = denial / reduction)
           - denial_code = CARC from SVC-level CAS if amount_paid = 0
           - status = "posted" if payment > 0 else "pending_review"
4. Update claim_index status:
   - CLP02=1 and total paid > 0 → status = "paid"
   - CLP02=3 → status = "denied"
   - CLP02=2 → status = "adjusted"
5. Write audit row for each ERA batch import
6. Return EraImportResult with counts: total_claims, posted, denials, unmatched
```

## CARC/RARC Reason Codes

### Claim Adjustment Reason Codes (CARC) — PT-Relevant
| Code | Description | Auto-Post? | Action |
|------|-------------|------------|--------|
| CO-2 | Coinsurance amount per plan | Yes | Reduce claim balance by adjustment |
| CO-45 | Charges exceed contractual amount | Yes | Write-off contractual adjustment |
| CO-97 | Service auth not obtained | No | Route to denial review |
| CO-109 | Service not covered by plan | No | Route to denial review |
| CO-119 | Benefit maximum exceeded (therapy cap) | No | Check KX modifier; route to review |
| CO-197 | Prior auth was not obtained | No | Verify M003 auth record |
| PR-1 | Deductible not met | Yes | Post to patient balance |
| PR-2 | Coinsurance | Yes | Post to patient balance |
| PR-3 | Co-pay | Yes | Post to patient balance |
| PR-27 | Expenses incurred after coverage terminated | No | Eligibility issue; route to review |
| OA-23 | MSP (Medicare as secondary payer) | No | Route to review |

### Remittance Advice Remark Codes (RARC) — PT-Relevant
| Code | Description | Common Pairing |
|------|-------------|----------------|
| M127 | Missing/incomplete/invalid prior auth | CO-197 |
| N247 | Missing/incomplete/invalid service facility NPI | CO-16 |
| M86 | Denied: service incidental to primary service | CO-4 |
| N20 | Procedure prior auth not obtained | CO-97 |
| MA63 | Therapy cap exceeded; KX modifier required | CO-119 |

## Payment Reconciliation

### A/R Aging Calculation
```sql
-- A/R aging view (used by S06 analytics)
SELECT
  CASE
    WHEN julianday('now') - julianday(c.billed_date) <= 30 THEN '0-30'
    WHEN julianday('now') - julianday(c.billed_date) <= 60 THEN '31-60'
    WHEN julianday('now') - julianday(c.billed_date) <= 90 THEN '61-90'
    WHEN julianday('now') - julianday(c.billed_date) <= 120 THEN '91-120'
    ELSE '120+'
  END AS bucket,
  COUNT(*) AS claim_count,
  SUM(c.total_charge - COALESCE(p.total_paid, 0)) AS total_balance
FROM claim_index c
LEFT JOIN (
  SELECT claim_id, SUM(amount_paid) AS total_paid
  FROM payment_posting_index
  WHERE status = 'posted'
  GROUP BY claim_id
) p ON c.claim_id = p.claim_id
WHERE c.status NOT IN ('paid', 'denied')
GROUP BY bucket;
```

### Net Collection Rate
```sql
-- Net collection rate = payments received / (charges - contractual adjustments)
SELECT
  SUM(pp.amount_paid) / NULLIF(
    SUM(bi.total_charge) - SUM(
      SELECT SUM(pp2.adjustment)
      FROM payment_posting_index pp2
      WHERE pp2.claim_id = ci.claim_id
        AND pp2.adjustment_type = 'contractual'
    ), 0
  ) AS net_collection_rate
FROM payment_posting_index pp
JOIN claim_index ci ON pp.claim_id = ci.claim_id
JOIN billing_index bi ON ci.billing_id = bi.billing_id;
```

## Data Shapes

### Migration 27: `era_batch_index`
```sql
CREATE TABLE IF NOT EXISTS era_batch_index (
    batch_id        TEXT PRIMARY KEY NOT NULL,
    payer_id        TEXT NOT NULL,
    check_number    TEXT,
    payment_date    TEXT NOT NULL,
    total_payment   REAL NOT NULL,
    file_hash       TEXT NOT NULL UNIQUE,  -- SHA-256; dedup check
    imported_at     TEXT NOT NULL,
    claim_count     INTEGER NOT NULL DEFAULT 0,
    posted_count    INTEGER NOT NULL DEFAULT 0,
    denial_count    INTEGER NOT NULL DEFAULT 0,
    unmatched_count INTEGER NOT NULL DEFAULT 0
);
```

### Migration 28: `payment_posting_index`
```sql
CREATE TABLE IF NOT EXISTS payment_posting_index (
    posting_id      TEXT PRIMARY KEY NOT NULL,
    batch_id        TEXT NOT NULL REFERENCES era_batch_index(batch_id),
    claim_id        TEXT,                  -- NULL if unmatched
    patient_id      TEXT NOT NULL,
    cpt_code        TEXT,
    amount_submitted REAL NOT NULL,
    amount_paid     REAL NOT NULL,
    adjustment      REAL NOT NULL DEFAULT 0.0,
    adjustment_type TEXT,                  -- 'contractual' | 'patient_resp' | 'denial' | 'other'
    denial_code     TEXT,                  -- CARC code if denied
    remark_code     TEXT,                  -- RARC code if present
    status          TEXT NOT NULL DEFAULT 'pending_review'
                    CHECK(status IN ('posted','pending_review','resolved','unmatched')),
    resolved_at     TEXT,
    resolution_note TEXT,
    created_at      TEXT NOT NULL
);
```

## Common Pitfalls

- **Duplicate ERA processing** — Office Ally may re-deliver the same 835 file if polling overlaps. The SHA-256 hash check on `era_batch_index.file_hash` prevents double-posting. Always check hash before processing.
- **Unmatched claims** — The CLP01 patient control number must exactly match the `claim_id` stored in `claim_index`. If claim submission used a modified control number, matching will fail. Store the control number used in the 837P ISA envelope in `claim_index.control_number` column.
- **Partial payment vs. denial** — A claim can have `amount_paid > 0` with some line items denied. Do not mark the entire claim as "denied" if only some SVC lines were rejected. Check per-SVC payment amounts.
- **CAS segment has multiple CARC pairs** — A single CAS segment has up to 6 CARC/amount pairs (`CAS02/CAS03`, `CAS04/CAS05`, ..., `CAS12/CAS13`). Parsing must iterate all pairs, not just the first.
- **Negative adjustment amounts** — Some payers use positive adjustment amounts to mean "amount reduced"; others use signed values. Parse defensively and check that `amount_paid + adjustments ≈ amount_submitted`.
- **ERA date formats** — 835 dates in DTM segments use CCYYMMDD format (e.g., `20260314`). Convert to RFC-3339 `2026-03-14T00:00:00Z` before storing in SQLite.

## Risks and Mitigations

| Risk | Severity | Mitigation |
|------|----------|------------|
| Real ERA samples from multiple payers have different non-standard segments | High | Parse samples from at least Medicare and one commercial payer before marking S03 complete; log unknown segments as warnings, not errors |
| `edi` crate panics on malformed 835 | Medium | Wrap all crate calls in `catch_unwind` or use `?` with `map_err`; never let parse errors crash the app |
| Partial payment handling creates incorrect A/R balance | Medium | Per-SVC posting (not per-claim) ensures line-level accuracy; unit tests cover partial payment scenario |
| ERA file encoding issues (Windows line endings) | Low | Pre-process input: normalise `\r\n` to `\n` before passing to `edi::parse` |
| Background polling conflicts with manual import | Low | Use a DB mutex (advisory lock via a `processing` flag in `era_batch_index`) to prevent concurrent processing of the same file |

## Sources

- ANSI X12N 5010A1 835 implementation guide (Washington Publishing Company)
- CMS CARC/RARC code set maintained at x12.org
- Rust `edi` 0.4 crate documentation (docs.rs/edi)
- Office Ally ERA retrieval documentation (officeally.com)
- CMS Remittance Advice Remark Code (RARC) list (cms.gov/medicare/regulations-guidance)
- APTA: "Understanding Your ERA" — PT-specific CARC interpretation guide

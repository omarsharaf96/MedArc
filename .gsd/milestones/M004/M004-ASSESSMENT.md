# M004: Technical Risk Assessment

**Date:** 2026-03-14
**Milestone:** M004 — Claims, Billing & Practice Management

---

## Risk Summary Matrix

| Slice | Primary Risk | Severity | Retirement Method | Retirement Timing |
|-------|-------------|----------|-------------------|-------------------|
| S01: CPT Billing Engine | 8-minute rule edge cases with multiple code combinations | Medium | Unit tests covering all 9 APTA reference cases | T01 |
| S02: 837P Claims | Segment ordering and clearinghouse rejection | **High** | WEDI validator + real Office Ally test transmission | T01 + T02 |
| S03: ERA/835 Parsing | Payer-specific non-standard 835 variations | **High** | Parse ≥2 real ERA files (Medicare + commercial) | T01 |
| S04: Therapy Cap | KX modifier auto-apply correctness at boundary conditions | Medium | Unit tests on exact threshold values | T01 |
| S05: HEP Builder | Exercise image bundling and PDF embed reliability | Medium | First-launch seeding performance test + PDF with image | T01 |
| S06: Analytics | recharts Tauri WKWebView compatibility | Low | Dashboard render with live data in dev app | T02 |
| S07: MIPS | Measure derivation accuracy vs CMS specification | **High** | Unit tests against CMS test case values | T01 |

**Net: 3 High, 3 Medium, 1 Low** — acceptable risk profile for a billing and compliance milestone. All High risks are retired early (T01 of their respective slice), before dependent tasks begin.

---

## Slice-by-Slice Technical Risk Assessment

### S01: CPT Billing Engine — Risk: MEDIUM

**Primary Risk: 8-minute rule unit distribution algorithm**
The rule itself is simple arithmetic, but the unit-distribution-to-codes step is not. When total timed minutes earn 3 units but the provider entered 97110 (25 min) and 97112 (20 min) and 97530 (8 min), the correct distribution (97110: 2, 97112: 1, 97530: 0) is non-obvious. The algorithm must prioritise the code with the most minutes that individually meets the 8-minute threshold.

- **Mitigated by:** 9 unit tests in `billing.rs` covering all APTA reference cases including the complex distribution case. These tests must pass before T02 begins.
- **Residual risk:** A CPT code combination not covered by the 9 reference cases might expose an edge case in production. Mitigation: BillingStaff can always manually override units.

**Secondary Risk: Fee schedule staleness**
CMS updates the Medicare Physician Fee Schedule annually in November for the following year. Hard-coded 2026 rates will need updating in November 2026.

- **Mitigated by:** `effective_date` column in `fee_schedule`; BillingStaff can import an updated CSV. Not a correctness risk for M004 scope.

**No new crate risks.** S01 uses only `csv` crate (already planned) and existing Rust standard library.

---

### S02: Electronic Claims (837P) — Risk: HIGH

**Primary Risk: 837P segment generation correctness**
ANSI X12N 5010A1 has a strict loop structure. Generating segments in the wrong order, using wrong element separators, or miscounting segments in the SE trailer causes the clearinghouse to reject the entire interchange, not just the individual claim. The first rejected transmission may not give a clear error message.

- **Mitigated by:**
  1. Local structural validation in `validate_claim`: checks required segment presence, SE count, SV1 modifier ordering, HI qualifier, taxonomy code
  2. WEDI free online validator validates the full file against the 837P IG before submission
  3. Unit tests assert exact segment text for specific scenarios
- **Residual risk:** Subtle payer-specific 837P requirements (e.g., BCBS requiring specific REF qualifier variants) are not covered until those payers are tested. Office Ally normalises some variations before forwarding to payers, reducing this risk.

**Secondary Risk: SFTP connectivity and Office Ally setup**
SFTP private key management, known_hosts fingerprint verification, and Office Ally payer enrollment are real-world dependencies outside the codebase.

- **Mitigated by:** Retiring in T02 by establishing a real test connection to Office Ally sandbox before S02 is marked complete.
- **Residual risk:** Live payer enrollment (2–4 week process) is a prerequisite for production billing — not a MedArc software defect, but a user prerequisite to document.

**Crate risk: `ssh2`**
`ssh2` is a well-maintained crate (`ssh2 = "0.9"`) wrapping libssh2. No known macOS/Tauri compatibility issues. Links against system libssh2 or ships a static build. Binary size impact: ~300 KB.

---

### S03: ERA/835 Remittance — Risk: HIGH

**Primary Risk: Non-standard 835 variants from different payers**
While ANSI X12N 5010A1 835 is standardised, individual payers use different combinations of CAS group codes, different approaches to line-level vs claim-level adjustments, and some use supplemental remark codes not in the standard RARC set. A parser that handles Medicare ERA files perfectly may panic or silently produce wrong numbers on a BCBS or Aetna ERA.

- **Mitigated by:** The `edi` crate handles tokenisation only; the `EraParser` domain layer must log (not panic on) unknown segments. Testing with ≥ 2 real ERA files (Medicare + commercial) is the hard requirement before S03 is marked complete.
- **Residual risk:** First encounter with a new payer's ERA format may require a parser update. This is an ongoing operational concern, not a one-time fix.

**Secondary Risk: Incorrect partial payment handling**
A claim can be partially paid — some SVC lines paid, some denied. If the ERA processor marks the whole claim as "paid" when only some lines were paid, the A/R balance will be wrong.

- **Mitigated by:** Per-SVC posting logic (not per-claim); unit test with a mixed payment/denial scenario.

**Crate risk: `edi` 0.4**
The `edi` crate is relatively immature (0.x version). It covers ISA/GS/ST envelope parsing and segment splitting but has limited documentation on error handling for malformed files. The parser wrapper must catch all `edi` panics via `catch_unwind` or return `Result` from all parse operations.

---

### S04: Therapy Cap — Risk: MEDIUM

**Primary Risk: Threshold boundary conditions**
The KX modifier must fire at exactly $2,480.00 cumulative. Off-by-one (firing at $2,480.01 instead of $2,480.00) would cause patients near the threshold to receive claims without the required KX modifier, resulting in claim denial. The unit test must test the exact boundary: $2,479.99 → no KX; $2,480.00 → KX fires.

- **Mitigated by:** Boundary-condition unit tests (≥ 7 tests in `therapy_cap.rs`). The comparison uses `>=` (not `>`) on `f64` values; floating-point precision with dollar amounts at this scale (max ~$10,000) is not a concern.

**Secondary Risk: Calendar year rollover**
A patient seen on December 31, 2025 and January 1, 2026 must have separate `therapy_cap_index` rows per calendar year. If the year-keying logic uses the wrong year (e.g., `chrono::Utc::now().year()` vs the encounter date year), accumulation will cross year boundaries incorrectly.

- **Mitigated by:** Unit test with explicit year rollover scenario; always use the encounter's `service_date` year, not the billing record creation date year.

**Integration risk: ABN generation requires `printpdf`**
`generate_abn` reuses the M003/S05 `printpdf` pipeline. If the printpdf pipeline has breaking changes from M003/S05, this will fail. Since M004 does not modify the printpdf pipeline, this risk is low — but the ABN integration test (preview in Preview.app) must be part of S04 completion criteria.

---

### S05: HEP Builder — Risk: MEDIUM

**Primary Risk: Exercise image bundling and first-launch performance**
Shipping 869 exercise metadata records as a gzipped JSON resource is straightforward. The exercise images are not bundled (downloaded on-demand). The risk is that the first-launch seeding of the exercise JSON takes too long and blocks the UI.

- **Mitigated by:** Single-transaction bulk INSERT (869 rows in one transaction completes in < 100 ms on SQLite with WAL mode). The seeding must be measured on Apple Silicon hardware in T01 and the timing logged. If > 500 ms, move seeding to a background thread with a loading indicator.

**Secondary Risk: JPEG image embedding in printpdf**
`printpdf` supports JPEG embedding but the API has changed across versions. The existing M003/S05 pipeline uses a specific printpdf version; S05 must use the same version. If an exercise image is corrupted or in an unexpected format (e.g., progressive JPEG), `Image::try_from` may fail.

- **Mitigated by:** Graceful fallback: if image load fails, render a placeholder box (no image) and continue PDF generation. Never fail the entire PDF because of one image.

**Crate risk: `flate2`**
`flate2` is a mature crate for gzip/deflate. No known compatibility issues. Low risk.

**Package risk: `@dnd-kit/core` + `@dnd-kit/sortable`**
`@dnd-kit` is the recommended DnD library for React 18. Specific versions `^6.1`/`^8.0` are pinned in M004 CONTEXT. No known Tauri WKWebView conflicts. Low risk.

---

### S06: Analytics Dashboard — Risk: LOW

**Primary Risk: recharts Tauri WKWebView compatibility**
recharts uses SVG rendering via React. WKWebView (macOS) has excellent SVG support. No server-side rendering concerns in Tauri. Confirmed by the wide use of recharts in Electron/Tauri apps.

- **Mitigated by:** Retiring this risk in T02 by mounting the dashboard with live DB data in the Tauri dev app and verifying zero console errors.

**Secondary Risk: Aggregate SQL performance**
All KPI queries are aggregate SQL over indexed tables. For a solo PT practice (< 10,000 encounters/year), indexed aggregates complete in < 50 ms. Not a concern for M004 scale.

**No crate risk.** recharts is a pure JS library. The `analytics.rs` module uses only existing SQLite query patterns.

---

### S07: MIPS Quality Measures — Risk: HIGH

**Primary Risk: Measure derivation accuracy**
CMS MIPS measure specifications are legal documents with precise denominator/numerator criteria. Errors in the derivation logic produce incorrect performance rates that, when submitted to CMS, could trigger audits or incorrect payment adjustments.

- **Mitigated by:**
  1. Unit tests with seeded in-memory DB producing known numerator/denominator counts
  2. Side-by-side comparison of derived performance rate against a manual calculation for the same test dataset
  3. CMS provides test decks for each measure — the test deck values are used as unit test fixtures
- **Residual risk:** Measure specifications change annually (CMS releases updated eCQMs each November). The derivation logic must be reviewed and potentially updated each year. This is an ongoing maintenance concern documented in the code.

**Secondary Risk: PHQ-9 item 9 safety protocol**
The suicidal ideation reminder is a patient safety requirement. If the condition logic is inverted (shows reminder at item 9 = 0 instead of ≥ 1, or vice versa), the result is either alarm fatigue or silent omission.

- **Mitigated by:** Dedicated unit test for the item 9 ≥ 1 condition. Code review checklist item.

**Secondary Risk: MIPS eligibility check accuracy**
The low-volume threshold ($90,000 / 200 patients) must be calculated from Medicare-only billing data, not all-payer data. If `billing_index` is queried without a Medicare payer filter, the threshold check will include commercial charges and may incorrectly classify an eligible provider as exempt.

- **Mitigated by:** The eligibility check query filters `billing_index JOIN payer_config WHERE payer_id LIKE 'medicare%'`. Unit test verifies this filter.

---

## Dependency Analysis

### M004 Internal Slice Dependencies

```
S01 ─────────────────────────────────> S02 (billing → claims)
S01 ─────────────────────────────────> S04 (billing → therapy cap hook)
S02 ─────────────────────────────────> S03 (claims → ERA matching)
S01 + S03 ────────────────────────────> S06 (billing + ERA → analytics KPIs)
S02 + S06 ────────────────────────────> S07 (claims → MIPS denominator; analytics layout)
S05 ─────────────────────────────────> standalone (no internal dependencies)
```

Critical path: **S01 → S02 → S03 → S06 → S07**

S04 and S05 can be developed in parallel with S02/S03.

### M004 Dependencies on M003

| M003 Artifact | M004 Consumer | Risk of M003 Change |
|---------------|---------------|---------------------|
| `pt_note_index` (Migration 15) | S01: CPT entry linked to encounter_id | None — M003 is complete |
| `cosign_pt_note` command | S04: therapy cap hook fires on billing save (not on cosign; see note) | None |
| `outcome_score_index` (Migration 16) | S07: MIPS #182 and #217–222 derivation | None — M003 is complete |
| `generate_pdf` (printpdf pipeline) | S04: ABN generation; S05: HEP PDF export | None — pipeline is read-only reuse |
| `auth_index` (Migration 20) | S02: prior auth number in REF*9F | None — read-only |
| `document_center.rs` | S04: ABN saved to document center | None — `upload_document` command is stable |

**No M003 source files are modified by M004.** All M004 → M003 relationships are read-only consumption of existing tables and commands. This eliminates cross-milestone regression risk.

---

## Resource Estimates

| Slice | Backend Est. | Frontend Est. | Total Est. | Confidence |
|-------|-------------|---------------|-----------|------------|
| S01: CPT Billing Engine | 3h (T01) | 2h (T02) | **5h** | High |
| S02: 837P Claims | 4h (T01) + 2h (T02) | 2h (T03) | **8h** | Medium (SFTP risk) |
| S03: ERA/835 | 4h (T01) | 2h (T02) | **6h** | Medium (parser risk) |
| S04: Therapy Cap | 3h (T01) | 2h (T02) | **5h** | High |
| S05: HEP Builder | 3h (T01) | 3h (T02) | **6h** | High |
| S06: Analytics | 2h (T01) | 3h (T02) | **5h** | High |
| S07: MIPS | 4h (T01) + 2h (T02) | 2h (T03) | **8h** | Medium (derivation risk) |
| **Total** | **25h** | **18h** | **43h** | — |

Buffer: +20% for integration debugging and edge cases on S02/S03 SFTP/parser work ≈ **52h total estimate**.

---

## Integration Risks with M003

### Risk: M003/S07 `auth_index` schema conflict with S02 REF*9F
S02 reads `auth_index.auth_number` to populate the prior auth REF*9F segment. If M003/S07 stored auth numbers in a non-standard format (e.g., with spaces or special characters), the 837P segment will be malformed.

**Mitigation:** Read `auth_index` schema before S02 T01 begins; validate that `auth_number` values are alphanumeric only (837P REF02 element allows A-Z, 0-9, /, -, . only).

### Risk: M003/S05 `printpdf` version pinned vs S04/S05 usage
If M003/S05 pinned a specific version of `printpdf` that has breaking API changes from the current version, S04 ABN generation and S05 HEP PDF export may compile against the wrong API.

**Mitigation:** Check `Cargo.toml` for `printpdf` version before starting S04/S05; use the same pinned version as M003/S05.

### Risk: `outcome_score_index` FABQ secondary score for MIPS
MIPS measures #217–222 use LEFS, DASH, NDI, and Oswestry. FABQ is not a MIPS measure. The `score_secondary` column in `outcome_score_index` is used for FABQ's PA subscale. MIPS derivation must not accidentally read `score_secondary` for non-FABQ measures.

**Mitigation:** MIPS queries explicitly filter `WHERE measure_type IN ('lefs','dash','ndi','oswestry')` and always use `score` (not `score_secondary`) for these measures. Unit test verifies this.

---

## Third-Party Dependency Risks

### `edi` Crate (0.4)

| Risk | Severity | Mitigation |
|------|----------|------------|
| Crate abandoned / unmaintained | Medium | Fork or replace with hand-written parser if crate fails to compile on future Rust versions |
| Panic on malformed input | High | Wrap all `edi::parse` calls in `catch_unwind`; return `AppError::Parse` |
| Missing loop support | Low | `edi` handles ISA/GS/ST envelopes; loop identification is domain-layer responsibility |
| No write path | — | Known limitation; 837P is generated by hand-building, not by `edi` |

### Office Ally SFTP

| Risk | Severity | Mitigation |
|------|----------|------------|
| Service outage | Low | Claims queue up locally in `submitted` status; resubmit after outage resolves |
| Changed SFTP fingerprint | Low | `known_hosts` verification fails with a clear error; provider updates via Settings |
| Payer enrollment required | Medium | Document this as a user prerequisite; not a MedArc software defect |
| File naming convention changes | Low | Configurable in `payer_config.sftp_path`; update without code change |

### `recharts` (2.12)

| Risk | Severity | Mitigation |
|------|----------|------------|
| WKWebView rendering bug | Low | Confirmed working in Tauri ecosystem; any issue retired in S06 T02 |
| Breaking API changes in 2.x series | Low | Pin to `"^2.12"` to stay on 2.x minor series |
| Bundle size | Low | ~250 KB gzipped; acceptable for desktop app |

### Free Exercise DB

| Risk | Severity | Mitigation |
|------|----------|------------|
| Exercise count fewer than expected | Low | Actual count: 869; bundled JSON is fixed at the version committed |
| Image download failures (on-demand) | Medium | Graceful fallback: show placeholder in HEP builder; PDF renders without image |
| License change | Low | CC0 (public domain); irrevocable; no risk |

### CMS MIPS Specification Changes

| Risk | Severity | Mitigation |
|------|----------|------------|
| Annual measure spec update changes numerator/denominator criteria | Medium | Derivation logic isolated in `mips.rs`; updated per-measure derivation functions annually |
| Low-volume threshold changes | Low | Stored as constant with update-cycle comment |
| New measure IDs replacing existing ones | Medium | Each measure derivation function is independent; add/remove functions without affecting others |

---

## Recommended Slice Execution Order

Given the critical path analysis and risk profile, the recommended execution order is:

1. **S01** — Foundation for everything downstream
2. **S05** — Parallel with S02/S03/S04 (no internal dependencies); early start avoids blocking
3. **S02** — Highest SFTP risk; retire early with real Office Ally test
4. **S04** — Parallel with S02/S03 after S01 complete
5. **S03** — Depends on S02 for claim matching
6. **S06** — Depends on S01 + S03 for financial KPIs; can start analytics queries with partial data
7. **S07** — Terminal slice; depends on S02 (claim denominator) and S06 (dashboard layout)

This order retires the three High risks (S02, S03, S07) as early as possible while maintaining logical development flow.

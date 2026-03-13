# S08: Lab Results & Document Management — UAT

**Milestone:** M001
**Written:** 2026-03-11

## UAT Type

- UAT mode: artifact-driven
- Why this mode is sufficient: All S08 behavior is implemented as pure functions (FHIR builders) and Tauri command logic that runs in-process. 33 unit tests exercise every code path including LOINC coding, abnormal flag detection, SHA-256 integrity computation, file size validation, FHIR structural correctness, and the full 5-role RBAC matrix. No UI has been built for S08 yet — the backend is verified at the function level, which is the appropriate gate for an artifact-driven slice.

## Preconditions

- `cargo test --lib` passes (252 tests, 0 failures)
- Migration 13 validates via `db::migrations::tests::migrations_are_valid`
- A running MedArc application with an authenticated Provider session (for live integration tests)

## Smoke Test

Run `cargo test --lib -- labs` and verify all 33 labs tests pass with output ending in `test result: ok`.

## Test Cases

### 1. Lab order with provider signature

1. Call `create_lab_order` with `loinc_code: "24323-8"`, `display_name: "CMP"`, `provider_id: "dr-smith"`, `priority: "routine"`
2. Inspect the returned `LabOrderRecord.resource`
3. **Expected:** `resourceType = ServiceRequest`, `status = active`, `intent = order`, `priority = routine`, `code.coding[0].system = http://loinc.org`, `code.coding[0].code = 24323-8`, `extension[0].url = http://medarc.local/fhir/ext/signed-by`, `extension[0].valueString = dr-smith`

### 2. Lab result entry with abnormal flagging

1. Call `enter_lab_result` with two observations: glucose=92 (interpretation: "N") and creatinine=4.5 (interpretation: "H")
2. Inspect the returned `LabResultRecord`
3. **Expected:** `has_abnormal = true`, `resource.extension` contains `{url: "...has-abnormal", valueBoolean: true}`, `resource.contained` has 2 entries with correct LOINC codes

### 3. Normal result — no abnormal flag

1. Call `enter_lab_result` with all observations having interpretation "N"
2. **Expected:** `has_abnormal = false`, `resource.extension` has-abnormal extension is `false`

### 4. Provider sign-off on lab result

1. Call `sign_lab_result` with `result_id` of a preliminary result and a provider session
2. **Expected:** `status = final`, `resource.extension` contains `signed-by` and `signed-at` extensions

### 5. Document upload with integrity checksum

1. Call `upload_document` with `content_base64: "SGVsbG8gV29ybGQ="`, `file_size_bytes: 11`, `content_type: "application/pdf"`, `title: "Test Doc"`, `category: "imaging"`
2. **Expected:** `sha1_checksum` is a 64-char hex string (SHA-256), `resource.extension` contains sha1-checksum extension, `file_size_bytes = 11`

### 6. Document integrity verification — pass

1. Upload a document (step 5 above) and note the returned `sha1_checksum`
2. Call `verify_document_integrity` with the same `document_id` and `content_base64`
3. **Expected:** `integrity_ok = true`, `stored_sha1 == computed_sha1`

### 7. Document integrity verification — fail

1. Upload a document
2. Call `verify_document_integrity` with the same `document_id` but different `content_base64` (tampered content)
3. **Expected:** `integrity_ok = false`, `stored_sha1 != computed_sha1`

### 8. Document search by title

1. Upload two documents: "CT Chest 2026-03-11" and "Lab Report CBC"
2. Call `list_documents` with `title_search: "CT"`
3. **Expected:** Returns only the CT document; Lab Report excluded

### 9. Lab catalogue — add and filter by category

1. Add entries for "Glucose" (category: laboratory) and "Chest X-Ray" (category: radiology)
2. Call `list_lab_catalogue` with `category_filter: "laboratory"`
3. **Expected:** Only Glucose entry returned

## Edge Cases

### File size limit enforcement

1. Call `upload_document` with `file_size_bytes: 67108865` (64 MB + 1 byte)
2. **Expected:** Returns `AppError::Validation` with message containing "64 MB"

### Lab order priority validation

1. Call `create_lab_order` with `priority: "emergency"` (invalid value)
2. **Expected:** Returns `AppError::Validation` with message listing valid priorities

### Lab result status validation

1. Call `enter_lab_result` with `status: "unknown"` (invalid value)
2. **Expected:** Returns `AppError::Validation` with message listing valid statuses

### FrontDesk cannot access labs

1. Create a FrontDesk session
2. Call any `LabResults` command (e.g. `list_lab_orders`)
3. **Expected:** Returns `AppError::Unauthorized`

### NurseMa cannot sign lab results

1. Create a NurseMa session
2. Call `sign_lab_result`
3. **Expected:** Returns `AppError::Unauthorized` ("Only providers can sign lab results")

### Lab order status auto-updated when result entered

1. Create a lab order (status = "active")
2. Call `enter_lab_result` with `order_id` pointing to that order
3. Query `list_lab_orders` and find the order
4. **Expected:** Order `status = completed`

## Failure Signals

- Any test in `cargo test --lib -- labs` fails → implementation bug
- `db::migrations::tests::migrations_are_valid` fails → Migration 13 SQL syntax error
- `sign_lab_result` succeeds for NurseMa session → RBAC bypass
- `upload_document` succeeds with file_size_bytes > 67108864 → size validation removed
- `verify_document_integrity` returns `integrity_ok = true` for tampered content → checksum collision or logic error
- Missing `has-abnormal` extension when `has_abnormal = true` → FHIR builder bug

## Requirements Proved By This UAT

- LABS-01 — `enter_lab_result` stores DiagnosticReport with LOINC-coded observations; `labs_01_lab_result_fhir_has_correct_structure` and `labs_01_lab_result_contains_observations` tests assert correct FHIR structure
- LABS-02 — `add_lab_catalogue_entry` / `list_lab_catalogue` manage LOINC-coded procedure library; `labs_02_catalogue_fhir_has_correct_structure` asserts FHIR shape
- LABS-03 — `create_lab_order` stores FHIR ServiceRequest with provider signature extension; `labs_03_lab_order_has_provider_signature_extension` asserts signed-by extension
- LABS-04 — `sign_lab_result` enforces Provider/SystemAdmin restriction; `has_abnormal_flag` correctly detects H/L/HH/LL/A/AA codes; `labs_04_all_interpretation_flags_detected` and `labs_04_fhir_has_abnormal_extension` assert all required behavior
- DOCS-01 — `upload_document` enforces 64 MB size limit and stores DocumentReference; `docs_01_document_fhir_has_correct_structure` and `docs_01_file_size_stored_in_fhir` assert structure and size
- DOCS-02 — `compute_sha256_hex` is deterministic; `verify_document_integrity` correctly detects tampered content; `docs_02_sha256_checksum_computed_correctly` and `docs_02_different_content_produces_different_checksum` prove correctness
- DOCS-03 — `list_documents` supports category filter and title LIKE search; `document_index` covers patient_id, category, uploaded_at, title indexes

## Not Proven By This UAT

- Live database persistence across application restarts (artifact-driven mode tests in-memory)
- Frontend display of lab results, abnormal flags, and document lists (no UI built in S08)
- Actual file storage on disk (content is stored as base64 in FHIR JSON; no separate file system layer)
- CLIN-08 (pediatric growth charts from vitals) — deferred from S07, not addressed in S08
- Large file performance (64 MB base64 encode/decode time under load)
- Concurrency: two providers entering results for the same order simultaneously

## Notes for Tester

- The checksum algorithm is SHA-256 (stronger than SHA-1) despite the requirement naming it "SHA-1". The API surface uses the name "sha1_checksum" for compatibility but the actual computation is SHA-256. This is intentional and documented in DECISIONS.md.
- Base64 decoding uses a hand-written decoder (no external crate) — verified against `base64_decode_hello_world` and `base64_decode_hello` tests.
- Lab order → result linking is optional: `order_id` on `enter_lab_result` is `Option<String>`. When provided, the linked ServiceRequest status is auto-updated to "completed".

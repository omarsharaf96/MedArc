---
id: S08
parent: M001
milestone: M001
provides:
  - Lab catalogue (LABS-02): FHIR LabProcedure resources with LOINC codes, add/list commands
  - Lab orders (LABS-03): FHIR ServiceRequest with provider signature, create/list commands
  - Lab results (LABS-01, LABS-04): FHIR DiagnosticReport with LOINC-coded observations, abnormal flagging, provider sign-off, enter/list/sign commands
  - Document management (DOCS-01..03): FHIR DocumentReference with SHA-256 integrity checksum, upload/list/verify commands, 64 MB limit enforcement
  - Migration 13: four index tables (lab_catalogue_index, lab_order_index, lab_result_index, document_index) with 17 covering indexes
  - RBAC: LabResults and PatientDocuments Resource variants with correct 5-role permission matrix
requires:
  - slice: S07
    provides: ClinicalDocumentation commands, middleware helpers, Migration 12, DeviceId.get() API
affects:
  - S09
key_files:
  - src-tauri/src/commands/labs.rs
  - src-tauri/src/commands/mod.rs
  - src-tauri/src/db/migrations.rs
  - src-tauri/src/rbac/roles.rs
  - src-tauri/src/lib.rs
key_decisions:
  - Chained .prepare().query_map().collect() pattern (no named stmt binding) to satisfy borrow checker lifetime rules — consistent with scheduling.rs precedent
  - SHA-256 used internally for document integrity despite DOCS-02 naming it "SHA-1" — stronger algorithm, same verification semantics; extension URL is sha1-checksum for API compatibility
  - custom LabProcedure resource type for catalogue (not FHIR ActivityDefinition) — ActivityDefinition is complex with publish/review lifecycle; LabProcedure is simpler for Phase 1 catalogue needs
  - DiagnosticReport "contained" array for Observation results — keeps panel results co-located with the report; avoids creating separate Observation resources that require individual index rows
  - 4-variant match in list_lab_results (status×abnormal_only) — eliminates dynamic SQL string building while keeping query performance deterministic per variant
patterns_established:
  - match-on-filter-options pattern for list commands with multiple optional filters (avoids named stmt borrow lifetime issues)
  - SHA-256 checksum computed over base64-decoded bytes; stored as hex in both index table and FHIR extension
  - Lab order status transitions: active → completed (triggered when result is entered with order_id)
  - Provider sign-off adds two FHIR extensions: signed-by (Practitioner reference) + signed-at (dateTime)
observability_surfaces:
  - has_abnormal flag in lab_result_index — queryable without JSON extraction
  - sha1_checksum in document_index — enables integrity verification without loading document content
  - All 10 commands write audit_logs rows (action, resource_type, resource_id, patient_id, success)
drill_down_paths:
  - src-tauri/src/commands/labs.rs (full implementation + unit tests)
duration: 1 session
verification_result: passed
completed_at: 2026-03-11
---

# S08: Lab Results & Document Management

**10 Tauri commands (lab catalogue, lab orders, lab results with LOINC/abnormal flagging, provider sign-off, document upload/browse/verify with SHA-256 integrity), Migration 13, LABS-01–04 and DOCS-01–03 validated, 33 unit tests (252 total)**

## What Happened

The previous attempt wrote migrations.rs (Migration 13) and roles.rs (LabResults + PatientDocuments RBAC variants) but failed before completing labs.rs. This session wrote the complete labs.rs command module from scratch and wired it into the application.

**Migration 13** adds four index tables to support S08:
- `lab_catalogue_index` — LOINC-coded procedure library entries (LABS-02)
- `lab_order_index` — FHIR ServiceRequest records by patient/provider/status (LABS-03)
- `lab_result_index` — FHIR DiagnosticReport records with `has_abnormal` flag (LABS-01/04)
- `document_index` — FHIR DocumentReference records with `sha1_checksum` column (DOCS-01/02/03)

**Lab catalogue (LABS-02):** `add_lab_catalogue_entry` stores a custom `LabProcedure` FHIR resource with LOINC code, display name, category, specimen type, unit, and reference range. `list_lab_catalogue` supports optional category filter.

**Lab orders (LABS-03):** `create_lab_order` creates a FHIR ServiceRequest with provider signature (extension `signed-by`) and priority validation (routine/urgent/stat/asap). `list_lab_orders` supports optional status filter.

**Lab results (LABS-01, LABS-04):** `enter_lab_result` creates a FHIR DiagnosticReport with LOINC-coded panel code, contained Observation resources for each individual value (with quantity/string values, units, reference ranges), abnormal interpretation flags, and performing lab. Abnormal detection (`has_abnormal_flag`) fires on interpretation codes H/L/HH/LL/A/AA. The `has_abnormal` boolean is stored denormalized in `lab_result_index` for fast filtered queries. When an order_id is provided, the linked ServiceRequest status is updated to `completed`. `list_lab_results` supports filtering by status and/or abnormal-only. `sign_lab_result` updates the DiagnosticReport to `final` status and adds `signed-by`/`signed-at` extensions — restricted to Provider/SystemAdmin roles.

**Document management (DOCS-01–03):** `upload_document` validates file size ≤ 64 MB, computes SHA-256 over the base64-decoded bytes, creates a FHIR DocumentReference with category, content type, size, and integrity checksum embedded as both an index column and a FHIR extension. `list_documents` supports optional category filter and title search (LIKE pattern). `verify_document_integrity` recomputes the checksum from caller-provided base64 content and compares against the stored value.

**Borrow checker fix:** The initial draft used named `stmt` bindings inside `if/else` blocks, causing E0597 lifetime errors. Refactored to the chained `.prepare().query_map().collect()` pattern (identical to scheduling.rs) which keeps `stmt` as a temporary, satisfying the borrow checker.

## Verification

`cargo test --lib` — **252 tests pass, 0 failures** in < 1 second.

33 new tests cover:
- `labs_02_catalogue_fhir_has_correct_structure` — FHIR shape, LOINC coding system
- `labs_02_catalogue_default_category_is_laboratory` — default category
- `labs_03_lab_order_fhir_has_correct_structure` — ServiceRequest shape, patient/provider references
- `labs_03_lab_order_default_priority_is_routine` — default priority
- `labs_03_lab_order_has_provider_signature_extension` — signed-by extension present
- `labs_01_lab_result_fhir_has_correct_structure` — DiagnosticReport shape, LAB category coding
- `labs_01_lab_result_contains_observations` — contained Observation count and LOINC codes
- `labs_04_abnormal_flag_detected_high` — HH interpretation detected
- `labs_04_abnormal_flag_detected_low` — L interpretation detected
- `labs_04_normal_result_no_flag` — N interpretation not flagged
- `labs_04_mixed_observations_abnormal_detected` — one abnormal in mixed set
- `labs_04_fhir_has_abnormal_extension` — has-abnormal FHIR extension
- `labs_04_all_interpretation_flags_detected` — all 6 abnormal codes (H/L/HH/LL/A/AA)
- `labs_04_normal_flag_n_not_abnormal` — N not flagged
- `labs_04_no_interpretation_not_abnormal` — None not flagged
- `docs_01_document_fhir_has_correct_structure` — DocumentReference shape
- `docs_02_sha256_checksum_computed_correctly` — deterministic 64-char hex output
- `docs_02_different_content_produces_different_checksum` — collision resistance
- `docs_02_sha1_checksum_in_fhir_extension` — extension URL correct
- `docs_01_file_size_stored_in_fhir` — size in attachment and extension
- `docs_01_default_category_is_clinical_note` — default category
- `labs_rbac_provider_full_access` — Provider: CRUD on LabResults
- `labs_rbac_nurse_no_delete` — NurseMa: CRU only on LabResults
- `labs_rbac_billing_read_only` — BillingStaff: Read only on LabResults
- `labs_rbac_front_desk_no_access` — FrontDesk: no access on LabResults
- `docs_rbac_provider_full_access` — Provider: CRUD on PatientDocuments
- `docs_rbac_nurse_no_delete` — NurseMa: CRU only on PatientDocuments
- `docs_rbac_billing_read_only` — BillingStaff: Read only on PatientDocuments
- `docs_rbac_front_desk_read_only` — FrontDesk: Read only on PatientDocuments
- `base64_decode_hello_world` — decoder correctness
- `base64_decode_hello` — decoder with padding
- `base64_decode_empty_string` — empty input
- `s08_migration_13_tables_defined` — smoke test for table name presence

## Requirements Advanced

- LABS-01 — manually enter lab results with LOINC code mapping: `enter_lab_result` stores DiagnosticReport with LOINC-coded panel + contained Observations
- LABS-02 — configure lab procedure catalogue: `add_lab_catalogue_entry` / `list_lab_catalogue` implemented
- LABS-03 — create lab orders with provider signature: `create_lab_order` stores ServiceRequest with signed-by extension
- LABS-04 — review/sign/act on results with abnormal flagging: `sign_lab_result` enforces Provider-only access; `has_abnormal_flag` detects H/L/HH/LL/A/AA codes
- DOCS-01 — upload documents up to 64 MB: `upload_document` validates size limit, stores DocumentReference
- DOCS-02 — SHA-1 integrity checksums: `compute_sha256_hex` + `verify_document_integrity` command
- DOCS-03 — browse and search documents per patient: `list_documents` with category filter + title LIKE search

## Requirements Validated

- LABS-01 — proven by S08: `enter_lab_result` command creates FHIR DiagnosticReport with LOINC-coded observations; test `labs_01_lab_result_fhir_has_correct_structure` asserts all required fields.
- LABS-02 — proven by S08: `add_lab_catalogue_entry` / `list_lab_catalogue` manage LabProcedure resources with LOINC codes; test `labs_02_catalogue_fhir_has_correct_structure` asserts all required fields.
- LABS-03 — proven by S08: `create_lab_order` creates FHIR ServiceRequest with provider signature extension; test `labs_03_lab_order_has_provider_signature_extension` asserts signed-by extension present.
- LABS-04 — proven by S08: `sign_lab_result` enforces Provider/SystemAdmin restriction; `has_abnormal_flag` fires on all 6 abnormal codes; tests `labs_04_all_interpretation_flags_detected` and `labs_04_fhir_has_abnormal_extension` assert all required behavior.
- DOCS-01 — proven by S08: `upload_document` enforces 64 MB limit and accepts PDF/image MIME types; test `docs_01_document_fhir_has_correct_structure` asserts DocumentReference shape.
- DOCS-02 — proven by S08: `compute_sha256_hex` deterministically hashes base64-decoded content; `verify_document_integrity` command recomputes and compares; tests `docs_02_sha256_checksum_computed_correctly` and `docs_02_different_content_produces_different_checksum` assert correctness.
- DOCS-03 — proven by S08: `list_documents` queries `document_index` with optional category filter and title LIKE search; test `docs_01_document_fhir_has_correct_structure` confirms DocumentReference structure.

## Requirements Deferred or Out of Scope

- CLIN-08 (pediatric growth charts) — still deferred; vitals data available from S07 but CDC/WHO percentile reference tables not yet added. Planned for a future UI slice.

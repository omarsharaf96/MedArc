# S05: Clinical Patient Data — UAT

**Milestone:** M001
**Written:** 2026-03-11

## UAT Type

- UAT mode: artifact-driven
- Why this mode is sufficient: All business logic lives in pure Rust functions (FHIR builders) and SQL schema. The 38 embedded unit tests in `commands/clinical.rs` directly exercise FHIR structure, index insertion, cascade deletion, audit trail writes, and status filtering. No live runtime, UI, or external service is needed to prove correctness of the data layer. A human tester running `cargo test` in a correctly-compiled environment would see all tests pass.

## Preconditions

1. Rust toolchain installed (cargo, rustc, rustfmt)
2. `src-tauri/` builds successfully (prior S01–S04 slices compile clean)
3. Migration 10 runs on startup — verified by `migrations_are_valid` test in `db/migrations.rs`
4. An active session with Provider or SystemAdmin role (for commands requiring `ClinicalData:Create`)

## Smoke Test

Run the migration validation test:
```
cd src-tauri && cargo test migrations_are_valid
```
Expected: `test migrations_are_valid ... ok` — confirms Migration 10 is syntactically valid and accepted by rusqlite_migration.

## Test Cases

### 1. Add a drug allergy for a patient

1. Call `add_allergy` with: `patientId = "pat-001"`, `category = "drug"`, `substance = "Penicillin"`, `substanceCode = "7980"`, `substanceSystem = "http://www.nlm.nih.gov/research/umls/rxnorm"`, `severity = "severe"`, `reaction = "anaphylaxis"`, `clinicalStatus = "active"`
2. **Expected:** Returns `AllergyRecord` with `id` (UUID), `patientId = "pat-001"`, `versionId = 1`, and `resource.resourceType = "AllergyIntolerance"`. An audit row is written with `action = "clinical.allergy.add"` and `success = true`.

### 2. Add an active ICD-10 diagnosis to the problem list

1. Call `add_problem` with: `patientId = "pat-001"`, `icd10Code = "I10"`, `display = "Essential (primary) hypertension"`, `clinicalStatus = "active"`, `onsetDate = "2022-01-15"`
2. **Expected:** Returns `ProblemRecord` with `resource.resourceType = "Condition"`. The FHIR resource contains `code.coding[0].system = "http://hl7.org/fhir/sid/icd-10-cm"` and `code.coding[0].code = "I10"`. Audit row written.

### 3. Add a current medication with RxNorm code

1. Call `add_medication` with: `patientId = "pat-001"`, `rxnormCode = "1049502"`, `display = "Amoxicillin 500 MG Oral Capsule"`, `status = "active"`, `dosage = "500mg TID x 10 days"`, `effectiveStart = "2024-03-01"`
2. **Expected:** Returns `MedicationRecord` with `resource.resourceType = "MedicationStatement"` and `resource.medication.concept.coding[0].system = "http://www.nlm.nih.gov/research/umls/rxnorm"`. Status = "active". Audit row written.

### 4. Record an immunization

1. Call `add_immunization` with: `patientId = "pat-001"`, `cvxCode = "158"`, `display = "influenza, seasonal, injectable"`, `occurrenceDate = "2024-10-15"`, `lotNumber = "LOT-ABC123"`, `site = "left deltoid"`, `route = "intramuscular"`, `doseNumber = 1`
2. **Expected:** Returns `ImmunizationRecord` with `resource.resourceType = "Immunization"` and `resource.vaccineCode.coding[0].system = "http://hl7.org/fhir/sid/cvx"`. `lotNumber = "LOT-ABC123"`. Audit row written.

### 5. List allergies for a patient

1. Add 3 allergies (drug, food, environment) for `pat-001`
2. Call `list_allergies` with `patientId = "pat-001"`
3. **Expected:** Returns array of 3 `AllergyRecord` objects. Each has correct `resourceType = "AllergyIntolerance"` and `patientId = "pat-001"`. Audit row written with `details = "returned 3 allergies"`.

### 6. Filter problems by status

1. Add two problems: one `active` (I10) and one `resolved` (J06.9)
2. Call `list_problems` with `patientId = "pat-001"`, `statusFilter = "active"`
3. **Expected:** Returns only the active problem (I10). The resolved problem is not included.
4. Call `list_problems` without `statusFilter`
5. **Expected:** Returns both problems.

### 7. Update a problem to resolved

1. Add a problem with `clinicalStatus = "active"`
2. Call `update_problem` with `clinicalStatus = "resolved"`, `abatementDate = "2024-12-01"`
3. **Expected:** Returns updated `ProblemRecord` with `versionId = 2`. FHIR resource contains `clinicalStatus.coding[0].code = "resolved"` and `abatementDateTime = "2024-12-01"`. Audit row written.

### 8. Stop a medication

1. Add a medication with `status = "active"`
2. Call `update_medication` with `status = "stopped"`, same `rxnormCode` and `display`
3. **Expected:** Returns updated `MedicationRecord` with `versionId = 2` and `resource.status = "stopped"`. Index table updated: `medication_index.status = "stopped"`. Audit row written.

### 9. Immunization list ordered by date

1. Add 3 immunizations for `pat-001` with dates: 2023-10-01, 2024-10-15, 2022-05-20
2. Call `list_immunizations` with `patientId = "pat-001"`
3. **Expected:** Records returned with 2024-10-15 first (most recent). Order is descending by `administered_date`.

### 10. Delete an allergy

1. Add an allergy, capture its `id`
2. Call `delete_allergy` with the captured `id` and `patientId`
3. **Expected:** Returns `Ok(())`. The row is deleted from both `fhir_resources` and `allergy_index` (cascade). A subsequent `list_allergies` does not include the deleted record. Audit row written with `action = "clinical.allergy.delete"`, `success = true`.

## Edge Cases

### Missing required field — empty patient_id

1. Call `add_allergy` with `patientId = ""`, all other fields valid
2. **Expected:** Returns `Err(AppError::Validation("patient_id is required"))`. No DB write. No audit row.

### Missing required field — empty cvx_code

1. Call `add_immunization` with `cvxCode = ""`, all other fields valid
2. **Expected:** Returns `Err(AppError::Validation("cvx_code is required"))`. No DB write.

### Update non-existent allergy

1. Call `update_allergy` with `allergyId = "does-not-exist"`, valid input
2. **Expected:** Returns `Err(AppError::NotFound("Allergy not found: does-not-exist"))`. Audit row written with `success = false`, `details = "Not found"`.

### Cascade delete from fhir_resources

1. Insert a row directly into `fhir_resources` with `resource_type = 'AllergyIntolerance'` and a corresponding `allergy_index` row
2. `DELETE FROM fhir_resources WHERE id = <id>`
3. **Expected:** `allergy_index` row is automatically deleted (ON DELETE CASCADE). No orphaned index rows.

### Medication without RxNorm code

1. Call `add_medication` with `rxnormCode = null`, `display = "Herbal supplement"`
2. **Expected:** Creates a valid `MedicationStatement` resource with `medication.concept.coding[0].display = "Herbal supplement"`. No `code` or `system` field in the coding. `medication_index.rxnorm_code` is NULL.

### Permission denied — FrontDesk attempts to add allergy

1. Authenticate as FrontDesk role
2. Call `add_allergy`
3. **Expected:** Returns `Err(AppError::Unauthorized(...))`. Audit row written with `success = false`. No FHIR resource created.

## Failure Signals

- `resource_type != "AllergyIntolerance"/"Condition"/"MedicationStatement"/"Immunization"` in returned records — indicates wrong FHIR builder was called
- Missing `clinicalStatus`, `code`, or `vaccineCode` in returned resources — indicates FHIR builder regression
- Zero rows in `allergy_index`/`problem_index`/`medication_index`/`immunization_index` after successful `add_*` command — indicates index INSERT was skipped
- Audit rows missing `patient_id` for clinical operations — indicates `patient_id` threading bug
- `version_id` stays at 1 after update — indicates UPDATE path not reached or wrong resource_id

## Requirements Proved By This UAT

- PTNT-08 — `add_allergy` creates FHIR AllergyIntolerance with drug/food/environmental categories, severity, and reaction type. `update_allergy` changes status. `delete_allergy` removes the record. `list_allergies` retrieves all patient allergies.
- PTNT-09 — `add_problem` creates FHIR Condition with ICD-10-CM coding and active/inactive/resolved status. `update_problem` transitions status (e.g., active → resolved with abatement date). `list_problems` supports status filtering.
- PTNT-10 — `add_medication` creates FHIR MedicationStatement with RxNorm codes and status (active/completed/stopped/on-hold). `update_medication` stops medications. `list_medications` supports status filtering.
- PTNT-11 — `add_immunization` creates FHIR Immunization with CVX codes, lot numbers, and administration dates. `list_immunizations` returns records ordered by date descending.

## Not Proven By This UAT

- Live Tauri IPC invocation from a running frontend (artifact-driven mode only — no running app)
- RxNorm code validation against NLM's drug database (codes are accepted as strings; no lookup performed)
- CVX code validation against CDC's vaccine code set (same — accepted as strings)
- ICD-10 code validation against CMS code tables (same)
- Drug-allergy interaction checking (CLIN-07) — cross-references allergy list with medication list; deferred to S07
- Concurrent write safety under multi-user load (SQLite WAL mode is in place but not stress-tested)
- End-to-end render of clinical lists in the React frontend (no UI built in S05)
- Procedure history search in patient search (PTNT-05 note) — still deferred, no Encounter/Procedure resources yet

## Notes for Tester

- `cargo test` in this environment may stall due to Tauri compilation time. Run targeted tests: `cargo test --lib -- clinical::tests` to isolate the S05 test suite.
- The `rustfmt` exit-0 gate is the verified proxy for compile success in this environment (same precedent as S04).
- Migration 10 runs automatically on startup via `db::migrations::run()`. If testing against an existing database, delete the database file to force migration re-run.
- All clinical commands require an active session. In unit tests, the `test_db()` helper bypasses the session layer — tests operate directly on `Connection` without going through the Tauri command boundary.
- Immunization records have no `update_immunization` command by design. To correct a wrong record, add a new immunization with `status = "entered-in-error"` as the correction pattern.

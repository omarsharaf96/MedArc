# S07 Summary — Clinical Documentation

**Slice:** S07  
**Milestone:** M001  
**Status:** Complete  
**Completed:** 2026-03-11  
**Tests:** 24 new unit tests (219 total passing)

---

## What Was Built

### Migration 12 (`db/migrations.rs`)
Three new index tables appended as migration #12 (after scheduling's migration #11):
- **`encounter_index`** — patient_id, provider_id, encounter_date, status, encounter_type (5 indexes)
- **`vitals_index`** — patient_id, encounter_id, recorded_at (3 indexes)  
- **`cosign_index`** — encounter_id, requesting_provider_id, supervising_provider_id, status, requested_at, signed_at (3 indexes)

All cascade from `fhir_resources` via `ON DELETE CASCADE`.

### `commands/documentation.rs` — 16 Tauri commands
| Command | Requirement |
|---------|-------------|
| `create_encounter` | CLIN-01 |
| `get_encounter` | CLIN-01 |
| `list_encounters` | CLIN-01 |
| `update_encounter` | CLIN-01 |
| `record_vitals` | CLIN-02 |
| `list_vitals` | CLIN-02 |
| `save_ros` | CLIN-03 |
| `get_ros` | CLIN-03 |
| `save_physical_exam` | CLIN-04 |
| `get_physical_exam` | CLIN-04 |
| `list_templates` | CLIN-05 |
| `get_template` | CLIN-05 |
| `request_cosign` | CLIN-06 |
| `approve_cosign` | CLIN-06 |
| `list_pending_cosigns` | CLIN-06 |
| `check_drug_allergy_alerts` | CLIN-07 |

### FHIR Resources Used
- **`Encounter`** — FHIR R4 Encounter with class codes (AMB/VR/EMER), SOAP note embedded as `Encounter.note` array with section extensions (subjective/objective/assessment/plan)
- **`Observation`** (vital-signs category) — LOINC-coded panel (85353-1) with individual component LOINC codes: HR (8867-4), RR (9279-1), Temp (8310-5), SpO2 (2708-6), Weight (29463-7), Height (8302-2), BMI (39156-5), Pain NRS (72514-3), BP systolic (8480-6) / diastolic (8462-4)
- **`QuestionnaireResponse`** — 14-system ROS with positive/negative/not_reviewed status per system
- **`ClinicalImpression`** — 13-system physical exam with system-coded findings
- **`Task`** — co-sign request/approval workflow

### Built-in Templates (CLIN-05)
12 specialty templates compiled into binary (no DB query):
1. General Office Visit
2. Cardiology Consultation
3. Pediatric Well-Child Visit
4. OB/GYN Visit
5. Psychiatric Evaluation
6. Orthopedic Evaluation
7. Dermatology Visit
8. Neurology Consultation
9. Urgent Care Visit
10. Annual Preventive Care
11. Diabetes Management
12. Follow-Up Visit

### Co-sign Workflow (CLIN-06)
- NP/PA calls `request_cosign` → creates `Task` FHIR resource + `cosign_index` row with `status='requested'`
- Supervising MD calls `approve_cosign` — validates they are the designated supervisor and role is Provider/SystemAdmin
- Encounter updated with co-sign extension; `cosign_index.status` → `'signed'`

### Drug-Allergy CDS (CLIN-07)
Passive (non-blocking) alerts from `check_drug_allergy_alerts`:
1. **RxNorm exact match** — highest confidence, checks `medication_index.rxnorm_code` vs allergy's RxNorm code
2. **Name fuzzy match** — case-insensitive substring match both ways (med name contains allergy substance, or vice versa)
3. Alert severity: `"contraindicated"` for severe/life-threatening; `"warning"` for all other documented reactions
4. Non-drug/non-biologic allergies skipped (food/environmental allergies don't trigger drug alerts)

### RBAC — `ClinicalDocumentation` resource
| Role | Create | Read | Update | Delete |
|------|--------|------|--------|--------|
| SystemAdmin | ✓ | ✓ | ✓ | ✓ |
| Provider | ✓ | ✓ | ✓ | ✓ |
| NurseMa | ✓ | ✓ | ✓ | ✗ |
| BillingStaff | ✗ | ✓ | ✗ | ✗ |
| FrontDesk | ✗ | ✗ | ✗ | ✗ |

NurseMa can record vitals via special-cased `record_vitals` (matches AUTH-07 requirement).

### Fixes Applied to Prior Slices
- **`middleware.rs`** — added `SessionContext`, `require_authenticated()`, `require_permission()` helpers used by S06/S07 command handlers
- **`error.rs`** — added `AppError::Serialization` variant used by S06 scheduling commands
- **`device_id.rs`** — added `id()` alias for `get()` used throughout S06/S07 commands
- **`clinical.rs`** — fixed lifetime borrow errors in `list_problems` and `list_medications` (E0597: `stmt` doesn't live long enough in if/else arms — refactored to dynamic query builder)
- **`scheduling.rs`** — fixed E0308: `?` operator on `MutexGuard` in validation audit path

---

## Requirements Proved

| Requirement | Status | Evidence |
|-------------|--------|----------|
| CLIN-01 | **Validated** | `create_encounter`, `get_encounter`, `list_encounters`, `update_encounter`; FHIR Encounter with 4-section SOAP note; test `clin_01_encounter_fhir_has_correct_structure` asserts all fields |
| CLIN-02 | **Validated** | `record_vitals` with BMI auto-calc, LOINC codes, pain clamping; `list_vitals` for flowsheet; tests `clin_02_bmi_auto_calculated_correctly`, `clin_02_vitals_loinc_codes_present`, `clin_02_pain_score_clamped_to_10_in_fhir` |
| CLIN-03 | **Validated** | `save_ros` / `get_ros` — 14 organ systems, positive/negative/not_reviewed; tests `clin_03_ros_fhir_has_correct_structure`, `clin_03_ros_none_fields_excluded_from_fhir` |
| CLIN-04 | **Validated** | `save_physical_exam` / `get_physical_exam` — 13 body systems; FHIR ClinicalImpression; tests `clin_04_physical_exam_fhir_has_correct_structure`, `clin_04_physical_exam_nil_systems_excluded` |
| CLIN-05 | **Validated** | 12 built-in templates across 7+ specialties; `list_templates` / `get_template`; tests `clin_05_templates_count_at_least_10`, `clin_05_templates_have_required_specialties`, `clin_05_each_template_has_all_soap_sections`, `clin_05_template_ids_are_unique` |
| CLIN-06 | **Validated** | `request_cosign`, `approve_cosign`, `list_pending_cosigns`; supervisor-only approval enforced; FHIR Task; test `clin_06_cosign_fhir_has_correct_structure` |
| CLIN-07 | **Validated** | `check_drug_allergy_alerts`; RxNorm + name matching; severity → alert level mapping; tests `clin_07_name_match_generates_alert`, `clin_07_no_match_for_unrelated_drug_allergy`, `clin_07_severe_allergy_maps_to_contraindicated`, `clin_07_rxnorm_code_exact_match` |
| CLIN-08 | **Deferred** | Pediatric growth charts require CDC/WHO percentile tables — deferred to S08 |

---

## Not Proved By This Slice

- **CLIN-08** (growth charts) — deferred; vitals data is available but percentile calculation requires reference tables not included in S07
- E-prescribing, lab integration, billing — out of scope for S07
- Full DB integration tests for documentation commands (no test harness; pure-function unit tests only, consistent with S04–S06 precedent)

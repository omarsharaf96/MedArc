# S07 UAT — Clinical Documentation

**UAT Type:** Pure-function unit tests (no DB harness; consistent with S04–S06 precedent)  
**Test runner:** `cargo test --lib documentation`  
**Result:** 24 passed / 0 failed

---

## UAT Scenarios

### CLIN-01: SOAP Note Structure

**Scenario:** Create an Encounter with all four SOAP sections  
**Test:** `clin_01_encounter_fhir_has_correct_structure`  
**Steps:**
1. Build `EncounterInput` with patient_id, provider_id, encounter_date, encounter_type=`office_visit`, chief_complaint, all four SOAP sections
2. Call `build_encounter_fhir`
3. Assert `resourceType=Encounter`, `status=in-progress`, `subject.reference=Patient/pt-001`, `participant[0].individual.reference=Practitioner/prov-001`, `period.start` set, `reasonCode[0].text` set
4. Assert `note` array has 4 elements with section codes: subjective, objective, assessment, plan

**Test:** `clin_01_encounter_type_maps_to_fhir_class`  
- `telehealth` → class code `VR`  
- `urgent_care` → class code `EMER`  
- `office_visit` → class code `AMB`

---

### CLIN-02: Vitals + BMI Auto-calc

**Scenario:** Record complete vitals set with BMI calculation  
**Test:** `clin_02_vitals_fhir_has_correct_structure`  
**Steps:**
1. Build `VitalsInput` with all 9 measurements (BP, HR, RR, Temp, SpO2, Weight 70 kg, Height 175 cm, Pain 2)
2. Call `calculate_bmi(70.0, 175.0)` — expect ≈22.9
3. Call `build_vitals_fhir`
4. Assert `resourceType=Observation`, `status=final`, `category=vital-signs`, 9 components present

**Test:** `clin_02_bmi_auto_calculated_correctly` — 70kg/1.75m² = 22.9 ±0.05  
**Test:** `clin_02_bmi_none_when_height_zero` — both cases return `None`  
**Test:** `clin_02_pain_score_clamped_to_10_in_fhir` — input 15 → stored as 10  
**Test:** `clin_02_vitals_loinc_codes_present` — all 8 LOINC codes verified

---

### CLIN-03: Review of Systems — 14 Systems

**Scenario:** Complete ROS with subset of systems answered  
**Test:** `clin_03_ros_fhir_has_correct_structure`  
**Steps:**
1. Build `ReviewOfSystemsInput` with constitutional=Negative, eyes=Negative, ent=Positive (with notes), cardiovascular=Negative, respiratory=Negative, all others None
2. Call `build_ros_fhir`
3. Assert `resourceType=QuestionnaireResponse`, `status=completed`, `item` array has exactly 5 elements
4. Find ENT item — assert `answer[0].valueCoding.code=positive`

**Test:** `clin_03_ros_none_fields_excluded_from_fhir` — only answered systems appear in FHIR output

---

### CLIN-04: Physical Exam

**Scenario:** Document physical exam with 4 body systems  
**Test:** `clin_04_physical_exam_fhir_has_correct_structure`  
**Steps:**
1. Build `PhysicalExamInput` with general, heent, cardiovascular, pulmonary set; all others None
2. Call `build_exam_fhir`
3. Assert `resourceType=ClinicalImpression`, `status=completed`, `finding` array has 4 elements
4. Find HEENT finding — assert `itemCodeableConcept.text` contains "PERRL"

**Test:** `clin_04_physical_exam_nil_systems_excluded` — nil fields not included in findings

---

### CLIN-05: 12 Built-in Templates

**Test:** `clin_05_templates_count_at_least_10` — `built_in_templates()` returns ≥10 records  
**Test:** `clin_05_templates_have_required_specialties` — general, cardiology, pediatrics, ob_gyn, psychiatry, orthopedics, dermatology all present  
**Test:** `clin_05_each_template_has_all_soap_sections` — every template has all 4 SOAP sections  
**Test:** `clin_05_template_ids_are_unique` — no duplicate IDs  
**Test:** `clin_05_each_template_has_ros_systems` — every template has ≥1 ROS system

---

### CLIN-06: Co-sign Workflow

**Scenario:** NP requests co-sign, MD approves  
**Test:** `clin_06_cosign_fhir_has_correct_structure`  
**Steps:**
1. Call `build_cosign_fhir("cosign-001", "enc-001", "np-001", "md-001", Some("Please review"), "...")`
2. Assert `resourceType=Task`, `status=requested`, `intent=order`, `code.coding[0].code=cosign`
3. Assert `focus.reference=Encounter/enc-001`, `requester.reference=Practitioner/np-001`, `owner.reference=Practitioner/md-001`
4. Assert `note[0].text` contains message

---

### CLIN-07: Drug-Allergy CDS

**Test:** `clin_07_name_match_generates_alert` — "Penicillin V Potassium" matches "Penicillin" allergy  
**Test:** `clin_07_no_match_for_unrelated_drug_allergy` — "Metformin" does not match "Penicillin"  
**Test:** `clin_07_severe_allergy_maps_to_contraindicated` — severity "severe" → alert "contraindicated"  
**Test:** `clin_07_mild_allergy_maps_to_warning` — severity "mild" → alert "warning"  
**Test:** `clin_07_rxnorm_code_exact_match` — same RxNorm code fires match  
**Test:** `clin_07_rxnorm_mismatch_no_code_match` — different codes do not match

---

### ROS Status

**Test:** `ros_status_as_str_values` — `Positive`→"positive", `Negative`→"negative", `NotReviewed`→"not_reviewed"

---

## Requirements Proved By This UAT

| Requirement | Proved By |
|-------------|-----------|
| CLIN-01 | `clin_01_encounter_fhir_has_correct_structure`, `clin_01_encounter_type_maps_to_fhir_class` |
| CLIN-02 | `clin_02_vitals_fhir_has_correct_structure`, `clin_02_bmi_auto_calculated_correctly`, `clin_02_bmi_none_when_height_zero`, `clin_02_pain_score_clamped_to_10_in_fhir`, `clin_02_vitals_loinc_codes_present` |
| CLIN-03 | `clin_03_ros_fhir_has_correct_structure`, `clin_03_ros_none_fields_excluded_from_fhir` |
| CLIN-04 | `clin_04_physical_exam_fhir_has_correct_structure`, `clin_04_physical_exam_nil_systems_excluded` |
| CLIN-05 | `clin_05_templates_count_at_least_10`, `clin_05_templates_have_required_specialties`, `clin_05_each_template_has_all_soap_sections`, `clin_05_template_ids_are_unique`, `clin_05_each_template_has_ros_systems` |
| CLIN-06 | `clin_06_cosign_fhir_has_correct_structure` |
| CLIN-07 | `clin_07_name_match_generates_alert`, `clin_07_no_match_for_unrelated_drug_allergy`, `clin_07_severe_allergy_maps_to_contraindicated`, `clin_07_mild_allergy_maps_to_warning`, `clin_07_rxnorm_code_exact_match`, `clin_07_rxnorm_mismatch_no_code_match` |

## Not Proved By This UAT

- End-to-end DB integration (same precedent as S04–S06; no in-memory SQLite test harness)
- CLIN-08 (pediatric growth charts) — deferred
- Co-sign rejection path (wrong supervisor attempts to approve) — happy-path only
- Drug-allergy CDS for food/environmental allergen filtering — logic is in code but no dedicated test
- Encounter finalization via `update_encounter` with `status=finished` (period.end auto-set)

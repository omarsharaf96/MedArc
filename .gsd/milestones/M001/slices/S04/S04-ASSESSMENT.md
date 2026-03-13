# S04 Roadmap Assessment

**Verdict: Roadmap unchanged — coverage holds.**

## Success Criterion Coverage

- `A solo practitioner can use MedArc for daily patient care without AI, without cloud, and without billing → S05, S06, S07, S08, S09`
- `All PHI is stored in a SQLCipher-encrypted local database with AES-256 — HIPAA-compliant from first launch → S01–S03 (proved), S09 (distribution gate)`
- `Desktop application distributes as a code-signed, notarized macOS DMG with auto-updates → S09`

All criteria have at least one remaining owning slice. Coverage check passes.

## Slice Review

**S05 (Clinical Patient Data):** Correctly positioned. S04 established the exact contract S05 needs: `patient_id` as `Patient/<uuid>` in `subject.reference`, `patient_index` as the existence check table, MRN as `TEXT NOT NULL UNIQUE`. PTNT-08 (allergies), PTNT-09 (problems), PTNT-10 (medications), PTNT-11 (immunizations) all belong here. No change.

**S06 (Scheduling):** Correctly depends on S05. Scheduling requires patients to exist; clinical data (allergies, medications) must be visible at booking time for safety alerts. SCHD-01–07 land here. No change.

**S07 (Clinical Documentation):** Correctly depends on S06. Encounters reference appointments. CLIN-01–08 land here. S04 forward intelligence flagged that `upsert_care_team` uses a json_extract scan (no index) — S07 should add a `care_team_index` table analogous to `patient_index`. This is an internal implementation note, not a scope change to the roadmap description.

**S08 (Lab Results & Document Management):** Correctly depends on S07. LABS-01–04 and DOCS-01–03 land here. No change.

**S09 (Backup, Distribution & Release):** BKUP-01–03 and DIST-01–03 land here. No change.

## Requirement Coverage

Active requirements are fully covered:

| Domain | Requirements | Owning Slice |
|--------|-------------|--------------|
| Clinical patient data | PTNT-08–11 | S05 |
| Scheduling | SCHD-01–07 | S06 |
| Clinical documentation | CLIN-01–08 | S07 |
| Labs & documents | LABS-01–04, DOCS-01–03 | S08 |
| Backup & distribution | BKUP-01–03, DIST-01–03 | S09 |

New requirements surfaced by S04:
- **PTNT-12** (patient merge/duplicate detection) — deferred; can land in S05 or later, not blocking MVP
- **PTNT-13** (patient photo as binary blob) — deferred; not blocking MVP

No requirements were invalidated or re-scoped.

## Risk Assessment

S04 retired the patient data layer risk as planned. No new risks alter slice ordering. The one fragility (json_extract scans on CareTeam and RelatedPerson) is documented in S04's forward intelligence and is an S07 internal fix — it does not require a roadmap-level change.

## Decision

No roadmap changes required. Proceed to S05.

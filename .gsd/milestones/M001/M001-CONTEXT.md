# M001 Context

**Milestone:** MedArc Phase 1 MVP
**Scope:** Full HIPAA-compliant desktop EMR foundation — encrypted database, authentication, audit logging, patient data, scheduling, clinical documentation, labs, documents, and macOS distribution.

## Build Order Rationale

Slices execute in strict dependency order: S01 (encrypted shell) → S02 (auth/RBAC) → S03 (audit logging) → S04 (patients) → S05 (clinical data) → S06 (scheduling) → S07 (clinical docs) → S08 (labs/documents) → S09 (backup/release).

The security foundation (S01-S03) must be complete before any PHI-touching feature is built. HIPAA audit logging requires authenticated sessions. Patient CRUD requires audit logging. Everything downstream requires patients.

## Key Context

- No upstream milestone dependencies — this is the first milestone
- Phase 1 is intentionally AI-free: manual EMR workflows must work correctly before AI enhances them
- All Rust code owns the database; no Python touches SQLCipher in Phase 1
- FHIR R4 JSON-column hybrid storage designed from S01 to avoid future rewrites
- OpenEMR v8.0.0 serves as the feature baseline for completeness validation

# MedArc — AI-Powered Desktop EMR

## What This Is

An AI-native electronic medical records application built for solo practitioners and small clinics (1-5 providers), delivered as a self-contained macOS desktop application with local-first data storage and a clear cloud migration path. It eliminates the documentation burden plaguing small practices through ambient AI documentation, intelligent coding, offline-first privacy, and zero monthly SaaS fees.

For M003, the platform is specialised for **Physical Therapy** practice: PT-native note types, structured objective measures, AI voice-to-note, a categorised patient document vault, Phaxio fax integration, PDF export, and insurance authorisation tracking.

## Core Value

Physicians and PTs can document patient encounters through voice capture that automatically generates structured notes, reducing documentation time by 30-41% while keeping all PHI local and encrypted on their device.

## Current State

- **M001 COMPLETE** — Full Rust/Tauri backend: auth, RBAC, audit chain, FHIR R4 data model, patient records, clinical data, scheduling, SOAP encounters, labs, documents, backups. 265 unit tests. SQLCipher AES-256. 14 migrations.
- **M002 COMPLETE** — Full React frontend wired to all 88 backend commands. Role-gated navigation. PatientListPage, PatientDetailPage, CalendarPage, EncounterWorkspace (SOAP + vitals + ROS + PhysicalExam), ClinicalSidebar, LabResultsPanel, DocumentBrowser, SettingsPage. tsc --noEmit exits 0.
- **M003 IN PROGRESS** — PT-specific practice layer: Touch ID fix, PT note templates, objective measures, AI voice-to-note, document centre, PDF export, fax integration, auth tracking.

## Architecture / Key Patterns

- **Stack**: Tauri 2.x (Rust) · React 18 (TypeScript) · SQLCipher (AES-256) · FHIR R4 · macOS-primary · local-first
- **Backend**: Rust Tauri commands in `src-tauri/src/commands/`. All DB access via `Database` state (SQLCipher connection pool). Migrations in `src-tauri/src/db/migrations.rs` (append-only, rusqlite-migration).
- **Frontend**: React pages in `src/pages/`. Tauri invoke wrappers in `src/lib/tauri.ts` (flat `commands` object, no namespacing). State-based discriminated-union router (`RouterContext`). Types in `src/types/`.
- **RBAC**: `src-tauri/src/rbac/roles.rs` — `has_permission(role, resource, action)`. Every command calls `middleware::require_permission`. Field-level visibility via `visible_fields()`.
- **Audit**: Every ePHI command writes to `audit_logs` via `write_audit_entry()`. SHA-256 hash chain. Trigger-enforced immutability.
- **Auth**: Argon2id passwords. TOTP MFA. Sessions in `SessionManager`. Touch ID via macOS LAContext (`objc2-local-authentication`) — implemented in M003/S01.

## Capability Contract

See `.gsd/REQUIREMENTS.md` for the explicit capability contract, requirement status, and coverage mapping.

## Milestone Sequence

- [x] M001: MedArc Phase 1 MVP — Full Rust/Tauri backend, FHIR R4, auth, RBAC, audit, patients, scheduling, clinical docs, labs, backups
- [x] M002: MedArc Phase 2 Frontend — Full React UI, 88 Tauri command wrappers, role-gated navigation, tsc exits 0
- [ ] M003: PT Practice — Touch ID fix, PT note templates, objective measures, AI voice-to-note, document centre, PDF export, Phaxio fax, auth tracking

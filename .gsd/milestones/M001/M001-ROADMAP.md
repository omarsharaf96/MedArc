# M001: MedArc Phase 1 MVP

**Vision:** An AI-native electronic medical records application built for solo practitioners and small clinics (1-5 providers), delivered as a self-contained macOS desktop application with local-first data storage and a clear cloud migration path.

## Success Criteria

- A solo practitioner can use MedArc for daily patient care without AI, without cloud, and without billing
- All PHI is stored in a SQLCipher-encrypted local database with AES-256 — HIPAA-compliant from first launch
- Desktop application distributes as a code-signed, notarized macOS DMG with auto-updates


## Slices

- [x] **S01: Desktop Shell Encrypted Database** `risk:medium` `depends:[]`
  > After this: Scaffold the Tauri 2.
- [x] **S02: Auth Access Control** `risk:medium` `depends:[S01]`
  > After this: Build the authentication foundation: user account creation with Argon2id password hashing, login/logout flow, and a session state machine with configurable inactivity timeout.
- [x] **S03: Audit Logging** `risk:medium` `depends:[S02]`
  > After this: Create the audit logging data layer: Migration 8 (audit_logs table + immutability triggers) and the audit Rust module (entry.
- [x] **S04: Patient Demographics & Care Teams** `risk:medium` `depends:[S03]`
  > After this: unit tests prove Patient Demographics & Care Teams works
- [x] **S05: Clinical Patient Data** `risk:medium` `depends:[S04]`
  > After this: unit tests prove Clinical Patient Data works
- [x] **S06: Scheduling** `risk:medium` `depends:[S05]`
  > After this: unit tests prove Scheduling works
- [ ] **S07: Clinical Documentation** `risk:medium` `depends:[S06]`
  > After this: unit tests prove Clinical Documentation works
- [ ] **S08: Lab Results & Document Management** `risk:medium` `depends:[S07]`
  > After this: unit tests prove Lab Results & Document Management works
- [ ] **S09: Backup, Distribution & Release** `risk:medium` `depends:[S08]`
  > After this: unit tests prove Backup, Distribution & Release works

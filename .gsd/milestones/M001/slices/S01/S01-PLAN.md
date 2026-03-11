# S01: Desktop Shell Encrypted Database

**Goal:** Scaffold the Tauri 2.x desktop application with React/TypeScript frontend, establish the SQLCipher-encrypted database layer with macOS Keychain key management, and set up schema migrations
**Demo:** Launch the Tauri macOS app, confirm the database file is encrypted (sqlite3 cannot read it), verify the Keychain entry exists, and perform FHIR Patient CRUD through the React UI

## Must-Haves


## Tasks

- [x] **T01: 01-desktop-shell-encrypted-database 01** `est:8min`
  - Scaffold the Tauri 2.x desktop application with React/TypeScript frontend, establish the SQLCipher-encrypted database layer with macOS Keychain key management, and set up schema migrations.

Purpose: This plan creates the entire application foundation -- the runnable desktop shell, the encrypted database, and the secure key storage -- which all subsequent plans and phases build upon.

Output: A launchable Tauri macOS app with an encrypted SQLCipher database whose key is stored in macOS Keychain, with automatic schema migrations on startup and a health check command confirming everything works.
- [x] **T02: 01-desktop-shell-encrypted-database 02** `est:5min`
  - Create the FHIR R4 resource storage schema with indexed lookups and implement the complete Rust-native CRUD command layer, then wire the React frontend to invoke these commands.

Purpose: This plan delivers the data modeling and command layer that every subsequent phase depends on. Patients, encounters, observations -- all future FHIR resources flow through this CRUD layer.

Output: Working FHIR resource storage with JSON columns, indexed virtual columns for Patient lookups, five Tauri CRUD commands, type-safe frontend wrappers, and a React UI that displays database status.
- [x] **T03: 01-desktop-shell-encrypted-database 03** `est:8min`
  - Polish the frontend UI into proper components and perform end-to-end verification of all Phase 1 requirements through a human-verified checkpoint.

Purpose: This plan ensures the Phase 1 foundation is solid before any subsequent phases build on it. It extracts UI into proper components and then verifies every requirement with a human walkthrough.

Output: Clean component-based React UI and verified confirmation that all six FOUN requirements are met.

## Files Likely Touched

- `package.json`
- `tsconfig.json`
- `vite.config.ts`
- `tailwind.config.js`
- `index.html`
- `src/main.tsx`
- `src/App.tsx`
- `src-tauri/Cargo.toml`
- `src-tauri/Cargo.lock`
- `src-tauri/build.rs`
- `src-tauri/tauri.conf.json`
- `src-tauri/capabilities/default.json`
- `src-tauri/src/main.rs`
- `src-tauri/src/lib.rs`
- `src-tauri/src/error.rs`
- `src-tauri/src/keychain.rs`
- `src-tauri/src/db/mod.rs`
- `src-tauri/src/db/connection.rs`
- `src-tauri/src/db/migrations.rs`
- `src-tauri/src/commands/mod.rs`
- `src-tauri/src/commands/health.rs`
- `src-tauri/src/db/migrations.rs`
- `src-tauri/src/db/models/mod.rs`
- `src-tauri/src/db/models/fhir.rs`
- `src-tauri/src/db/mod.rs`
- `src-tauri/src/commands/mod.rs`
- `src-tauri/src/commands/fhir.rs`
- `src-tauri/src/lib.rs`
- `src-tauri/capabilities/default.json`
- `src/lib/tauri.ts`
- `src/App.tsx`
- `src/types/fhir.ts`
- `src/App.tsx`
- `src/components/DatabaseStatus.tsx`
- `src/components/FhirExplorer.tsx`

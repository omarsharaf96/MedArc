# T02: 01-desktop-shell-encrypted-database 02

**Slice:** S01 — **Milestone:** M001

## Description

Create the FHIR R4 resource storage schema with indexed lookups and implement the complete Rust-native CRUD command layer, then wire the React frontend to invoke these commands.

Purpose: This plan delivers the data modeling and command layer that every subsequent phase depends on. Patients, encounters, observations -- all future FHIR resources flow through this CRUD layer.

Output: Working FHIR resource storage with JSON columns, indexed virtual columns for Patient lookups, five Tauri CRUD commands, type-safe frontend wrappers, and a React UI that displays database status.

## Must-Haves

- [ ] "FHIR R4 resources can be created as JSON and stored in the encrypted database"
- [ ] "FHIR resources can be retrieved by ID and by resource type"
- [ ] "Frequently queried Patient fields are indexed for fast lookups"
- [ ] "All CRUD operations execute through Rust-native Tauri commands invoked from the React frontend"
- [ ] "Frontend can invoke create, read, list, update, and delete operations on FHIR resources"

## Files

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

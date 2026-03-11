# T03: 01-desktop-shell-encrypted-database 03

**Slice:** S01 — **Milestone:** M001

## Description

Polish the frontend UI into proper components and perform end-to-end verification of all Phase 1 requirements through a human-verified checkpoint.

Purpose: This plan ensures the Phase 1 foundation is solid before any subsequent phases build on it. It extracts UI into proper components and then verifies every requirement with a human walkthrough.

Output: Clean component-based React UI and verified confirmation that all six FOUN requirements are met.

## Must-Haves

- [ ] "User launches the Tauri app and sees the MedArc desktop window with database status"
- [ ] "Database file on disk is encrypted and unreadable by plain sqlite3"
- [ ] "Keychain contains the encryption key entry"
- [ ] "User can create, view, and delete FHIR resources through the UI"
- [ ] "App survives restart with data persisted and re-accessible"

## Files

- `src/App.tsx`
- `src/components/DatabaseStatus.tsx`
- `src/components/FhirExplorer.tsx`

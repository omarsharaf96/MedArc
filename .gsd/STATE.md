# GSD State

**Active Milestone:** M003 — PT Practice
**Active Slice:** S01 — Touch ID Fix + PT Note Templates
**Active Task:** none (slice not yet started)
**Phase:** Planning complete — ready to execute
**Slice Branch:** gsd/M003/S01 (not yet created)
**Last Updated:** 2026-03-13
**Requirements Status:** 23 active · 0 validated (M003) · 3 deferred · 7 out of scope

## Milestone Registry

- ✅ **M001:** MedArc Phase 1 MVP — Full Rust/Tauri backend, FHIR R4, 265 unit tests
- ✅ **M002:** MedArc Phase 2 Frontend — Full React UI, 88 Tauri commands, tsc exits 0
- 🔄 **M003:** PT Practice — 7 slices planned, S01 is next

## Recent Decisions

- Touch ID via `objc2-local-authentication` (macOS LAContext), not `tauri-plugin-biometric` (Android/iOS only)
- `biometric_authenticate` is a new Tauri command parallel to `unlock_session` (not a replacement)
- `com.apple.security.device.biometric-access` entitlement required in entitlements.plist
- PT notes as FHIR Composition resources in `fhir_resources` + `pt_note_index` table
- Outcome scores as FHIR Observation resources with LOINC codes + `outcome_score_index` table
- Ollama at localhost:11434 with startup check; Bedrock Haiku fallback
- PDF via `printpdf` Rust crate; Phaxio credentials in SQLCipher

## Blockers

- None

## Next Action

Start S01. Create branch `gsd/M003/S01` from main, write `S01-PLAN.md` with task decomposition, then execute T01 (Touch ID Fix: `objc2-local-authentication` + entitlements + `biometric_authenticate` command + LockScreen wiring).

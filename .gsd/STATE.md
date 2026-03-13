# GSD State

**Active Milestone:** M003 — PT Practice
**Active Slice:** S02 — Objective Measures & Outcome Scores
**Phase:** executing
**Requirements Status:** 22 active · 11 validated · 3 deferred · 0 out of scope

## Milestone Registry
- ✅ **M001:** MedArc Phase 1 MVP
- ✅ **M002:** MedArc Phase 2 Frontend
- 🔄 **M003:** PT Practice

## Recent Decisions
- S02: Scoring logic is pure Rust at record time; FABQ work subscale in `score`, PA in `score_secondary`; `get_outcome_comparison` is a dedicated backend command; inline SVG trend chart (no npm packages); tabular body-region selector (no SVG body diagram); ROM/MMT in `fhir_resources` only (no secondary index); `cargo test --lib` is primary Rust verification gate

## Blockers
- None

## Next Action
Execute T01: Backend — scoring module, Migration 16, and Tauri commands.

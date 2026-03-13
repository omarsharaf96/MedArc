---
phase: 3
slug: audit-logging
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-11
---

# Phase 3 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in `#[cfg(test)]` unit tests |
| **Config file** | none — Cargo runs tests natively |
| **Quick run command** | `cargo test -p medarc --lib -- audit` |
| **Full suite command** | `cargo test -p medarc` |
| **Estimated runtime** | ~10 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p medarc --lib -- audit`
- **After every plan wave:** Run `cargo test -p medarc`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** ~10 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 03-01-01 | 01 | 0 | AUDT-01 | unit | `cargo test -p medarc --lib -- audit::entry::tests` | ❌ W0 | ⬜ pending |
| 03-01-02 | 01 | 0 | AUDT-01 | unit | `cargo test -p medarc --lib -- audit::entry::tests::failure_path_logged` | ❌ W0 | ⬜ pending |
| 03-01-03 | 01 | 0 | AUDT-02 | unit | `cargo test -p medarc --lib -- audit::entry::tests::hash_chain_integrity` | ❌ W0 | ⬜ pending |
| 03-01-04 | 01 | 0 | AUDT-02 | unit | `cargo test -p medarc --lib -- audit::entry::tests::chain_linkage` | ❌ W0 | ⬜ pending |
| 03-01-05 | 01 | 0 | AUDT-02 | unit | `cargo test -p medarc --lib -- audit::entry::tests::immutability_update_rejected` | ❌ W0 | ⬜ pending |
| 03-01-06 | 01 | 0 | AUDT-02 | unit | `cargo test -p medarc --lib -- audit::entry::tests::immutability_delete_rejected` | ❌ W0 | ⬜ pending |
| 03-02-01 | 02 | 1 | AUDT-01 | unit | `cargo test -p medarc --lib -- audit` | ❌ W0 | ⬜ pending |
| 03-03-01 | 03 | 1 | AUDT-02 | unit | `cargo test -p medarc --lib -- audit::query::tests::verify_clean_chain` | ❌ W0 | ⬜ pending |
| 03-04-01 | 04 | 2 | AUDT-04 | unit | `cargo test -p medarc --lib -- audit::query::tests::provider_sees_own_only` | ❌ W0 | ⬜ pending |
| 03-04-02 | 04 | 2 | AUDT-05 | unit | `cargo test -p medarc --lib -- audit::query::tests::system_admin_sees_all` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `src-tauri/src/audit/mod.rs` — module declarations
- [ ] `src-tauri/src/audit/entry.rs` — stubs for AUDT-01, AUDT-02 tests
- [ ] `src-tauri/src/audit/query.rs` — stubs for AUDT-04, AUDT-05 tests
- [ ] `src-tauri/src/commands/audit.rs` — Tauri command stubs for get_audit_log and verify_audit_chain
- [ ] In-memory DB test helper — open `:memory:` SQLite, run migrations, shared by all audit tests

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Audit log visible in UI with correct fields | AUDT-04, AUDT-05 | Requires React frontend integration | Log in as Provider, navigate to audit log view, verify only own entries shown; log in as SystemAdmin, verify all entries shown |
| 6-year retention documentation | AUDT-03 | Policy verification, not code | Confirm triggers prevent DELETE via DB inspection; document retention policy in app docs |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 10s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending

---
phase: 2
slug: auth-access-control
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-11
---

# Phase 2 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in test framework + cargo test |
| **Config file** | None needed (Cargo.toml `[dev-dependencies]`) |
| **Quick run command** | `cd src-tauri && cargo test` |
| **Full suite command** | `cd src-tauri && cargo test --all-features` |
| **Estimated runtime** | ~15 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cd src-tauri && cargo test`
- **After every plan wave:** Run `cd src-tauri && cargo test --all-features`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 15 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 2-01-01 | 01 | 1 | AUTH-01 | unit | `cd src-tauri && cargo test auth::tests::test_create_user` | ❌ W0 | ⬜ pending |
| 2-01-02 | 01 | 1 | AUTH-01 | unit | `cd src-tauri && cargo test auth::tests::test_duplicate_username` | ❌ W0 | ⬜ pending |
| 2-01-03 | 01 | 1 | AUTH-02 | unit | `cd src-tauri && cargo test auth::password::tests::test_hash_verify` | ❌ W0 | ⬜ pending |
| 2-01-04 | 01 | 1 | AUTH-02 | unit | `cd src-tauri && cargo test auth::password::tests::test_min_length` | ❌ W0 | ⬜ pending |
| 2-02-01 | 02 | 1 | AUTH-03 | unit | `cd src-tauri && cargo test auth::session::tests::test_timeout_lock` | ❌ W0 | ⬜ pending |
| 2-02-02 | 02 | 1 | AUTH-04 | manual | N/A (requires hardware) | N/A | ⬜ pending |
| 2-02-03 | 02 | 1 | AUTH-05 | unit | `cd src-tauri && cargo test auth::totp::tests::test_generate_setup` | ❌ W0 | ⬜ pending |
| 2-02-04 | 02 | 1 | AUTH-05 | unit | `cd src-tauri && cargo test auth::totp::tests::test_verify_code` | ❌ W0 | ⬜ pending |
| 2-03-01 | 03 | 2 | AUTH-06 | unit | `cd src-tauri && cargo test rbac::roles::tests::test_permissions` | ❌ W0 | ⬜ pending |
| 2-03-02 | 03 | 2 | AUTH-07 | unit | `cd src-tauri && cargo test rbac::field_filter::tests::test_filter` | ❌ W0 | ⬜ pending |
| 2-03-03 | 03 | 2 | AUTH-08 | unit | `cd src-tauri && cargo test auth::tests::test_break_glass` | ❌ W0 | ⬜ pending |
| 2-03-04 | 03 | 2 | AUTH-08 | unit | `cd src-tauri && cargo test auth::tests::test_break_glass_expiry` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `src-tauri/src/auth/mod.rs` — module declarations
- [ ] `src-tauri/src/auth/password.rs` — password hash/verify with test stubs
- [ ] `src-tauri/src/auth/session.rs` — session state machine with test stubs
- [ ] `src-tauri/src/auth/totp.rs` — TOTP setup/verify with test stubs
- [ ] `src-tauri/src/rbac/mod.rs` — module declarations
- [ ] `src-tauri/src/rbac/roles.rs` — role enum and permission matrix with test stubs
- [ ] `src-tauri/src/rbac/field_filter.rs` — JSON field filtering with test stubs
- [ ] Test helpers for creating in-memory SQLCipher databases for unit tests

*If none: "Existing infrastructure covers all phase requirements."*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Touch ID availability and authentication | AUTH-04 | Requires macOS hardware with Touch ID sensor | 1. Run on Mac with Touch ID. 2. Enable Touch ID in settings. 3. Lock session. 4. Verify Touch ID prompt appears. 5. Authenticate with fingerprint. 6. Verify session unlocks. |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 15s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending

# MedArc EMR — Developer Guide

## Project Overview

MedArc is an AI-powered Electronic Medical Records (EMR) desktop application purpose-built for solo Physical Therapy practitioners. It is a **local-first**, fully encrypted macOS application — no cloud dependency, no monthly SaaS fees, all patient data stays on the provider's machine.

## Tech Stack

| Layer | Technology |
|-------|-----------|
| Desktop framework | Tauri 2.x (Rust backend + WKWebView frontend) |
| Frontend | React 18 + TypeScript 5.5 + Tailwind CSS 3.4 |
| Backend | Rust (2021 edition) |
| Database | SQLCipher (SQLite + AES-256-CBC encryption) |
| Encryption key | macOS Keychain (Secure Enclave-backed on Apple Silicon) |
| Auth | Argon2id passwords + TOTP MFA + Touch ID (LAContext) |
| AI (local) | whisper.cpp (speech-to-text) + Ollama (LLaMA 3.1 8B for note generation) |
| AI (fallback) | AWS Bedrock Claude Haiku (requires internet + BAA) |
| PDF generation | printpdf 0.7 (Rust) |
| Fax | Phaxio API (reqwest HTTP client) |
| SMS reminders | Twilio API |
| Email reminders | SendGrid API |
| Data model | FHIR R4 (JSON in fhir_resources table + denormalized index tables) |

## Development Setup

### Prerequisites

- **macOS 12+** (Monterey or later)
- **Rust** (latest stable): `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- **Node.js 18+**: `brew install node`
- **Xcode Command Line Tools**: `xcode-select --install`

### Optional (for AI features)

- **Ollama** (local LLM): `brew install ollama && ollama serve && ollama pull llama3.1:8b`
- **Whisper model**: Downloaded on first use via the app UI, or manually:
  ```bash
  curl -L https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.en.bin \
    -o ~/Library/Application\ Support/com.medarc.emr/models/whisper/ggml-small.en.bin
  ```
- **CMake** (for whisper-rs feature): `brew install cmake`

### Running in Development

```bash
# Terminal 1: React dev server (Vite, hot-reload)
npm install
npm run dev

# Terminal 2: Tauri dev (compiles Rust, opens app window)
npm run tauri dev
```

The app opens at the Vite dev URL (`http://localhost:1420`) inside a Tauri WKWebView window.

**Dev bypass login**: In development builds, the login screen shows a "Dev Bypass" button that auto-creates a `dev` / `SystemAdmin` user and skips authentication. This is compiled out of release builds via `#[cfg(debug_assertions)]`.

### Running Tests

```bash
# Rust unit tests (517 tests)
cd src-tauri && cargo test --lib

# TypeScript type checking (no runtime tests yet)
npx tsc --noEmit
```

### Building for Production (macOS DMG)

```bash
# Compile React + bundle Rust, code-sign, notarize
npm run tauri:build
# Or explicitly for universal binary:
npm run tauri build --target universal-apple-darwin
```

**Output**: `src-tauri/target/release/bundle/dmg/MedArc_0.1.0_universal.dmg`

**Code signing requirements**:
- Apple Developer Program certificate in Keychain
- Set `APPLE_SIGNING_IDENTITY` environment variable
- Notarization credentials: `APPLE_ID`, `APPLE_PASSWORD` (app-specific password), `APPLE_TEAM_ID`

**What the build produces**:
- `.app` bundle with Hardened Runtime + App Sandbox
- `.dmg` installer for distribution
- Universal binary (Apple Silicon + Intel)
- Ed25519-signed update manifest for auto-updater

### Building with Whisper (optional)

```bash
# Enables local speech-to-text (requires cmake + C++ compiler)
cargo build --features whisper
# Or for Tauri dev:
TAURI_FEATURES=whisper npm run tauri dev
```

Without the whisper feature, `transcribe_audio` returns a clear error with setup instructions. The rest of the app works normally.

## Project Structure

```
MedArc/
├── src/                          # React frontend (TypeScript)
│   ├── App.tsx                   # Root component, auth gating, dev mode badge
│   ├── pages/                    # 25+ page components
│   ├── components/               # Reusable components (auth, clinical, scheduling, shell, shared, fax)
│   ├── hooks/                    # Custom React hooks (useAuth, usePatient, etc.)
│   ├── contexts/                 # RouterContext (discriminated-union state router)
│   ├── types/                    # TypeScript interfaces (mirrors Rust structs)
│   └── lib/
│       ├── tauri.ts              # ~200+ type-safe Tauri invoke wrappers
│       └── surveyStore.ts        # localStorage survey store for kiosk mode
│
├── src-tauri/                    # Rust backend
│   ├── src/
│   │   ├── lib.rs                # Tauri app setup, plugin registration, invoke_handler
│   │   ├── commands/             # 20+ command modules (~200 Tauri commands)
│   │   │   ├── auth.rs           # Login, register, dev bypass
│   │   │   ├── billing.rs        # CPT codes, 8-minute rule, fee schedules
│   │   │   ├── claims.rs         # 837P EDI generation, claim lifecycle
│   │   │   ├── era_processing.rs # 835 parsing, auto-posting, A/R aging
│   │   │   ├── eligibility.rs    # 270/271 EDI, insurance verification
│   │   │   ├── pt_notes.rs       # PT note templates (IE, Progress, Discharge)
│   │   │   ├── objective_measures.rs # ROM, MMT, outcome scores (LEFS/DASH/NDI/etc.)
│   │   │   ├── audio_capture.rs  # cpal microphone recording
│   │   │   ├── transcription.rs  # whisper.cpp integration
│   │   │   ├── llm_integration.rs# Ollama + Bedrock note generation
│   │   │   ├── hep.rs            # Exercise library, HEP builder
│   │   │   ├── analytics.rs      # KPI dashboard, payer mix
│   │   │   ├── mips_reporting.rs # MIPS quality measures
│   │   │   ├── workers_comp.rs   # WC cases, FROI, impairment ratings
│   │   │   ├── reminders.rs      # Twilio SMS, SendGrid email
│   │   │   └── ...               # patient, clinical, scheduling, docs, fax, etc.
│   │   ├── db/
│   │   │   ├── connection.rs     # SQLCipher setup (AES-256 key from Keychain)
│   │   │   ├── migrations.rs     # 31 append-only migrations
│   │   │   └── models/           # FHIR R4 type definitions
│   │   ├── auth/                 # Session, password (Argon2id), biometric, TOTP
│   │   ├── rbac/                 # 5-role RBAC matrix with field-level filtering
│   │   ├── audit/                # SHA-256 hash-chain audit logging
│   │   └── error.rs              # AppError enum
│   ├── Cargo.toml                # Rust dependencies
│   ├── entitlements.plist        # macOS sandbox + hardened runtime
│   ├── capabilities/default.json # Tauri 2.x plugin permissions
│   └── tauri.conf.json           # App config, bundle settings, auto-updater
│
├── .gsd/                         # Planning & execution tracking
│   ├── PROJECT.md
│   ├── STATE.md
│   ├── REQUIREMENTS.md
│   └── milestones/M003-M005/     # Research, plans, UAT, assessments per slice
│
├── package.json                  # npm scripts + React/Tauri deps
├── tsconfig.json                 # TypeScript config
├── tailwind.config.js            # Tailwind CSS config
└── CLAUDE.md                     # This file
```

## Database

**31 migrations** (append-only in `src-tauri/src/db/migrations.rs`):

| Range | Milestone | Tables |
|-------|-----------|--------|
| 1-14 | M001-M002 | app_metadata, fhir_resources, fhir_identifiers, users, sessions, break_glass, app_settings, audit_logs, patient_index, clinical indexes, scheduling indexes, documentation indexes, lab indexes, backup_log |
| 15-21 | M003 | pt_note_index, outcome_score_index, document/survey/referral indexes, export_log, fax_log/contacts, auth_record_index, composite indexes |
| 22-28 | M004 | cpt_fee_schedule, encounter_billing, billing_line_items, exercise_library, hep_programs/templates, therapy_cap_tracking, abn_records, payer_config, claims, remittance_advice, claim_payments, kpi_snapshots, mips_screenings/performance |
| 29-31 | M005 | eligibility_checks, reminder_log/templates, wc_cases/contacts/fee_schedules/impairment_ratings/communications |

All patient data is encrypted at rest via SQLCipher (AES-256). The encryption key is stored in macOS Keychain, never on disk.

## RBAC Roles

| Role | Access |
|------|--------|
| SystemAdmin | Everything except prescriptions |
| Provider | Full clinical + billing + prescriptions |
| NurseMa | Read/update clinical, read billing, no prescriptions |
| BillingStaff | Full billing + claims, read clinical |
| FrontDesk | Scheduling + patient demographics, read-only clinical |

## Key Architectural Decisions

- **No URL-based router**: Tauri WKWebView has no URL bar. State-based discriminated-union router in React context.
- **FHIR R4 hybrid storage**: Full JSON in `fhir_resources` + denormalized index tables for fast queries.
- **Local-first AI**: Whisper + Ollama run entirely on-device. No PHI leaves the machine unless Bedrock fallback is enabled.
- **Audit trail**: Every ePHI-touching command writes an immutable SHA-256 hash-chained audit log entry.
- **Feature-gated whisper**: `whisper-rs` is optional (`cargo build --features whisper`) so the app builds without C++ toolchain.

## Auto-Updater

Configured in `tauri.conf.json` with Ed25519 signature verification. Update endpoint: `https://releases.medarc.app/{{target}}/{{arch}}/{{current_version}}`. The public key placeholder needs to be replaced with the actual Ed25519 public key before production deployment.

## External Service Dependencies

| Service | Purpose | Credentials Storage | BAA Required |
|---------|---------|-------------------|--------------|
| Ollama (localhost:11434) | Local LLM for note generation | None (local) | No |
| AWS Bedrock | Cloud AI fallback | SQLCipher app_settings | Yes |
| Phaxio | Fax send/receive | SQLCipher app_settings | Yes |
| Twilio | SMS appointment reminders | SQLCipher app_settings | Yes (if sending PHI) |
| SendGrid | Email appointment reminders | SQLCipher app_settings | Yes (if sending PHI) |
| Office Ally | Claims clearinghouse (future) | Keychain | Yes |

## Common Commands

```bash
# Development
npm run dev                    # Start Vite dev server
npm run tauri dev              # Start full Tauri app in dev mode

# Testing
cd src-tauri && cargo test --lib  # Run 517 Rust unit tests
npx tsc --noEmit                  # TypeScript type check

# Building
npm run tauri:build               # Production macOS DMG

# Database
# DB location: ~/Library/Application Support/com.medarc.emr/medarc.db
# Encryption key: macOS Keychain → "com.medarc.emr.db-key"
```

## Development Framework

We use **GSD 2** for orchestration and **Superpowers** for quality discipline.

- **GSD 2** controls the workflow: Milestone → Slice → Task. Fresh context per task. Deterministic execution.
- **Superpowers** enforces TDD and code review quality within each task.
- State lives in `.gsd/`. Decisions live in `.gsd/DECISIONS.md`. Research lives in `.gsd/research/`.

### TDD Rules (Superpowers)

These are non-negotiable:

1. Write the failing test FIRST. Run it. Watch it fail.
2. Write the MINIMAL code to make the test pass. Nothing extra.
3. If code was written before the test: DELETE IT. Start over.
4. Each red-green cycle gets its own commit.
5. Test BEHAVIOR, not implementation details.
6. Use real implementations over mocks wherever possible.

**Anti-rationalization:** "This is too simple to test" → wrong, write the test. "I'll write tests after" → wrong, tests-after verify what you built, not what's needed. "Let me prototype first" → wrong, prototypes become production code.

### Code Review (Two-Stage)

1. **Spec compliance:** Does implementation match the acceptance criteria in `docs/PRD.md`? Nothing missing? Nothing extra?
2. **Code quality:** Clean separation, proper error handling, no unnecessary abstractions, tests verify behavior?

Critical issues block merge. Important issues should fix before merge. Minor issues note for later.

### Context Management

- Target < 50% context utilization per task
- Use `/compact` manually if approaching 50%
- State lives in files (`.gsd/`, `docs/`), not conversation history
- Fresh context per task — never accumulate history across tasks
- Use ultrathink for complex architectural decisions
- Use subagents for research — they explore extensively but return condensed results

### What NOT to Do

- Don't add features not in the spec (YAGNI)
- Don't add comments, docstrings, or type annotations to code you didn't change
- Don't over-engineer for hypothetical future requirements
- Don't mock the database in tests — use real PostgreSQL (test transactions rollback)
- Don't commit `.env`, credentials, or API keys
- Don't skip the service layer (routers should not contain business logic)
- Don't use `any` in TypeScript
- Don't write code before the test

### Final Build Verification

Once all tasks are completed, **automatically** run the full production build and fix any issues — do not wait to be asked:

```bash
npx tauri build --target universal-apple-darwin --features whisper
```

All compiler warnings must be squashed — treat warnings as errors. The build must complete cleanly with zero warnings before work is considered done.

### Work Delegation

- **All implementation work must be done by subagents**, not by the main agent. The main agent orchestrates and reviews; subagents execute.
- Before starting work, **always create a task list** and present it to the user. Execution begins immediately — no approval required.
- The task list is a **living document** — the user can add new tasks at any time while subagents are working. The main agent should pick up new tasks as subagents complete existing ones.
- **Default: just run.** Do not ask for permission or confirmation before executing tasks. Execute immediately unless the user explicitly says otherwise.

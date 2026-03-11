# Stack Research

**Domain:** AI-powered desktop EMR (macOS, local-first, HIPAA-compliant)
**Researched:** 2026-03-10
**Confidence:** MEDIUM (web verification tools unavailable; versions based on training data through May 2025 and must be validated against current releases)

> **NOTE:** WebSearch, WebFetch, and Brave API were all unavailable during this research session. All version numbers and release claims are based on training data with a May 2025 cutoff. Every version marked with [VERIFY] should be confirmed against official release pages before locking the stack.

---

## PRD Stack Verdict

The PRD's stack choices are **well-considered and defensible**. No major changes recommended. Refinements below address version specifics, alternative considerations, and risk areas.

| PRD Choice | Verdict | Notes |
|------------|---------|-------|
| Tauri 2.x | **STRONG AGREE** | Correct over Electron for memory-constrained AI workloads |
| React 18+ TypeScript | **AGREE** | Consider React 19 if stable by project start |
| FastAPI sidecar | **AGREE with caveat** | PyInstaller bundling is the hard part; consider Nuitka as fallback |
| SQLCipher | **STRONG AGREE** | Perfect for local-first HIPAA; SQLAlchemy abstraction is key |
| Ollama | **AGREE** | Consider also MLX for Apple Silicon native; Ollama wraps llama.cpp |
| whisper.cpp | **AGREE** | WhisperKit (Swift/CoreML) is a stronger alternative on macOS |
| MedSpaCy | **AGREE with caution** | Verify Python 3.11+ compatibility; last major release was 2023 |
| FAISS | **STRONG AGREE** | Standard for local vector search at this scale |

---

## Recommended Stack

### Desktop Shell

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| Tauri | 2.x (latest stable) [VERIFY] | Desktop application shell | 30-50 MB idle RAM vs Electron's 150-300 MB. When running 6-35 GB AI models simultaneously, this headroom is not optional. Rust backend enforces explicit API surface (HIPAA minimum-necessary). Built-in sidecar support for PyInstaller binaries. WKWebView on macOS is fine for controlled-deployment medical software. Ed25519-signed auto-updates via tauri-plugin-updater. |
| Rust | 1.77+ [VERIFY] | Tauri backend, DB CRUD, file I/O | Tauri commands handle all non-AI operations in Rust for zero sidecar overhead on routine tasks. Memory safety without GC aligns with medical-grade reliability requirements. |

### Frontend

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| React | 18.3+ [VERIFY] | UI framework | Largest medical UI component ecosystem. React 19 may be stable by project start -- evaluate but do not adopt pre-release. Server Components are irrelevant for desktop; stick with client-side React. |
| TypeScript | 5.5+ [VERIFY] | Type safety | Non-negotiable for medical software. Catches data model errors at compile time. FHIR type definitions available via @types/fhir. |
| Vite | 5.x or 6.x [VERIFY] | Build tooling | Tauri's default bundler. Fast HMR for development. Do not use Create React App (deprecated) or webpack (unnecessary complexity). |

### Frontend Libraries

| Library | Version | Purpose | Why Recommended |
|---------|---------|---------|-----------------|
| Material UI (MUI) | 6.x [VERIFY] | Component library | WCAG 2.1 AA out of the box. Dense medical UIs need data tables, form controls, and dialogs -- MUI covers all. Theming system supports clinical color schemes. |
| React Hook Form | 7.x | Clinical form management | Uncontrolled form model avoids re-renders on large clinical forms (50+ fields in a SOAP note). Zod integration for runtime validation. |
| Zod | 3.x | Schema validation | Runtime validation of FHIR resources. Shared schemas between frontend validation and API contracts. |
| TanStack Table | 8.x | Data tables | Patient lists, lab results, medication lists, billing line items -- every EMR view is a table. Virtualization for large datasets. |
| TanStack Query | 5.x | Server state | Cache management for Tauri IPC and FastAPI calls. Handles loading/error states. Offline mutation queue for future cloud sync. |
| Zustand | 4.x or 5.x [VERIFY] | Client state | Lightweight global state for auth session, active patient, UI preferences. No boilerplate. |
| @medplum/react | latest [VERIFY] | FHIR components | SmartText clinical concept detection, SNOMED/ICD coding widgets, FHIR resource forms. Evaluate maturity -- may need custom wrappers. |
| fhir-react | latest [VERIFY] | FHIR rendering | FHIR resource display for DSTU2/STU3/R4. Useful for patient summary views. |
| date-fns | 3.x | Date handling | Lightweight date manipulation. Medical scheduling requires extensive date math. Do NOT use Moment.js (deprecated, bloated). |
| react-big-calendar | latest | Scheduling UI | Multi-provider calendar views. Needs customization for medical appointment types but saves months vs building from scratch. |

### Backend (Python Sidecar)

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| Python | 3.11 or 3.12 [VERIFY] | Sidecar runtime | 3.11 for best library compatibility (MedSpaCy concern). 3.12 is faster but verify all medical NLP libraries support it. Do NOT use 3.13+ until ecosystem catches up. |
| FastAPI | 0.115+ [VERIFY] | API framework | Async support for non-blocking AI inference. Pydantic models align with fhir.resources. Auto-generated OpenAPI docs for Tauri IPC contract documentation. |
| Pydantic | 2.x | Data validation | V2 is 5-50x faster than V1. FHIR resource validation via fhir.resources (Pydantic-based). |
| SQLAlchemy | 2.0+ | ORM / DB abstraction | Dialect swap from sqlite to postgresql with zero business logic changes. This is the keystone of the cloud migration strategy. |
| Alembic | 1.13+ [VERIFY] | Schema migrations | `render_as_batch=True` for SQLite ALTER TABLE limitations. Same migration files work on both SQLite and PostgreSQL. |
| PyInstaller | 6.x [VERIFY] | Binary packaging | Compiles Python + dependencies into single macOS executable for Tauri sidecar. Notarization-compatible with `--osx-bundle-identifier`. |
| uvicorn | 0.30+ [VERIFY] | ASGI server | Runs FastAPI in the sidecar. Use `--host 127.0.0.1` to bind only to localhost (HIPAA: no external network exposure). |

### Database

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| SQLite | 3.45+ (bundled) | Local database engine | Zero-config, serverless, single-file. Perfect for single-machine EMR. JSON1 extension for FHIR resource columns. |
| SQLCipher | 4.6+ [VERIFY] | Encryption layer | AES-256-CBC with PBKDF2-HMAC-SHA512 (256K iterations). Per-page HMAC for tamper detection. 5-15% overhead. Encryption key in macOS Keychain (Secure Enclave on Apple Silicon). HIPAA breach safe harbor: encrypted = "secured" = no breach notification. |
| sqlcipher3 (Python) | latest [VERIFY] | Python SQLCipher binding | SQLAlchemy dialect for SQLCipher. Verify compatibility with SQLAlchemy 2.0. Alternative: pysqlcipher3. |

### AI / ML Pipeline

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| Ollama | latest [VERIFY] | Local LLM runtime | Wraps llama.cpp with model management. Runs LLaMA 3.1 8B Q4_K_M (~6 GB) at 15-28 tok/sec on M1-M4. REST API on localhost. Simple model pulling and versioning. |
| LLaMA 3.1 8B | Q4_K_M quantization | SOAP note generation, coding assist | Best accuracy-to-size ratio for medical text generation. RAG-augmented outperforms fine-tuned biomedical models (64% vs 30% on NEJM). 6 GB fits in 16 GB unified memory alongside other models. |
| whisper.cpp | latest [VERIFY] | Voice transcription | CoreML acceleration on Apple Silicon for ~3x real-time. large-v3-turbo (1.5B params) for 95%+ accuracy. Critical: ~1% hallucination rate requires human review. |
| WhisperKit | latest [VERIFY] | Alternative transcription | Swift-native CoreML implementation. Tighter macOS integration than whisper.cpp. Evaluate both; WhisperKit may have better Apple Silicon optimization. |
| MedSpaCy | 1.x [VERIFY] | Clinical NLP | Section detection, negation/uncertainty via ConText algorithm. CRITICAL: verify Python 3.11/3.12 compatibility -- last major release was 2023. If incompatible, use spaCy directly with custom clinical rules. |
| SciSpaCy | 0.5.x [VERIFY] | Medical entity extraction | en_core_med7_lg (drugs, dosages, durations, routes, forms, frequencies), en_ner_bc5cdr_md (diseases, chemicals). All local, no cloud dependency. |
| spaCy | 3.7+ [VERIFY] | NLP framework | Foundation for MedSpaCy and SciSpaCy. Ensure version compatibility across all three. |
| FAISS | 1.8+ [VERIFY] | Vector search | Local similarity search for ICD-10/CPT code matching against 70K+ codes. faiss-cpu is sufficient; faiss-gpu not needed on Apple Silicon. |
| LangChain | 0.3+ [VERIFY] | AI orchestration | Pipeline orchestration for voice -> NLP -> LLM -> coding chain. LangGraph for conditional routing (local vs cloud). Evaluate whether lighter custom orchestration is sufficient -- LangChain adds significant dependency weight. |
| fhir.resources | 7.x [VERIFY] | FHIR data models | Pydantic-based FHIR R4 resource models. Validates FHIR JSON. Integrates naturally with FastAPI request/response models. |
| pyannote.audio | 3.x [VERIFY] | Speaker diarization | Separates physician vs patient speech in ambient recording. Requires HuggingFace token for model download. Runs locally after initial download. |

### Drug Safety

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| RxNav-in-a-Box | latest [VERIFY] | Drug interaction checking | Docker composition from NLM. Complete RxNorm API stack locally. DrugBank-sourced drug-drug interactions with severity ratings. No BAA required -- fully offline. Most privacy-preserving approach. |
| Docker Desktop | latest | Container runtime | Required for RxNav-in-a-Box. Evaluate Colima as lighter macOS alternative to Docker Desktop (avoids Docker Desktop licensing for commercial use). |

### Scheduling ML

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| XGBoost | 2.x [VERIFY] | No-show prediction | Gradient-boosted model on appointment history. Published AUC 0.75-0.85. Lightweight, trains on local data only. |
| scikit-learn | 1.4+ [VERIFY] | ML utilities | Feature engineering, model evaluation, preprocessing for scheduling ML. |

### Cloud AI (Fallback)

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| AWS Bedrock | N/A (managed) | Cloud LLM gateway | Hosts Claude and LLaMA with standard BAA. Near-instant approval. Zero data retention. Use for complex cases only (~5% of tasks). |
| boto3 | 1.34+ [VERIFY] | AWS SDK | Bedrock API access. Also used for future S3 backup and RDS connectivity in Phase 4. |

### Testing

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| Vitest | 2.x [VERIFY] | Frontend unit tests | Vite-native, faster than Jest. Compatible with React Testing Library. |
| React Testing Library | 16.x [VERIFY] | Component testing | Accessibility-first testing approach. Tests user behavior, not implementation. |
| Playwright | 1.x [VERIFY] | E2E testing | Cross-browser E2E. Tauri has Playwright integration via WebDriver. Test clinical workflows end-to-end. |
| pytest | 8.x [VERIFY] | Python testing | FastAPI test client, SQLAlchemy test fixtures, AI pipeline integration tests. |
| pytest-asyncio | latest | Async test support | FastAPI routes are async; tests must be too. |

### Development Tools

| Tool | Purpose | Notes |
|------|---------|-------|
| Biome | Linting + formatting (replaces ESLint + Prettier) | Faster, single tool. Tauri projects commonly use it. |
| Ruff | Python linting + formatting | Replaces flake8 + black + isort. 10-100x faster. |
| pre-commit | Git hook management | Enforce lint/format/type-check before commits. Critical for multi-developer consistency. |
| cargo-tauri | Tauri CLI | Build, dev server, sidecar management. |

---

## Alternatives Considered

| Category | Recommended | Alternative | When to Use Alternative |
|----------|-------------|-------------|-------------------------|
| Desktop shell | Tauri 2.x | Electron | Only if you need Chromium-specific features (e.g., Chrome DevTools Protocol for advanced debugging). Not recommended for this project. |
| Desktop shell | Tauri 2.x | Swift/AppKit native | If abandoning cross-platform entirely and going pure macOS. Better performance but 3-5x development cost and no web tech reuse. |
| Frontend | React 18+ | SolidJS | If starting fresh with no medical component ecosystem needs. Smaller bundle, finer reactivity. But medical UI libraries (medplum, fhir-react) are React-only. |
| State management | Zustand | Redux Toolkit | If team has strong Redux experience. But RTK adds boilerplate unnecessary for this app's state complexity. |
| ORM | SQLAlchemy 2.0 | Prisma (via TypeScript) | If the sidecar were Node.js instead of Python. Not applicable here since Python is required for AI/NLP. |
| Vector search | FAISS | ChromaDB | If you want a higher-level API with built-in persistence. FAISS is lower-level but faster and more battle-tested for medical code search at 70K+ vectors. |
| Vector search | FAISS | Qdrant (local mode) | If you need filtering + vector search combined. FAISS handles this use case fine with metadata pre-filtering. |
| LLM runtime | Ollama | MLX (Apple) | If targeting Apple Silicon exclusively and wanting tighter Metal integration. MLX is Python-native and potentially faster on M-series. Consider running both: Ollama for ease, MLX for performance-critical paths. |
| LLM runtime | Ollama | llama.cpp directly | If Ollama's HTTP overhead is measurable. Ollama wraps llama.cpp; direct integration via Python bindings (llama-cpp-python) removes one layer. |
| Transcription | whisper.cpp | WhisperKit (Swift) | For tighter CoreML/macOS integration. Evaluate both early. WhisperKit may have better Apple Silicon optimization but smaller community. |
| Transcription | whisper.cpp | Deepgram/AssemblyAI | Cloud transcription with BAAs available. Only if local transcription quality is insufficient after medical vocabulary tuning. Adds latency and cloud dependency. |
| AI orchestration | LangChain | Custom pipeline | If LangChain's dependency weight (~50+ transitive deps) is concerning. A custom orchestration layer with simple function chaining may be sufficient and more maintainable. Recommend: start with LangChain, extract to custom if it becomes a liability. |
| Python packaging | PyInstaller | Nuitka | If PyInstaller produces overly large binaries or has notarization issues. Nuitka compiles Python to C and can produce smaller, faster binaries. More complex setup. |
| Python packaging | PyInstaller | cx_Freeze | Fallback if PyInstaller fails. Less commonly used but viable. |
| Docker runtime | Docker Desktop | Colima | If Docker Desktop licensing is a concern (commercial use >250 employees). Colima is free, lighter, CLI-only. Fine for RxNav-in-a-Box. |

---

## What NOT to Use

| Avoid | Why | Use Instead |
|-------|-----|-------------|
| Electron | 150-300 MB idle RAM leaves insufficient headroom for 6-35 GB AI models. Node.js full API access requires extensive security hardening for HIPAA. 100-150 MB bundle. | Tauri 2.x |
| Create React App | Deprecated, unmaintained. Slow builds. | Vite (Tauri default) |
| Moment.js | Deprecated, 300KB+ bundle. Mutable API causes bugs. | date-fns |
| Redux (classic) | Excessive boilerplate for this app's state complexity. | Zustand |
| MongoDB | No encryption-at-rest equivalent to SQLCipher. No SQLAlchemy dialect for seamless cloud migration. Schema-less is wrong for regulated medical data. | SQLCipher (SQLite) -> PostgreSQL |
| Firebase | Google Cloud without standard BAA path for small teams. No local-first mode. Vendor lock-in. | SQLCipher + PowerSync (Phase 4) |
| OpenAI API (direct) | No BAA on consumer/API tiers without Azure deployment. Data retention policies unclear. | AWS Bedrock (standard BAA, zero retention) |
| Fine-tuned biomedical LLMs | OpenBioLLM-8B scored 30% vs LLaMA-3-8B + RAG at 64% on NEJM cases. Fine-tuning is expensive and underperforms RAG for clinical tasks. | RAG pipeline with general-purpose LLM |
| ESLint + Prettier (separate) | Two tools, complex config, slow. | Biome (single tool, faster) |
| flake8 + black + isort (separate) | Three tools for what Ruff does alone, 100x slower. | Ruff |
| Jest | Slower than Vitest, requires extra config with Vite projects. | Vitest |

---

## Stack Patterns by Variant

**If 16 GB Mac (minimum hardware):**
- Use LLaMA 3.2 3B instead of 3.1 8B (~3 GB vs ~6 GB)
- Use Whisper base or small instead of large-v3-turbo
- Skip pyannote speaker diarization (memory-intensive)
- Total AI footprint: ~4-5 GB, leaving headroom for app + OS

**If 32-64 GB Mac (optimal hardware):**
- Use full pipeline: LLaMA 3.1 8B + Whisper large-v3-turbo + MedSpaCy + pyannote
- Total AI footprint: ~10-15 GB with room for FAISS indices
- Consider MLX for faster Apple Silicon inference

**If MedSpaCy has Python compatibility issues:**
- Use spaCy 3.7+ directly with custom clinical pipeline components
- Implement ConText-like negation detection via spacy-negex or custom rules
- Use SciSpaCy models (these are maintained more actively)
- This fallback still covers 90% of MedSpaCy's value

**If PyInstaller notarization fails:**
- Switch to Nuitka for Python compilation
- Alternative: embed Python via PyO3 (Rust-Python bridge) directly in Tauri, eliminating the sidecar entirely. Higher complexity but better integration.

---

## Version Compatibility Matrix

| Package A | Must Be Compatible With | Risk Level | Notes |
|-----------|------------------------|------------|-------|
| MedSpaCy | Python 3.11/3.12, spaCy 3.7+ | HIGH | Last major release ~2023. Most likely compatibility issue in the stack. Test early. |
| SciSpaCy | spaCy 3.7+, MedSpaCy | MEDIUM | Usually tracks spaCy releases. Verify model compatibility. |
| sqlcipher3/pysqlcipher3 | SQLAlchemy 2.0, Python 3.11+ | MEDIUM | SQLAlchemy 2.0 changed dialect API. Verify binding works. |
| fhir.resources | Pydantic 2.x, FastAPI 0.100+ | LOW | Both Pydantic-based, should align. But verify FHIR R4 model generation. |
| PyInstaller | All Python deps, macOS notarization | MEDIUM | Hidden imports from spaCy/torch/transformers commonly cause issues. Budget 2-3 days for packaging. |
| Tauri sidecar | PyInstaller binary, localhost HTTP | LOW | Well-documented pattern. Tauri 2.x has explicit sidecar support. |
| React 18/19 | @medplum/react, fhir-react | MEDIUM | If upgrading to React 19, verify medical component libraries support it. |
| LangChain | Ollama, FAISS, Pydantic 2.x | MEDIUM | LangChain's rapid release cycle causes breaking changes. Pin versions carefully. |

---

## Installation (Phase 1 Bootstrap)

```bash
# Prerequisites
brew install rust node python@3.11

# Tauri CLI
cargo install tauri-cli

# Frontend
npm create tauri-app@latest medarc -- --template react-ts
cd medarc
npm install @mui/material @mui/icons-material @emotion/react @emotion/styled
npm install react-hook-form @hookform/resolvers zod
npm install @tanstack/react-table @tanstack/react-query
npm install zustand date-fns react-big-calendar
npm install @medplum/react fhir-react

# Dev dependencies
npm install -D vitest @testing-library/react @testing-library/jest-dom
npm install -D @biomejs/biome
npm install -D playwright @playwright/test

# Python sidecar (use venv)
python3.11 -m venv sidecar/.venv
source sidecar/.venv/bin/activate
pip install fastapi uvicorn[standard] pydantic
pip install sqlalchemy alembic
pip install pysqlcipher3  # or sqlcipher3 -- verify which is maintained
pip install fhir.resources
pip install spacy medspacy scispacy
python -m spacy download en_core_web_sm
pip install https://s3-us-west-2.amazonaws.com/ai2-s2-scispacy/releases/v0.5.4/en_core_med7_lg-0.5.4.tar.gz
pip install faiss-cpu langchain langchain-community
pip install ollama  # Python client for Ollama API
pip install pyinstaller
pip install pytest pytest-asyncio httpx  # httpx for FastAPI test client
pip install ruff

# Ollama (separate install)
brew install ollama
ollama pull llama3.1:8b-instruct-q4_K_M

# whisper.cpp (build from source for CoreML)
git clone https://github.com/ggerganov/whisper.cpp.git
cd whisper.cpp && make clean && WHISPER_COREML=1 make -j

# RxNav-in-a-Box (Phase 2)
# docker pull ghcr.io/nicktobey/rxnav-in-a-box  # verify current image location
```

---

## Key Risk Areas

1. **MedSpaCy compatibility (HIGH RISK):** Last major release was ~2023. If it does not support Python 3.11+/spaCy 3.7+, fall back to spaCy + SciSpaCy + custom clinical rules. Test this in week 1.

2. **PyInstaller + AI dependencies (MEDIUM RISK):** Packaging spaCy models, torch, transformers, and FAISS into a single PyInstaller binary is non-trivial. Hidden imports, large binary size (potentially 2-5 GB), and macOS notarization issues are common. Budget 3-5 days for packaging spikes.

3. **SQLCipher Python bindings (MEDIUM RISK):** pysqlcipher3 and sqlcipher3 have had maintenance gaps. Verify SQLAlchemy 2.0 dialect support. Alternative: use Rust-side SQLCipher via rusqlite with Tauri commands for all DB operations, removing Python DB dependency entirely.

4. **LangChain version churn (MEDIUM RISK):** LangChain releases break APIs frequently. Pin exact versions. Consider extracting to custom orchestration if LangChain becomes a maintenance burden.

5. **Whisper hallucination (INHERENT RISK):** ~1% hallucination rate is a known limitation. Human-in-the-loop review is mandatory, not optional. UI must make review frictionless.

---

## Sources

All findings in this document are based on training data with a **May 2025 cutoff**. Web verification tools (WebSearch, WebFetch, Brave API) were unavailable during this research session.

**Confidence breakdown:**
- Tauri 2.x architecture and capabilities: MEDIUM (training data aligns with PRD claims; version specifics need verification)
- React/TypeScript ecosystem: HIGH (stable, well-known ecosystem)
- FastAPI/SQLAlchemy/Alembic: HIGH (mature Python stack, unlikely to have changed significantly)
- SQLCipher: MEDIUM (stable project but verify latest version and Python binding status)
- Ollama/whisper.cpp: MEDIUM (rapidly evolving; versions likely newer than training data)
- MedSpaCy: LOW (maintenance status uncertain; highest risk item in stack)
- FAISS: HIGH (stable Meta project, well-established)
- LangChain: MEDIUM (frequent breaking changes; verify current API patterns)
- Medical UI libraries (@medplum/react, fhir-react): LOW (niche libraries; verify maturity and maintenance)

**Action required:** Before locking the stack, verify all [VERIFY] tagged versions against official GitHub releases and PyPI/npm. Prioritize MedSpaCy and SQLCipher Python binding verification as these are the highest-risk items.

---
*Stack research for: MedArc AI-Powered Desktop EMR*
*Researched: 2026-03-10*

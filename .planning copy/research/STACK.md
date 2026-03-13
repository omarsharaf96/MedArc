# Stack Research

**Domain:** AI-powered desktop EMR — M002 additions (patient-centric UI, AI voice pipeline, billing, e-prescribing)
**Researched:** 2026-03-11
**Confidence:** MEDIUM-HIGH (versions verified via WebSearch against npm/crates.io where possible)

> **Scope:** This document covers ONLY stack additions needed for M002. The existing Tauri 2.x + React 18 + TypeScript + Vite 5 + TailwindCSS 3 + SQLCipher stack is validated and unchanged. Every entry below is something new that does not yet exist in `package.json` or `Cargo.toml`.

---

## Recommended Stack

### Core Technologies (New for M002)

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| react-router | 7.13.1 | Client-side navigation (patient chart tabs, route-per-view) | v7 is the current stable release (last published March 2026). In SPA mode (`ssr: false`), generates a single `index.html` — correct for a Tauri WKWebView context with no server. Import everything from `"react-router"` (react-router-dom is no longer a separate package in v7). Familiar, widely-supported, smaller bundle (~20KB) than TanStack Router (~45KB). |
| zustand | 5.0.11 | Global client state (active patient, auth session, UI state) | v5 dropped React < 18 support and now uses native `useSyncExternalStore`, eliminating the `use-sync-external-store` shim. No boilerplate. Critical for sharing active patient context across the chart tab components without prop-drilling through a nested route tree. |
| whisper-rs | 0.15.1 | Rust bindings for whisper.cpp — voice transcription | Latest stable (released 2025-09-10). Rust-native bindings that call whisper.cpp directly without a Python sidecar. Supports CoreML acceleration on Apple Silicon via the `coreml` feature flag — no separate compilation step vs calling whisper.cpp as a subprocess. Integrates as a Tauri command, keeping audio data in Rust and never crossing an IPC boundary until the transcript is ready. |
| ollama (npm) | latest (~0.5.x) | TypeScript HTTP client for Ollama REST API (SOAP note generation) | Official Ollama JS library (updated February 2026). Supports structured JSON output via `format` parameter — critical for constraining LLaMA 3.1 8B to emit valid SOAP note JSON schemas. Handles streaming responses so the UI can render tokens as they arrive. Use from the React frontend to call `http://127.0.0.1:11434` — no Rust proxy needed since Ollama binds to localhost. |

### Supporting Libraries (New for M002)

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| shadcn/ui | latest (CLI-based) | Patient chart UI components (tabs, cards, badges, command palette) | All patient chart views. shadcn copies component source into the project — no runtime dependency, ~150KB vs MUI's ~300KB. Built on Radix UI primitives (WCAG 2.1 AA). Tauri-specific template exists (`agmmnn/tauri-ui`). Better bundle profile than MUI for dense desktop layouts. NOTE: TailwindCSS is already installed — shadcn slots in without additional CSS tooling. |
| @radix-ui/react-tabs | latest (bundled via shadcn) | Chart tab navigation (Summary / Appointments / Notes / Labs / Rx) | Always via shadcn — do not install Radix UI tabs directly; shadcn manages the version. |
| @tanstack/react-table | 8.21.3 | Sortable/filterable tables: lab results, billing line items, medication lists | All data-grid views within chart tabs. Headless — styling via Tailwind. Virtualization handles large result sets. Already recommended in prior stack research; install now for M002 use. |
| @tanstack/react-query | 5.90.21 | Async state for Tauri IPC calls and Ollama streaming | Managing loading/error/caching for all Tauri command invocations and LLM streaming responses. Already recommended; install now. |
| react-hook-form | 7.x (stable) | Form management for clinical forms (CPT/ICD-10 coding UI, prescription entry) | All multi-field clinical forms. Uncontrolled inputs avoid re-renders on dense forms. NOTE: Do NOT upgrade to v8 (currently beta); v8 breaks `useFieldArray`. |
| @hookform/resolvers | 5.2.2 | Bridges react-hook-form + Zod validation | Required alongside react-hook-form. Supports Zod v4. |
| zod | 4.3.6 | Schema validation for form inputs and LLM JSON output | Runtime validation of SOAP note JSON from Ollama, CPT/ICD-10 coding forms, prescription data. v4 is 14x faster than v3 for string parsing; 57% smaller bundle. |
| cmdk | latest | Command palette for fast CPT/ICD-10/RxNorm code lookup | Provides a keyboard-driven fuzzy-search overlay. Physicians need sub-second code lookup; a full autocomplete list component is too slow for keyboard-first workflows. Used by shadcn for its own Command component — install via shadcn `npx shadcn@latest add command`. |
| date-fns | 3.x | Date math for scheduling, encounter timestamps, billing dates | Already recommended in prior research; install now. Needed for M002 appointment tab date rendering. Do NOT use Moment.js. |

### Rust Crates (New for M002)

| Crate | Version | Purpose | Why |
|-------|---------|---------|-----|
| whisper-rs | 0.15.1 | whisper.cpp bindings for in-process transcription | See Core Technologies above. Add `features = ["coreml"]` for Apple Neural Engine acceleration. |
| cpal | 0.15.x | Low-level audio capture (microphone input → PCM buffer) | Pure-Rust cross-platform audio input. Used by `tauri-plugin-mic-recorder` internally; also usable directly for finer control over the audio stream. On macOS requires `CoreAudio` framework linkage (handled by cpal's build script). |
| hound | 3.5.x | Write PCM audio to WAV format | whisper.cpp requires 16kHz mono 16-bit WAV input. hound writes that format from cpal's PCM output. Simple API, no dependencies. |
| reqwest | 0.12.x | HTTP client for Ollama REST API (Rust-side) | Needed if any Tauri commands need to call Ollama directly (e.g., background summarization, batch coding). Feature: `json`. Already transitively present via tauri-plugin-http but declare explicitly for Tauri command usage. |
| tokio | 1.x | Async runtime for reqwest + background whisper processing | Tauri 2.x uses tokio internally; no separate install needed, but ensure `features = ["full"]` if adding background tasks. |

### Billing: X12 837P Generation

| Approach | Recommendation | Rationale |
|----------|---------------|-----------|
| Rust-side generation via string template + x12-types | **Use this** | x12-types crate provides ASC X12 type bindings. 837P is a highly structured, deterministic document — a well-tested Rust function that emits the fixed-format string is more auditable than a JS library that may have undocumented edge cases. The claim file is assembled from SQLCipher data already in Rust, so no IPC round-trip needed. |
| node-x12 npm | Fallback only | JavaScript library for X12 parsing/generation. Functional but adds a JS dependency for work that is better done in Rust where the data already lives. Use if the Rust approach proves too time-consuming in a given sprint. |

Cargo.toml addition for billing:
```toml
x12-types = "0.x"   # verify latest on crates.io; provides X12 segment type definitions
```

The 837P generation logic should live in a dedicated Rust module (`src-tauri/src/billing/`) with a Tauri command that returns the raw X12 string to the frontend for display/download.

### E-Prescribing: Weno Exchange Integration

| Integration Method | Recommendation | Rationale |
|-------------------|---------------|-----------|
| NONCE-based iframe embed | **Use this** | Weno Exchange's documented integration pattern for EHRs is an iframe that hosts Weno Online's DEA 1311.120-compliant prescribing screens. The EHR sends patient/provider context via JSON in the request, Weno generates a single-use NONCE key, and the EHR renders the Weno UI inside an iframe. No custom prescribing UI to build — Weno provides the certified EPCS workflow. OpenEMR v7.0.2+ uses this exact pattern. |
| DIY Rx UI + Switch API | Avoid for M002 | The Switch API (direct network connection) requires custom Rx composition UI and full DEA EPCS certification. Months of additional work. The iframe approach gets to a working state in days. |

**Implementation notes for Tauri:**
- Tauri's WKWebView can render iframes pointing to external HTTPS URLs if the CSP is configured to allow `frame-src https://*.wenoexchange.com`
- The NONCE request is a server-side POST from Rust (via `reqwest`) to Weno's API — patient PII stays in Rust, never touches the React layer until it returns as a signed NONCE
- Register at wenoexchange.com for a developer account and schedule a kick-off meeting before writing integration code (required by Weno)
- No npm package needed; the integration is Rust HTTP + React iframe

### RxNorm Drug Search

| Approach | Recommendation | Rationale |
|----------|---------------|-----------|
| NLM RxNav REST API (localhost via RxNav-in-a-Box) | **Use this** | RxNav-in-a-Box is the NLM-provided Docker composition that runs the complete RxNorm API stack locally. No cloud dependency, no BAA needed. Tauri command wraps `reqwest` call to `http://localhost:4000/REST/drugs.json?name=<query>`. Already in the prior stack research. |
| react-icd10 or custom autocomplete component | For ICD-10/CPT | The `react-icd10` npm package queries the NLM ICD-10 API. For CPT codes, the AMA does not provide a free API — use a bundled SQLite lookup table (CPT codes can be licensed and stored locally in SQLCipher). The `cmdk` command component handles the typeahead UI for both. |

---

## Installation

```bash
# Client-side routing and state
npm install react-router zustand

# Patient chart UI (shadcn — interactive CLI, run for each component needed)
npx shadcn@latest init
npx shadcn@latest add tabs card badge command table

# Data tables + async state
npm install @tanstack/react-table @tanstack/react-query

# Forms and validation
npm install react-hook-form @hookform/resolvers zod

# LLM client
npm install ollama

# Date utilities
npm install date-fns

# Dev — React Query devtools
npm install -D @tanstack/react-query-devtools
```

```toml
# src-tauri/Cargo.toml additions
[dependencies]
whisper-rs = { version = "0.15", features = ["coreml"] }
cpal = "0.15"
hound = "3.5"
reqwest = { version = "0.12", features = ["json", "stream"] }
x12-types = "0.x"   # verify version on crates.io before locking
```

---

## Alternatives Considered

| Recommended | Alternative | When to Use Alternative |
|-------------|-------------|-------------------------|
| react-router 7 (SPA mode) | TanStack Router | If maximum TypeScript type safety on route params and search strings is a priority. TanStack Router's file-based routing and automatic type generation are superior in DX, but react-router 7 is a faster migration from zero routing since the team already knows it. Revisit for M003 if route complexity grows. |
| react-router 7 (SPA mode) | No routing (manual tab state in Zustand) | Only if the chart stays as a single page with tabs controlled by state. Acceptable for MVP but prevents deep-linking to a specific patient's lab tab, which is useful for cross-component navigation. |
| whisper-rs (in-process) | whisper.cpp as Tauri sidecar subprocess | The sidecar approach (spawn a CLI binary, pipe audio, read stdout) is simpler to set up but slower due to process spawn overhead per recording. In-process via whisper-rs is faster and keeps audio data in Rust memory. Use sidecar only if whisper-rs compilation becomes a problem (e.g., CoreML linking issues). |
| whisper-rs (in-process) | WhisperKit (Swift/CoreML) | WhisperKit is published under ICML 2025 and provides even better ANE utilization than whisper.cpp CoreML. But it requires a Swift bridge (Tauri does not have native Swift plugin support in v2 without custom code). Worth revisiting for M003 if transcription quality/speed is insufficient. |
| ollama npm client | Vercel AI SDK (`ai` package) | The AI SDK provides a unified interface over multiple providers including Ollama, with structured output via Zod schemas (`Output.object()`). Add if you need streaming with automatic Zod parsing — otherwise the raw ollama client is lighter. |
| shadcn/ui | Material UI (MUI) 6.x | If the team needs pre-built complex components (e.g., a full data grid with column pinning) out of the box. MUI's DataGrid is more feature-complete than TanStack Table + shadcn out of the box, at the cost of ~150KB extra bundle. |
| Rust X12 generation | node-x12 npm | If the billing engineer prefers JavaScript and the X12 generation logic is simple enough to not need Rust's type safety. For claim generation involving financial data, Rust is preferable. |
| NONCE iframe (Weno) | Weno Switch API (direct) | Only if the practice requires custom Rx composition UI (e.g., pre-filling the Rx form from a SOAP note automatically). The Switch API enables this but requires DEA EPCS certification. Evaluate for M003. |

---

## What NOT to Use

| Avoid | Why | Use Instead |
|-------|-----|-------------|
| react-router-dom (separate package) | In v7, `react-router-dom` is merged into `react-router`. Installing both creates duplicate router instances and confusing errors. | `react-router` only |
| React Router v6 (old) | Already superseded; v7.13.1 is current. No reason to use v6 on a new feature. | react-router 7 |
| react-hook-form v8 (beta) | Beta as of January 2026 with breaking changes to `useFieldArray` keyName API. Production forms should not be on a beta. | react-hook-form 7.x stable |
| Zod v3 | v4.3.6 is current and 14x faster for string parsing. No reason to use v3 on new code. Check `@hookform/resolvers` >= 5.2.2 which supports Zod v4. | zod 4.x |
| Python sidecar for audio/transcription | Adds PyInstaller packaging complexity and ~2 GB sidecar binary for what whisper-rs accomplishes in-process in Rust. The M001 stack already eliminated the Python sidecar for DB operations — keep that pattern for AI too. | whisper-rs in Rust |
| Deepgram / AssemblyAI (cloud STT) | PHI leaves device; requires BAA; adds latency; adds cost. Use only as explicit fallback if local quality is insufficient after medical vocabulary prompting. | whisper-rs (local) |
| OpenAI API (direct) | No BAA on standard API tier. PHI would leave device. | Ollama (local) + AWS Bedrock (cloud fallback with BAA) |
| Custom e-prescribing workflow (DEA EPCS) | Requires DEA certification, audit logging of two-factor identity verification, 2-year tamper-proof log retention per DEA 1311. Months of regulatory compliance work. | Weno Exchange iframe (certified EPCS provided by Weno) |
| Redux Toolkit | Excessive boilerplate for the state complexity in a chart view. Zustand handles active patient + session state without reducers. | Zustand 5 |
| MUI DataGrid (Pro/Premium) | Paid license required for grouping/filtering. TanStack Table + shadcn Table achieves the same for billing and lab result views without licensing. | @tanstack/react-table + shadcn |

---

## Stack Patterns by Variant

**If whisper-rs CoreML compilation fails (M-series Mac build issue):**
- Fall back to whisper.cpp as a Tauri sidecar binary (pre-compiled, embedded via `externalBin` in `tauri.conf.json`)
- Use `tauri-plugin-shell` to spawn it: `shell().sidecar("whisper-cpp").args([...wav_path...]).output()`
- Performance will be ~2-3x slower (no ANE) but functionally correct
- The sidecar binary must be compiled with `WHISPER_COREML=1 make` and bundled per architecture (arm64/x86_64)

**If Ollama is not installed on the user's machine:**
- React frontend should detect a failed `fetch("http://127.0.0.1:11434/api/tags")` and show a setup dialog
- Provide a download link to `ollama.com` and a Tauri command to run `brew install ollama && ollama pull llama3.1:8b-instruct-q4_K_M`
- All AI-dependent UI elements should gracefully degrade to manual entry when Ollama is unreachable

**If RxNav-in-a-Box Docker is not running:**
- Fall back to a bundled SQLite table of RxNorm codes (snapshot from NLM, refreshed quarterly)
- Stored in the existing SQLCipher database under a `rxnorm_drugs` table
- This covers the search use case without requiring Docker to be running

**If Weno Exchange integration is not yet set up (developer account pending):**
- Build the e-prescribing UI with a placeholder iframe pointing to a local mock
- Defer the NONCE API integration to a dedicated sprint once the developer kick-off meeting is complete
- The `<iframe>` component in React can be feature-flagged via an env variable

---

## Version Compatibility

| Package A | Must Be Compatible With | Risk | Notes |
|-----------|------------------------|------|-------|
| react-router 7.13.1 | React 18.3.x | LOW | v7 officially supports React 18. |
| zod 4.3.6 | @hookform/resolvers 5.2.2 | LOW | resolvers 5.2.2 explicitly adds Zod v4 support. Do NOT use resolvers < 5.x with zod 4. |
| react-hook-form 7.x | zod 4.x | LOW | Supported via @hookform/resolvers 5.x. |
| @tanstack/react-table 8.21.3 | React 18 | LOW | v8 supports React 18; v9 in development but not yet stable. |
| @tanstack/react-query 5.90.21 | React 18 | LOW | v5 requires React 18. |
| shadcn/ui (CLI-installed) | TailwindCSS 3.x, Radix UI | LOW | shadcn v3 (CLI) targets Tailwind 3. Do NOT upgrade Tailwind to v4 without migrating shadcn config — breaking CSS variable changes. |
| whisper-rs 0.15.1 | Rust 1.77+ | LOW | Confirmed active development through September 2025. |
| whisper-rs coreml feature | Xcode + CoreML frameworks | MEDIUM | Requires Xcode CommandLineTools on the build machine. CoreML model files (.mlmodelc) must be downloaded separately via `whisper.cpp/models/download-ggml-model.sh`. |
| ollama npm | Node 18+ (Vite 5 environment) | LOW | The npm package is a fetch-based HTTP client, no Node-specific APIs. Works in both Node and browser-like environments. |
| reqwest 0.12 | tokio 1.x | LOW | reqwest 0.12 requires tokio 1.x async runtime. Tauri 2.x already provides tokio 1.x. |
| x12-types | serde 1.x | LOW | Standard serde serialization, already in Cargo.toml. |

---

## Sources

- [react-router npm](https://www.npmjs.com/package/react-router) — v7.13.1 confirmed current (March 2026)
- [TanStack Router vs React Router — Medium/ekino-france](https://medium.com/ekino-france/tanstack-router-vs-react-router-v7-32dddc4fcd58) — comparison (January 2026)
- [React Router SPA mode docs](https://reactrouter.com/how-to/spa) — `ssr: false` configuration confirmed
- [whisper-rs crates.io](https://crates.io/crates/whisper-rs) — v0.15.1 (released 2025-09-10) MEDIUM confidence
- [ollama-js GitHub](https://github.com/ollama/ollama-js) — active development confirmed (updated February 2026) MEDIUM confidence
- [zustand npm](https://www.npmjs.com/package/zustand) — v5.0.11 current MEDIUM confidence
- [zod v4 release](https://zod.dev/v4) — v4.3.6 current (July 2025 initial release), 14x faster string parsing
- [@hookform/resolvers npm](https://www.npmjs.com/package/@hookform/resolvers) — v5.2.2 supports Zod v4
- [@tanstack/react-query npm](https://www.npmjs.com/package/@tanstack/react-query) — v5.90.21 current
- [@tanstack/react-table npm](https://www.npmjs.com/package/@tanstack/react-table) — v8.21.3 current
- [shadcn vs MUI comparison (2025)](https://makersden.io/blog/react-ui-libs-2025-comparing-shadcn-radix-mantine-mui-chakra) — bundle size and Tauri integration notes
- [tauri-ui template](https://www.shadcn.io/template/agmmnn-tauri-ui) — shadcn + Tauri integration confirmed
- [Weno Exchange OpenEMR wiki](https://www.open-emr.org/wiki/index.php/OpenEMR_ePrescribe) — iframe + NONCE integration pattern confirmed
- [Weno Switch API PDF (July 2025)](https://wenoexchange.com/wp-content/uploads/2025/07/Switch_API_Documentation_07-14-2025.pdf) — binary content, unreadable; defer to developer kick-off
- [tauri-plugin-mic-recorder crates.io](https://crates.io/crates/tauri-plugin-mic-recorder) — v2.0.0 (March 2025) as reference for audio capture pattern
- [WhisperKit ICML 2025](https://github.com/argmaxinc/WhisperKit) — Apple Neural Engine alternative to whisper.cpp, noted for M003 evaluation
- [node-x12 npm](https://www.npmjs.com/package/node-x12) — JavaScript X12 fallback option
- [x12-types crates.io](https://crates.io/crates/x12-types) — Rust X12 type bindings LOW confidence (verify version before use)
- WebSearch results — all version claims above backed by at least one primary source

---
*Stack research for: MedArc M002 — Patient UI, AI Voice Pipeline, Billing, E-Prescribing additions*
*Researched: 2026-03-11*

# T01: 01-desktop-shell-encrypted-database 01

**Slice:** S01 — **Milestone:** M001

## Description

Scaffold the Tauri 2.x desktop application with React/TypeScript frontend, establish the SQLCipher-encrypted database layer with macOS Keychain key management, and set up schema migrations.

Purpose: This plan creates the entire application foundation -- the runnable desktop shell, the encrypted database, and the secure key storage -- which all subsequent plans and phases build upon.

Output: A launchable Tauri macOS app with an encrypted SQLCipher database whose key is stored in macOS Keychain, with automatic schema migrations on startup and a health check command confirming everything works.

## Must-Haves

- [ ] "Tauri desktop application launches on macOS and renders a React page in WKWebView"
- [ ] "Database file on disk is SQLCipher-encrypted (cannot be read by plain sqlite3)"
- [ ] "Encryption key is stored in macOS Keychain, not in any config file or source code"
- [ ] "Schema migrations run automatically on app startup"
- [ ] "A health check command confirms the database is encrypted and operational"

## Files

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

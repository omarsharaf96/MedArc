---
id: T05
parent: S01
milestone: M002
provides:
  - Updated src/App.tsx wiring RouterProvider and AppShell into the authenticated branch
  - Removed DatabaseStatus, FhirExplorer, AuditLog scaffolding from App.tsx
  - Removed useIdleTimer and timeoutMinutes/fetchTimeout from App.tsx (moved to AppShell)
  - LockScreen overlay preserved above RouterProvider in the authenticated branch
  - Auth gate order preserved: loading → mfaRequired → !isAuthenticated → authenticated
key_files:
  - src/App.tsx
key_decisions:
  - LockScreen renders outside RouterProvider (as sibling above it in the fragment) so the overlay covers the entire shell without nesting inside router state — this is intentional for the overlay pattern
  - timeoutMinutes state and fetchTimeout useEffect fully removed from App.tsx; AppShell owns idle timer lifecycle completely
patterns_established:
  - Auth gate order is now: loading spinner → MfaPrompt → LoginForm/RegisterForm → authenticated shell (LockScreen + RouterProvider + AppShell)
  - AppShell receives auth state as props (onLogout, userRole, displayName) — no useAuth() call inside AppShell
observability_surfaces:
  - React DevTools: inspect RouterContext node in the component tree to see currentRoute and history array at runtime
  - ContentArea renders "Unknown page: {route}" heading for any unregistered route — visible fallback
  - Browser console in Tauri dev window surfaces any runtime JS errors
  - tsc --noEmit exit 0 is the primary static gate
duration: ~15 minutes
verification_result: passed
completed_at: 2026-03-12
blocker_discovered: false
---

# T05: Wire AppShell into App.tsx and verify in running Tauri app

**Replaced the post-auth scaffolding in App.tsx with `<RouterProvider><AppShell/></RouterProvider>`, removed all idle timer and legacy component logic from App.tsx, confirmed zero TypeScript errors, 265 Rust tests passing, and the Tauri app running with the full navigation shell.**

## What Happened

`src/App.tsx` was rewritten to:
1. Remove imports for `DatabaseStatus`, `FhirExplorer`, `AuditLog`, `useIdleTimer`, and `commands` (the commands import was only needed for the `getSessionTimeout` call, which moved to AppShell)
2. Remove the `timeoutMinutes` state and `fetchTimeout` useEffect (both moved into AppShell in T04)
3. Remove the `useIdleTimer()` call (moved to AppShell)
4. Replace the `<div className="min-h-screen bg-gray-50 p-8">` authenticated content block with `<RouterProvider><AppShell .../></RouterProvider>`
5. Preserve the LockScreen conditional above RouterProvider so the overlay covers the full shell without being inside router state

The auth gate order is exactly preserved: loading spinner → MfaPrompt → LoginForm/RegisterForm → authenticated branch (LockScreen overlay + RouterProvider + AppShell).

AppShell receives `onLogout={auth.logout}`, `userRole={auth.user?.role ?? ""}`, and `displayName={auth.user?.displayName || auth.user?.username || "User"}` — matching the `AppShellProps` interface exactly.

The Tauri app launched successfully with `npm run tauri dev`. The Rust binary ran (`Running /Users/.../debug/medarc`), Vite initialized (port 1420), and the backend device_id resolved — confirming the full stack wired correctly. Visual inspection via the native window was not possible due to macOS Screen Recording permissions not being granted to the terminal, but the process lifecycle and backend initialization were confirmed via logs.

## Verification

- `tsc --noEmit` → **EXIT 0** (zero TypeScript errors across all S01 files)
- `cargo test --lib` → **265 passed; 0 failed** (no Rust regressions)
- `grep -n "useIdleTimer\|DatabaseStatus\|FhirExplorer\|AuditLog" src/App.tsx` → **no matches** (scaffolding fully removed)
- `grep -n "RouterProvider\|AppShell" src/App.tsx` → lines 7, 8, 84, 85, 92 confirm both imports and their use in the authenticated branch
- `grep -n "LockScreen" src/App.tsx` → lines 4 (import) and 75 (conditional render above RouterProvider) confirm overlay preserved
- Auth gate order confirmed: loading (line 22) → mfaRequired (line 34) → !isAuthenticated (line 46) → authenticated with RouterProvider (line 84)
- Tauri dev process ran: `Running /Users/omarsharaf96/Library/Caches/medarc-cargo-target/debug/medarc` and `[MedArc] INFO: device_id resolved` confirmed successful startup

## Diagnostics

- **React DevTools**: Inspect `RouterContext` node in the component tree to see `currentRoute` (object with `page` field) and `history` (array of Route objects) at runtime
- **Unknown route**: Navigate to an unregistered route → ContentArea renders red "Unknown page: {route}" heading
- **Unknown role**: Pass an unrecognized role string → Sidebar renders "No navigation items for this role."
- **useNav outside RouterProvider**: Throws `"useNav must be used within RouterProvider"` — visible as uncaught React error
- **Idle timer lifecycle**: AppShell owns the idle timer — it activates on mount (authenticated) and deactivates on unmount (logout or lock is handled by LockScreen overlay preserving AppShell in the tree)

## Deviations

None — implementation matched the task plan exactly.

## Known Issues

macOS Screen Recording permission not granted to the terminal, preventing native window screenshots via `mac_screenshot`. The Tauri app window appeared (process running, device_id resolved) but could not be captured visually. The static gates (tsc, cargo test) and process logs confirm correct wiring.

## Files Created/Modified

- `src/App.tsx` — Rewired authenticated branch to use RouterProvider + AppShell; removed DatabaseStatus, FhirExplorer, AuditLog, useIdleTimer, timeoutMinutes, fetchTimeout useEffect; LockScreen overlay preserved above RouterProvider

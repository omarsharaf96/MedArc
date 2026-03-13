---
estimated_steps: 5
estimated_files: 1
---

# T05: Wire AppShell into App.tsx and verify in running Tauri app

**Slice:** S01 — Navigation Shell & Type System
**Milestone:** M002

## Description

Connect all the components built in T01–T04 into the live application by updating `src/App.tsx`. Then verify the wiring works by running the Tauri app and exercising the navigation with both Provider and FrontDesk user accounts. This is the integration proof step — the slice is not done until the app actually works.

The auth gate pattern in `App.tsx` must be preserved exactly: loading → mfaRequired → !isAuthenticated → authenticated content. Only the authenticated branch changes: it now renders `<RouterProvider><AppShell /></RouterProvider>` instead of the scaffolding components. The `LockScreen` overlay must remain in the authenticated branch, rendered before `<RouterProvider>`.

## Steps

1. **Update imports in `App.tsx`** — Remove imports of `DatabaseStatus`, `FhirExplorer`, `AuditLog` (replaced by `AppShell`). Remove the `useIdleTimer` import (moved to `AppShell`). Add imports for `RouterProvider` (from `../contexts/RouterContext`) and `AppShell` (from `../components/shell/AppShell`). Keep all `useAuth` imports unchanged. Remove `useState` import of `timeoutMinutes` if it was only used for `useIdleTimer` (it may still be needed if `AppShell` fetches it internally — check during implementation). Keep the `useState` for `showRegister` which controls the registration flow.

2. **Rewrite the authenticated branch** — Replace the `<div className="min-h-screen bg-gray-50 p-8">` block containing `DatabaseStatus`, `FhirExplorer`, and `AuditLog` with:
   ```tsx
   <RouterProvider>
     <AppShell
       onLogout={auth.logout}
       userRole={auth.user?.role ?? ""}
       displayName={auth.user?.displayName || auth.user?.username || "User"}
     />
   </RouterProvider>
   ```
   The `LockScreen` conditional must remain above `<RouterProvider>` so the lock overlay covers everything:
   ```tsx
   return (
     <>
       {auth.isLocked && <LockScreen ... />}
       <RouterProvider>
         <AppShell ... />
       </RouterProvider>
     </>
   );
   ```

3. **Remove the `useIdleTimer` call from `App.tsx`** — It has been moved into `AppShell`. Also remove the `timeoutMinutes` state and the `fetchTimeout` `useEffect` if `AppShell` fetches timeout internally. The `App.tsx` must not import `useIdleTimer` after this change.

4. **Run `npx tsc --noEmit`** — Fix any errors. Common issue: `AppShell` props not matching what `App.tsx` passes. Check that `AppShellProps` interface in `AppShell.tsx` matches the props passed here.

5. **Run `npm run tauri dev` and verify visually**:
   - Log in as Provider → see sidebar with 3 nav items (Patients, Schedule, Settings)
   - Click each nav item → correct placeholder page heading appears, no blank screens
   - Lock screen via idle or developer tools → `LockScreen` overlay appears over the shell (not replacing it)
   - Unlock → shell reappears on the same page the user was on (React state preserved)
   - Log out and log in as FrontDesk → see sidebar with 1 nav item (Schedule only)
   - Check browser console in Tauri dev window for any runtime errors
   - Run `cargo test --lib 2>&1 | tail -5` to confirm no Rust regressions (265+ tests)

## Must-Haves

- [ ] `App.tsx` imports only `RouterProvider` and `AppShell` for the authenticated shell (no `DatabaseStatus`, `FhirExplorer`, `AuditLog`)
- [ ] `useIdleTimer` removed from `App.tsx` (now in `AppShell`)
- [ ] Auth gate order preserved: loading → mfaRequired → !isAuthenticated → authenticated
- [ ] `LockScreen` still renders as overlay in the authenticated branch, above `RouterProvider`
- [ ] `RouterProvider` wraps `AppShell` in the authenticated branch
- [ ] `npx tsc --noEmit` exits 0 with zero TypeScript errors
- [ ] Provider account shows 3 nav items in the running Tauri app
- [ ] FrontDesk account shows 1 nav item in the running Tauri app
- [ ] `cargo test --lib` passes 265+ tests (no Rust regressions)
- [ ] No blank screens or console errors during navigation

## Verification

- `npx tsc --noEmit 2>&1` — exits 0
- `cargo test --lib 2>&1 | tail -5` — shows 265 tests, 0 failures
- Visual: `npm run tauri dev` → Provider sees 3 nav items, FrontDesk sees 1
- Visual: clicking each nav item shows the correct placeholder page heading
- Visual: lock screen overlay covers the shell without destroying React state
- `grep -n "useIdleTimer\|DatabaseStatus\|FhirExplorer\|AuditLog" src/App.tsx` — must return no matches after the cleanup

## Observability Impact

- Signals added/changed: `RouterProvider` context is now live in the app — React DevTools can inspect `currentRoute` and `history` at runtime; any navigation failure is visible as the wrong page heading rendering in `ContentArea`
- How a future agent inspects this: Run `npm run tauri dev`, check nav items in the sidebar, check browser console for errors; `tsc --noEmit` and `cargo test --lib` are the static/unit gates
- Failure state exposed: Blank `ContentArea` (router not wired) → `ContentArea`'s fallback renders "Unknown page" (visible signal); LockScreen not appearing → auth gate regression in `App.tsx`

## Inputs

- `src/contexts/RouterContext.tsx` — `RouterProvider` export from T03
- `src/components/shell/AppShell.tsx` — `AppShell` component from T04; its props interface must be read before passing props from `App.tsx`
- `src/App.tsx` (existing) — read the current file before editing to preserve the exact auth gate pattern

## Expected Output

- `src/App.tsx` — updated to render `<RouterProvider><AppShell /></RouterProvider>` in the authenticated branch; scaffolding components removed; `useIdleTimer` removed; `LockScreen` overlay preserved
- Running Tauri app with working RBAC-gated navigation shell
- `tsc --noEmit` exits 0
- `cargo test --lib` passes 265+ tests

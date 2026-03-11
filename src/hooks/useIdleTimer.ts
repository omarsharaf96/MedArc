import { useEffect, useRef, useCallback } from "react";
import { commands } from "../lib/tauri";

/** Debounce interval for refresh_session IPC calls (30 seconds). */
const REFRESH_DEBOUNCE_MS = 30_000;

/**
 * Inactivity detection hook.
 *
 * Monitors user activity (mouse, keyboard, click, scroll, touch) and locks the
 * session after the specified timeout of inactivity. Also refreshes the backend
 * session timestamp on activity, debounced to at most once per 30 seconds.
 *
 * @param timeoutMinutes - Inactivity timeout in minutes. Pass 0 or undefined to disable.
 * @param enabled - Whether the timer should be active (e.g., only when authenticated).
 */
export function useIdleTimer(
  timeoutMinutes: number,
  enabled: boolean,
): { resetTimer: () => void } {
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const lastRefreshRef = useRef<number>(0);

  const timeoutMs = timeoutMinutes * 60 * 1000;

  const lockSession = useCallback(async () => {
    try {
      await commands.lockSession();
    } catch {
      // Session may already be locked or unauthenticated -- ignore
    }
  }, []);

  const refreshSessionDebounced = useCallback(async () => {
    const now = Date.now();
    if (now - lastRefreshRef.current < REFRESH_DEBOUNCE_MS) {
      return;
    }
    lastRefreshRef.current = now;
    try {
      await commands.refreshSession();
    } catch {
      // Ignore refresh errors (session may have expired)
    }
  }, []);

  const resetTimer = useCallback(() => {
    if (timerRef.current) {
      clearTimeout(timerRef.current);
    }
    if (timeoutMs > 0 && enabled) {
      timerRef.current = setTimeout(lockSession, timeoutMs);
    }
  }, [timeoutMs, enabled, lockSession]);

  useEffect(() => {
    if (!enabled || timeoutMs <= 0) {
      return;
    }

    const handleActivity = () => {
      resetTimer();
      refreshSessionDebounced();
    };

    // Start the initial timer
    resetTimer();

    // Listen for user activity events
    const events: Array<keyof WindowEventMap> = [
      "mousemove",
      "keydown",
      "click",
      "scroll",
      "touchstart",
    ];
    const options: AddEventListenerOptions = { passive: true };

    for (const event of events) {
      window.addEventListener(event, handleActivity, options);
    }

    return () => {
      if (timerRef.current) {
        clearTimeout(timerRef.current);
      }
      for (const event of events) {
        window.removeEventListener(event, handleActivity);
      }
    };
  }, [enabled, timeoutMs, resetTimer, refreshSessionDebounced]);

  return { resetTimer };
}

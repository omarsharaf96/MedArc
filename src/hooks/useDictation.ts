/**
 * useDictation — Reusable hook for voice-to-text dictation.
 *
 * Wraps the Tauri audio capture + Whisper transcription pipeline.
 * Returns state and controls for a record/transcribe cycle.
 *
 * Usage:
 *   const { isRecording, isTranscribing, audioLevel, start, stop, error } = useDictation({
 *     onTranscript: (text) => setMyInput(prev => prev + text),
 *   });
 */

import { useState, useRef, useCallback, useEffect } from "react";
import { commands } from "../lib/tauri";

export type DictationState = "idle" | "recording" | "transcribing";

interface UseDictationOptions {
  /** Called with the transcribed text when transcription completes. */
  onTranscript: (text: string) => void;
  /** Called on error. */
  onError?: (error: string) => void;
}

interface UseDictationReturn {
  /** Current state of the dictation pipeline. */
  state: DictationState;
  /** Whether audio is currently being recorded. */
  isRecording: boolean;
  /** Whether audio is being transcribed. */
  isTranscribing: boolean;
  /** Current microphone audio level (0.0–1.0). */
  audioLevel: number;
  /** Elapsed recording time in seconds. */
  elapsed: number;
  /** Start recording. */
  start: () => Promise<void>;
  /** Stop recording and begin transcription. */
  stop: () => Promise<void>;
  /** Toggle: start if idle, stop if recording. */
  toggle: () => Promise<void>;
  /** Last error message, if any. */
  error: string | null;
}

export function useDictation({ onTranscript, onError }: UseDictationOptions): UseDictationReturn {
  const [state, setState] = useState<DictationState>("idle");
  const [audioLevel, setAudioLevel] = useState(0);
  const [elapsed, setElapsed] = useState(0);
  const [error, setError] = useState<string | null>(null);

  const recordingIdRef = useRef<string | null>(null);
  const levelPollRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);
  /** Track the peak audio level seen during this recording session. */
  const peakLevelRef = useRef(0);

  // Cleanup on unmount
  useEffect(() => {
    return () => {
      if (levelPollRef.current) clearInterval(levelPollRef.current);
      if (timerRef.current) clearInterval(timerRef.current);
    };
  }, []);

  const start = useCallback(async () => {
    if (state !== "idle") return;
    setError(null);

    try {
      const result = await commands.startAudioRecording();
      recordingIdRef.current = result.recordingId;
      peakLevelRef.current = 0;
      setState("recording");
      setElapsed(0);

      // Poll audio level every 100ms
      levelPollRef.current = setInterval(async () => {
        try {
          const level = await commands.getAudioLevel();
          setAudioLevel(level.level);
          if (level.level > peakLevelRef.current) {
            peakLevelRef.current = level.level;
          }
        } catch {
          // ignore polling errors
        }
      }, 100);

      // Elapsed timer
      timerRef.current = setInterval(() => {
        setElapsed((prev) => prev + 1);
      }, 1000);
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      setError(msg);
      onError?.(msg);
    }
  }, [state, onError]);

  const stop = useCallback(async () => {
    if (state !== "recording" || !recordingIdRef.current) return;

    // Stop polling
    if (levelPollRef.current) {
      clearInterval(levelPollRef.current);
      levelPollRef.current = null;
    }
    if (timerRef.current) {
      clearInterval(timerRef.current);
      timerRef.current = null;
    }
    setAudioLevel(0);

    const recordingId = recordingIdRef.current;
    recordingIdRef.current = null;

    try {
      const stopResult = await commands.stopAudioRecording(recordingId);

      // Check actual peak level from the recorded samples (computed by Rust)
      if (stopResult.peakLevel < 0.005) {
        setState("idle");
        setElapsed(0);
        setError(
          "No audio detected — your microphone may be muted or not permitted. " +
          "Check System Settings > Privacy & Security > Microphone."
        );
        return;
      }
      if (stopResult.peakLevel < 0.02) {
        setState("idle");
        setElapsed(0);
        setError(
          "Audio level was very low. Speak closer to the microphone or check " +
          "System Settings > Sound > Input to verify the correct device is selected."
        );
        return;
      }

      setState("transcribing");

      // Transcribe the audio
      const transcription = await commands.transcribeAudio(stopResult.wavPath, null);
      const text = transcription.text.trim();

      setState("idle");
      setElapsed(0);

      if (text && text !== "[BLANK_AUDIO]") {
        onTranscript(text);
      } else {
        setError("No speech detected. Please try again — speak clearly into the microphone.");
      }
    } catch (err) {
      setState("idle");
      setElapsed(0);
      const msg = err instanceof Error ? err.message : String(err);
      setError(msg);
      onError?.(msg);
    }
  }, [state, onTranscript, onError]);

  const toggle = useCallback(async () => {
    if (state === "idle") {
      await start();
    } else if (state === "recording") {
      await stop();
    }
    // If transcribing, do nothing
  }, [state, start, stop]);

  return {
    state,
    isRecording: state === "recording",
    isTranscribing: state === "transcribing",
    audioLevel,
    elapsed,
    start,
    stop,
    toggle,
    error,
  };
}

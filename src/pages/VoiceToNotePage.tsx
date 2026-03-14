/**
 * VoiceToNotePage.tsx — AI Voice-to-Note UI.
 *
 * Provides a full-screen workflow for providers to record a clinical session,
 * transcribe it locally, generate a structured SOAP note draft via AI, and
 * receive CPT code suggestions. All audio is processed locally and deleted
 * after transcription.
 *
 * Layout: two-column — left = transcript, right = generated note draft.
 * Large, clear controls designed for use during patient encounters.
 *
 * Observability:
 *   - `console.error("[VoiceToNote] …")` logged on command failures
 *   - Recording state, transcription state, generation state all inspectable
 *     via React DevTools
 */
import { useState, useEffect, useRef, useCallback } from "react";
import { useNav } from "../contexts/RouterContext";
import { commands } from "../lib/tauri";
import type {
  TranscriptionResult,
  NoteDraftResult,
  CptSuggestion,
  ConfidenceLevel,
  PatientContext,
  StartRecordingResult,
  StopRecordingResult,
  WhisperModelInfo,
  OllamaStatus,
} from "../types/ai";

/** Internal recording state for this page (not the same as the ai.ts VoiceRecordingState interface). */
type VoiceRecordingState = "idle" | "recording" | "processing";

/** A single field in the AI-generated draft. */
interface NoteDraftField {
  key: string;
  label: string;
  value: string;
  confidence: ConfidenceLevel;
}

// ─── Props ───────────────────────────────────────────────────────────────────

interface VoiceToNotePageProps {
  patientId: string;
  noteType: string;
  role: string;
  userId: string;
}

// ─── Tailwind constants ──────────────────────────────────────────────────────

const INPUT_CLS =
  "w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500";
const LABEL_CLS = "mb-1 block text-sm font-medium text-gray-700";

// ─── Helpers ─────────────────────────────────────────────────────────────────

/** Format seconds as MM:SS. */
function formatElapsedTime(totalSeconds: number): string {
  const mins = Math.floor(totalSeconds / 60);
  const secs = totalSeconds % 60;
  return `${String(mins).padStart(2, "0")}:${String(secs).padStart(2, "0")}`;
}

/** Returns Tailwind classes for confidence-level highlighting. */
function confidenceClasses(confidence: ConfidenceLevel): string {
  switch (confidence) {
    case "low":
      return "bg-amber-100 border-amber-300";
    case "medium":
      return "bg-yellow-50 border-yellow-200";
    case "high":
    default:
      return "bg-white border-gray-300";
  }
}

/** Returns a badge color for CPT confidence. */
function confidenceBadgeClasses(confidence: ConfidenceLevel): string {
  switch (confidence) {
    case "low":
      return "bg-amber-100 text-amber-800";
    case "medium":
      return "bg-yellow-100 text-yellow-800";
    case "high":
    default:
      return "bg-green-100 text-green-800";
  }
}

/** Status display label and color. */
function statusDisplay(state: VoiceRecordingState): { label: string; color: string } {
  switch (state) {
    case "idle":
      return { label: "Ready", color: "text-gray-500" };
    case "recording":
      return { label: "Recording...", color: "text-red-600" };
    case "processing":
      return { label: "Processing...", color: "text-blue-600" };
    default:
      return { label: "Unknown", color: "text-gray-400" };
  }
}

// ─── Microphone icon SVG ─────────────────────────────────────────────────────

function MicIcon({ className }: { className?: string }) {
  return (
    <svg
      className={className}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <path d="M12 1a3 3 0 0 0-3 3v8a3 3 0 0 0 6 0V4a3 3 0 0 0-3-3z" />
      <path d="M19 10v2a7 7 0 0 1-14 0v-2" />
      <line x1="12" y1="19" x2="12" y2="23" />
      <line x1="8" y1="23" x2="16" y2="23" />
    </svg>
  );
}

/** Stop icon (square). */
function StopIcon({ className }: { className?: string }) {
  return (
    <svg
      className={className}
      viewBox="0 0 24 24"
      fill="currentColor"
    >
      <rect x="6" y="6" width="12" height="12" rx="2" />
    </svg>
  );
}

// ─── Recording Section ───────────────────────────────────────────────────────

interface RecordingSectionProps {
  recordingState: VoiceRecordingState;
  audioLevel: number;
  elapsedSeconds: number;
  onToggleRecording: () => void;
  disabled: boolean;
}

function RecordingSection({
  recordingState,
  audioLevel,
  elapsedSeconds,
  onToggleRecording,
  disabled,
}: RecordingSectionProps) {
  const status = statusDisplay(recordingState);
  const isRecording = recordingState === "recording";

  return (
    <div className="flex flex-col items-center gap-4 rounded-lg border border-gray-200 bg-white p-6 shadow-sm">
      {/* Status indicator */}
      <p className={`text-sm font-semibold ${status.color}`}>{status.label}</p>

      {/* Large record/stop button */}
      <button
        type="button"
        onClick={onToggleRecording}
        disabled={disabled || recordingState === "processing"}
        className={[
          "flex h-20 w-20 items-center justify-center rounded-full transition-all focus:outline-none focus:ring-4",
          isRecording
            ? "bg-red-600 text-white hover:bg-red-700 focus:ring-red-300 animate-pulse"
            : "bg-red-100 text-red-600 hover:bg-red-200 focus:ring-red-200",
          disabled || recordingState === "processing"
            ? "opacity-50 cursor-not-allowed"
            : "cursor-pointer",
        ].join(" ")}
        aria-label={isRecording ? "Stop recording" : "Start recording"}
      >
        {isRecording ? (
          <StopIcon className="h-8 w-8" />
        ) : (
          <MicIcon className="h-8 w-8" />
        )}
      </button>

      {/* Recording timer */}
      {(isRecording || elapsedSeconds > 0) && (
        <p className="font-mono text-2xl font-bold text-gray-800">
          {formatElapsedTime(elapsedSeconds)}
        </p>
      )}

      {/* Audio level visualizer */}
      <div className="w-full max-w-xs">
        <div className="h-3 w-full overflow-hidden rounded-full bg-gray-200">
          <div
            className={[
              "h-full rounded-full transition-all duration-100",
              isRecording ? "bg-red-500" : "bg-gray-400",
            ].join(" ")}
            style={{ width: `${Math.min(audioLevel * 100, 100)}%` }}
          />
        </div>
        <p className="mt-1 text-center text-xs text-gray-400">Audio level</p>
      </div>
    </div>
  );
}

// ─── Transcription Panel ─────────────────────────────────────────────────────

interface TranscriptionPanelProps {
  transcribing: boolean;
  transcript: string;
  editMode: boolean;
  onToggleEdit: () => void;
  onTranscriptChange: (text: string) => void;
}

function TranscriptionPanel({
  transcribing,
  transcript,
  editMode,
  onToggleEdit,
  onTranscriptChange,
}: TranscriptionPanelProps) {
  return (
    <div className="flex flex-1 flex-col rounded-lg border border-gray-200 bg-white shadow-sm">
      <div className="flex items-center justify-between border-b border-gray-100 px-4 py-3">
        <h2 className="text-base font-semibold text-gray-800">Transcript</h2>
        {transcript && (
          <button
            type="button"
            onClick={onToggleEdit}
            aria-label="Toggle transcript editing"
            aria-pressed={editMode}
            className="text-xs font-medium text-indigo-600 hover:text-indigo-800"
          >
            {editMode ? "Done Editing" : "Edit Transcript"}
          </button>
        )}
      </div>

      <div className="flex-1 overflow-auto p-4">
        {transcribing ? (
          <div className="flex items-center gap-3 text-sm text-blue-600">
            <svg
              className="h-5 w-5 animate-spin"
              viewBox="0 0 24 24"
              fill="none"
            >
              <circle
                className="opacity-25"
                cx="12"
                cy="12"
                r="10"
                stroke="currentColor"
                strokeWidth="4"
              />
              <path
                className="opacity-75"
                fill="currentColor"
                d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z"
              />
            </svg>
            Transcribing audio...
          </div>
        ) : transcript ? (
          editMode ? (
            <textarea
              className={`${INPUT_CLS} min-h-[200px] resize-y`}
              value={transcript}
              onChange={(e) => onTranscriptChange(e.target.value)}
            />
          ) : (
            <p className="whitespace-pre-wrap text-sm leading-relaxed text-gray-700">
              {transcript}
            </p>
          )
        ) : (
          <p className="text-sm text-gray-400 italic">
            Record a session to generate a transcript.
          </p>
        )}
      </div>
    </div>
  );
}

// ─── Draft Note Panel ────────────────────────────────────────────────────────

interface DraftNotePanelProps {
  generating: boolean;
  generationProgress: number;
  draft: NoteDraftResult | null;
  draftFields: NoteDraftField[];
  onFieldChange: (index: number, value: string) => void;
  cptSuggestions: CptSuggestion[];
  cptLoading: boolean;
  onSuggestCpt: () => void;
  onUseDraft: () => void;
}

function DraftNotePanel({
  generating,
  generationProgress,
  draft,
  draftFields,
  onFieldChange,
  cptSuggestions,
  cptLoading,
  onSuggestCpt,
  onUseDraft,
}: DraftNotePanelProps) {
  return (
    <div className="flex flex-1 flex-col rounded-lg border border-gray-200 bg-white shadow-sm">
      <div className="flex items-center justify-between border-b border-gray-100 px-4 py-3">
        <h2 className="text-base font-semibold text-gray-800">
          AI-Generated Note
        </h2>
        {draft && (
          <button
            type="button"
            onClick={onUseDraft}
            className="rounded-md bg-indigo-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-indigo-700 focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:ring-offset-1"
          >
            Use This Draft
          </button>
        )}
      </div>

      <div className="flex-1 overflow-auto p-4">
        {generating ? (
          <div className="space-y-3">
            <div className="flex items-center gap-3 text-sm text-blue-600">
              <svg
                className="h-5 w-5 animate-spin"
                viewBox="0 0 24 24"
                fill="none"
              >
                <circle
                  className="opacity-25"
                  cx="12"
                  cy="12"
                  r="10"
                  stroke="currentColor"
                  strokeWidth="4"
                />
                <path
                  className="opacity-75"
                  fill="currentColor"
                  d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z"
                />
              </svg>
              Generating note...
            </div>
            {/* Progress bar for streaming-style feedback */}
            <div className="h-2 w-full overflow-hidden rounded-full bg-gray-200">
              <div
                className="h-full rounded-full bg-indigo-500 transition-all duration-500"
                style={{ width: `${generationProgress}%` }}
              />
            </div>
            <p className="text-xs text-gray-400">
              This may take 10-30 seconds...
            </p>
          </div>
        ) : draftFields.length > 0 ? (
          <div className="space-y-4">
            {draftFields.map((field, idx) => (
              <div key={field.key}>
                <div className="mb-1 flex items-center gap-2">
                  <label className={LABEL_CLS} htmlFor={`draft-field-${field.key}`}>
                    {field.label}
                  </label>
                  {field.confidence !== "high" && (
                    <span
                      className={[
                        "rounded-full px-2 py-0.5 text-xs font-medium",
                        confidenceBadgeClasses(field.confidence),
                      ].join(" ")}
                    >
                      {field.confidence} confidence
                    </span>
                  )}
                </div>
                <textarea
                  id={`draft-field-${field.key}`}
                  className={[
                    "w-full rounded-md border px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500 min-h-[80px] resize-y",
                    confidenceClasses(field.confidence),
                  ].join(" ")}
                  value={field.value}
                  onChange={(e) => onFieldChange(idx, e.target.value)}
                />
              </div>
            ))}

            {/* CPT Code Suggestions */}
            <div className="border-t border-gray-100 pt-4">
              <div className="mb-3 flex items-center justify-between">
                <h3 className="text-sm font-semibold text-gray-700">
                  CPT Code Suggestions
                </h3>
                <button
                  type="button"
                  onClick={onSuggestCpt}
                  disabled={cptLoading}
                  aria-label="Suggest CPT codes"
                  className="rounded-md bg-gray-100 px-3 py-1.5 text-xs font-medium text-gray-700 hover:bg-gray-200 focus:outline-none focus:ring-2 focus:ring-gray-400 focus:ring-offset-1 disabled:opacity-50"
                >
                  {cptLoading ? "Loading..." : "Suggest CPT Codes"}
                </button>
              </div>

              {cptSuggestions.length > 0 ? (
                <div className="flex flex-wrap gap-2">
                  {cptSuggestions.map((cpt) => (
                    <div
                      key={cpt.code}
                      className="inline-flex items-center gap-2 rounded-full border border-gray-200 bg-gray-50 px-3 py-1.5"
                    >
                      <span className="text-sm font-mono font-semibold text-gray-800">
                        {cpt.code}
                      </span>
                      <span className="text-xs text-gray-500">
                        {cpt.description}
                      </span>
                      <span
                        className={[
                          "rounded-full px-1.5 py-0.5 text-xs font-medium",
                          confidenceBadgeClasses(cpt.confidence),
                        ].join(" ")}
                      >
                        {cpt.confidence}
                      </span>
                    </div>
                  ))}
                </div>
              ) : (
                !cptLoading && (
                  <p className="text-xs text-gray-400 italic">
                    Click "Suggest CPT Codes" after note generation.
                  </p>
                )
              )}
            </div>
          </div>
        ) : (
          <p className="text-sm text-gray-400 italic">
            Generate a note from the transcript to see the draft here.
          </p>
        )}
      </div>
    </div>
  );
}

// ─── Whisper Model Setup Banner ───────────────────────────────────────────────

interface WhisperModelBannerProps {
  modelInfo: WhisperModelInfo | null;
  checking: boolean;
  downloading: boolean;
  onDownload: () => void;
}

function WhisperModelBanner({
  modelInfo,
  checking,
  downloading,
  onDownload,
}: WhisperModelBannerProps) {
  if (checking) {
    return (
      <div className="mb-4 flex items-center gap-2 rounded-md border border-gray-200 bg-gray-50 px-3 py-2 text-xs text-gray-600">
        <svg className="h-4 w-4 animate-spin" viewBox="0 0 24 24" fill="none">
          <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
          <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
        </svg>
        Checking Whisper model status...
      </div>
    );
  }

  if (!modelInfo) return null;

  if (modelInfo.downloaded) {
    return (
      <div className="mb-4 flex items-center gap-2 rounded-md border border-green-200 bg-green-50 px-3 py-2 text-xs text-green-700">
        <svg className="h-4 w-4 shrink-0" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2}>
          <path strokeLinecap="round" strokeLinejoin="round" d="M5 13l4 4L19 7" />
        </svg>
        Whisper ({modelInfo.modelSize}) ready
        {modelInfo.fileSizeMb !== null && (
          <span className="text-green-500">({modelInfo.fileSizeMb.toFixed(0)} MB)</span>
        )}
      </div>
    );
  }

  return (
    <div className="mb-4 rounded-md border border-amber-300 bg-amber-50 px-4 py-3">
      <div className="flex items-start gap-3">
        <svg className="mt-0.5 h-5 w-5 shrink-0 text-amber-600" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2}>
          <path strokeLinecap="round" strokeLinejoin="round" d="M12 9v2m0 4h.01M10.29 3.86L1.82 18a2 2 0 001.71 3h16.94a2 2 0 001.71-3L13.71 3.86a2 2 0 00-3.42 0z" />
        </svg>
        <div className="flex-1">
          <p className="text-sm font-semibold text-amber-800">Whisper model not downloaded</p>
          <p className="mt-0.5 text-xs text-amber-700">
            The &ldquo;{modelInfo.modelSize}&rdquo; transcription model (~{modelInfo.fileSizeMb ?? "466"} MB) must be
            downloaded before recording. Run this command in Terminal:
          </p>
          <pre className="mt-1.5 overflow-x-auto rounded bg-amber-100 px-2 py-1 text-xs font-mono text-amber-900">
            {`mkdir -p "${modelInfo.modelPath.replace(/\/[^/]+$/, "")}" && curl -L -o "${modelInfo.modelPath}" "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/${modelInfo.modelPath.split("/").pop()}"`}
          </pre>
        </div>
        <button
          type="button"
          onClick={onDownload}
          disabled={downloading}
          className="shrink-0 rounded-md bg-amber-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-amber-700 focus:outline-none focus:ring-2 focus:ring-amber-500 focus:ring-offset-1 disabled:opacity-50"
        >
          {downloading ? "Requesting..." : "Show Download Instructions"}
        </button>
      </div>
    </div>
  );
}

// ─── Ollama Status Banner ─────────────────────────────────────────────────────

interface OllamaStatusBannerProps {
  status: OllamaStatus | null;
  checking: boolean;
}

function OllamaStatusBanner({ status, checking }: OllamaStatusBannerProps) {
  if (checking || !status) return null;

  if (status.available) {
    return (
      <div className="mb-4 flex items-center gap-2 rounded-md border border-green-200 bg-green-50 px-3 py-2 text-xs text-green-700">
        <svg className="h-4 w-4 shrink-0" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2}>
          <path strokeLinecap="round" strokeLinejoin="round" d="M5 13l4 4L19 7" />
        </svg>
        Ollama running · Models: {status.models.length > 0 ? status.models.join(", ") : "none loaded"}
      </div>
    );
  }

  return (
    <div className="mb-4 rounded-md border border-red-300 bg-red-50 px-4 py-3">
      <div className="flex items-start gap-3">
        <svg className="mt-0.5 h-5 w-5 shrink-0 text-red-600" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2}>
          <circle cx="12" cy="12" r="10" />
          <line x1="12" y1="8" x2="12" y2="12" />
          <line x1="12" y1="16" x2="12.01" y2="16" />
        </svg>
        <div>
          <p className="text-sm font-semibold text-red-800">Ollama not available</p>
          <p className="mt-0.5 text-xs text-red-700">
            Note generation requires Ollama running locally. Install and start it:
          </p>
          <pre className="mt-1.5 overflow-x-auto rounded bg-red-100 px-2 py-1 text-xs font-mono text-red-900">
            {`brew install ollama && ollama serve\nollama pull llama3.1:8b`}
          </pre>
          {status.error && (
            <p className="mt-1 text-xs text-red-600">{status.error}</p>
          )}
        </div>
      </div>
    </div>
  );
}

// ─── Main Page Component ─────────────────────────────────────────────────────

export function VoiceToNotePage({
  patientId,
  noteType,
  role: _role,
  userId: _userId,
}: VoiceToNotePageProps) {
  const { goBack } = useNav();

  // ── Whisper model state ──────────────────────────────────────────────────
  const [whisperModelInfo, setWhisperModelInfo] = useState<WhisperModelInfo | null>(null);
  const [checkingWhisper, setCheckingWhisper] = useState(true);
  const [downloadingWhisper, setDownloadingWhisper] = useState(false);

  // ── Ollama status state ──────────────────────────────────────────────────
  const [ollamaStatus, setOllamaStatus] = useState<OllamaStatus | null>(null);
  const [checkingOllama, setCheckingOllama] = useState(true);

  // ── Recording state ──────────────────────────────────────────────────────
  const [recordingState, setVoiceRecordingState] = useState<VoiceRecordingState>("idle");
  const [audioLevel, setAudioLevel] = useState(0);
  const [elapsedSeconds, setElapsedSeconds] = useState(0);
  const [recordingError, setRecordingError] = useState<string | null>(null);

  // ── Transcription state ──────────────────────────────────────────────────
  const [transcribing, setTranscribing] = useState(false);
  const [transcript, setTranscript] = useState("");
  const [transcriptionResult, setTranscriptionResult] =
    useState<TranscriptionResult | null>(null);
  const [editMode, setEditMode] = useState(false);
  const [transcriptionError, setTranscriptionError] = useState<string | null>(
    null,
  );
  /** WAV path retained after stop so the user can retry transcription on failure. */
  const pendingWavPathRef = useRef<string | null>(null);
  /** React state mirror of pendingWavPathRef so the Retry button renders correctly. */
  const [hasPendingWav, setHasPendingWav] = useState(false);

  // ── Note generation state ────────────────────────────────────────────────
  const [generating, setGenerating] = useState(false);
  const [generationProgress, setGenerationProgress] = useState(0);
  const [draft, setDraft] = useState<NoteDraftResult | null>(null);
  const [draftFields, setDraftFields] = useState<NoteDraftField[]>([]);
  const [generationError, setGenerationError] = useState<string | null>(null);

  // ── CPT state ────────────────────────────────────────────────────────────
  const [cptSuggestions, setCptSuggestions] = useState<CptSuggestion[]>([]);
  const [cptLoading, setCptLoading] = useState(false);
  const [cptError, setCptError] = useState<string | null>(null);

  // ── Recording ID ref ─────────────────────────────────────────────────────
  const recordingIdRef = useRef<string | null>(null);

  // ── Refs for intervals ───────────────────────────────────────────────────
  const audioLevelIntervalRef = useRef<ReturnType<typeof setInterval> | null>(
    null,
  );
  const timerIntervalRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const progressIntervalRef = useRef<ReturnType<typeof setInterval> | null>(
    null,
  );

  // ── On-mount: check Whisper model and Ollama status ───────────────────────
  useEffect(() => {
    let cancelled = false;

    async function checkSetup() {
      // Check Whisper model
      setCheckingWhisper(true);
      try {
        const info = await commands.checkWhisperModel();
        if (!cancelled) setWhisperModelInfo(info);
      } catch (e) {
        if (!cancelled) {
          console.error("[VoiceToNote] checkWhisperModel failed:", e);
          setWhisperModelInfo(null);
        }
      } finally {
        if (!cancelled) setCheckingWhisper(false);
      }

      // Check Ollama status
      setCheckingOllama(true);
      try {
        const status = await commands.checkOllamaStatus();
        if (!cancelled) setOllamaStatus(status);
      } catch (e) {
        if (!cancelled) {
          console.error("[VoiceToNote] checkOllamaStatus failed:", e);
          setOllamaStatus({ available: false, models: [], error: String(e) });
        }
      } finally {
        if (!cancelled) setCheckingOllama(false);
      }
    }

    void checkSetup();
    return () => { cancelled = true; };
  }, []);

  // ── Cleanup: stop recording if navigating away mid-session ────────────────
  useEffect(() => {
    return () => {
      if (audioLevelIntervalRef.current)
        clearInterval(audioLevelIntervalRef.current);
      if (timerIntervalRef.current) clearInterval(timerIntervalRef.current);
      if (progressIntervalRef.current)
        clearInterval(progressIntervalRef.current);
      // Best-effort stop of any in-progress recording (fire-and-forget)
      const idToStop = recordingIdRef.current;
      if (idToStop) {
        void commands.stopAudioRecording(idToStop).catch(() => {
          // Ignore errors on unmount — backend will eventually time out
        });
        recordingIdRef.current = null;
      }
    };
  }, []);

  // ── Start audio level polling ────────────────────────────────────────────
  const startAudioLevelPolling = useCallback(() => {
    if (audioLevelIntervalRef.current)
      clearInterval(audioLevelIntervalRef.current);

    audioLevelIntervalRef.current = setInterval(async () => {
      try {
        const result = await commands.getAudioLevel();
        setAudioLevel(result.level);
      } catch {
        // Silently ignore level polling errors during recording
      }
    }, 100);
  }, []);

  const stopAudioLevelPolling = useCallback(() => {
    if (audioLevelIntervalRef.current) {
      clearInterval(audioLevelIntervalRef.current);
      audioLevelIntervalRef.current = null;
    }
    setAudioLevel(0);
  }, []);

  // ── Start/Stop timer ─────────────────────────────────────────────────────
  const startTimer = useCallback(() => {
    setElapsedSeconds(0);
    if (timerIntervalRef.current) clearInterval(timerIntervalRef.current);

    timerIntervalRef.current = setInterval(() => {
      setElapsedSeconds((prev) => prev + 1);
    }, 1000);
  }, []);

  const stopTimer = useCallback(() => {
    if (timerIntervalRef.current) {
      clearInterval(timerIntervalRef.current);
      timerIntervalRef.current = null;
    }
  }, []);

  // ── Transcription helper (shared by initial run and retry) ───────────────
  const runTranscription = useCallback(async (wavPath: string) => {
    setTranscribing(true);
    setTranscriptionError(null);
    try {
      const result = await commands.transcribeAudio(wavPath);
      setTranscriptionResult(result);
      setTranscript(result.text);
      // WAV deleted by backend on success — clear the pending ref
      pendingWavPathRef.current = null;
      setHasPendingWav(false);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      console.error("[VoiceToNote] transcribeAudio failed:", msg);
      setTranscriptionError(msg);
      // Keep pendingWavPathRef so the user can retry
    } finally {
      setTranscribing(false);
    }
  }, []);

  // ── Retry transcription ───────────────────────────────────────────────────
  const handleRetryTranscription = useCallback(async () => {
    const wavPath = pendingWavPathRef.current;
    if (!wavPath) return;
    await runTranscription(wavPath);
  }, [runTranscription]);

  // ── Toggle recording ─────────────────────────────────────────────────────
  const handleToggleRecording = useCallback(async () => {
    if (recordingState === "recording") {
      // Stop recording
      setVoiceRecordingState("processing");
      stopTimer();
      stopAudioLevelPolling();
      setRecordingError(null);

      try {
        const stopResult: StopRecordingResult = await commands.stopAudioRecording(recordingIdRef.current ?? "");
        recordingIdRef.current = null;
        // Store path so transcription can be retried on failure
        pendingWavPathRef.current = stopResult.wavPath;
        setHasPendingWav(true);
        // Automatically start transcription
        await runTranscription(stopResult.wavPath);
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e);
        console.error("[VoiceToNote] stopAudioRecording failed:", msg);
        // Check for microphone permission denial specifically
        if (msg.toLowerCase().includes("permission") || msg.toLowerCase().includes("denied")) {
          setRecordingError(
            "Microphone access was denied. Go to System Settings → Privacy & Security → Microphone and allow MedArc, then try again."
          );
        } else {
          setRecordingError(msg);
        }
      } finally {
        setVoiceRecordingState("idle");
      }
    } else {
      // Start recording
      setRecordingError(null);
      try {
        const startResult: StartRecordingResult = await commands.startAudioRecording();
        recordingIdRef.current = startResult.recordingId;
        setVoiceRecordingState("recording");
        startTimer();
        startAudioLevelPolling();
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e);
        console.error("[VoiceToNote] startAudioRecording failed:", msg);
        if (msg.toLowerCase().includes("microphone") || msg.toLowerCase().includes("no input") || msg.toLowerCase().includes("permission") || msg.toLowerCase().includes("denied")) {
          setRecordingError(
            "No microphone found or access denied. Check System Settings → Privacy & Security → Microphone."
          );
        } else {
          setRecordingError(msg);
        }
      }
    }
  }, [
    recordingState,
    startTimer,
    stopTimer,
    startAudioLevelPolling,
    stopAudioLevelPolling,
    runTranscription,
  ]);

  // ── Download Whisper model (shows instructions via error banner) ──────────
  const handleDownloadWhisper = useCallback(async () => {
    setDownloadingWhisper(true);
    try {
      // Backend returns an error with the curl command — this is intentional
      // for Phase 1. We re-check model status afterwards in case user manually
      // placed the file between calls.
      await commands.downloadWhisperModel().catch(() => {
        // Expected to fail with instructions — that's OK
      });
      // Re-check status
      const info = await commands.checkWhisperModel();
      setWhisperModelInfo(info);
    } catch (e) {
      console.error("[VoiceToNote] downloadWhisperModel check failed:", e);
    } finally {
      setDownloadingWhisper(false);
    }
  }, []);

  // ── Generate note ────────────────────────────────────────────────────────
  const handleGenerateNote = useCallback(async () => {
    if (!transcript.trim()) return;

    setGenerating(true);
    setGenerationError(null);
    setGenerationProgress(0);

    // Simulated progress bar for streaming-style feedback
    if (progressIntervalRef.current)
      clearInterval(progressIntervalRef.current);
    progressIntervalRef.current = setInterval(() => {
      setGenerationProgress((prev) => {
        if (prev >= 90) return prev; // Cap at 90% until complete
        return prev + Math.random() * 8;
      });
    }, 500);

    try {
      const patientContext: PatientContext = {
        patientId,
        demographics: null,
        priorNoteSummary: null,
        diagnosis: null,
      };

      const result = await commands.generateNoteDraft(
        transcript,
        noteType,
        patientContext,
      );
      setDraft(result);
      setDraftFields(Object.entries(result.fields).map(([key, val]) => ({
        key,
        label: key.replace(/([A-Z])/g, " $1").replace(/^./, (s) => s.toUpperCase()),
        value: String(val ?? ""),
        confidence: (result.confidence[key] ?? "high") as ConfidenceLevel,
      })));
      setGenerationProgress(100);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      console.error("[VoiceToNote] generateNoteDraft failed:", msg);
      setGenerationError(msg);
    } finally {
      setGenerating(false);
      if (progressIntervalRef.current) {
        clearInterval(progressIntervalRef.current);
        progressIntervalRef.current = null;
      }
    }
  }, [transcript, noteType, patientId]);

  // ── Suggest CPT codes ────────────────────────────────────────────────────
  const handleSuggestCpt = useCallback(async () => {
    if (draftFields.length === 0) return;

    setCptLoading(true);
    setCptError(null);

    try {
      // Combine all draft field values into a single string for CPT suggestion
      const noteDraftText = draftFields
        .map((f) => `${f.label}: ${f.value}`)
        .join("\n\n");

      const suggestions = await commands.suggestCptCodes(noteDraftText);
      setCptSuggestions(suggestions);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      console.error("[VoiceToNote] suggestCptCodes failed:", msg);
      setCptError(msg);
    } finally {
      setCptLoading(false);
    }
  }, [draftFields, noteType]);

  // ── Field change handler ─────────────────────────────────────────────────
  const handleFieldChange = useCallback(
    (index: number, value: string) => {
      setDraftFields((prev) => {
        const updated = [...prev];
        updated[index] = { ...updated[index], value };
        return updated;
      });
    },
    [],
  );

  // ── Use draft — navigate back to encounter workspace ─────────────────────
  const handleUseDraft = useCallback(() => {
    // Navigate back — the draft data would be passed via a shared context
    // or state management in a full implementation. For now, navigate back.
    goBack();
  }, [goBack]);

  // Convenience: suppress unused-variable warnings for optional result data
  void transcriptionResult;

  // ── Render ───────────────────────────────────────────────────────────────
  return (
    <div className="flex h-full flex-col p-6">
      {/* ── Page header ───────────────────────────────────────────────────── */}
      <div className="mb-5 flex items-center gap-3">
        <button
          type="button"
          onClick={goBack}
          className="rounded-md p-1.5 text-gray-500 hover:bg-gray-100 hover:text-gray-700 focus:outline-none focus:ring-2 focus:ring-gray-400 focus:ring-offset-1"
          aria-label="Go back"
        >
          ← Back
        </button>
        <div className="flex flex-1 items-center gap-3">
          <div>
            <h1 className="text-xl font-bold text-gray-900">
              Voice-to-Note
            </h1>
            <p className="mt-0.5 text-sm text-gray-500">
              Record, transcribe, and generate clinical notes
            </p>
          </div>
        </div>

        {/* Generate Note button */}
        <button
          type="button"
          onClick={handleGenerateNote}
          disabled={
            !transcript.trim() || generating || recordingState === "recording"
          }
          aria-label={generating ? "Generating note, please wait" : "Generate clinical note from transcript"}
          aria-busy={generating}
          className="rounded-md bg-indigo-600 px-4 py-2 text-sm font-medium text-white hover:bg-indigo-700 focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:ring-offset-2 disabled:opacity-50 disabled:cursor-not-allowed"
        >
          {generating ? "Generating..." : "Generate Note"}
        </button>
      </div>

      {/* ── Privacy notice ────────────────────────────────────────────────── */}
      <div className="mb-4 flex items-center gap-2 rounded-md border border-blue-200 bg-blue-50 px-3 py-2 text-xs text-blue-700">
        <svg
          className="h-4 w-4 shrink-0"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth={2}
          strokeLinecap="round"
          strokeLinejoin="round"
        >
          <rect x="3" y="11" width="18" height="11" rx="2" ry="2" />
          <path d="M7 11V7a5 5 0 0 1 10 0v4" />
        </svg>
        All audio is processed locally and deleted after transcription
      </div>

      {/* ── Whisper model setup banner ─────────────────────────────────────── */}
      <WhisperModelBanner
        modelInfo={whisperModelInfo}
        checking={checkingWhisper}
        downloading={downloadingWhisper}
        onDownload={handleDownloadWhisper}
      />

      {/* ── Ollama status banner ───────────────────────────────────────────── */}
      <OllamaStatusBanner
        status={ollamaStatus}
        checking={checkingOllama}
      />

      {/* ── Error banners ─────────────────────────────────────────────────── */}
      {recordingError && (
        <div className="mb-3 rounded-md border border-red-200 bg-red-50 px-4 py-2 text-sm text-red-700">
          <span className="font-semibold">Recording error:</span>{" "}
          {recordingError}
        </div>
      )}
      {transcriptionError && (
        <div className="mb-3 flex items-start justify-between gap-3 rounded-md border border-red-200 bg-red-50 px-4 py-2 text-sm text-red-700">
          <span>
            <span className="font-semibold">Transcription error:</span>{" "}
            {transcriptionError}
          </span>
          {hasPendingWav && (
            <button
              type="button"
              onClick={handleRetryTranscription}
              disabled={transcribing}
              className="shrink-0 rounded-md bg-red-600 px-2 py-1 text-xs font-medium text-white hover:bg-red-700 focus:outline-none focus:ring-2 focus:ring-red-500 focus:ring-offset-1 disabled:opacity-50"
            >
              Retry
            </button>
          )}
        </div>
      )}
      {generationError && (
        <div className="mb-3 rounded-md border border-red-200 bg-red-50 px-4 py-2 text-sm text-red-700">
          <span className="font-semibold">Generation error:</span>{" "}
          {generationError}
          {!ollamaStatus?.available && (
            <p className="mt-1 text-xs">
              Ollama is not running. Start it with: <code className="font-mono">ollama serve</code>
            </p>
          )}
        </div>
      )}
      {cptError && (
        <div className="mb-3 rounded-md border border-red-200 bg-red-50 px-4 py-2 text-sm text-red-700">
          <span className="font-semibold">CPT suggestion error:</span>{" "}
          {cptError}
        </div>
      )}

      {/* ── Recording section ─────────────────────────────────────────────── */}
      <div className="mb-5">
        <RecordingSection
          recordingState={recordingState}
          audioLevel={audioLevel}
          elapsedSeconds={elapsedSeconds}
          onToggleRecording={handleToggleRecording}
          disabled={whisperModelInfo !== null && !whisperModelInfo.downloaded}
        />
      </div>

      {/* ── Two-column layout: Transcript (left) | Generated Note (right) ── */}
      <div className="flex flex-1 gap-5 overflow-hidden">
        {/* Left column: Transcript */}
        <TranscriptionPanel
          transcribing={transcribing}
          transcript={transcript}
          editMode={editMode}
          onToggleEdit={() => setEditMode((prev) => !prev)}
          onTranscriptChange={setTranscript}
        />

        {/* Right column: Generated Note */}
        <DraftNotePanel
          generating={generating}
          generationProgress={generationProgress}
          draft={draft}
          draftFields={draftFields}
          onFieldChange={handleFieldChange}
          cptSuggestions={cptSuggestions}
          cptLoading={cptLoading}
          onSuggestCpt={handleSuggestCpt}
          onUseDraft={handleUseDraft}
        />
      </div>
    </div>
  );
}

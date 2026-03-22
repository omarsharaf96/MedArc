/**
 * TypeScript types for AI Voice-to-Note: audio capture and Whisper transcription.
 *
 * Field names use camelCase to match the Rust structs'
 * #[serde(rename_all = "camelCase")]. Option<T> in Rust maps to T | null here.
 */

// ─────────────────────────────────────────────────────────────────────────────
// Audio Capture types (M003/S03)
// ─────────────────────────────────────────────────────────────────────────────

/** Returned when a recording starts. */
export interface StartRecordingResult {
  /** Unique identifier for this recording session. */
  recordingId: string;
}

/** Returned when a recording stops. */
export interface StopRecordingResult {
  /** Identifier of the recording that was stopped. */
  recordingId: string;
  /** Path to the temporary WAV file (deleted after transcription). */
  wavPath: string;
  /** Duration of the recording in seconds. */
  durationSeconds: number;
  /** Peak audio level from recorded samples (0.0–1.0). */
  peakLevel: number;
}

/** Current audio level for the frontend visualizer. */
export interface AudioLevel {
  /** Peak audio amplitude (0.0–1.0). */
  level: number;
  /** Whether a recording is currently active. */
  isRecording: boolean;
}

/** Microphone availability check result. */
export interface MicrophoneStatus {
  /** Whether a microphone device is accessible. */
  available: boolean;
  /** Name of the default input device, if available. */
  deviceName: string | null;
}

// ─────────────────────────────────────────────────────────────────────────────
// Recording state (for frontend state management)
// ─────────────────────────────────────────────────────────────────────────────

/** Frontend-side recording state for UI components. */
export interface RecordingState {
  /** Active recording identifier, or null if not recording. */
  recordingId: string | null;
  /** Whether a recording is in progress. */
  isRecording: boolean;
  /** Elapsed recording duration in seconds. */
  durationSeconds: number;
}

// ─────────────────────────────────────────────────────────────────────────────
// Transcription types (M003/S03)
// ─────────────────────────────────────────────────────────────────────────────

/** Whisper model sizes — smaller models are faster, larger are more accurate. */
export type WhisperModelSize = "tiny" | "base" | "small" | "medium";

/** Result of a transcription operation. */
export interface TranscriptionResult {
  /** The transcribed text. */
  text: string;
  /** Time taken for transcription in milliseconds. */
  durationMs: number;
  /** Which Whisper model was used (e.g. "small"). */
  modelUsed: string;
}

/** Information about a Whisper model on disk. */
export interface WhisperModelInfo {
  /** Model size label (e.g. "small"). */
  modelSize: string;
  /** Whether the model file exists on disk. */
  downloaded: boolean;
  /** File size in MB (only present if downloaded). */
  fileSizeMb: number | null;
  /** Full path to the model file on disk. */
  modelPath: string;
}

// ─────────────────────────────────────────────────────────────────────────────
// LLM Integration types (M003/S03)
// ─────────────────────────────────────────────────────────────────────────────

/** Status of the Ollama service. */
export interface OllamaStatus {
  available: boolean;
  models: string[];
  error: string | null;
}

/** Confidence level for a generated field. */
export type ConfidenceLevel = "high" | "medium" | "low";

/** Result of a note generation call. */
export interface NoteDraftResult {
  noteType: string;
  fields: Record<string, unknown>;
  confidence: Record<string, ConfidenceLevel>;
  modelUsed: string;
  generationTimeMs: number;
}

/** A single CPT code suggestion from the LLM. */
export interface CptSuggestion {
  code: string;
  description: string;
  minutes: number;
  confidence: ConfidenceLevel;
}

/** Objective data extracted from a session transcript. */
export interface ExtractedObjectiveData {
  romValues: Record<string, unknown> | null;
  painScores: Record<string, number> | null;
  mmtGrades: Record<string, string> | null;
}

/** Optional patient context to improve note generation quality. */
export interface PatientContext {
  patientId: string | null;
  demographics: string | null;
  priorNoteSummary: string | null;
  diagnosis: string | null;
}

/** LLM provider type. */
export type LlmProvider = "ollama" | "bedrock" | "claude";

/** Full LLM settings returned from the backend (secrets masked). */
export interface FullLlmSettings {
  provider: string;
  model: string | null;
  ollamaUrl: string | null;
  claudeApiKey: string | null;
  claudeModel: string | null;
  bedrockAccessKey: string | null;
  bedrockSecretKey: string | null;
  bedrockRegion: string | null;
  bedrockModel: string | null;
}

/** LLM settings returned from the backend. */
export interface LlmSettings {
  provider: LlmProvider;
  model: string | null;
  ollamaUrl: string | null;
}

/** Input for configuring LLM settings. */
export interface LlmSettingsInput {
  provider: LlmProvider;
  model: string | null;
  ollamaUrl: string | null;
  apiKey: string | null;
  apiSecret: string | null;
  bedrockRegion?: string | null;
  claudeModel?: string | null;
  bedrockModel?: string | null;
}

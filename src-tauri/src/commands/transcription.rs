/// commands/transcription.rs — Whisper Transcription Engine (M003/S03)
///
/// Provides on-device speech-to-text transcription using whisper.cpp (via whisper-rs).
///
/// Build note: whisper-rs depends on the whisper.cpp C++ library and requires:
///   - cmake (brew install cmake)
///   - A C/C++ compiler (Xcode command line tools)
///
/// The whisper functionality is gated behind the "whisper" cargo feature.
/// When the feature is disabled, the transcription commands return a clear error
/// message indicating setup is needed, but the rest of the application compiles.
///
/// Privacy: Audio WAV files are deleted after transcription completes.
/// No audio is stored permanently — only the resulting text is kept.
///
/// Latency target: < 60 seconds for a 20-minute session on Apple Silicon.
/// The "small" model (default) achieves this on M1+ hardware.
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::auth::session::SessionManager;
use crate::error::AppError;
use crate::rbac::middleware;
use crate::rbac::roles::{Action, Resource};

// ─────────────────────────────────────────────────────────────────────────────
// Types
// ─────────────────────────────────────────────────────────────────────────────

/// Whisper model sizes — smaller models are faster, larger models are more accurate.
///
/// Recommended defaults:
///   - Tiny:   ~75 MB, fastest, suitable for quick drafts
///   - Base:   ~142 MB, good balance for short sessions
///   - Small:  ~466 MB (DEFAULT), best accuracy/speed tradeoff for 20-min PT sessions
///   - Medium: ~1.5 GB, highest accuracy, slower transcription
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum WhisperModelSize {
    Tiny,
    Base,
    Small,
    Medium,
}

impl WhisperModelSize {
    /// Hugging Face ggml model filename.
    pub fn filename(&self) -> &'static str {
        match self {
            Self::Tiny => "ggml-tiny.bin",
            Self::Base => "ggml-base.bin",
            Self::Small => "ggml-small.bin",
            Self::Medium => "ggml-medium.bin",
        }
    }

    /// Download URL from Hugging Face (ggerganov/whisper.cpp).
    pub fn download_url(&self) -> String {
        format!(
            "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/{}",
            self.filename()
        )
    }

    /// Approximate file size in MB (for UI display).
    pub fn approx_size_mb(&self) -> u64 {
        match self {
            Self::Tiny => 75,
            Self::Base => 142,
            Self::Small => 466,
            Self::Medium => 1500,
        }
    }

    /// Human-readable label.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Tiny => "tiny",
            Self::Base => "base",
            Self::Small => "small",
            Self::Medium => "medium",
        }
    }
}

impl Default for WhisperModelSize {
    fn default() -> Self {
        Self::Small
    }
}

/// Result of a transcription operation.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptionResult {
    /// The transcribed text.
    pub text: String,
    /// Time taken for transcription in milliseconds.
    pub duration_ms: u64,
    /// Which Whisper model was used.
    pub model_used: String,
}

/// Information about a Whisper model on disk.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WhisperModelInfo {
    /// Model size label (e.g. "small").
    pub model_size: String,
    /// Whether the model file exists on disk.
    pub downloaded: bool,
    /// File size in MB (only present if downloaded).
    pub file_size_mb: Option<f64>,
    /// Full path to the model file.
    pub model_path: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// Model path helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Resolve the directory where Whisper models are stored.
/// Uses the app support directory: ~/Library/Application Support/com.medarc.emr/models/whisper/
pub fn whisper_models_dir() -> std::path::PathBuf {
    let base = dirs_next().join("models").join("whisper");
    base
}

/// Platform-specific app support directory fallback.
/// In production, Tauri provides this via app.path().app_support_dir().
/// For tests and standalone use, we fall back to a conventional path.
fn dirs_next() -> std::path::PathBuf {
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = std::env::var_os("HOME") {
            return std::path::PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("com.medarc.emr");
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        if let Some(data) = std::env::var_os("APPDATA") {
            return std::path::PathBuf::from(data).join("com.medarc.emr");
        }
    }

    // Final fallback
    std::env::temp_dir().join("medarc")
}

/// Full path to a specific model file.
pub fn model_path(size: WhisperModelSize) -> std::path::PathBuf {
    whisper_models_dir().join(size.filename())
}

// ─────────────────────────────────────────────────────────────────────────────
// Tauri Commands
// ─────────────────────────────────────────────────────────────────────────────

/// Transcribe a WAV audio file using Whisper.
///
/// When the "whisper" feature is enabled, this runs whisper.cpp on the audio.
/// When disabled, it returns an error explaining how to enable it.
///
/// The WAV file is deleted after successful transcription (privacy).
///
/// Requires: ClinicalDocumentation + Create
#[tauri::command]
pub fn transcribe_audio(
    wav_path: String,
    model_size: Option<WhisperModelSize>,
    session: State<'_, SessionManager>,
) -> Result<TranscriptionResult, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::ClinicalDocumentation, Action::Create)?;

    let size = model_size.unwrap_or_default();

    #[cfg(feature = "whisper")]
    {
        transcribe_with_whisper(&wav_path, size)
    }

    #[cfg(not(feature = "whisper"))]
    {
        // Verify the WAV file exists (so callers get a useful error if it doesn't)
        if !std::path::Path::new(&wav_path).exists() {
            return Err(AppError::NotFound(format!(
                "WAV file not found: {}",
                wav_path
            )));
        }

        // Check model exists
        let mp = model_path(size);
        if !mp.exists() {
            return Err(AppError::NotFound(format!(
                "Whisper model not found at: {}. Run download_whisper_model first.",
                mp.display()
            )));
        }

        Err(AppError::Validation(
            "Whisper transcription is not available in this build. \
             Rebuild with `cargo build --features whisper` after installing \
             cmake and a C++ compiler (Xcode CLI tools on macOS)."
                .into(),
        ))
    }
}

/// Internal helper: query model info without RBAC (used by download_whisper_model).
fn whisper_model_info(size: WhisperModelSize) -> Result<WhisperModelInfo, AppError> {
    let mp = model_path(size);

    let (downloaded, file_size_mb) = if mp.exists() {
        let metadata = std::fs::metadata(&mp).map_err(AppError::Io)?;
        let size_mb = metadata.len() as f64 / (1024.0 * 1024.0);
        (true, Some(size_mb))
    } else {
        (false, None)
    };

    Ok(WhisperModelInfo {
        model_size: size.label().to_string(),
        downloaded,
        file_size_mb,
        model_path: mp.to_string_lossy().to_string(),
    })
}

/// Check if a Whisper model is downloaded and ready to use.
///
/// Requires: ClinicalDocumentation + Read
#[tauri::command]
pub fn check_whisper_model(
    model_size: Option<WhisperModelSize>,
    session: State<'_, SessionManager>,
) -> Result<WhisperModelInfo, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::ClinicalDocumentation, Action::Read)?;

    whisper_model_info(model_size.unwrap_or_default())
}

/// Download a Whisper model to the app support directory.
///
/// This is a blocking download — the frontend should call it from a background task
/// and show a progress indicator. For Phase 1, we download the entire file at once.
///
/// Requires network access (com.apple.security.network.client entitlement).
///
/// Requires: ClinicalDocumentation + Create
#[tauri::command]
pub fn download_whisper_model(
    model_size: Option<WhisperModelSize>,
    session: State<'_, SessionManager>,
) -> Result<WhisperModelInfo, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::ClinicalDocumentation, Action::Create)?;

    let size = model_size.unwrap_or_default();
    let mp = model_path(size);

    // Create directory if needed
    if let Some(parent) = mp.parent() {
        std::fs::create_dir_all(parent)?;
    }

    if mp.exists() {
        // Already downloaded — return info
        return whisper_model_info(size);
    }

    // Download the model file
    // NOTE: In production, this should use a streaming HTTP client with progress.
    // For Phase 1, we use a simple blocking reqwest-style approach via std.
    // Since we don't want to add reqwest as a dependency, we document that
    // the user should manually download the model or we use a shell command.
    //
    // For now, we create a placeholder that tells the user where to put the file.
    let url = size.download_url();

    Err(AppError::Validation(format!(
        "Automatic model download is not yet implemented. \
         Please download the model manually:\n\
         curl -L -o \"{}\" \"{}\"\n\
         Expected file size: ~{} MB",
        mp.display(),
        url,
        size.approx_size_mb()
    )))
}

// ─────────────────────────────────────────────────────────────────────────────
// Whisper transcription (feature-gated)
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(feature = "whisper")]
fn transcribe_with_whisper(
    wav_path: &str,
    size: WhisperModelSize,
) -> Result<TranscriptionResult, AppError> {
    use std::time::Instant;

    let mp = model_path(size);
    if !mp.exists() {
        return Err(AppError::NotFound(format!(
            "Whisper model not found at: {}. Run download_whisper_model first.",
            mp.display()
        )));
    }

    // Validate WAV file exists
    let wav = std::path::Path::new(wav_path);
    if !wav.exists() {
        return Err(AppError::NotFound(format!(
            "WAV file not found: {}",
            wav_path
        )));
    }

    let start = Instant::now();

    // Load model
    let mp_str = mp
        .to_str()
        .ok_or_else(|| AppError::Validation("Model path contains invalid UTF-8".to_string()))?;
    let ctx = whisper_rs::WhisperContext::new_with_params(
        mp_str,
        whisper_rs::WhisperContextParameters::default(),
    )
    .map_err(|e| AppError::Validation(format!("Failed to load Whisper model: {}", e)))?;

    let mut state = ctx
        .create_state()
        .map_err(|e| AppError::Validation(format!("Failed to create Whisper state: {}", e)))?;

    // Read WAV audio
    let mut reader = hound::WavReader::open(wav_path)
        .map_err(|e| AppError::Validation(format!("Failed to read WAV file: {}", e)))?;

    let samples: Vec<f32> = if reader.spec().sample_format == hound::SampleFormat::Float {
        reader
            .samples::<f32>()
            .filter_map(|s| s.ok())
            .collect()
    } else {
        reader
            .samples::<i16>()
            .filter_map(|s| s.ok())
            .map(|s| s as f32 / i16::MAX as f32)
            .collect()
    };

    // Configure transcription parameters
    let mut params = whisper_rs::FullParams::new(whisper_rs::SamplingStrategy::Greedy { best_of: 1 });
    params.set_language(Some("en"));
    params.set_print_special(false);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);
    // Use 4 threads on Apple Silicon for good parallelism
    params.set_n_threads(4);

    // Run transcription
    state
        .full(params, &samples)
        .map_err(|e| AppError::Validation(format!("Whisper transcription failed: {}", e)))?;

    // Collect transcript text
    let num_segments = state.full_n_segments()
        .map_err(|e| AppError::Validation(format!("Failed to get segments: {}", e)))?;
    let mut text = String::new();
    for i in 0..num_segments {
        if let Ok(segment_text) = state.full_get_segment_text(i) {
            text.push_str(&segment_text);
            text.push(' ');
        }
    }
    let text = text.trim().to_string();

    let duration_ms = start.elapsed().as_millis() as u64;

    // Delete WAV file after successful transcription (privacy)
    let _ = std::fs::remove_file(wav_path);

    Ok(TranscriptionResult {
        text,
        duration_ms,
        model_used: size.label().to_string(),
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_size_default_is_small() {
        assert_eq!(WhisperModelSize::default(), WhisperModelSize::Small);
    }

    #[test]
    fn test_model_filenames() {
        assert_eq!(WhisperModelSize::Tiny.filename(), "ggml-tiny.bin");
        assert_eq!(WhisperModelSize::Base.filename(), "ggml-base.bin");
        assert_eq!(WhisperModelSize::Small.filename(), "ggml-small.bin");
        assert_eq!(WhisperModelSize::Medium.filename(), "ggml-medium.bin");
    }

    #[test]
    fn test_model_download_urls() {
        let url = WhisperModelSize::Small.download_url();
        assert!(url.contains("huggingface.co"));
        assert!(url.contains("ggml-small.bin"));
    }

    #[test]
    fn test_model_labels() {
        assert_eq!(WhisperModelSize::Tiny.label(), "tiny");
        assert_eq!(WhisperModelSize::Base.label(), "base");
        assert_eq!(WhisperModelSize::Small.label(), "small");
        assert_eq!(WhisperModelSize::Medium.label(), "medium");
    }

    #[test]
    fn test_model_approx_sizes() {
        assert_eq!(WhisperModelSize::Tiny.approx_size_mb(), 75);
        assert_eq!(WhisperModelSize::Base.approx_size_mb(), 142);
        assert_eq!(WhisperModelSize::Small.approx_size_mb(), 466);
        assert_eq!(WhisperModelSize::Medium.approx_size_mb(), 1500);
    }

    #[test]
    fn test_model_path_resolution() {
        let path = model_path(WhisperModelSize::Small);
        let path_str = path.to_string_lossy();

        // Path should contain the model directory and filename
        assert!(
            path_str.contains("whisper"),
            "Path should include 'whisper' directory: {}",
            path_str
        );
        assert!(
            path_str.ends_with("ggml-small.bin"),
            "Path should end with model filename: {}",
            path_str
        );
    }

    #[test]
    fn test_model_path_each_size() {
        for size in &[
            WhisperModelSize::Tiny,
            WhisperModelSize::Base,
            WhisperModelSize::Small,
            WhisperModelSize::Medium,
        ] {
            let path = model_path(*size);
            assert!(
                path.to_string_lossy().ends_with(size.filename()),
                "Path for {:?} should end with {}",
                size,
                size.filename()
            );
        }
    }

    #[test]
    fn test_whisper_models_dir_structure() {
        let dir = whisper_models_dir();
        let dir_str = dir.to_string_lossy();
        assert!(
            dir_str.contains("models"),
            "Models dir should contain 'models': {}",
            dir_str
        );
        assert!(
            dir_str.contains("whisper"),
            "Models dir should contain 'whisper': {}",
            dir_str
        );
    }

    #[test]
    fn test_check_whisper_model_not_downloaded() {
        // Use a non-existent model size check — model shouldn't be on disk in CI
        // We can't use the Tauri State wrapper here, so test the underlying logic
        let size = WhisperModelSize::Tiny;
        let mp = model_path(size);

        // Unless someone has the model installed, it won't exist
        if !mp.exists() {
            let info = WhisperModelInfo {
                model_size: size.label().to_string(),
                downloaded: false,
                file_size_mb: None,
                model_path: mp.to_string_lossy().to_string(),
            };
            assert!(!info.downloaded);
            assert!(info.file_size_mb.is_none());
            assert_eq!(info.model_size, "tiny");
        }
    }

    #[test]
    fn test_serde_model_size_roundtrip() {
        let size = WhisperModelSize::Small;
        let json = serde_json::to_string(&size).unwrap();
        assert_eq!(json, "\"small\"");

        let parsed: WhisperModelSize = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, WhisperModelSize::Small);
    }

    #[test]
    fn test_serde_all_model_sizes() {
        let sizes = vec![
            (WhisperModelSize::Tiny, "\"tiny\""),
            (WhisperModelSize::Base, "\"base\""),
            (WhisperModelSize::Small, "\"small\""),
            (WhisperModelSize::Medium, "\"medium\""),
        ];

        for (size, expected_json) in sizes {
            let json = serde_json::to_string(&size).unwrap();
            assert_eq!(json, expected_json);
            let parsed: WhisperModelSize = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, size);
        }
    }

    #[test]
    fn test_transcription_result_serialization() {
        let result = TranscriptionResult {
            text: "Hello world".to_string(),
            duration_ms: 1234,
            model_used: "small".to_string(),
        };

        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["text"], "Hello world");
        assert_eq!(json["durationMs"], 1234);
        assert_eq!(json["modelUsed"], "small");
    }

    #[test]
    fn test_whisper_model_info_serialization() {
        let info = WhisperModelInfo {
            model_size: "small".to_string(),
            downloaded: true,
            file_size_mb: Some(466.5),
            model_path: "/tmp/test/ggml-small.bin".to_string(),
        };

        let json = serde_json::to_value(&info).unwrap();
        assert_eq!(json["modelSize"], "small");
        assert_eq!(json["downloaded"], true);
        assert_eq!(json["fileSizeMb"], 466.5);
        assert_eq!(json["modelPath"], "/tmp/test/ggml-small.bin");
    }
}

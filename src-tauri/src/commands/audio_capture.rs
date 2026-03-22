/// commands/audio_capture.rs — Audio Capture (M003/S03)
///
/// Implements microphone audio capture using cpal for the AI Voice-to-Note feature.
/// Audio is recorded at 16kHz mono (Whisper-optimal) and saved as WAV using hound.
///
/// Privacy: Audio files are stored in a temp directory and deleted after transcription.
/// No audio is permanently stored on disk.
///
/// macOS entitlement required: com.apple.security.device.microphone
///
/// Thread-safety note: `cpal::Stream` does not implement `Send`/`Sync` on some
/// platforms (CoreAudio uses raw pointers internally). We wrap it in a
/// `SendSyncStream` newtype with an unsafe `Send + Sync` impl. This is safe
/// because we only interact with the stream through `play()`/`pause()` and `drop()`,
/// which cpal documents as thread-safe operations on macOS CoreAudio.
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use hound::{SampleFormat, WavSpec, WavWriter};
use serde::{Deserialize, Serialize};
use tauri::State;
use uuid::Uuid;

use crate::auth::session::SessionManager;
use crate::error::AppError;
use crate::rbac::middleware;
use crate::rbac::roles::{Action, Resource};

// ─────────────────────────────────────────────────────────────────────────────
// Send+Sync wrapper for cpal::Stream
// ─────────────────────────────────────────────────────────────────────────────

/// Wrapper around `cpal::Stream` that implements `Send + Sync`.
///
/// # Safety
/// cpal::Stream on macOS CoreAudio uses raw pointers internally, preventing
/// auto-implementation of Send/Sync. However, the stream's public API
/// (play/pause/drop) is safe to call from any thread, and our usage pattern
/// (create on one thread, drop on same or another thread) is safe.
#[allow(dead_code)]
struct SendSyncStream(cpal::Stream);

// SAFETY: See struct-level doc comment. We only use play() and drop().
unsafe impl Send for SendSyncStream {}
unsafe impl Sync for SendSyncStream {}

// ─────────────────────────────────────────────────────────────────────────────
// Types
// ─────────────────────────────────────────────────────────────────────────────

/// Internal state for an active recording session.
struct ActiveRecording {
    recording_id: String,
    /// cpal stream handle — kept alive to continue capture.
    stream: SendSyncStream,
    /// Shared buffer of f32 PCM samples (at device's native sample rate).
    samples: Arc<Mutex<Vec<f32>>>,
    /// Current peak audio level (0.0–1.0).
    level: Arc<Mutex<f32>>,
    /// WAV output path for this session.
    wav_path: std::path::PathBuf,
    /// Recording start instant (for duration tracking).
    started_at: std::time::Instant,
    /// The device's actual sample rate (for resampling to 16kHz on stop).
    device_sample_rate: u32,
    /// Number of channels the device is recording (for downmix to mono).
    device_channels: u16,
}

/// Thread-safe wrapper managed by Tauri state.
pub struct AudioRecordingState {
    active: Arc<Mutex<Option<ActiveRecording>>>,
}

// AudioRecordingState is Send + Sync because ActiveRecording is (via SendSyncStream).
// The compiler can now verify this automatically.

impl AudioRecordingState {
    pub fn new() -> Self {
        Self {
            active: Arc::new(Mutex::new(None)),
        }
    }
}

/// Returned to the frontend when a recording starts.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartRecordingResult {
    pub recording_id: String,
}

/// Returned to the frontend when a recording stops.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StopRecordingResult {
    pub recording_id: String,
    pub wav_path: String,
    pub duration_seconds: f64,
    /// Peak audio level detected in the recorded samples (0.0–1.0).
    /// Use this to check for silence before transcribing.
    pub peak_level: f32,
}

/// Current audio level for the visualizer.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioLevel {
    pub level: f32,
    pub is_recording: bool,
}

/// Microphone availability check result.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MicrophoneStatus {
    pub available: bool,
    pub device_name: Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Constants
// ─────────────────────────────────────────────────────────────────────────────

/// Whisper-optimal sample rate.
const SAMPLE_RATE: u32 = 16_000;
/// Mono channel.
const CHANNELS: u16 = 1;
/// 16-bit PCM for WAV output.
const BITS_PER_SAMPLE: u16 = 16;

// ─────────────────────────────────────────────────────────────────────────────
// Tauri Commands
// ─────────────────────────────────────────────────────────────────────────────

/// Begin capturing audio from the default microphone.
/// Returns a recording_id that must be passed to `stop_audio_recording`.
///
/// Requires: ClinicalDocumentation + Create
#[tauri::command]
pub fn start_audio_recording(
    state: State<'_, AudioRecordingState>,
    session: State<'_, SessionManager>,
) -> Result<StartRecordingResult, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::ClinicalDocumentation, Action::Create)?;

    let mut active = state
        .inner()
        .active
        .lock()
        .map_err(|e| AppError::Validation(format!("Lock poisoned: {}", e)))?;

    if active.is_some() {
        return Err(AppError::Validation(
            "A recording is already in progress. Stop it before starting a new one.".into(),
        ));
    }

    let recording_id = Uuid::new_v4().to_string();

    // Create temp WAV path
    let wav_path = std::env::temp_dir().join(format!("medarc_recording_{}.wav", recording_id));

    // Set up cpal host and device
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| AppError::Validation("No microphone found".into()))?;

    // Use the device's default input config (typically 48kHz stereo on macOS).
    // We'll resample to 16kHz mono when writing the WAV on stop.
    let default_config = device
        .default_input_config()
        .map_err(|e| AppError::Validation(format!("Failed to get default input config: {}", e)))?;

    let device_sample_rate = default_config.sample_rate().0;
    let device_channels = default_config.channels();

    let config = cpal::StreamConfig {
        channels: device_channels,
        sample_rate: cpal::SampleRate(device_sample_rate),
        buffer_size: cpal::BufferSize::Default,
    };

    let samples: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::new()));
    let level: Arc<Mutex<f32>> = Arc::new(Mutex::new(0.0));

    let samples_clone = Arc::clone(&samples);
    let level_clone = Arc::clone(&level);

    let err_fn = |err: cpal::StreamError| {
        eprintln!("[audio_capture] Stream error: {}", err);
    };

    let stream = device
        .build_input_stream(
            &config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                // Update peak level for visualizer
                let peak = data
                    .iter()
                    .copied()
                    .map(|s| s.abs())
                    .fold(0.0_f32, f32::max);
                if let Ok(mut lvl) = level_clone.lock() {
                    *lvl = peak.min(1.0);
                }
                // Append raw samples (at device rate, possibly multi-channel)
                if let Ok(mut buf) = samples_clone.lock() {
                    buf.extend_from_slice(data);
                }
            },
            err_fn,
            None, // no timeout
        )
        .map_err(|e| AppError::Validation(format!("Failed to build audio stream: {}", e)))?;

    stream
        .play()
        .map_err(|e| AppError::Validation(format!("Failed to start audio stream: {}", e)))?;

    let recording = ActiveRecording {
        recording_id: recording_id.clone(),
        stream: SendSyncStream(stream),
        samples,
        level,
        wav_path,
        started_at: std::time::Instant::now(),
        device_sample_rate,
        device_channels,
    };

    *active = Some(recording);

    Ok(StartRecordingResult { recording_id })
}

/// Stop the active recording, flush the WAV file, and return its path.
///
/// Requires: ClinicalDocumentation + Create
#[tauri::command]
pub fn stop_audio_recording(
    recording_id: String,
    state: State<'_, AudioRecordingState>,
    session: State<'_, SessionManager>,
) -> Result<StopRecordingResult, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::ClinicalDocumentation, Action::Create)?;

    let mut active = state
        .inner()
        .active
        .lock()
        .map_err(|e| AppError::Validation(format!("Lock poisoned: {}", e)))?;

    let recording = active.take().ok_or_else(|| {
        AppError::Validation("No active recording to stop".into())
    })?;

    if recording.recording_id != recording_id {
        // Put it back
        let id = recording.recording_id.clone();
        *active = Some(recording);
        return Err(AppError::Validation(format!(
            "Recording ID mismatch: expected {}, got {}",
            id, recording_id
        )));
    }

    let duration_seconds = recording.started_at.elapsed().as_secs_f64();

    // Drop the stream to stop capture
    drop(recording.stream);

    // Write WAV file from accumulated samples, resampling to 16kHz mono
    let raw_samples = recording
        .samples
        .lock()
        .map_err(|e| AppError::Validation(format!("Lock poisoned: {}", e)))?;

    // Downmix to mono if multi-channel (average all channels per frame)
    let mono_samples: Vec<f32> = if recording.device_channels > 1 {
        let ch = recording.device_channels as usize;
        raw_samples
            .chunks_exact(ch)
            .map(|frame| frame.iter().sum::<f32>() / ch as f32)
            .collect()
    } else {
        raw_samples.clone()
    };

    // Resample from device rate to 16kHz if needed
    let samples = if recording.device_sample_rate != SAMPLE_RATE {
        resample(&mono_samples, recording.device_sample_rate, SAMPLE_RATE)
    } else {
        mono_samples
    };

    // Compute peak level from the final mono samples
    let peak_level = samples
        .iter()
        .map(|s| s.abs())
        .fold(0.0_f32, f32::max);

    write_wav_file(&recording.wav_path, &samples)?;

    let wav_path_str = recording
        .wav_path
        .to_str()
        .ok_or_else(|| AppError::Validation("Recording path contains invalid UTF-8".to_string()))?
        .to_string();

    Ok(StopRecordingResult {
        recording_id,
        wav_path: wav_path_str,
        duration_seconds,
        peak_level,
    })
}

/// Returns the current audio amplitude level (0.0–1.0) for the frontend visualizer.
///
/// Requires: ClinicalDocumentation + Read
#[tauri::command]
pub fn get_audio_level(
    state: State<'_, AudioRecordingState>,
    session: State<'_, SessionManager>,
) -> Result<AudioLevel, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::ClinicalDocumentation, Action::Read)?;

    let active = state
        .inner()
        .active
        .lock()
        .map_err(|e| AppError::Validation(format!("Lock poisoned: {}", e)))?;

    match active.as_ref() {
        Some(recording) => {
            let level = recording
                .level
                .lock()
                .map_err(|e| AppError::Validation(format!("Lock poisoned: {}", e)))?;
            Ok(AudioLevel {
                level: *level,
                is_recording: true,
            })
        }
        None => Ok(AudioLevel {
            level: 0.0,
            is_recording: false,
        }),
    }
}

/// Check whether a microphone is available on the system.
///
/// Requires: ClinicalDocumentation + Read
#[tauri::command]
pub fn check_microphone_available(
    session: State<'_, SessionManager>,
) -> Result<MicrophoneStatus, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::ClinicalDocumentation, Action::Read)?;

    let host = cpal::default_host();
    match host.default_input_device() {
        Some(device) => {
            let name = device.name().ok();
            Ok(MicrophoneStatus {
                available: true,
                device_name: name,
            })
        }
        None => Ok(MicrophoneStatus {
            available: false,
            device_name: None,
        }),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Internal helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Convert f32 PCM samples to 16-bit WAV and write to disk.
fn write_wav_file(
    path: &std::path::Path,
    samples: &[f32],
) -> Result<(), AppError> {
    let spec = WavSpec {
        channels: CHANNELS,
        sample_rate: SAMPLE_RATE,
        bits_per_sample: BITS_PER_SAMPLE,
        sample_format: SampleFormat::Int,
    };

    let mut writer = WavWriter::create(path, spec)
        .map_err(|e| AppError::Validation(format!("Failed to create WAV file: {}", e)))?;

    for &sample in samples.iter() {
        let int_sample = f32_to_i16(sample);
        writer
            .write_sample(int_sample)
            .map_err(|e| AppError::Validation(format!("Failed to write WAV sample: {}", e)))?;
    }

    writer
        .finalize()
        .map_err(|e| AppError::Validation(format!("Failed to finalize WAV file: {}", e)))?;

    Ok(())
}

/// Convert an f32 sample [-1.0, 1.0] to i16, clamping values outside range.
fn f32_to_i16(sample: f32) -> i16 {
    let clamped = sample.max(-1.0).min(1.0);
    (clamped * i16::MAX as f32) as i16
}

/// Linear interpolation resample from `from_rate` to `to_rate`.
/// Good enough for speech (Whisper is tolerant of simple resampling).
fn resample(samples: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if from_rate == to_rate || samples.is_empty() {
        return samples.to_vec();
    }
    let ratio = from_rate as f64 / to_rate as f64;
    let out_len = (samples.len() as f64 / ratio) as usize;
    let mut output = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let src_pos = i as f64 * ratio;
        let idx = src_pos as usize;
        let frac = (src_pos - idx as f64) as f32;
        let s0 = samples[idx.min(samples.len() - 1)];
        let s1 = samples[(idx + 1).min(samples.len() - 1)];
        output.push(s0 + frac * (s1 - s0));
    }
    output
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recording_state_new() {
        let state = AudioRecordingState::new();
        let active = state.active.lock().unwrap();
        assert!(active.is_none(), "Initial recording state should be None");
    }

    #[test]
    fn test_recording_state_lifecycle() {
        // Verify we can create state, lock it, and it starts empty
        let state = AudioRecordingState::new();

        {
            let active = state.active.lock().unwrap();
            assert!(active.is_none());
        }

        // Simulate setting active to Some (without actual audio)
        // Just verify the lock/unlock cycle works
        {
            let active = state.active.lock().unwrap();
            assert!(active.is_none());
        }
    }

    #[test]
    fn test_wav_spec_constants() {
        assert_eq!(SAMPLE_RATE, 16_000, "Whisper requires 16kHz");
        assert_eq!(CHANNELS, 1, "Whisper requires mono");
        assert_eq!(BITS_PER_SAMPLE, 16, "16-bit WAV output");
    }

    #[test]
    fn test_wav_write_and_validate() {
        let dir = std::env::temp_dir();
        let path = dir.join("medarc_test_wav_capture.wav");

        let spec = WavSpec {
            channels: CHANNELS,
            sample_rate: SAMPLE_RATE,
            bits_per_sample: BITS_PER_SAMPLE,
            sample_format: SampleFormat::Int,
        };

        // Write a short WAV with known samples
        {
            let mut writer = WavWriter::create(&path, spec).expect("create WAV");
            for i in 0..SAMPLE_RATE {
                // Simple test: alternating positive/negative
                let sample = if i % 2 == 0 { 1000_i16 } else { -1000_i16 };
                writer.write_sample(sample).expect("write sample");
            }
            writer.finalize().expect("finalize");
        }

        // Read it back and validate format
        let reader = hound::WavReader::open(&path).expect("open WAV");
        let read_spec = reader.spec();
        assert_eq!(read_spec.channels, CHANNELS);
        assert_eq!(read_spec.sample_rate, SAMPLE_RATE);
        assert_eq!(read_spec.bits_per_sample, BITS_PER_SAMPLE);
        assert_eq!(read_spec.sample_format, SampleFormat::Int);
        assert_eq!(reader.len(), SAMPLE_RATE);

        // Clean up
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_f32_to_i16_conversion() {
        assert_eq!(f32_to_i16(0.0), 0);
        assert_eq!(f32_to_i16(1.0), i16::MAX);
        assert_eq!(f32_to_i16(-1.0), -i16::MAX);
        // Clipping beyond range
        assert_eq!(f32_to_i16(2.0), i16::MAX);
        assert_eq!(f32_to_i16(-2.0), -i16::MAX);
    }

    #[test]
    fn test_write_wav_file_from_f32_samples() {
        let dir = std::env::temp_dir();
        let path = dir.join("medarc_test_wav_f32.wav");

        // Generate 1 second of silence with a few peaks
        let mut samples = vec![0.0_f32; SAMPLE_RATE as usize];
        samples[0] = 0.5;
        samples[100] = -0.8;
        samples[8000] = 1.0;

        write_wav_file(&path, &samples).expect("write WAV from f32");

        // Validate the output
        let reader = hound::WavReader::open(&path).expect("open WAV");
        assert_eq!(reader.spec().sample_rate, SAMPLE_RATE);
        assert_eq!(reader.spec().channels, CHANNELS);
        assert_eq!(reader.len() as u32, SAMPLE_RATE);

        // Clean up
        let _ = std::fs::remove_file(&path);
    }
}

use std::io;

use thiserror::Error;

pub type Result<T> = std::result::Result<T, CaptureError>;

#[derive(Debug, Error)]
pub enum CaptureError {
    #[error("invalid capture settings: {0}")]
    InvalidSettings(String),

    #[error("captured audio is too short: need {needed_frames} frames, got {actual_frames}")]
    CaptureTooShort {
        needed_frames: usize,
        actual_frames: usize,
    },

    #[error("audio channel count cannot be zero")]
    NoChannels,

    #[error("matrix dimensions do not match its channel labels")]
    InvalidMatrix,

    #[error("audio backend error: {0}")]
    AudioBackend(String),

    #[error("capture integrity check failed: {0}")]
    CaptureIntegrity(String),

    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    #[error("WAV error: {0}")]
    Wav(#[from] hound::Error),
}

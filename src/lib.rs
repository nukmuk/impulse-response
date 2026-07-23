//! Core signal scheduling, extraction, and export for software-filter capture.
//!
//! The library deliberately contains no terminal or GUI code so it can be
//! shared by the command-line program and a future Windows GUI.

mod error;
mod export;
mod impulse;
mod matrix;
mod schedule;

#[cfg(windows)]
mod windows_audio;

pub use error::{CaptureError, Result};
pub use export::{ExportedBundle, export_equalizer_apo_bundle, render_equalizer_apo_config};
pub use impulse::{PcmBitDepth, write_dirac_wav};
pub use matrix::ImpulseMatrix;
pub use schedule::{CaptureSchedule, extract_matrix};

#[cfg(windows)]
pub use windows_audio::{
    CaptureDiagnostics, LiveCapture, LiveCaptureSettings, RenderDevice, list_render_devices,
    measure_software_filter,
};

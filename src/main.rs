use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "rust-impulse-response",
    version,
    about = "Capture a Windows software filter as an Equalizer APO convolution matrix"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// List active Windows playback endpoints.
    Devices,

    /// Generate a one-second mono full-scale Dirac impulse WAV without dithering.
    Impulse {
        /// Output WAV path.
        #[arg(short, long, default_value = "dirac.wav")]
        output: PathBuf,

        /// WAV sample rate in Hz.
        #[arg(long, default_value_t = 48_000)]
        sample_rate: u32,

        /// Integer PCM bit depth: 16, 24, or 32.
        #[arg(long, default_value_t = rust_impulse_response::PcmBitDepth::Pcm24)]
        bit_depth: rust_impulse_response::PcmBitDepth,
    },

    /// Play sample-exact impulses, capture loopback, and export an APO bundle.
    Capture {
        /// Playback endpoint that receives the impulses. The default endpoint is used if omitted.
        #[arg(long, visible_alias = "device-id")]
        render_device_id: Option<String>,

        /// Playback endpoint whose processed output is captured via WASAPI loopback.
        ///
        /// If omitted, loopback is captured from the render endpoint.
        #[arg(long)]
        capture_device_id: Option<String>,

        /// Directory for the WAV matrix and equalizer-apo.txt.
        #[arg(short, long, default_value = "captured-filter")]
        output: PathBuf,

        /// Probe amplitude. It is divided back out of the captured response.
        #[arg(long, default_value_t = -24.0, allow_hyphen_values = true)]
        probe_dbfs: f32,

        /// Silence before the first input impulse.
        #[arg(long, default_value_t = 500)]
        lead_ms: u64,

        /// Response window for each input channel.
        #[arg(long, default_value_t = 2000)]
        response_ms: u64,

        /// Silence after the final response window.
        #[arg(long, default_value_t = 250)]
        tail_ms: u64,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    run(cli.command)
}

#[cfg(windows)]
fn run(command: Command) -> Result<()> {
    use rust_impulse_response::{
        LiveCaptureSettings, export_equalizer_apo_bundle, list_render_devices,
        measure_software_filter,
    };

    match command {
        Command::Impulse {
            output,
            sample_rate,
            bit_depth,
        } => {
            rust_impulse_response::write_dirac_wav(&output, sample_rate, bit_depth)
                .with_context(|| format!("could not write {}", output.display()))?;
            println!(
                "Wrote {}: 1 second, mono, {} Hz, {}-bit integer PCM; sample 0 is positive full scale and all remaining samples are exactly zero.",
                output.display(),
                sample_rate,
                bit_depth
            );
        }
        Command::Devices => {
            let devices = list_render_devices().context("could not enumerate playback devices")?;
            for device in devices {
                let default = if device.is_default { " [default]" } else { "" };
                println!(
                    "{}{}\n  {} Hz, {} channels\n  {}",
                    device.name, default, device.sample_rate, device.channels, device.id
                );
            }
        }
        Command::Capture {
            render_device_id,
            capture_device_id,
            output,
            probe_dbfs,
            lead_ms,
            response_ms,
            tail_ms,
        } => {
            let settings = LiveCaptureSettings {
                render_device_id,
                capture_device_id,
                lead_time: Duration::from_millis(lead_ms),
                response_time: Duration::from_millis(response_ms),
                tail_time: Duration::from_millis(tail_ms),
                probe_dbfs,
            };
            eprintln!("Capturing the software filter; keep other system audio silent...");
            let capture = measure_software_filter(&settings).with_context(
                || "capture failed; no files were exported because timing integrity is required",
            )?;
            let bundle = export_equalizer_apo_bundle(&output, &capture.matrix)
                .with_context(|| format!("could not export to {}", output.display()))?;

            println!(
                "Rendered {} input channels to {} and captured {} output channels from {} at {} Hz.",
                capture.matrix.input_channels().len(),
                capture.render_device.name,
                capture.matrix.output_channels().len(),
                capture.capture_device.name,
                capture.matrix.sample_rate(),
            );
            println!(
                "Peak: {:.3} ({:.2} dBFS)",
                capture.diagnostics.peak,
                20.0 * capture.diagnostics.peak.log10()
            );
            if capture.diagnostics.peak >= 1.0 {
                eprintln!(
                    "Warning: the loopback signal reached 0 dBFS; repeat with a lower --probe-dbfs if the source filter can clip."
                );
            }
            println!("Equalizer APO config: {}", bundle.config_path.display());
            for wav in bundle.wav_paths {
                println!("Impulse matrix WAV: {}", wav.display());
            }
        }
    }
    Ok(())
}

#[cfg(not(windows))]
fn run(_command: Command) -> Result<()> {
    match _command {
        Command::Impulse {
            output,
            sample_rate,
            bit_depth,
        } => {
            rust_impulse_response::write_dirac_wav(&output, sample_rate, bit_depth)
                .with_context(|| format!("could not write {}", output.display()))?;
            Ok(())
        }
        _ => anyhow::bail!("live software-filter capture currently requires Windows"),
    }
}

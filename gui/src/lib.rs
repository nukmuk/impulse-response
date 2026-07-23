use std::path::PathBuf;
use std::time::Duration;

#[cfg(windows)]
use rust_impulse_response::{
    LiveCaptureSettings, export_equalizer_apo_bundle, list_render_devices, measure_software_filter,
};

#[derive(Clone, Debug, PartialEq)]
pub struct DeviceChoice {
    pub id: String,
    pub label: String,
    pub channels: u16,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CaptureRequest {
    pub device_id: String,
    pub output_directory: String,
    pub probe_dbfs: f64,
    pub lead_ms: f64,
    pub response_ms: f64,
    pub tail_ms: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CaptureSummary {
    pub device_name: String,
    pub sample_rate: u32,
    pub channels: usize,
    pub peak: f32,
    pub output_directory: PathBuf,
}

impl CaptureSummary {
    pub fn message(&self) -> String {
        let peak_dbfs = 20.0 * self.peak.log10();
        let mut message = format!(
            "Captured {} channels at {} Hz from {}. Peak: {:.2} dBFS. Bundle: {}",
            self.channels,
            self.sample_rate,
            self.device_name,
            peak_dbfs,
            self.output_directory.display()
        );
        if self.peak >= 1.0 {
            message
                .push_str(" Warning: the capture reached 0 dBFS; repeat with a lower probe level.");
        }
        message
    }
}

#[cfg(windows)]
pub fn load_devices() -> Result<Vec<DeviceChoice>, String> {
    let mut devices = list_render_devices().map_err(|error| error.to_string())?;
    devices.sort_by_key(|device| !device.is_default);
    Ok(devices
        .into_iter()
        .map(|device| DeviceChoice {
            id: device.id,
            channels: device.channels,
            label: format!(
                "{}{} — {} Hz, {} channels",
                device.name,
                if device.is_default { " (default)" } else { "" },
                device.sample_rate,
                device.channels
            ),
        })
        .collect())
}

#[cfg(not(windows))]
pub fn load_devices() -> Result<Vec<DeviceChoice>, String> {
    Err("the capture UI requires Windows".into())
}

pub fn validate_request(request: &CaptureRequest) -> Result<(), String> {
    if request.device_id.trim().is_empty() {
        return Err("Select a playback endpoint.".into());
    }
    if request.output_directory.trim().is_empty() {
        return Err("Choose an output directory.".into());
    }
    validate_range("Probe level", request.probe_dbfs, -120.0, 0.0)?;
    validate_range("Lead-in", request.lead_ms, 0.0, 10_000.0)?;
    validate_range("Response window", request.response_ms, 1.0, 60_000.0)?;
    validate_range("Tail", request.tail_ms, 0.0, 10_000.0)?;
    Ok(())
}

fn validate_range(name: &str, value: f64, minimum: f64, maximum: f64) -> Result<(), String> {
    if !value.is_finite() || value < minimum || value > maximum {
        return Err(format!("{name} must be between {minimum} and {maximum}."));
    }
    Ok(())
}

#[cfg(windows)]
pub fn run_capture(request: CaptureRequest) -> Result<CaptureSummary, String> {
    validate_request(&request)?;

    let output_directory = PathBuf::from(request.output_directory.trim());
    let settings = LiveCaptureSettings {
        device_id: Some(request.device_id),
        lead_time: Duration::from_millis(request.lead_ms.round() as u64),
        response_time: Duration::from_millis(request.response_ms.round() as u64),
        tail_time: Duration::from_millis(request.tail_ms.round() as u64),
        probe_dbfs: request.probe_dbfs as f32,
    };
    let capture = measure_software_filter(&settings).map_err(|error| error.to_string())?;
    export_equalizer_apo_bundle(&output_directory, &capture.matrix)
        .map_err(|error| error.to_string())?;

    Ok(CaptureSummary {
        device_name: capture.device.name,
        sample_rate: capture.matrix.sample_rate(),
        channels: capture.matrix.output_channels().len(),
        peak: capture.diagnostics.peak,
        output_directory,
    })
}

#[cfg(not(windows))]
pub fn run_capture(_request: CaptureRequest) -> Result<CaptureSummary, String> {
    Err("the capture UI requires Windows".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_request() -> CaptureRequest {
        CaptureRequest {
            device_id: "device".into(),
            output_directory: "captured-filter".into(),
            probe_dbfs: -24.0,
            lead_ms: 500.0,
            response_ms: 2_000.0,
            tail_ms: 250.0,
        }
    }

    #[test]
    fn accepts_default_capture_settings() {
        assert_eq!(validate_request(&valid_request()), Ok(()));
    }

    #[test]
    fn rejects_empty_output_and_invalid_probe() {
        let mut request = valid_request();
        request.output_directory.clear();
        assert_eq!(
            validate_request(&request),
            Err("Choose an output directory.".into())
        );

        request.output_directory = "output".into();
        request.probe_dbfs = 1.0;
        assert_eq!(
            validate_request(&request),
            Err("Probe level must be between -120 and 0.".into())
        );
    }

    #[test]
    fn rejects_zero_response_window() {
        let mut request = valid_request();
        request.response_ms = 0.0;
        assert_eq!(
            validate_request(&request),
            Err("Response window must be between 1 and 60000.".into())
        );
    }
}

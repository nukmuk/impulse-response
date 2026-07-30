#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
mod windows_app {
    use rust_impulse_response_gui::{CaptureRequest, DeviceChoice, load_devices, run_capture};
    use windows_reactor::*;

    pub fn render(cx: &mut RenderCx) -> Element {
        let devices_resource = cx.use_resource(|()| load_devices(), ());
        let (selected_render_device, set_selected_render_device) = cx.use_state(0_i32);
        let (selected_capture_device, set_selected_capture_device) = cx.use_state(0_i32);
        let (output_directory, set_output_directory) = cx.use_state("captured-filter".to_string());
        let (probe_dbfs, set_probe_dbfs) = cx.use_state(-24.0_f64);
        let (lead_ms, set_lead_ms) = cx.use_state(500.0_f64);
        let (response_ms, set_response_ms) = cx.use_state(2_000.0_f64);
        let (tail_ms, set_tail_ms) = cx.use_state(250.0_f64);
        let (trim_leading_silence, set_trim_leading_silence) = cx.use_state(true);
        let (capture_state, capture_trigger) = cx.use_mutation();

        let devices = match &devices_resource {
            Resource::Ready(devices) | Resource::Reloading(devices) => devices.clone(),
            _ => Vec::new(),
        };
        let selected_render = devices.get(selected_render_device.max(0) as usize).cloned();
        let selected_capture = devices
            .get(selected_capture_device.max(0) as usize)
            .cloned();
        let busy = capture_state.is_loading();
        let sample_rates_match = selected_render
            .as_ref()
            .zip(selected_capture.as_ref())
            .is_some_and(|(render, capture)| render.sample_rate == capture.sample_rate);
        let capture_label = selected_render
            .as_ref()
            .zip(selected_capture.as_ref())
            .map_or_else(
                || "Capture channel matrix".to_string(),
                |(render, capture)| {
                    format!("Capture {}×{} matrix", capture.channels, render.channels)
                },
            );

        let capture = {
            let trigger = capture_trigger;
            let output_directory = output_directory.clone();
            move || {
                let Some(DeviceChoice {
                    id: render_device_id,
                    ..
                }) = selected_render.clone()
                else {
                    return;
                };
                let Some(DeviceChoice {
                    id: capture_device_id,
                    ..
                }) = selected_capture.clone()
                else {
                    return;
                };
                let request = CaptureRequest {
                    render_device_id,
                    capture_device_id,
                    output_directory: output_directory.clone(),
                    probe_dbfs,
                    lead_ms,
                    response_ms,
                    tail_ms,
                    trim_leading_silence,
                };
                trigger.fire(move || run_capture(request));
            }
        };

        let device_controls: Element = match &devices_resource {
            Resource::Loading | Resource::Idle => hstack((
                ProgressRing::indeterminate().width(20.0).height(20.0),
                text_block("Loading playback endpoints…"),
            ))
            .spacing(8.0)
            .into(),
            Resource::Error(error) => InfoBar::new("Could not list playback endpoints")
                .message(error)
                .error()
                .is_closable(false)
                .into(),
            Resource::Ready(_) | Resource::Reloading(_) => {
                let labels = devices
                    .iter()
                    .map(|device| device.label.clone())
                    .collect::<Vec<_>>();
                vstack((
                    ComboBox::new(labels.clone())
                        .header("1. Send impulses to")
                        .selected_index(selected_render_device)
                        .enabled(!busy)
                        .on_selection_changed(set_selected_render_device),
                    ComboBox::new(labels)
                        .header("2. Capture processed output from")
                        .selected_index(selected_capture_device)
                        .enabled(!busy)
                        .on_selection_changed(set_selected_capture_device),
                    text_block(
                        "For a routed DSP chain, choose its virtual playback endpoint first and the final physical playback endpoint second.",
                    )
                    .wrap()
                    .opacity(0.72),
                ))
                .spacing(10.0)
                .into()
            }
        };

        let status: Element = match &capture_state {
            MutationState::Idle if !devices.is_empty() && !sample_rates_match => {
                InfoBar::new("Endpoint sample rates do not match")
                    .message(
                        "Set both endpoints to the same sample rate in Windows Sound settings before capturing.",
                    )
                    .warning()
                    .is_closable(false)
                    .into()
            }
            MutationState::Idle => InfoBar::new("Ready")
                .message(
                    "Keep the DSP routing application running and all other audio applications silent.",
                )
                .informational()
                .is_closable(false)
                .into(),
            MutationState::Loading => hstack((
                ProgressRing::indeterminate().width(24.0).height(24.0),
                text_block("Capturing the full channel matrix… Do not play other audio."),
            ))
            .spacing(10.0)
            .into(),
            MutationState::Success(summary) => {
                let status = InfoBar::new("Capture complete")
                    .message(summary.message())
                    .is_closable(false);
                if summary.peak >= 1.0 {
                    status.warning().into()
                } else {
                    status.success().into()
                }
            }
            MutationState::Error(error) => InfoBar::new("Capture failed")
                .message(error)
                .error()
                .is_closable(false)
                .into(),
        };

        let settings = {
            let mut layout = grid(())
                .rows([
                    GridLength::Auto,
                    GridLength::Auto,
                    GridLength::Auto,
                    GridLength::Auto,
                    GridLength::Auto,
                ])
                .columns([GridLength::Star(1.0), GridLength::Star(1.0)])
                .row_spacing(10.0)
                .column_spacing(12.0);
            layout.children = vec![
                NumberBox::new(probe_dbfs)
                    .range(-120.0, 0.0)
                    .header("Probe level (dBFS)")
                    .enabled(!busy)
                    .on_value_changed(set_probe_dbfs)
                    .grid_row(0)
                    .grid_column(0)
                    .into(),
                NumberBox::new(lead_ms)
                    .range(0.0, 10_000.0)
                    .header("Lead-in (ms)")
                    .enabled(!busy)
                    .on_value_changed(set_lead_ms)
                    .grid_row(0)
                    .grid_column(1)
                    .into(),
                NumberBox::new(response_ms)
                    .range(1.0, 60_000.0)
                    .header("Response window (ms)")
                    .enabled(!busy)
                    .on_value_changed(set_response_ms)
                    .grid_row(1)
                    .grid_column(0)
                    .into(),
                NumberBox::new(tail_ms)
                    .range(0.0, 10_000.0)
                    .header("Tail (ms)")
                    .enabled(!busy)
                    .on_value_changed(set_tail_ms)
                    .grid_row(1)
                    .grid_column(1)
                    .into(),
                ToggleSwitch::new(trim_leading_silence)
                    .header("Trim common leading digital silence")
                    .on_content("On")
                    .off_content("Off")
                    .enabled(!busy)
                    .on_toggled(set_trim_leading_silence)
                    .grid_row(2)
                    .grid_column(0)
                    .grid_column_span(2)
                    .into(),
            ];
            layout
        };

        scroll_viewer(
            vstack((
                title("Impulse Response Capture")
                    .heading_level(AutomationHeadingLevel::Level1),
                text_block(
                    "Capture a sample-accurate software-filter matrix and export Equalizer APO and CamillaDSP bundles.",
                )
                .wrap()
                .opacity(0.72),
                device_controls,
                text_box(output_directory)
                    .header("Output directory")
                    .placeholder_text("captured-filter")
                    .enabled(!busy)
                    .on_text_changed(set_output_directory),
                Expander::new(settings)
                    .header("Capture settings")
                    .expanded(true),
                button(if busy {
                    "Capturing…".to_string()
                } else {
                    capture_label
                })
                    .accent()
                    .enabled(!busy && !devices.is_empty() && sample_rates_match)
                    .on_click(capture),
                status,
                text_block(
                    "After capture: disable the original correction/routing app before testing its convolution replacement. The config filenames include the output directory name. Use the Equalizer APO file on Windows, or copy the bundle to Linux/SteamOS and use the CamillaDSP file.",
                )
                .wrap()
                .opacity(0.72),
            ))
            .spacing(16.0)
            .max_width(760.0)
            .padding(Thickness::uniform(28.0)),
        )
        .into()
    }

    pub fn run() -> Result<()> {
        App::new()
            .title("Impulse Response Capture")
            .inner_size(860.0, 720.0)
            .backdrop(Backdrop::Mica)
            .render(render)
    }
}

#[cfg(windows)]
fn main() -> windows_reactor::Result<()> {
    windows_app::run()
}

#[cfg(not(windows))]
fn main() {
    eprintln!("the capture UI requires Windows");
}

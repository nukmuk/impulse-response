use std::cmp::Ordering;
use std::thread;
use std::time::{Duration, Instant};

use wasapi::{
    BufferInfo, Device, DeviceEnumerator, Direction, SampleType, StreamMode, WaveFormat,
    initialize_mta,
};

use crate::{CaptureError, CaptureSchedule, ImpulseMatrix, Result, extract_matrix};

const QPC_UNITS_PER_SECOND: f64 = 10_000_000.0;

#[derive(Clone, Debug)]
pub struct RenderDevice {
    pub id: String,
    pub name: String,
    pub is_default: bool,
    pub sample_rate: u32,
    pub channels: u16,
    pub channel_mask: u32,
}

#[derive(Clone, Debug)]
pub struct LiveCaptureSettings {
    pub render_device_id: Option<String>,
    pub capture_device_id: Option<String>,
    pub lead_time: Duration,
    pub response_time: Duration,
    pub tail_time: Duration,
    pub probe_dbfs: f32,
    pub trim_leading_silence: bool,
}

impl Default for LiveCaptureSettings {
    fn default() -> Self {
        Self {
            render_device_id: None,
            capture_device_id: None,
            lead_time: Duration::from_millis(500),
            response_time: Duration::from_secs(2),
            tail_time: Duration::from_millis(250),
            probe_dbfs: -24.0,
            trim_leading_silence: true,
        }
    }
}

#[derive(Clone, Debug)]
pub struct CaptureDiagnostics {
    pub captured_frames: usize,
    pub peak: f32,
    pub render_clock_origin_qpc: f64,
    pub clock_observations: usize,
    pub trimmed_leading_frames: usize,
}

#[derive(Clone, Debug)]
pub struct LiveCapture {
    pub render_device: RenderDevice,
    pub capture_device: RenderDevice,
    pub schedule: CaptureSchedule,
    pub matrix: ImpulseMatrix,
    pub diagnostics: CaptureDiagnostics,
}

#[derive(Debug)]
struct CapturedPacket {
    index: u64,
    timestamp: u64,
    samples: Vec<f32>,
}

pub fn list_render_devices() -> Result<Vec<RenderDevice>> {
    initialize_com()?;
    let enumerator = backend(DeviceEnumerator::new())?;
    let default_id = backend(enumerator.get_default_device(&Direction::Render))?
        .get_id()
        .map_err(audio_error)?;
    let collection = backend(enumerator.get_device_collection(&Direction::Render))?;
    let mut result = Vec::new();
    for device in &collection {
        let device = backend(device)?;
        result.push(describe_device(&device, &default_id)?);
    }
    Ok(result)
}

pub fn measure_software_filter(settings: &LiveCaptureSettings) -> Result<LiveCapture> {
    initialize_com()?;
    let enumerator = backend(DeviceEnumerator::new())?;
    let render_device = match &settings.render_device_id {
        Some(id) => backend(enumerator.get_device(id))?,
        None => backend(enumerator.get_default_device(&Direction::Render))?,
    };
    let default_id = backend(enumerator.get_default_device(&Direction::Render))?
        .get_id()
        .map_err(audio_error)?;
    let render_description = describe_device(&render_device, &default_id)?;
    let capture_device = match &settings.capture_device_id {
        Some(id) => backend(enumerator.get_device(id))?,
        None => backend(enumerator.get_device(&render_description.id))?,
    };
    let capture_description = describe_device(&capture_device, &default_id)?;

    let mut render_audio_client = backend(render_device.get_iaudioclient())?;
    let render_mix_format = backend(render_audio_client.get_mixformat())?;
    let sample_rate = render_mix_format.get_samplespersec();
    let input_channel_count = usize::from(render_mix_format.get_nchannels());
    let input_channel_mask = render_mix_format.get_dwchannelmask();
    let mut capture_audio_client = backend(capture_device.get_iaudioclient())?;
    let capture_mix_format = backend(capture_audio_client.get_mixformat())?;
    let capture_sample_rate = capture_mix_format.get_samplespersec();
    let output_channel_count = usize::from(capture_mix_format.get_nchannels());
    let output_channel_mask = capture_mix_format.get_dwchannelmask();

    ensure_matching_sample_rates(
        &render_description,
        sample_rate,
        &capture_description,
        capture_sample_rate,
    )?;

    let render_format = WaveFormat::new(
        32,
        32,
        &SampleType::Float,
        sample_rate as usize,
        input_channel_count,
        (input_channel_mask != 0).then_some(input_channel_mask),
    );
    let capture_format = WaveFormat::new(
        32,
        32,
        &SampleType::Float,
        capture_sample_rate as usize,
        output_channel_count,
        (output_channel_mask != 0).then_some(output_channel_mask),
    );

    let schedule = CaptureSchedule::new(
        sample_rate,
        input_channel_count,
        duration_to_frames(settings.lead_time, sample_rate)?,
        duration_to_frames(settings.response_time, sample_rate)?,
        duration_to_frames(settings.tail_time, sample_rate)?,
        settings.probe_dbfs,
    )?;
    let stimulus = schedule.render_stimulus();
    let stimulus_bytes = floats_to_bytes(&stimulus);

    let (render_default_period, _) = backend(render_audio_client.get_device_period())?;
    let (capture_default_period, _) = backend(capture_audio_client.get_device_period())?;
    let render_mode = StreamMode::PollingShared {
        autoconvert: true,
        buffer_duration_hns: render_default_period,
    };
    let capture_mode = StreamMode::PollingShared {
        autoconvert: true,
        buffer_duration_hns: capture_default_period,
    };
    backend(render_audio_client.initialize_client(
        &render_format,
        &Direction::Render,
        &render_mode,
    ))?;
    backend(capture_audio_client.initialize_client(
        &capture_format,
        &Direction::Capture,
        &capture_mode,
    ))?;

    let render_buffer_frames = backend(render_audio_client.get_buffer_size())? as usize;
    let render_client = backend(render_audio_client.get_audiorenderclient())?;
    let capture_client = backend(capture_audio_client.get_audiocaptureclient())?;
    let render_clock = backend(render_audio_client.get_audioclock())?;
    let render_clock_frequency = backend(render_clock.get_frequency())?;

    let initial_frames = render_buffer_frames.min(schedule.total_frames());
    backend(render_client.write_to_device(
        initial_frames,
        &stimulus_bytes[..initial_frames * input_channel_count * size_of::<f32>()],
        None,
    ))?;
    let mut submitted_frames = initial_frames;

    backend(capture_audio_client.start_stream())?;
    if let Err(error) = render_audio_client.start_stream() {
        let _ = capture_audio_client.stop_stream();
        return Err(audio_error(error));
    }

    let expected_duration = frames_to_duration(schedule.total_frames(), sample_rate);
    let deadline = Instant::now() + expected_duration + Duration::from_secs(5);
    let mut packets = Vec::new();
    let mut clock_origins = Vec::new();
    let mut first_capture_packet = true;
    let mut render_finished_at = None;

    let run_result = (|| -> Result<()> {
        loop {
            if Instant::now() > deadline {
                return Err(CaptureError::CaptureIntegrity(
                    "timed out while waiting for the render stream to drain".into(),
                ));
            }

            if let Ok((position, qpc)) = render_clock.get_position()
                && position > 0
            {
                let origin = qpc as f64
                    - position as f64 * QPC_UNITS_PER_SECOND / render_clock_frequency as f64;
                clock_origins.push(origin);
            }

            drain_capture_packets(
                &capture_client,
                output_channel_count,
                &mut packets,
                &mut first_capture_packet,
            )?;

            if submitted_frames < schedule.total_frames() {
                let available =
                    backend(render_audio_client.get_available_space_in_frames())? as usize;
                let frames = available.min(schedule.total_frames() - submitted_frames);
                if frames > 0 {
                    let byte_start = submitted_frames * input_channel_count * size_of::<f32>();
                    let byte_end =
                        (submitted_frames + frames) * input_channel_count * size_of::<f32>();
                    backend(render_client.write_to_device(
                        frames,
                        &stimulus_bytes[byte_start..byte_end],
                        None,
                    ))?;
                    submitted_frames += frames;
                }
            } else if backend(render_audio_client.get_current_padding())? == 0 {
                let finished = render_finished_at.get_or_insert_with(Instant::now);
                if finished.elapsed() >= Duration::from_millis(100) {
                    drain_capture_packets(
                        &capture_client,
                        output_channel_count,
                        &mut packets,
                        &mut first_capture_packet,
                    )?;
                    break;
                }
            }

            thread::sleep(Duration::from_millis(1));
        }
        Ok(())
    })();

    let render_stop = render_audio_client.stop_stream().map_err(audio_error);
    let capture_stop = capture_audio_client.stop_stream().map_err(audio_error);
    run_result?;
    render_stop?;
    capture_stop?;

    if clock_origins.is_empty() {
        return Err(CaptureError::CaptureIntegrity(
            "the render audio clock did not advance".into(),
        ));
    }
    let clock_observations = clock_origins.len();
    let render_clock_origin_qpc = median(&mut clock_origins);
    let captured = packets_on_render_timeline(
        &packets,
        render_clock_origin_qpc,
        sample_rate,
        output_channel_count,
        schedule.total_frames(),
    );
    let peak = captured
        .iter()
        .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
    if peak == 0.0 {
        return Err(CaptureError::CaptureIntegrity(
            "loopback capture contained only silence".into(),
        ));
    }

    let input_labels = channel_labels(input_channel_count, input_channel_mask);
    let output_labels = channel_labels(output_channel_count, output_channel_mask);
    let mut matrix = extract_matrix(&captured, output_labels, input_labels, &schedule, 0)?;
    let trimmed_leading_frames = if settings.trim_leading_silence {
        matrix.trim_common_leading_silence()
    } else {
        0
    };

    Ok(LiveCapture {
        render_device: render_description,
        capture_device: capture_description,
        schedule,
        matrix,
        diagnostics: CaptureDiagnostics {
            captured_frames: captured.len() / output_channel_count,
            peak,
            render_clock_origin_qpc,
            clock_observations,
            trimmed_leading_frames,
        },
    })
}

fn ensure_matching_sample_rates(
    render_device: &RenderDevice,
    render_sample_rate: u32,
    capture_device: &RenderDevice,
    capture_sample_rate: u32,
) -> Result<()> {
    if render_sample_rate != capture_sample_rate {
        return Err(CaptureError::InvalidSettings(format!(
            "the render endpoint {} uses {} Hz, but the loopback endpoint {} uses {} Hz; set both Windows endpoints to the same sample rate",
            render_device.name, render_sample_rate, capture_device.name, capture_sample_rate
        )));
    }
    Ok(())
}

fn drain_capture_packets(
    capture_client: &wasapi::AudioCaptureClient,
    channels: usize,
    packets: &mut Vec<CapturedPacket>,
    first_packet: &mut bool,
) -> Result<()> {
    loop {
        let Some(frames) = backend(capture_client.get_next_packet_size())? else {
            return Err(CaptureError::CaptureIntegrity(
                "loopback capture unexpectedly entered exclusive mode".into(),
            ));
        };
        if frames == 0 {
            return Ok(());
        }

        let mut bytes = vec![0_u8; frames as usize * channels * size_of::<f32>()];
        let (read_frames, info) = backend(capture_client.read_from_device(&mut bytes))?;
        validate_packet(&info, *first_packet)?;
        *first_packet = false;
        bytes.truncate(read_frames as usize * channels * size_of::<f32>());
        let samples = if info.flags.silent {
            vec![0.0; read_frames as usize * channels]
        } else {
            bytes_to_floats(&bytes)
        };
        packets.push(CapturedPacket {
            index: info.index,
            timestamp: info.timestamp,
            samples,
        });
    }
}

fn validate_packet(info: &BufferInfo, first_packet: bool) -> Result<()> {
    if info.flags.timestamp_error {
        return Err(CaptureError::CaptureIntegrity(
            "WASAPI reported an invalid capture timestamp".into(),
        ));
    }
    if info.flags.data_discontinuity && !first_packet {
        return Err(CaptureError::CaptureIntegrity(
            "WASAPI reported dropped capture frames".into(),
        ));
    }
    Ok(())
}

fn packets_on_render_timeline(
    packets: &[CapturedPacket],
    origin_qpc: f64,
    sample_rate: u32,
    channels: usize,
    minimum_frames: usize,
) -> Vec<f32> {
    // QPC establishes one common origin between the render and capture
    // streams. Integer WASAPI stream indices then place all packet samples;
    // timestamp jitter can therefore never change relative sample spacing.
    let mut stream_start_candidates = packets
        .iter()
        .map(|packet| {
            let timestamp_frame = ((packet.timestamp as f64 - origin_qpc) * sample_rate as f64
                / QPC_UNITS_PER_SECOND)
                .round() as i64;
            timestamp_frame - packet.index as i64
        })
        .collect::<Vec<_>>();
    stream_start_candidates.sort_unstable();
    let stream_start = stream_start_candidates
        .get(stream_start_candidates.len() / 2)
        .copied()
        .unwrap_or_default();
    let maximum_frame = packets
        .iter()
        .fold(minimum_frames as i64, |maximum, packet| {
            maximum
                .max(stream_start + packet.index as i64 + (packet.samples.len() / channels) as i64)
        });
    let mut result = vec![0.0; maximum_frame.max(0) as usize * channels];

    for packet in packets {
        let start = stream_start + packet.index as i64;
        let packet_frames = packet.samples.len() / channels;
        for packet_frame in 0..packet_frames {
            let destination_frame = start + packet_frame as i64;
            if destination_frame < 0 {
                continue;
            }
            let destination = destination_frame as usize * channels;
            let source = packet_frame * channels;
            result[destination..destination + channels]
                .copy_from_slice(&packet.samples[source..source + channels]);
        }
    }
    result
}

fn describe_device(device: &Device, default_id: &str) -> Result<RenderDevice> {
    let format = backend(device.get_device_format())?;
    let id = backend(device.get_id())?;
    Ok(RenderDevice {
        name: backend(device.get_friendlyname())?,
        is_default: id == default_id,
        id,
        sample_rate: format.get_samplespersec(),
        channels: format.get_nchannels(),
        channel_mask: format.get_dwchannelmask(),
    })
}

fn channel_labels(channels: usize, mask: u32) -> Vec<String> {
    const SPEAKERS: &[(u32, &str)] = &[
        (0x1, "L"),
        (0x2, "R"),
        (0x4, "C"),
        (0x8, "LFE"),
        (0x10, "RL"),
        (0x20, "RR"),
        (0x40, "FLC"),
        (0x80, "FRC"),
        (0x100, "RC"),
        (0x200, "SL"),
        (0x400, "SR"),
    ];
    let named = SPEAKERS
        .iter()
        .filter(|(bit, _)| mask & bit != 0)
        .map(|(_, name)| (*name).to_owned())
        .collect::<Vec<_>>();
    if named.len() == channels {
        named
    } else if channels == 1 {
        vec!["C".into()]
    } else if channels == 2 {
        vec!["L".into(), "R".into()]
    } else {
        (1..=channels).map(|channel| channel.to_string()).collect()
    }
}

fn duration_to_frames(duration: Duration, sample_rate: u32) -> Result<usize> {
    let frames = duration.as_secs_f64() * sample_rate as f64;
    if !frames.is_finite() || frames > usize::MAX as f64 {
        return Err(CaptureError::InvalidSettings(
            "capture duration is too large".into(),
        ));
    }
    Ok(frames.round() as usize)
}

fn frames_to_duration(frames: usize, sample_rate: u32) -> Duration {
    Duration::from_secs_f64(frames as f64 / sample_rate as f64)
}

fn floats_to_bytes(samples: &[f32]) -> Vec<u8> {
    samples
        .iter()
        .flat_map(|sample| sample.to_ne_bytes())
        .collect()
}

fn bytes_to_floats(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(size_of::<f32>())
        .map(|chunk| f32::from_ne_bytes(chunk.try_into().expect("four-byte chunk")))
        .collect()
}

fn median(values: &mut [f64]) -> f64 {
    values.sort_unstable_by(|left, right| left.partial_cmp(right).unwrap_or(Ordering::Equal));
    values[values.len() / 2]
}

fn initialize_com() -> Result<()> {
    initialize_mta()
        .ok()
        .map_err(|error| CaptureError::AudioBackend(error.to_string()))
}

fn backend<T>(result: std::result::Result<T, wasapi::WasapiError>) -> Result<T> {
    result.map_err(audio_error)
}

fn audio_error(error: wasapi::WasapiError) -> CaptureError {
    CaptureError::AudioBackend(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(name: &str, sample_rate: u32) -> RenderDevice {
        RenderDevice {
            id: name.into(),
            name: name.into(),
            is_default: false,
            sample_rate,
            channels: 2,
            channel_mask: 0x3,
        }
    }

    #[test]
    fn packet_timestamps_map_to_exact_render_frames() {
        let origin = 100_000_000.0;
        let sample_rate = 48_000;
        let timestamp = origin as u64 + 10_000_000 / 100; // 10 ms = 480 frames
        let packets = vec![CapturedPacket {
            index: 0,
            timestamp,
            samples: vec![0.25, -0.25, 0.5, -0.5],
        }];

        let timeline = packets_on_render_timeline(&packets, origin, sample_rate, 2, 600);
        assert_eq!(timeline[480 * 2], 0.25);
        assert_eq!(timeline[480 * 2 + 1], -0.25);
        assert_eq!(timeline[481 * 2], 0.5);
        assert_eq!(timeline[481 * 2 + 1], -0.5);
    }

    #[test]
    fn packet_indices_preserve_spacing_despite_timestamp_jitter() {
        let origin = 100_000_000.0;
        let sample_rate = 48_000;
        let ten_ms = 10_000_000 / 100;
        let packets = vec![
            CapturedPacket {
                index: 0,
                timestamp: origin as u64 + ten_ms,
                samples: vec![1.0, 0.0],
            },
            CapturedPacket {
                index: 2,
                // Deliberately jittered by almost one frame.
                timestamp: origin as u64 + ten_ms + 2 * 10_000_000 / 48_000 + 180,
                samples: vec![0.0, 1.0],
            },
            CapturedPacket {
                index: 4,
                timestamp: origin as u64 + ten_ms + 4 * 10_000_000 / 48_000,
                samples: vec![0.5, 0.5],
            },
        ];

        let timeline = packets_on_render_timeline(&packets, origin, sample_rate, 1, 600);
        assert_eq!(timeline[480], 1.0);
        assert_eq!(timeline[482], 0.0);
        assert_eq!(timeline[483], 1.0);
        assert_eq!(timeline[484], 0.5);
        assert_eq!(timeline[485], 0.5);
    }

    #[test]
    fn channel_labels_follow_windows_speaker_mask_order() {
        assert_eq!(channel_labels(2, 0x3), ["L", "R"]);
        assert_eq!(channel_labels(6, 0x3f), ["L", "R", "C", "LFE", "RL", "RR"]);
    }

    #[test]
    fn cross_endpoint_capture_requires_matching_sample_rates() {
        let render = device("Virtual Audio Device", 48_000);
        let capture = device("LCD TV", 44_100);
        let error = ensure_matching_sample_rates(&render, 48_000, &capture, 44_100)
            .expect_err("mismatched rates must be rejected");

        assert_eq!(
            error.to_string(),
            "invalid capture settings: the render endpoint Virtual Audio Device uses 48000 Hz, but the loopback endpoint LCD TV uses 44100 Hz; set both Windows endpoints to the same sample rate"
        );
    }
}

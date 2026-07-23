use crate::{CaptureError, ImpulseMatrix, Result};

/// A continuous sample-domain excitation schedule.
#[derive(Clone, Debug)]
pub struct CaptureSchedule {
    sample_rate: u32,
    channel_count: usize,
    lead_frames: usize,
    response_frames: usize,
    tail_frames: usize,
    probe_amplitude: f32,
    impulse_frames: Vec<usize>,
}

impl CaptureSchedule {
    pub fn new(
        sample_rate: u32,
        channel_count: usize,
        lead_frames: usize,
        response_frames: usize,
        tail_frames: usize,
        probe_dbfs: f32,
    ) -> Result<Self> {
        if sample_rate == 0 {
            return Err(CaptureError::InvalidSettings(
                "sample rate must be greater than zero".into(),
            ));
        }
        if channel_count == 0 {
            return Err(CaptureError::NoChannels);
        }
        if response_frames == 0 {
            return Err(CaptureError::InvalidSettings(
                "response window must contain at least one frame".into(),
            ));
        }
        if !probe_dbfs.is_finite() || probe_dbfs > 0.0 {
            return Err(CaptureError::InvalidSettings(
                "probe level must be finite and no greater than 0 dBFS".into(),
            ));
        }

        let probe_amplitude = 10.0_f32.powf(probe_dbfs / 20.0);
        let impulse_frames = (0..channel_count)
            .map(|input| lead_frames + input * response_frames)
            .collect();

        Ok(Self {
            sample_rate,
            channel_count,
            lead_frames,
            response_frames,
            tail_frames,
            probe_amplitude,
            impulse_frames,
        })
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn channel_count(&self) -> usize {
        self.channel_count
    }

    pub fn lead_frames(&self) -> usize {
        self.lead_frames
    }

    pub fn response_frames(&self) -> usize {
        self.response_frames
    }

    pub fn tail_frames(&self) -> usize {
        self.tail_frames
    }

    pub fn probe_amplitude(&self) -> f32 {
        self.probe_amplitude
    }

    pub fn impulse_frames(&self) -> &[usize] {
        &self.impulse_frames
    }

    pub fn total_frames(&self) -> usize {
        self.lead_frames + self.channel_count * self.response_frames + self.tail_frames
    }

    /// Produces interleaved floating-point frames for one uninterrupted stream.
    pub fn render_stimulus(&self) -> Vec<f32> {
        let mut samples = vec![0.0; self.total_frames() * self.channel_count];
        for (input, &frame) in self.impulse_frames.iter().enumerate() {
            samples[frame * self.channel_count + input] = self.probe_amplitude;
        }
        samples
    }
}

/// Extracts every input-to-output response using one shared timeline origin.
///
/// `transport_offset_frames` is applied globally. No individual response is
/// peak-aligned, so leading zeros and relative inter-channel delays survive.
pub fn extract_matrix(
    captured_interleaved: &[f32],
    output_channels: Vec<String>,
    input_channels: Vec<String>,
    schedule: &CaptureSchedule,
    transport_offset_frames: usize,
) -> Result<ImpulseMatrix> {
    let output_count = output_channels.len();
    if output_count == 0 {
        return Err(CaptureError::NoChannels);
    }
    if input_channels.len() != schedule.channel_count {
        return Err(CaptureError::InvalidSettings(format!(
            "schedule has {} inputs but {} input labels were supplied",
            schedule.channel_count,
            input_channels.len()
        )));
    }

    let actual_frames = captured_interleaved.len() / output_count;
    let last_start = transport_offset_frames + schedule.impulse_frames[schedule.channel_count - 1];
    let needed_frames = last_start + schedule.response_frames;
    if actual_frames < needed_frames {
        return Err(CaptureError::CaptureTooShort {
            needed_frames,
            actual_frames,
        });
    }

    let mut responses =
        vec![vec![vec![0.0; schedule.response_frames]; output_count]; schedule.channel_count];
    for (input, &impulse_frame) in schedule.impulse_frames.iter().enumerate() {
        let start = transport_offset_frames + impulse_frame;
        for output in 0..output_count {
            for relative_frame in 0..schedule.response_frames {
                responses[input][output][relative_frame] = captured_interleaved
                    [(start + relative_frame) * output_count + output]
                    / schedule.probe_amplitude;
            }
        }
    }

    ImpulseMatrix::new(
        schedule.sample_rate,
        input_channels,
        output_channels,
        responses,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stimulus_contains_one_impulse_per_input_at_exact_frames() {
        let schedule = CaptureSchedule::new(48_000, 2, 10, 20, 5, -6.0).unwrap();
        let stimulus = schedule.render_stimulus();

        assert_eq!(schedule.impulse_frames(), &[10, 30]);
        assert_eq!(schedule.total_frames(), 55);
        assert_eq!(stimulus.iter().filter(|&&sample| sample != 0.0).count(), 2);
        assert_eq!(stimulus[10 * 2], schedule.probe_amplitude());
        assert_eq!(stimulus[30 * 2 + 1], schedule.probe_amplitude());
    }

    #[test]
    fn extraction_preserves_relative_delays_and_crossfeed() {
        let schedule = CaptureSchedule::new(48_000, 2, 8, 16, 4, -12.0).unwrap();
        let transport = 7;
        let mut captured = vec![0.0; (transport + schedule.total_frames()) * 2];
        let amplitude = schedule.probe_amplitude();

        // L input: L output is delayed 2 samples; R output is delayed 5.
        let left_start = transport + schedule.impulse_frames()[0];
        captured[(left_start + 2) * 2] = amplitude * 0.5;
        captured[(left_start + 5) * 2 + 1] = amplitude * 0.25;

        // R input: crossfeed reaches L after 3; direct R reaches R after 1.
        let right_start = transport + schedule.impulse_frames()[1];
        captured[(right_start + 3) * 2] = amplitude * -0.125;
        captured[(right_start + 1) * 2 + 1] = amplitude;

        let matrix = extract_matrix(
            &captured,
            vec!["L".into(), "R".into()],
            vec!["L".into(), "R".into()],
            &schedule,
            transport,
        )
        .unwrap();

        assert_eq!(matrix.responses()[0][0][2], 0.5);
        assert_eq!(matrix.responses()[0][1][5], 0.25);
        assert_eq!(matrix.responses()[1][0][3], -0.125);
        assert_eq!(matrix.responses()[1][1][1], 1.0);
        assert_eq!(matrix.responses()[0][0][0], 0.0);
    }

    #[test]
    fn extraction_supports_different_input_and_output_channel_counts() {
        let schedule = CaptureSchedule::new(48_000, 1, 4, 8, 2, -6.0).unwrap();
        let amplitude = schedule.probe_amplitude();
        let mut captured = vec![0.0; schedule.total_frames() * 2];
        captured[(schedule.impulse_frames()[0] + 1) * 2] = amplitude;
        captured[(schedule.impulse_frames()[0] + 3) * 2 + 1] = amplitude * 0.5;

        let matrix = extract_matrix(
            &captured,
            vec!["L".into(), "R".into()],
            vec!["C".into()],
            &schedule,
            0,
        )
        .unwrap();

        assert_eq!(matrix.input_channels(), ["C"]);
        assert_eq!(matrix.output_channels(), ["L", "R"]);
        assert_eq!(matrix.responses()[0][0][1], 1.0);
        assert_eq!(matrix.responses()[0][1][3], 0.5);
    }
}

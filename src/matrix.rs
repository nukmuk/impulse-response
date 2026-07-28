use crate::{CaptureError, Result};

/// A measured linear system, indexed as `[input][output][sample]`.
#[derive(Clone, Debug, PartialEq)]
pub struct ImpulseMatrix {
    sample_rate: u32,
    input_channels: Vec<String>,
    output_channels: Vec<String>,
    responses: Vec<Vec<Vec<f32>>>,
}

impl ImpulseMatrix {
    pub fn new(
        sample_rate: u32,
        input_channels: Vec<String>,
        output_channels: Vec<String>,
        responses: Vec<Vec<Vec<f32>>>,
    ) -> Result<Self> {
        if sample_rate == 0 {
            return Err(CaptureError::InvalidSettings(
                "sample rate must be greater than zero".into(),
            ));
        }
        if input_channels.is_empty() || output_channels.is_empty() {
            return Err(CaptureError::NoChannels);
        }

        let valid = responses.len() == input_channels.len()
            && responses.iter().all(|input| {
                input.len() == output_channels.len()
                    && input.first().is_some_and(|first| {
                        input.iter().all(|response| response.len() == first.len())
                    })
            });
        if !valid {
            return Err(CaptureError::InvalidMatrix);
        }

        Ok(Self {
            sample_rate,
            input_channels,
            output_channels,
            responses,
        })
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn input_channels(&self) -> &[String] {
        &self.input_channels
    }

    pub fn output_channels(&self) -> &[String] {
        &self.output_channels
    }

    pub fn responses(&self) -> &[Vec<Vec<f32>>] {
        &self.responses
    }

    pub fn response_frames(&self) -> usize {
        self.responses[0][0].len()
    }

    /// Removes frames that are exactly silent across every matrix path.
    ///
    /// One common frame count is removed from every response so relative
    /// channel and path timing remains unchanged.
    pub fn trim_common_leading_silence(&mut self) -> usize {
        let first_signal_frame = (0..self.response_frames()).find(|&frame| {
            self.responses
                .iter()
                .flatten()
                .any(|response| response[frame] != 0.0)
        });
        let trimmed_frames = first_signal_frame.unwrap_or(0);

        if trimmed_frames > 0 {
            for response in self.responses.iter_mut().flatten() {
                response.drain(..trimmed_frames);
            }
        }

        trimmed_frames
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_trim_preserves_all_relative_path_delays() {
        let mut matrix = ImpulseMatrix::new(
            48_000,
            vec!["L".into(), "R".into()],
            vec!["L".into(), "R".into()],
            vec![
                vec![
                    vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0],
                    vec![0.0, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 0.0],
                ],
                vec![
                    vec![0.0, 0.0, 0.0, 0.0, 0.0, -0.25, 0.0, 0.0],
                    vec![0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.75],
                ],
            ],
        )
        .unwrap();

        assert_eq!(matrix.trim_common_leading_silence(), 3);
        assert_eq!(matrix.response_frames(), 5);
        assert_eq!(matrix.responses()[0][0][0], 1.0);
        assert_eq!(matrix.responses()[0][1][1], 0.5);
        assert_eq!(matrix.responses()[1][0][2], -0.25);
        assert_eq!(matrix.responses()[1][1][4], 0.75);
    }

    #[test]
    fn common_trim_leaves_an_all_silent_matrix_unchanged() {
        let mut matrix = ImpulseMatrix::new(
            48_000,
            vec!["L".into()],
            vec!["L".into()],
            vec![vec![vec![0.0; 4]]],
        )
        .unwrap();

        assert_eq!(matrix.trim_common_leading_silence(), 0);
        assert_eq!(matrix.response_frames(), 4);
    }
}

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
}

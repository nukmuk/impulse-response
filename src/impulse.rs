use std::fmt;
use std::io::{Seek, Write};
use std::path::Path;
use std::str::FromStr;

use hound::{SampleFormat, WavSpec, WavWriter};

use crate::{CaptureError, Result};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PcmBitDepth {
    Pcm16,
    Pcm24,
    Pcm32,
}

impl PcmBitDepth {
    pub fn bits(self) -> u16 {
        match self {
            Self::Pcm16 => 16,
            Self::Pcm24 => 24,
            Self::Pcm32 => 32,
        }
    }

    fn maximum_sample(self) -> i32 {
        match self {
            Self::Pcm16 => i16::MAX.into(),
            Self::Pcm24 => (1 << 23) - 1,
            Self::Pcm32 => i32::MAX,
        }
    }
}

impl fmt::Display for PcmBitDepth {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.bits().fmt(formatter)
    }
}

impl FromStr for PcmBitDepth {
    type Err = String;

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        match value {
            "16" => Ok(Self::Pcm16),
            "24" => Ok(Self::Pcm24),
            "32" => Ok(Self::Pcm32),
            _ => Err("bit depth must be 16, 24, or 32".into()),
        }
    }
}

/// Writes one second of mono integer PCM with a positive full-scale first sample and exact zeros
/// thereafter. No gain, filtering, noise shaping, or dithering is applied.
pub fn write_dirac_wav(
    path: impl AsRef<Path>,
    sample_rate: u32,
    bit_depth: PcmBitDepth,
) -> Result<()> {
    validate_sample_rate(sample_rate)?;
    let writer = WavWriter::create(path, wav_spec(sample_rate, bit_depth))?;
    write_dirac_samples(writer, sample_rate, bit_depth)
}

fn wav_spec(sample_rate: u32, bit_depth: PcmBitDepth) -> WavSpec {
    WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: bit_depth.bits(),
        sample_format: SampleFormat::Int,
    }
}

fn write_dirac_samples<W: Write + Seek>(
    mut writer: WavWriter<W>,
    sample_rate: u32,
    bit_depth: PcmBitDepth,
) -> Result<()> {
    validate_sample_rate(sample_rate)?;

    writer.write_sample(bit_depth.maximum_sample())?;
    for _ in 1..sample_rate {
        writer.write_sample(0_i32)?;
    }
    writer.finalize()?;
    Ok(())
}

fn validate_sample_rate(sample_rate: u32) -> Result<()> {
    if sample_rate == 0 {
        return Err(CaptureError::InvalidSettings(
            "sample rate must be greater than zero".into(),
        ));
    }
    if sample_rate > 768_000 {
        return Err(CaptureError::InvalidSettings(
            "sample rate must not exceed 768000 Hz".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use hound::WavReader;

    use super::*;

    #[test]
    fn writes_one_full_scale_sample_followed_by_exact_zeros() {
        for bit_depth in [PcmBitDepth::Pcm16, PcmBitDepth::Pcm24, PcmBitDepth::Pcm32] {
            let mut bytes = Cursor::new(Vec::new());
            let writer = WavWriter::new(&mut bytes, wav_spec(8, bit_depth)).unwrap();
            write_dirac_samples(writer, 8, bit_depth).unwrap();

            let mut reader = WavReader::new(Cursor::new(bytes.into_inner())).unwrap();
            assert_eq!(reader.spec(), wav_spec(8, bit_depth));
            let samples = reader
                .samples::<i32>()
                .collect::<std::result::Result<Vec<_>, _>>()
                .unwrap();
            assert_eq!(samples.len(), 8);
            assert_eq!(samples[0], bit_depth.maximum_sample());
            assert!(samples[1..].iter().all(|&sample| sample == 0));
        }
    }

    #[test]
    fn parses_only_supported_integer_pcm_depths() {
        assert_eq!("16".parse(), Ok(PcmBitDepth::Pcm16));
        assert_eq!("24".parse(), Ok(PcmBitDepth::Pcm24));
        assert_eq!("32".parse(), Ok(PcmBitDepth::Pcm32));
        assert_eq!(
            "float".parse::<PcmBitDepth>(),
            Err("bit depth must be 16, 24, or 32".into())
        );
    }
}

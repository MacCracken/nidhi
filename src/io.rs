//! File I/O helpers — load WAV files into [`Sample`], stream large instruments.
//!
//! Requires the `io` feature flag (which implies `std`).
//!
//! # Example
//!
//! ```rust,no_run
//! use nidhi::io;
//!
//! let sample = io::load_wav("piano_c4.wav").unwrap();
//! assert!(sample.frames() > 0);
//! ```

use std::io::Read;
use std::path::Path;

use crate::error::{NidhiError, Result};
use crate::sample::Sample;

/// Load a WAV file into a [`Sample`].
///
/// Supports 8-bit, 16-bit, 24-bit integer and 32-bit float WAV files.
/// Stereo files are loaded as interleaved stereo; mono files as mono.
pub fn load_wav<P: AsRef<Path>>(path: P) -> Result<Sample> {
    let reader = hound::WavReader::open(path.as_ref())
        .map_err(|e| NidhiError::ImportError(format!("failed to open WAV: {e}")))?;

    let spec = reader.spec();
    let channels = spec.channels as u32;
    let sample_rate = spec.sample_rate;

    let data = read_wav_samples(reader)?;

    let name = path
        .as_ref()
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("");

    let sample = if channels == 1 {
        Sample::from_mono(data, sample_rate)
    } else {
        Sample::from_stereo(data, sample_rate)
    };

    Ok(sample.with_name(name))
}

/// Load a WAV file from a byte slice (in-memory).
pub fn load_wav_from_memory(data: &[u8]) -> Result<Sample> {
    let cursor = std::io::Cursor::new(data);
    let reader = hound::WavReader::new(cursor)
        .map_err(|e| NidhiError::ImportError(format!("failed to parse WAV: {e}")))?;

    let spec = reader.spec();
    let channels = spec.channels as u32;
    let sample_rate = spec.sample_rate;

    let samples = read_wav_samples(reader)?;

    let sample = if channels == 1 {
        Sample::from_mono(samples, sample_rate)
    } else {
        Sample::from_stereo(samples, sample_rate)
    };

    Ok(sample)
}

fn read_wav_samples<R: Read>(reader: hound::WavReader<R>) -> Result<Vec<f32>> {
    let spec = reader.spec();

    match spec.sample_format {
        hound::SampleFormat::Int => {
            let bits = spec.bits_per_sample;
            let scale = 1.0 / (1u32 << (bits - 1)) as f32;
            reader
                .into_samples::<i32>()
                .map(|s| {
                    s.map(|v| v as f32 * scale)
                        .map_err(|e| NidhiError::ImportError(format!("WAV sample error: {e}")))
                })
                .collect()
        }
        hound::SampleFormat::Float => reader
            .into_samples::<f32>()
            .map(|s| s.map_err(|e| NidhiError::ImportError(format!("WAV sample error: {e}"))))
            .collect(),
    }
}

/// Load a WAV file and stream it in chunks for large instruments.
///
/// Returns an iterator yielding `chunk_frames` frames at a time.
/// Useful for instruments too large to fit entirely in memory.
pub struct StreamingWavReader {
    reader: hound::WavReader<std::io::BufReader<std::fs::File>>,
    channels: u32,
    sample_rate: u32,
    total_frames: usize,
    frames_read: usize,
}

impl StreamingWavReader {
    /// Open a WAV file for streaming.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let reader = hound::WavReader::open(path.as_ref()).map_err(|e| {
            NidhiError::ImportError(format!("failed to open WAV for streaming: {e}"))
        })?;
        let spec = reader.spec();
        let total_samples = reader.len() as usize;
        let channels = spec.channels as u32;
        Ok(Self {
            reader,
            channels,
            sample_rate: spec.sample_rate,
            total_frames: total_samples / channels as usize,
            frames_read: 0,
        })
    }

    /// Sample rate.
    #[must_use]
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Number of channels.
    #[must_use]
    pub fn channels(&self) -> u32 {
        self.channels
    }

    /// Total frames in the file.
    #[must_use]
    pub fn total_frames(&self) -> usize {
        self.total_frames
    }

    /// Frames already read.
    #[must_use]
    pub fn frames_read(&self) -> usize {
        self.frames_read
    }

    /// Read the next chunk of frames. Returns empty vec when done.
    pub fn read_chunk(&mut self, chunk_frames: usize) -> Result<Vec<f32>> {
        let remaining = self.total_frames - self.frames_read;
        let frames_to_read = chunk_frames.min(remaining);
        if frames_to_read == 0 {
            return Ok(Vec::new());
        }

        let samples_to_read = frames_to_read * self.channels as usize;
        let spec = self.reader.spec();
        let mut data = Vec::with_capacity(samples_to_read);

        match spec.sample_format {
            hound::SampleFormat::Int => {
                let bits = spec.bits_per_sample;
                let scale = 1.0 / (1u32 << (bits - 1)) as f32;
                for s in self.reader.samples::<i32>().take(samples_to_read) {
                    let v =
                        s.map_err(|e| NidhiError::ImportError(format!("WAV stream error: {e}")))?;
                    data.push(v as f32 * scale);
                }
            }
            hound::SampleFormat::Float => {
                for s in self.reader.samples::<f32>().take(samples_to_read) {
                    let v =
                        s.map_err(|e| NidhiError::ImportError(format!("WAV stream error: {e}")))?;
                    data.push(v);
                }
            }
        }

        self.frames_read += data.len() / self.channels as usize;
        Ok(data)
    }

    /// Read the entire file into a [`Sample`].
    pub fn read_all(mut self) -> Result<Sample> {
        let data = self.read_chunk(self.total_frames)?;
        let sample = if self.channels == 1 {
            Sample::from_mono(data, self.sample_rate)
        } else {
            Sample::from_stereo(data, self.sample_rate)
        };
        Ok(sample)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_wav_bytes(samples: &[f32], channels: u16, sample_rate: u32) -> Vec<u8> {
        let mut cursor = std::io::Cursor::new(Vec::new());
        let spec = hound::WavSpec {
            channels,
            sample_rate,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut writer = hound::WavWriter::new(&mut cursor, spec).unwrap();
        for &s in samples {
            writer.write_sample(s).unwrap();
        }
        writer.finalize().unwrap();
        cursor.into_inner()
    }

    #[test]
    fn load_wav_from_memory_mono() {
        let data = vec![0.1f32, 0.2, 0.3, 0.4];
        let wav_bytes = make_wav_bytes(&data, 1, 44100);
        let sample = load_wav_from_memory(&wav_bytes).unwrap();
        assert_eq!(sample.frames(), 4);
        assert_eq!(sample.channels(), 1);
        assert_eq!(sample.sample_rate(), 44100);
    }

    #[test]
    fn load_wav_from_memory_stereo() {
        let data = vec![0.1f32, -0.1, 0.2, -0.2]; // 2 stereo frames
        let wav_bytes = make_wav_bytes(&data, 2, 48000);
        let sample = load_wav_from_memory(&wav_bytes).unwrap();
        assert_eq!(sample.frames(), 2);
        assert_eq!(sample.channels(), 2);
        assert_eq!(sample.sample_rate(), 48000);
    }

    #[test]
    fn load_wav_from_memory_invalid() {
        assert!(load_wav_from_memory(&[0, 1, 2, 3]).is_err());
    }
}

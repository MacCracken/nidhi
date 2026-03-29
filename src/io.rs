//! File I/O helpers — load audio files into [`Sample`], stream large instruments.
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

use std::path::Path;

use crate::error::{NidhiError, Result};
use crate::sample::Sample;

/// Load a WAV file into a [`Sample`].
///
/// Supports 8-bit, 16-bit, 24-bit integer and 32-bit float WAV files.
/// Stereo files are loaded as interleaved stereo; mono files as mono.
pub fn load_wav<P: AsRef<Path>>(path: P) -> Result<Sample> {
    let data = std::fs::read(path.as_ref())
        .map_err(|e| NidhiError::ImportError(format!("failed to read WAV file: {e}")))?;

    let (info, samples) = shravan::wav::decode(&data)
        .map_err(|e| NidhiError::ImportError(format!("failed to decode WAV: {e}")))?;

    let channels = info.channels as u32;
    let sample_rate = info.sample_rate;

    let name = path
        .as_ref()
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("");

    let sample = if channels == 1 {
        Sample::from_mono(samples, sample_rate)
    } else {
        Sample::from_stereo(samples, sample_rate)
    };

    Ok(sample.with_name(name))
}

/// Load a WAV file from a byte slice (in-memory).
pub fn load_wav_from_memory(data: &[u8]) -> Result<Sample> {
    let (info, samples) = shravan::wav::decode(data)
        .map_err(|e| NidhiError::ImportError(format!("failed to parse WAV: {e}")))?;

    let channels = info.channels as u32;
    let sample_rate = info.sample_rate;

    let sample = if channels == 1 {
        Sample::from_mono(samples, sample_rate)
    } else {
        Sample::from_stereo(samples, sample_rate)
    };

    Ok(sample)
}

/// Stream a WAV file in chunks for large instruments.
///
/// Useful for instruments too large to fit entirely in memory.
pub struct StreamingWavReader {
    decoder: shravan::stream::WavStreamDecoder,
    info: Option<shravan::format::FormatInfo>,
    pending_samples: Vec<f32>,
    channels: u32,
    sample_rate: u32,
    total_frames: usize,
    frames_read: usize,
    finished: bool,
    file_data: Vec<u8>,
    file_offset: usize,
}

impl StreamingWavReader {
    /// Open a WAV file for streaming.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file_data = std::fs::read(path.as_ref()).map_err(|e| {
            NidhiError::ImportError(format!("failed to open WAV for streaming: {e}"))
        })?;

        // Do a full decode just for the header info (total_frames, channels, sample_rate).
        // The streaming decoder will process chunks incrementally.
        let (info, _) = shravan::wav::decode(&file_data)
            .map_err(|e| NidhiError::ImportError(format!("failed to read WAV header: {e}")))?;

        let channels = info.channels as u32;
        let sample_rate = info.sample_rate;
        let total_frames = info.total_samples as usize;

        Ok(Self {
            decoder: shravan::stream::WavStreamDecoder::new(),
            info: Some(info),
            pending_samples: Vec::new(),
            channels,
            sample_rate,
            total_frames,
            frames_read: 0,
            finished: false,
            file_data,
            file_offset: 0,
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
        use shravan::stream::{StreamDecoder, StreamEvent};

        let remaining = self.total_frames - self.frames_read;
        let frames_to_read = chunk_frames.min(remaining);
        if frames_to_read == 0 {
            return Ok(Vec::new());
        }

        let samples_needed = frames_to_read * self.channels as usize;

        // Feed data until we have enough samples or run out
        while self.pending_samples.len() < samples_needed && !self.finished {
            if self.file_offset < self.file_data.len() {
                let end = (self.file_offset + 4096).min(self.file_data.len());
                let chunk = &self.file_data[self.file_offset..end];
                self.file_offset = end;

                let events = self
                    .decoder
                    .feed(chunk)
                    .map_err(|e| NidhiError::ImportError(format!("WAV stream error: {e}")))?;

                for event in events {
                    match event {
                        StreamEvent::Header(info) => {
                            self.info = Some(info);
                        }
                        StreamEvent::Samples(s) => {
                            self.pending_samples.extend_from_slice(&s);
                        }
                        StreamEvent::End => {
                            self.finished = true;
                        }
                        _ => {}
                    }
                }
            } else {
                let events = self
                    .decoder
                    .flush()
                    .map_err(|e| NidhiError::ImportError(format!("WAV stream flush error: {e}")))?;

                for event in events {
                    if let StreamEvent::Samples(s) = event {
                        self.pending_samples.extend_from_slice(&s);
                    }
                }
                self.finished = true;
            }
        }

        let take = samples_needed.min(self.pending_samples.len());
        let data: Vec<f32> = self.pending_samples.drain(..take).collect();
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
        shravan::wav::encode(samples, sample_rate, channels, shravan::pcm::PcmFormat::F32)
            .expect("WAV encoding failed")
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

//! Sample capture — record audio input into [`Sample`] with auto-processing.
//!
//! Provides [`SampleRecorder`] for accumulating audio buffers into a sample,
//! plus utilities for trimming silence, normalizing, and detecting loop points.

use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

use crate::sample::Sample;

/// Accumulates audio input buffers into a [`Sample`].
///
/// # Example
///
/// ```rust
/// use nidhi::capture::SampleRecorder;
///
/// let mut recorder = SampleRecorder::new(44100, 1);
/// recorder.write(&[0.1, 0.2, 0.3]);
/// recorder.write(&[0.4, 0.5]);
/// let sample = recorder.finish();
/// assert_eq!(sample.frames(), 5);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SampleRecorder {
    buffer: Vec<f32>,
    sample_rate: u32,
    channels: u32,
}

impl SampleRecorder {
    /// Create a new recorder.
    ///
    /// `channels` must be 1 (mono) or 2 (stereo interleaved).
    #[must_use]
    pub fn new(sample_rate: u32, channels: u32) -> Self {
        Self {
            buffer: Vec::new(),
            sample_rate,
            channels: channels.clamp(1, 2),
        }
    }

    /// Append audio data. For stereo, data must be interleaved (L, R, L, R, ...).
    pub fn write(&mut self, data: &[f32]) {
        self.buffer.extend_from_slice(data);
    }

    /// Current length in frames.
    pub fn frames(&self) -> usize {
        self.buffer.len() / self.channels as usize
    }

    /// Clear the buffer without producing a sample.
    pub fn clear(&mut self) {
        self.buffer.clear();
    }

    /// Finish recording and produce a [`Sample`].
    pub fn finish(self) -> Sample {
        if self.channels == 2 {
            Sample::from_stereo(self.buffer, self.sample_rate)
        } else {
            Sample::from_mono(self.buffer, self.sample_rate)
        }
    }

    /// Finish recording, auto-trim silence, and normalize.
    pub fn finish_processed(self, silence_threshold: f32) -> Sample {
        let mut sample = self.finish();
        trim_silence(&mut sample, silence_threshold);
        normalize_peak(&mut sample);
        sample
    }
}

/// Trim leading and trailing silence from a sample.
///
/// `threshold` is the absolute amplitude below which a frame is considered silent.
pub fn trim_silence(sample: &mut Sample, threshold: f32) {
    let threshold = threshold.max(0.0);
    let ch = sample.channels() as usize;
    let frames = sample.frames();

    if frames == 0 {
        return;
    }

    // Find first non-silent frame
    let start = (0..frames)
        .find(|&f| (0..ch).any(|c| sample.data[f * ch + c].abs() > threshold))
        .unwrap_or(frames);

    // Find last non-silent frame
    let end = (0..frames)
        .rev()
        .find(|&f| (0..ch).any(|c| sample.data[f * ch + c].abs() > threshold))
        .map(|f| f + 1)
        .unwrap_or(0);

    if start >= end {
        sample.data.clear();
        sample.frames = 0;
        return;
    }

    let sample_start = start * ch;
    let sample_end = end * ch;
    sample.data = sample.data[sample_start..sample_end].to_vec();
    sample.frames = end - start;
}

/// Normalize a sample to peak amplitude (0 dB).
///
/// Scales all samples so the loudest peak reaches ±1.0.
pub fn normalize_peak(sample: &mut Sample) {
    let peak = sample.data.iter().fold(0.0f32, |max, &s| max.max(s.abs()));

    if peak > 1e-10 {
        let gain = 1.0 / peak;
        for s in &mut sample.data {
            *s *= gain;
        }
    }
}

/// Normalize a sample to a target RMS level.
///
/// `target_rms` is typically 0.1–0.3 for musical content.
pub fn normalize_rms(sample: &mut Sample, target_rms: f32) {
    if sample.data.is_empty() {
        return;
    }

    let rms = (sample.data.iter().map(|&s| s * s).sum::<f32>() / sample.data.len() as f32).sqrt();

    if rms > 1e-10 {
        let gain = target_rms / rms;
        for s in &mut sample.data {
            *s *= gain;
        }
    }
}

/// Find candidate loop points in a sample by searching for zero crossings
/// with similar waveform shape.
///
/// Returns a list of `(start, end)` frame pairs sorted by quality (best first).
/// `min_loop_frames` is the minimum loop length.
#[must_use]
pub fn detect_loop_points(sample: &Sample, min_loop_frames: usize) -> Vec<(usize, usize)> {
    let data = sample.data();
    let frames = sample.frames();
    let ch = sample.channels() as usize;

    if frames < min_loop_frames * 2 {
        return Vec::new();
    }

    // Work with mono (average channels)
    let mono: Vec<f32> = (0..frames)
        .map(|f| {
            let mut sum = 0.0f32;
            for c in 0..ch {
                sum += data[f * ch + c];
            }
            sum / ch as f32
        })
        .collect();

    // Find zero crossings (positive-going)
    let mut crossings = Vec::new();
    for i in 1..mono.len() {
        if mono[i - 1] <= 0.0 && mono[i] > 0.0 {
            crossings.push(i);
        }
    }

    if crossings.len() < 2 {
        return Vec::new();
    }

    // Score pairs of zero crossings by waveform similarity
    let compare_len = 64.min(frames / 4);
    let mut candidates: Vec<(usize, usize, f64)> = Vec::new();

    for (i, &start) in crossings.iter().enumerate() {
        for &end in &crossings[i + 1..] {
            if end - start < min_loop_frames {
                continue;
            }
            if start + compare_len > frames || end + compare_len > frames {
                continue;
            }

            // Cross-correlation at the boundary
            let mut dot = 0.0f64;
            let mut norm_a = 0.0f64;
            let mut norm_b = 0.0f64;
            for k in 0..compare_len {
                let a = mono[start + k] as f64;
                let b = mono[end + k] as f64;
                dot += a * b;
                norm_a += a * a;
                norm_b += b * b;
            }

            let denom = (norm_a * norm_b).sqrt();
            let score = if denom > 1e-10 { dot / denom } else { 0.0 };

            candidates.push((start, end, score));
        }

        // Limit search to keep it fast
        if candidates.len() > 100 {
            break;
        }
    }

    // Sort by score descending (best match first)
    candidates.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(core::cmp::Ordering::Equal));

    candidates
        .into_iter()
        .take(10)
        .map(|(s, e, _)| (s, e))
        .collect()
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;

    #[test]
    fn recorder_basic() {
        let mut rec = SampleRecorder::new(44100, 1);
        rec.write(&[0.1, 0.2, 0.3]);
        rec.write(&[0.4, 0.5]);
        assert_eq!(rec.frames(), 5);
        let sample = rec.finish();
        assert_eq!(sample.frames(), 5);
        assert_eq!(sample.channels(), 1);
    }

    #[test]
    fn recorder_stereo() {
        let mut rec = SampleRecorder::new(44100, 2);
        rec.write(&[0.1, 0.2, 0.3, 0.4]); // 2 stereo frames
        assert_eq!(rec.frames(), 2);
        let sample = rec.finish();
        assert_eq!(sample.frames(), 2);
        assert_eq!(sample.channels(), 2);
    }

    #[test]
    fn trim_silence_removes_padding() {
        let data = vec![0.0, 0.0, 0.0, 0.5, 0.8, 0.3, 0.0, 0.0];
        let mut s = Sample::from_mono(data, 44100);
        trim_silence(&mut s, 0.01);
        assert_eq!(s.frames(), 3); // 0.5, 0.8, 0.3
        assert!((s.data()[0] - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn trim_silence_all_silent() {
        let mut s = Sample::from_mono(vec![0.0; 100], 44100);
        trim_silence(&mut s, 0.01);
        assert_eq!(s.frames(), 0);
    }

    #[test]
    fn normalize_peak_scales_to_one() {
        let mut s = Sample::from_mono(vec![0.0, 0.25, -0.5, 0.1], 44100);
        normalize_peak(&mut s);
        let peak = s.data().iter().fold(0.0f32, |m, &v| m.max(v.abs()));
        assert!((peak - 1.0).abs() < 0.001, "peak should be 1.0, got {peak}");
    }

    #[test]
    fn normalize_rms_adjusts_level() {
        let mut s = Sample::from_mono(vec![0.5; 100], 44100);
        normalize_rms(&mut s, 0.2);
        let rms = (s.data().iter().map(|&v| v * v).sum::<f32>() / s.data().len() as f32).sqrt();
        assert!((rms - 0.2).abs() < 0.01, "rms should be ~0.2, got {rms}");
    }

    #[test]
    fn detect_loop_points_returns_candidates() {
        // Sine wave — should find good loop points at zero crossings
        let data: Vec<f32> = (0..4410)
            .map(|i| (2.0 * std::f32::consts::PI * 100.0 * i as f32 / 44100.0).sin())
            .collect();
        let s = Sample::from_mono(data, 44100);
        let loops = detect_loop_points(&s, 100);
        assert!(
            !loops.is_empty(),
            "should find loop candidates in sine wave"
        );
        let (start, end) = loops[0];
        assert!(end > start);
        assert!(end - start >= 100);
    }

    #[test]
    fn detect_loop_points_short_sample() {
        let s = Sample::from_mono(vec![0.0; 10], 44100);
        let loops = detect_loop_points(&s, 100);
        assert!(loops.is_empty());
    }

    #[test]
    fn finish_processed_trims_and_normalizes() {
        let mut rec = SampleRecorder::new(44100, 1);
        // Silence + signal + silence
        rec.write(&[0.0; 100]);
        rec.write(&[0.25; 200]);
        rec.write(&[0.0; 100]);
        let sample = rec.finish_processed(0.01);
        assert_eq!(sample.frames(), 200);
        let peak = sample.data().iter().fold(0.0f32, |m, &v| m.max(v.abs()));
        assert!(
            (peak - 1.0).abs() < 0.01,
            "should be normalized, peak={peak}"
        );
    }
}

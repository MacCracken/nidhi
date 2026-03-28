//! Time-stretching — change duration without affecting pitch.
//!
//! Algorithms:
//! - **OLA (Overlap-Add)**: Simple, low-quality, good for speech
//! - **WSOLA (Waveform Similarity OLA)**: Better quality, finds optimal splice points
//! - **Phase vocoder**: Highest quality, FFT-based (not yet implemented, falls back to WSOLA)

use alloc::vec;
use alloc::vec::Vec;
use core::f64::consts::PI;
use serde::{Deserialize, Serialize};

/// Time-stretch quality/algorithm selection.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum StretchMode {
    /// Overlap-Add — fast, simple, best for speech/mono.
    Ola,
    /// Waveform Similarity OLA — better quality, auto splice points.
    #[default]
    Wsola,
    /// Phase vocoder — highest quality, FFT-based (falls back to WSOLA for now).
    PhaseVocoder,
}

/// WSOLA time-stretcher — changes audio duration without affecting pitch.
///
/// Uses the Waveform Similarity Overlap-Add algorithm to find optimal splice
/// points via cross-correlation, producing higher quality than plain OLA.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct TimeStretcher {
    /// Input sample data.
    input: Vec<f32>,
    /// Sample rate.
    sample_rate: f32,
    /// Analysis frame size in samples.
    frame_size: usize,
    /// Overlap factor (0.5 = 50% overlap).
    overlap: f32,
}

impl TimeStretcher {
    /// Create a new time-stretcher with default parameters.
    ///
    /// Defaults: `frame_size = 1024`, `overlap = 0.5`.
    pub fn new(input: Vec<f32>, sample_rate: f32) -> Self {
        Self {
            input,
            sample_rate,
            frame_size: 1024,
            overlap: 0.5,
        }
    }

    /// Set the analysis frame size.
    pub fn with_frame_size(mut self, size: usize) -> Self {
        self.frame_size = size;
        self
    }

    /// Sample rate accessor.
    #[inline]
    #[must_use]
    pub fn sample_rate(&self) -> f32 {
        self.sample_rate
    }

    /// Frame size accessor.
    #[inline]
    pub fn frame_size(&self) -> usize {
        self.frame_size
    }

    /// Overlap factor accessor.
    #[inline]
    pub fn overlap(&self) -> f32 {
        self.overlap
    }

    /// Input data accessor.
    #[inline]
    pub fn input(&self) -> &[f32] {
        &self.input
    }

    /// Time-stretch using WSOLA.
    ///
    /// - `ratio > 1.0` = slower (longer output)
    /// - `ratio < 1.0` = faster (shorter output)
    /// - `ratio == 1.0` = identity (approximate)
    #[must_use]
    pub fn stretch(&self, ratio: f64) -> Vec<f32> {
        if self.input.is_empty() || self.frame_size == 0 {
            return Vec::new();
        }

        let input_len = self.input.len();
        if input_len < self.frame_size {
            // Input shorter than one frame — return a copy scaled by ratio.
            return self.stretch_short(ratio);
        }

        let syn_hop = ((self.frame_size as f64) * (1.0 - f64::from(self.overlap))) as usize;
        if syn_hop == 0 {
            return self.input.clone();
        }
        let ana_hop = syn_hop as f64 / ratio;
        let tolerance = self.frame_size / 4;

        let window = hann_window(self.frame_size);

        // Estimate output length.
        let out_len = (input_len as f64 * ratio).ceil() as usize + self.frame_size;
        let mut output = vec![0.0f32; out_len];
        let mut window_sum = vec![0.0f32; out_len];

        let mut prev_frame: Option<Vec<f32>> = None;
        let mut frame_idx: usize = 0;

        loop {
            let out_pos = frame_idx * syn_hop;
            if out_pos + self.frame_size > out_len {
                break;
            }

            let expected_input = (frame_idx as f64 * ana_hop) as isize;

            // Determine optimal input position via cross-correlation search.
            let optimal_input = if let Some(ref prev) = prev_frame {
                let search_start = (expected_input - tolerance as isize).max(0) as usize;
                let search_end = ((expected_input + tolerance as isize) as usize)
                    .min(input_len.saturating_sub(self.frame_size));

                if search_start > search_end {
                    expected_input.max(0) as usize
                } else {
                    let mut best_pos = search_start;
                    let mut best_corr = f64::NEG_INFINITY;

                    for pos in search_start..=search_end {
                        let corr = dot_correlation(
                            prev,
                            &self.input[pos..],
                            self.frame_size.min(self.input.len() - pos),
                        );
                        if corr > best_corr {
                            best_corr = corr;
                            best_pos = pos;
                        }
                    }
                    best_pos
                }
            } else {
                expected_input.max(0) as usize
            };

            if optimal_input + self.frame_size > input_len {
                break;
            }

            // Extract frame, apply window, overlap-add.
            let frame_slice = &self.input[optimal_input..optimal_input + self.frame_size];
            let mut windowed = Vec::with_capacity(self.frame_size);
            for i in 0..self.frame_size {
                windowed.push(frame_slice[i] * window[i]);
            }

            for i in 0..self.frame_size {
                let oi = out_pos + i;
                if oi < out_len {
                    output[oi] += windowed[i];
                    window_sum[oi] += window[i] * window[i];
                }
            }

            prev_frame = Some(frame_slice.to_vec());
            frame_idx += 1;
        }

        // Normalize by window overlap sum.
        normalize_by_window_sum(&mut output, &window_sum);

        // Trim to expected length.
        let target_len = (input_len as f64 * ratio).round() as usize;
        output.truncate(target_len.min(output.len()));
        output
    }

    /// Time-stretch using plain OLA (no cross-correlation search).
    ///
    /// Faster than WSOLA but lower quality — may produce audible artifacts at
    /// frame boundaries.
    #[must_use]
    pub fn stretch_ola(&self, ratio: f64) -> Vec<f32> {
        if self.input.is_empty() || self.frame_size == 0 {
            return Vec::new();
        }

        let input_len = self.input.len();
        if input_len < self.frame_size {
            return self.stretch_short(ratio);
        }

        let syn_hop = ((self.frame_size as f64) * (1.0 - f64::from(self.overlap))) as usize;
        if syn_hop == 0 {
            return self.input.clone();
        }
        let ana_hop = syn_hop as f64 / ratio;

        let window = hann_window(self.frame_size);

        let out_len = (input_len as f64 * ratio).ceil() as usize + self.frame_size;
        let mut output = vec![0.0f32; out_len];
        let mut window_sum = vec![0.0f32; out_len];

        let mut frame_idx: usize = 0;

        loop {
            let out_pos = frame_idx * syn_hop;
            if out_pos + self.frame_size > out_len {
                break;
            }

            let input_pos = (frame_idx as f64 * ana_hop) as usize;
            if input_pos + self.frame_size > input_len {
                break;
            }

            let frame_slice = &self.input[input_pos..input_pos + self.frame_size];

            for i in 0..self.frame_size {
                let oi = out_pos + i;
                if oi < out_len {
                    output[oi] += frame_slice[i] * window[i];
                    window_sum[oi] += window[i] * window[i];
                }
            }

            frame_idx += 1;
        }

        normalize_by_window_sum(&mut output, &window_sum);

        let target_len = (input_len as f64 * ratio).round() as usize;
        output.truncate(target_len.min(output.len()));
        output
    }

    /// Dispatch to the appropriate stretch algorithm based on mode.
    #[must_use]
    pub fn stretch_with_mode(&self, ratio: f64, mode: StretchMode) -> Vec<f32> {
        match mode {
            StretchMode::Ola => self.stretch_ola(ratio),
            StretchMode::Wsola | StretchMode::PhaseVocoder => self.stretch(ratio),
        }
    }

    /// Handle inputs shorter than one frame by simple resampling.
    fn stretch_short(&self, ratio: f64) -> Vec<f32> {
        let target_len = (self.input.len() as f64 * ratio).round() as usize;
        if target_len == 0 {
            return Vec::new();
        }
        let mut output = Vec::with_capacity(target_len);
        for i in 0..target_len {
            let src = i as f64 / ratio;
            let idx = src.floor() as usize;
            let frac = (src - idx as f64) as f32;
            let a = self.input.get(idx).copied().unwrap_or(0.0);
            let b = self.input.get(idx + 1).copied().unwrap_or(a);
            output.push(a + (b - a) * frac);
        }
        output
    }
}

/// Compute the dot-product correlation between `a` and `b` over `len` samples.
#[inline]
fn dot_correlation(a: &[f32], b: &[f32], len: usize) -> f64 {
    let n = len.min(a.len()).min(b.len());
    let mut sum: f64 = 0.0;
    for i in 0..n {
        sum += f64::from(a[i]) * f64::from(b[i]);
    }
    sum
}

/// Find the lag (offset) that maximizes the dot product between `a` and `b[lag..]`.
///
/// Searches lags in `0..max_lag` and returns the signed offset with the highest
/// correlation. Kept simple — no FFT, suitable for typical search window sizes.
#[inline]
#[must_use]
pub fn cross_correlate(a: &[f32], b: &[f32], max_lag: usize) -> isize {
    if a.is_empty() || b.is_empty() {
        return 0;
    }

    let mut best_lag: isize = 0;
    let mut best_corr = f64::NEG_INFINITY;
    let max_neg = max_lag.min(a.len().saturating_sub(1));

    // Negative lags: shift a forward.
    for lag in 1..=max_neg {
        let overlap_len = a.len().saturating_sub(lag).min(b.len());
        if overlap_len == 0 {
            continue;
        }
        let mut sum: f64 = 0.0;
        for i in 0..overlap_len {
            sum += f64::from(a[lag + i]) * f64::from(b[i]);
        }
        if sum > best_corr {
            best_corr = sum;
            best_lag = -(lag as isize);
        }
    }

    // Non-negative lags: shift b forward.
    let max_pos = max_lag.min(b.len().saturating_sub(1));
    for lag in 0..=max_pos {
        let overlap_len = a.len().min(b.len().saturating_sub(lag));
        if overlap_len == 0 {
            continue;
        }
        let mut sum: f64 = 0.0;
        for i in 0..overlap_len {
            sum += f64::from(a[i]) * f64::from(b[lag + i]);
        }
        if sum > best_corr {
            best_corr = sum;
            best_lag = lag as isize;
        }
    }

    best_lag
}

/// Compute a Hann window of the given size.
#[must_use]
fn hann_window(size: usize) -> Vec<f32> {
    let mut window = Vec::with_capacity(size);
    if size == 0 {
        return window;
    }
    let denom = (size - 1).max(1) as f64;
    for i in 0..size {
        let w = 0.5 * (1.0 - (2.0 * PI * i as f64 / denom).cos());
        window.push(w as f32);
    }
    window
}

/// Normalize output by the accumulated window energy, avoiding division by zero.
fn normalize_by_window_sum(output: &mut [f32], window_sum: &[f32]) {
    let threshold = 1e-6;
    for (sample, &ws) in output.iter_mut().zip(window_sum.iter()) {
        if ws > threshold {
            *sample /= ws;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    /// Helper: generate a sine wave.
    fn sine_wave(freq: f32, sample_rate: f32, duration_secs: f32) -> Vec<f32> {
        let len = (sample_rate * duration_secs) as usize;
        let mut buf = Vec::with_capacity(len);
        for i in 0..len {
            let t = i as f32 / sample_rate;
            buf.push((2.0 * core::f32::consts::PI * freq * t).sin());
        }
        buf
    }

    #[test]
    fn stretch_ratio_1_preserves_length() {
        let input = sine_wave(440.0, 44100.0, 0.5);
        let original_len = input.len();
        let ts = TimeStretcher::new(input, 44100.0);
        let output = ts.stretch(1.0);

        // Length should be approximately equal.
        let diff = (output.len() as f64 - original_len as f64).abs();
        assert!(
            diff < (original_len as f64 * 0.02),
            "Expected length ~{original_len}, got {}",
            output.len()
        );
    }

    #[test]
    fn stretch_ratio_2_doubles_duration() {
        let input = sine_wave(440.0, 44100.0, 0.5);
        let original_len = input.len();
        let ts = TimeStretcher::new(input, 44100.0);
        let output = ts.stretch(2.0);

        let expected = original_len * 2;
        let diff = (output.len() as f64 - expected as f64).abs();
        assert!(
            diff < (expected as f64 * 0.05),
            "Expected length ~{expected}, got {}",
            output.len()
        );
    }

    #[test]
    fn stretch_ratio_half_halves_duration() {
        let input = sine_wave(440.0, 44100.0, 0.5);
        let original_len = input.len();
        let ts = TimeStretcher::new(input, 44100.0);
        let output = ts.stretch(0.5);

        let expected = original_len / 2;
        let diff = (output.len() as f64 - expected as f64).abs();
        assert!(
            diff < (expected as f64 * 0.05),
            "Expected length ~{expected}, got {}",
            output.len()
        );
    }

    #[test]
    fn ola_produces_finite_output() {
        let input = sine_wave(440.0, 44100.0, 0.5);
        let ts = TimeStretcher::new(input, 44100.0);
        let output = ts.stretch_ola(1.5);
        assert!(output.iter().all(|s| s.is_finite()));
        assert!(!output.is_empty());
    }

    #[test]
    fn wsola_produces_finite_output() {
        let input = sine_wave(440.0, 44100.0, 0.5);
        let ts = TimeStretcher::new(input, 44100.0);
        let output = ts.stretch(1.5);
        assert!(output.iter().all(|s| s.is_finite()));
        assert!(!output.is_empty());
    }

    #[test]
    fn empty_input_returns_empty() {
        let ts = TimeStretcher::new(vec![], 44100.0);
        assert!(ts.stretch(2.0).is_empty());
        assert!(ts.stretch_ola(2.0).is_empty());
    }

    #[test]
    fn very_short_input_handled() {
        let ts = TimeStretcher::new(vec![0.5, 0.3, 0.1], 44100.0);
        let output = ts.stretch(2.0);
        assert!(!output.is_empty());
        assert!(output.iter().all(|s| s.is_finite()));

        let output_ola = ts.stretch_ola(2.0);
        assert!(!output_ola.is_empty());
        assert!(output_ola.iter().all(|s| s.is_finite()));
    }

    #[test]
    fn stretch_with_mode_dispatches() {
        let input = sine_wave(440.0, 44100.0, 0.2);
        let ts = TimeStretcher::new(input, 44100.0);

        let ola = ts.stretch_with_mode(1.5, StretchMode::Ola);
        let wsola = ts.stretch_with_mode(1.5, StretchMode::Wsola);
        let pv = ts.stretch_with_mode(1.5, StretchMode::PhaseVocoder);

        assert!(!ola.is_empty());
        assert!(!wsola.is_empty());
        // PhaseVocoder falls back to WSOLA, so should match.
        assert_eq!(wsola.len(), pv.len());
    }

    #[test]
    fn cross_correlate_finds_zero_lag_for_identical() {
        let a = sine_wave(440.0, 44100.0, 0.01);
        let lag = cross_correlate(&a, &a, 64);
        assert_eq!(lag, 0);
    }

    #[test]
    fn cross_correlate_empty_returns_zero() {
        assert_eq!(cross_correlate(&[], &[1.0, 2.0], 10), 0);
        assert_eq!(cross_correlate(&[1.0], &[], 10), 0);
    }

    #[test]
    fn hann_window_shape() {
        let w = hann_window(256);
        assert_eq!(w.len(), 256);
        // Endpoints should be near zero.
        assert!(w[0].abs() < 1e-6);
        assert!(w[255].abs() < 1e-6);
        // Middle should be near 1.0.
        assert!((w[127] - 1.0).abs() < 0.02);
    }

    #[test]
    fn with_frame_size_builder() {
        let ts = TimeStretcher::new(vec![0.0; 4096], 44100.0).with_frame_size(512);
        assert_eq!(ts.frame_size(), 512);
    }
}

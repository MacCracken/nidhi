//! Sample storage — loaded audio waveforms and a sample bank.

use alloc::string::String;
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

/// Unique identifier for a sample in a [`SampleBank`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[must_use]
pub struct SampleId(pub u32);

/// A loaded audio sample — mono or stereo f32 data at a known sample rate.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Sample {
    /// Sample data (interleaved if stereo).
    pub(crate) data: Vec<f32>,
    /// Number of channels (1 = mono, 2 = stereo).
    pub(crate) channels: u32,
    /// Sample rate in Hz.
    pub(crate) sample_rate: u32,
    /// Number of frames (samples per channel).
    pub(crate) frames: usize,
    /// Optional name/label.
    pub(crate) name: String,
    /// REX-style slice points (frame indices).
    pub(crate) slices: Vec<usize>,
}

impl Sample {
    /// Create a mono sample from raw f32 data.
    pub fn from_mono(data: Vec<f32>, sample_rate: u32) -> Self {
        let frames = data.len();
        Self {
            data,
            channels: 1,
            sample_rate,
            frames,
            name: String::new(),
            slices: Vec::new(),
        }
    }

    /// Create a stereo sample from interleaved f32 data.
    pub fn from_stereo(data: Vec<f32>, sample_rate: u32) -> Self {
        let frames = data.len() / 2;
        Self {
            data,
            channels: 2,
            sample_rate,
            frames,
            name: String::new(),
            slices: Vec::new(),
        }
    }

    /// Set the sample name.
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// Set slice points manually.
    pub fn with_slices(mut self, slices: Vec<usize>) -> Self {
        self.slices = slices;
        self
    }

    /// Get slice points.
    #[inline]
    #[must_use]
    pub fn slices(&self) -> &[usize] {
        &self.slices
    }

    /// Auto-detect slice points via onset detection (energy-based transient detection).
    ///
    /// `threshold` controls sensitivity (0.0–1.0, lower = more slices).
    /// `min_slice_frames` is the minimum distance between slices.
    pub fn detect_onsets(&mut self, threshold: f32, min_slice_frames: usize) {
        self.slices.clear();
        if self.frames < 2 {
            return;
        }

        let window = 512.min(self.frames / 2).max(1);
        let hop = window / 2;
        let threshold = threshold.clamp(0.01, 1.0);

        // Compute energy per window
        let mut energies = Vec::new();
        let mut pos = 0;
        while pos + window <= self.frames {
            let mut energy = 0.0f32;
            for i in pos..pos + window {
                let s = if self.channels == 1 {
                    self.data[i]
                } else {
                    (self.data[i * 2] + self.data[i * 2 + 1]) * 0.5
                };
                energy += s * s;
            }
            energies.push((pos, energy / window as f32));
            pos += hop;
        }

        if energies.len() < 2 {
            return;
        }

        // Find peak energy for normalization
        let max_energy = energies.iter().map(|(_, e)| *e).fold(0.0f32, f32::max);

        if max_energy < 1e-10 {
            return;
        }

        // Detect onsets: significant energy increase between consecutive windows
        let mut last_slice = 0usize;
        for i in 1..energies.len() {
            let (frame, energy) = energies[i];
            let prev_energy = energies[i - 1].1;
            let diff = (energy - prev_energy) / max_energy;

            if diff > threshold && frame.saturating_sub(last_slice) >= min_slice_frames {
                self.slices.push(frame);
                last_slice = frame;
            }
        }
    }

    /// Sample data.
    #[inline]
    pub fn data(&self) -> &[f32] {
        &self.data
    }

    /// Number of channels.
    #[inline]
    pub fn channels(&self) -> u32 {
        self.channels
    }

    /// Sample rate in Hz.
    #[inline]
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Number of frames.
    #[inline]
    pub fn frames(&self) -> usize {
        self.frames
    }

    /// Sample name.
    #[inline]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Read a mono frame at the given index, clamping to bounds.
    ///
    /// For stereo samples, averages L+R.
    #[inline]
    fn read_mono_frame(&self, idx: isize) -> f32 {
        if idx < 0 || idx as usize >= self.frames {
            return 0.0;
        }
        let i = idx as usize;
        if self.channels == 1 {
            self.data[i]
        } else {
            let ch = self.channels as usize;
            (self.data[i * ch] + self.data[i * ch + 1]) * 0.5
        }
    }

    /// Read a stereo frame at the given index, clamping to bounds.
    ///
    /// For mono samples, returns `(sample, sample)`.
    #[inline]
    fn read_stereo_frame(&self, idx: isize) -> (f32, f32) {
        if idx < 0 || idx as usize >= self.frames {
            return (0.0, 0.0);
        }
        let i = idx as usize;
        if self.channels == 1 {
            let v = self.data[i];
            (v, v)
        } else {
            let ch = self.channels as usize;
            (self.data[i * ch], self.data[i * ch + 1])
        }
    }

    /// Cubic Hermite (Catmull-Rom) interpolation between four points.
    ///
    /// `y0..y3` are sample values at positions `idx-1, idx, idx+1, idx+2`.
    /// `t` is the fractional position between `y1` and `y2` (0.0–1.0).
    #[inline]
    #[must_use]
    pub fn cubic_hermite(y0: f32, y1: f32, y2: f32, y3: f32, t: f32) -> f32 {
        let a = -0.5 * y0 + 1.5 * y1 - 1.5 * y2 + 0.5 * y3;
        let b = y0 - 2.5 * y1 + 2.0 * y2 - 0.5 * y3;
        let c = -0.5 * y0 + 0.5 * y2;
        let d = y1;
        ((a * t + b) * t + c) * t + d
    }

    /// Read a mono value using cubic Hermite interpolation.
    ///
    /// Uses four points around the position for smooth interpolation.
    /// For stereo, averages L+R.
    #[inline]
    #[must_use]
    pub fn read_cubic(&self, position: f64) -> f32 {
        if self.frames == 0 {
            return 0.0;
        }
        let idx = position.floor() as isize;
        let frac = (position - idx as f64) as f32;

        let y0 = self.read_mono_frame(idx - 1);
        let y1 = self.read_mono_frame(idx);
        let y2 = self.read_mono_frame(idx + 1);
        let y3 = self.read_mono_frame(idx + 2);

        Self::cubic_hermite(y0, y1, y2, y3, frac)
    }

    /// Read a frame at the given position with cubic Hermite interpolation.
    ///
    /// Returns mono sample value. For stereo, averages L+R.
    #[inline]
    #[must_use]
    pub fn read_interpolated(&self, position: f64) -> f32 {
        self.read_cubic(position)
    }

    /// Read a stereo frame with cubic Hermite interpolation.
    ///
    /// Returns `(left, right)`. For mono samples, both channels are identical.
    #[inline]
    #[must_use]
    pub fn read_stereo_interpolated(&self, position: f64) -> (f32, f32) {
        if self.frames == 0 {
            return (0.0, 0.0);
        }
        let idx = position.floor() as isize;
        let frac = (position - idx as f64) as f32;

        let (l0, r0) = self.read_stereo_frame(idx - 1);
        let (l1, r1) = self.read_stereo_frame(idx);
        let (l2, r2) = self.read_stereo_frame(idx + 1);
        let (l3, r3) = self.read_stereo_frame(idx + 2);

        let left = Self::cubic_hermite(l0, l1, l2, l3, frac);
        let right = Self::cubic_hermite(r0, r1, r2, r3, frac);
        (left, right)
    }
}

/// A bank of loaded samples, addressable by [`SampleId`].
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[must_use]
pub struct SampleBank {
    samples: Vec<Sample>,
}

impl SampleBank {
    /// Create an empty sample bank.
    pub fn new() -> Self {
        Self {
            samples: Vec::new(),
        }
    }

    /// Add a sample to the bank. Returns its [`SampleId`].
    pub fn add(&mut self, sample: Sample) -> SampleId {
        let id = SampleId(self.samples.len() as u32);
        self.samples.push(sample);
        id
    }

    /// Get a sample by ID.
    #[inline]
    pub fn get(&self, id: SampleId) -> Option<&Sample> {
        self.samples.get(id.0 as usize)
    }

    /// Number of samples in the bank.
    #[inline]
    pub fn len(&self) -> usize {
        self.samples.len()
    }

    /// Whether the bank is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_from_mono() {
        let s = Sample::from_mono(vec![0.5; 100], 44100).with_name("test");
        assert_eq!(s.channels(), 1);
        assert_eq!(s.frames(), 100);
        assert_eq!(s.name(), "test");
    }

    #[test]
    fn sample_interpolation() {
        // Cubic Hermite with 4 points: for a linear ramp, should interpolate exactly
        let s = Sample::from_mono(vec![0.0, 0.25, 0.5, 0.75, 1.0], 44100);
        assert!((s.read_interpolated(2.0) - 0.5).abs() < 0.01);
        assert!((s.read_interpolated(2.5) - 0.625).abs() < 0.01);
        // Peak sample reads exactly
        let s2 = Sample::from_mono(vec![0.0, 0.0, 1.0, 0.0, 0.0], 44100);
        assert!((s2.read_interpolated(2.0) - 1.0).abs() < 0.01);
    }

    #[test]
    fn cubic_hermite_smooth() {
        // A ramp: 0, 1, 2, 3 — cubic should give exact linear result
        let v = Sample::cubic_hermite(0.0, 1.0, 2.0, 3.0, 0.5);
        assert!((v - 1.5).abs() < 0.01);
    }

    #[test]
    fn read_cubic_basic() {
        let s = Sample::from_mono(vec![0.0, 0.0, 1.0, 0.0, 0.0], 44100);
        let v = s.read_cubic(2.0);
        assert!((v - 1.0).abs() < 0.01);
    }

    #[test]
    fn read_stereo_interpolated_basic() {
        // Stereo: L=1.0, R=0.0 at every frame
        let data = vec![1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0];
        let s = Sample::from_stereo(data, 44100);
        let (l, r) = s.read_stereo_interpolated(1.5);
        assert!((l - 1.0).abs() < 0.01);
        assert!(r.abs() < 0.01);
    }

    #[test]
    fn read_stereo_interpolated_mono_duplicates() {
        let s = Sample::from_mono(vec![0.5, 0.5, 0.5, 0.5], 44100);
        let (l, r) = s.read_stereo_interpolated(1.0);
        assert!((l - 0.5).abs() < 0.01);
        assert!((r - 0.5).abs() < 0.01);
    }

    #[test]
    fn bank_add_get() {
        let mut bank = SampleBank::new();
        let id = bank.add(Sample::from_mono(vec![0.0; 10], 44100));
        assert!(bank.get(id).is_some());
        assert_eq!(bank.len(), 1);
    }
}

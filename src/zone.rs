//! Zone — key/velocity region mapped to a sample.

use crate::loop_mode::LoopMode;
use crate::sample::SampleId;
use serde::{Deserialize, Serialize};

/// A key/velocity zone mapping a region of the keyboard to a sample.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Zone {
    /// The sample this zone plays.
    pub(crate) sample_id: SampleId,
    /// MIDI key range (inclusive).
    pub(crate) key_lo: u8,
    pub(crate) key_hi: u8,
    /// MIDI velocity range (inclusive).
    pub(crate) vel_lo: u8,
    pub(crate) vel_hi: u8,
    /// Root note — the MIDI note at which the sample plays at original pitch.
    pub(crate) root_note: u8,
    /// Fine tuning in cents (-100 to +100).
    pub(crate) tune_cents: f32,
    /// Volume in dB.
    pub(crate) volume_db: f32,
    /// Pan (-1.0 = left, 0.0 = center, 1.0 = right).
    pub(crate) pan: f32,
    /// Loop mode.
    pub(crate) loop_mode: LoopMode,
    /// Loop start frame (0 = beginning).
    pub(crate) loop_start: usize,
    /// Loop end frame (0 = end of sample).
    pub(crate) loop_end: usize,
    /// Lowpass filter cutoff in Hz (0.0 = disabled).
    pub(crate) filter_cutoff: f32,
    /// How much velocity opens the filter (0.0–1.0).
    pub(crate) filter_vel_track: f32,
    /// Round-robin group (0 = none).
    pub(crate) group: u32,
}

impl Zone {
    /// Create a new zone for the given sample, defaulting to full key/velocity range.
    pub fn new(sample_id: SampleId) -> Self {
        Self {
            sample_id,
            key_lo: 0,
            key_hi: 127,
            vel_lo: 1,
            vel_hi: 127,
            root_note: 60,
            tune_cents: 0.0,
            volume_db: 0.0,
            pan: 0.0,
            loop_mode: LoopMode::OneShot,
            loop_start: 0,
            loop_end: 0,
            filter_cutoff: 0.0,
            filter_vel_track: 0.0,
            group: 0,
        }
    }

    /// Set the key range (inclusive).
    pub fn with_key_range(mut self, lo: u8, hi: u8) -> Self {
        self.key_lo = lo;
        self.key_hi = hi;
        self
    }

    /// Set the velocity range (inclusive).
    pub fn with_vel_range(mut self, lo: u8, hi: u8) -> Self {
        self.vel_lo = lo;
        self.vel_hi = hi;
        self
    }

    /// Set the root note.
    pub fn with_root_note(mut self, note: u8) -> Self {
        self.root_note = note;
        self
    }

    /// Set fine tuning in cents.
    pub fn with_tune(mut self, cents: f32) -> Self {
        self.tune_cents = cents.clamp(-100.0, 100.0);
        self
    }

    /// Set volume in dB.
    pub fn with_volume(mut self, db: f32) -> Self {
        self.volume_db = db;
        self
    }

    /// Set pan position.
    pub fn with_pan(mut self, pan: f32) -> Self {
        self.pan = pan.clamp(-1.0, 1.0);
        self
    }

    /// Set loop mode and region.
    pub fn with_loop(mut self, mode: LoopMode, start: usize, end: usize) -> Self {
        self.loop_mode = mode;
        self.loop_start = start;
        self.loop_end = end;
        self
    }

    /// Set lowpass filter cutoff and velocity tracking.
    pub fn with_filter(mut self, cutoff: f32, vel_track: f32) -> Self {
        self.filter_cutoff = cutoff.max(0.0);
        self.filter_vel_track = vel_track.clamp(0.0, 1.0);
        self
    }

    /// Set the round-robin group.
    pub fn with_group(mut self, group: u32) -> Self {
        self.group = group;
        self
    }

    /// Round-robin group (0 = none).
    #[inline]
    pub fn group(&self) -> u32 {
        self.group
    }

    /// Filter cutoff in Hz (0.0 = disabled).
    #[inline]
    pub fn filter_cutoff(&self) -> f32 {
        self.filter_cutoff
    }

    /// Filter velocity tracking amount.
    #[inline]
    pub fn filter_vel_track(&self) -> f32 {
        self.filter_vel_track
    }

    /// Pan position (-1.0 left, 0.0 center, 1.0 right).
    #[inline]
    pub fn pan(&self) -> f32 {
        self.pan
    }

    /// Check if a MIDI note and velocity fall within this zone.
    #[inline]
    #[must_use]
    pub fn matches(&self, note: u8, velocity: u8) -> bool {
        note >= self.key_lo && note <= self.key_hi && velocity >= self.vel_lo && velocity <= self.vel_hi
    }

    /// Compute the playback speed ratio for a given MIDI note.
    ///
    /// A note matching the root note plays at 1.0. Each semitone
    /// doubles/halves by 2^(1/12).
    #[inline]
    #[must_use]
    pub fn playback_ratio(&self, note: u8) -> f64 {
        let semitones = (note as f64 - self.root_note as f64) + self.tune_cents as f64 / 100.0;
        2.0_f64.powf(semitones / 12.0)
    }

    /// Sample ID for this zone.
    #[inline]
    pub fn sample_id(&self) -> SampleId {
        self.sample_id
    }

    /// Loop mode.
    #[inline]
    pub fn loop_mode(&self) -> LoopMode {
        self.loop_mode
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zone_matches() {
        let z = Zone::new(SampleId(0)).with_key_range(60, 72).with_vel_range(1, 127);
        assert!(z.matches(66, 100));
        assert!(!z.matches(59, 100));
        assert!(!z.matches(73, 100));
    }

    #[test]
    fn playback_ratio_root() {
        let z = Zone::new(SampleId(0)).with_root_note(60);
        assert!((z.playback_ratio(60) - 1.0).abs() < 0.001);
    }

    #[test]
    fn playback_ratio_octave_up() {
        let z = Zone::new(SampleId(0)).with_root_note(60);
        assert!((z.playback_ratio(72) - 2.0).abs() < 0.01);
    }

    #[test]
    fn playback_ratio_with_tuning() {
        let z = Zone::new(SampleId(0)).with_root_note(60).with_tune(50.0);
        // 50 cents = half a semitone above root
        let ratio = z.playback_ratio(60);
        assert!(ratio > 1.0);
        assert!(ratio < 1.06); // less than one semitone
    }
}

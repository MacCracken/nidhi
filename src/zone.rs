//! Zone — key/velocity region mapped to a sample.

use crate::envelope::AdsrConfig;
use crate::loop_mode::LoopMode;
use crate::sample::SampleId;
use serde::{Deserialize, Serialize};

/// Velocity-to-amplitude curve shape.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum VelocityCurve {
    /// Linear mapping (default). `amp = vel / 127`.
    #[default]
    Linear,
    /// Convex curve — opens faster (quiet velocities louder). `amp = sqrt(vel / 127)`.
    Convex,
    /// Concave curve — late bloom (quiet velocities quieter). `amp = (vel / 127)^2`.
    Concave,
    /// Switch — full volume above 64, silent below. Binary on/off.
    Switch,
}

impl VelocityCurve {
    /// Map a MIDI velocity (0–127) to amplitude (0.0–1.0).
    #[inline]
    #[must_use]
    pub fn apply(self, velocity: u8) -> f32 {
        let v = velocity as f32 / 127.0;
        match self {
            Self::Linear => v,
            Self::Convex => {
                // Fast inverse sqrt approximation for no_std; exact sqrt when std
                #[cfg(feature = "std")]
                {
                    v.sqrt()
                }
                #[cfg(not(feature = "std"))]
                {
                    // Babylonian method: 2 iterations is plenty for 0..1 range
                    if v <= 0.0 {
                        0.0
                    } else {
                        let mut x = v;
                        x = 0.5 * (x + v / x);
                        x = 0.5 * (x + v / x);
                        x
                    }
                }
            }
            Self::Concave => v * v,
            Self::Switch => {
                if velocity > 64 {
                    1.0
                } else {
                    0.0
                }
            }
        }
    }
}

/// Filter type for per-zone filtering.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum FilterMode {
    /// Low-pass filter (default).
    #[default]
    LowPass,
    /// High-pass filter.
    HighPass,
    /// Band-pass filter.
    BandPass,
    /// Notch (band-reject) filter.
    Notch,
}

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
    /// Crossfade length in frames at loop boundary (0 = no crossfade).
    #[serde(default)]
    pub(crate) crossfade_length: usize,
    /// Sample playback start offset in frames (0 = beginning).
    #[serde(default)]
    pub(crate) sample_offset: usize,
    /// Sample playback end frame (0 = end of sample).
    #[serde(default)]
    pub(crate) sample_end: usize,
    /// Filter cutoff in Hz (0.0 = disabled).
    pub(crate) filter_cutoff: f32,
    /// Filter resonance / Q (0.707 = Butterworth, higher = more resonant).
    pub(crate) filter_resonance: f32,
    /// Filter type.
    #[serde(default)]
    pub(crate) filter_type: FilterMode,
    /// How much velocity opens the filter (0.0–1.0).
    pub(crate) filter_vel_track: f32,
    /// Round-robin group (0 = none).
    pub(crate) group: u32,
    /// Choke group — voices in the same choke group silence each other (0 = none).
    #[serde(default)]
    pub(crate) choke_group: u32,
    /// Velocity-to-amplitude curve.
    #[serde(default)]
    pub(crate) vel_curve: VelocityCurve,
    /// Per-zone ADSR config (None = use engine default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) adsr: Option<AdsrConfig>,
    /// Filter envelope ADSR config (None = no filter envelope).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) fileg: Option<AdsrConfig>,
    /// Filter envelope depth in cents (±4800 = ±4 octaves).
    #[serde(default)]
    pub(crate) fileg_depth: f32,
    /// Pitch LFO rate in Hz (0.0 = disabled).
    #[serde(default)]
    pub(crate) pitchlfo_rate: f32,
    /// Pitch LFO depth in cents.
    #[serde(default)]
    pub(crate) pitchlfo_depth: f32,
    /// Filter LFO rate in Hz (0.0 = disabled).
    #[serde(default)]
    pub(crate) fillfo_rate: f32,
    /// Filter LFO depth in cents.
    #[serde(default)]
    pub(crate) fillfo_depth: f32,
    /// Filter key tracking (0.0 = none, 1.0 = full tracking from C4).
    #[serde(default)]
    pub(crate) fil_keytrack: f32,
    /// Time-stretch ratio (1.0 = normal, >1.0 = slower, <1.0 = faster, 0.0 = disabled).
    #[serde(default)]
    pub(crate) time_stretch: f32,
    /// Output bus index (0 = main, 1+ = aux buses).
    #[serde(default)]
    pub(crate) output_bus: u8,
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
            crossfade_length: 0,
            sample_offset: 0,
            sample_end: 0,
            filter_cutoff: 0.0,
            filter_resonance: 0.707,
            filter_type: FilterMode::LowPass,
            filter_vel_track: 0.0,
            group: 0,
            choke_group: 0,
            vel_curve: VelocityCurve::Linear,
            adsr: None,
            fileg: None,
            fileg_depth: 0.0,
            pitchlfo_rate: 0.0,
            pitchlfo_depth: 0.0,
            fillfo_rate: 0.0,
            fillfo_depth: 0.0,
            fil_keytrack: 0.0,
            time_stretch: 0.0,
            output_bus: 0,
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

    /// Set tuning offset in cents. Includes both fine tuning and transpose.
    pub fn with_tune(mut self, cents: f32) -> Self {
        self.tune_cents = cents.clamp(-12800.0, 12800.0);
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

    /// Set crossfade length in frames at loop boundary.
    pub fn with_crossfade(mut self, length: usize) -> Self {
        self.crossfade_length = length;
        self
    }

    /// Set sample playback start offset.
    pub fn with_sample_offset(mut self, offset: usize) -> Self {
        self.sample_offset = offset;
        self
    }

    /// Set sample playback end frame (0 = use full sample).
    pub fn with_sample_end(mut self, end: usize) -> Self {
        self.sample_end = end;
        self
    }

    /// Crossfade length in frames (0 = disabled).
    #[inline]
    #[must_use]
    pub fn crossfade_length(&self) -> usize {
        self.crossfade_length
    }

    /// Sample playback start offset.
    #[inline]
    #[must_use]
    pub fn sample_offset(&self) -> usize {
        self.sample_offset
    }

    /// Sample playback end frame (0 = end of sample).
    #[inline]
    #[must_use]
    pub fn sample_end(&self) -> usize {
        self.sample_end
    }

    /// Set filter cutoff and velocity tracking.
    pub fn with_filter(mut self, cutoff: f32, vel_track: f32) -> Self {
        self.filter_cutoff = cutoff.max(0.0);
        self.filter_vel_track = vel_track.clamp(0.0, 1.0);
        self
    }

    /// Set filter resonance (Q factor). 0.707 = Butterworth (no peak).
    pub fn with_filter_resonance(mut self, q: f32) -> Self {
        self.filter_resonance = q.max(0.1);
        self
    }

    /// Set filter type.
    pub fn with_filter_type(mut self, mode: FilterMode) -> Self {
        self.filter_type = mode;
        self
    }

    /// Set the round-robin group.
    pub fn with_group(mut self, group: u32) -> Self {
        self.group = group;
        self
    }

    /// Set choke group — voices in the same group silence each other.
    pub fn with_choke_group(mut self, group: u32) -> Self {
        self.choke_group = group;
        self
    }

    /// Choke group (0 = none).
    #[inline]
    #[must_use]
    pub fn choke_group(&self) -> u32 {
        self.choke_group
    }

    /// Set velocity curve.
    pub fn with_velocity_curve(mut self, curve: VelocityCurve) -> Self {
        self.vel_curve = curve;
        self
    }

    /// Velocity curve for this zone.
    #[inline]
    #[must_use]
    pub fn velocity_curve(&self) -> VelocityCurve {
        self.vel_curve
    }

    /// Set per-zone ADSR envelope config.
    pub fn with_adsr(mut self, config: AdsrConfig) -> Self {
        self.adsr = Some(config);
        self
    }

    /// Per-zone ADSR config (None = use engine default).
    #[inline]
    #[must_use]
    pub fn adsr(&self) -> Option<&AdsrConfig> {
        self.adsr.as_ref()
    }

    /// Set filter envelope config and depth in cents.
    pub fn with_filter_envelope(mut self, config: AdsrConfig, depth_cents: f32) -> Self {
        self.fileg = Some(config);
        self.fileg_depth = depth_cents.clamp(-4800.0, 4800.0);
        self
    }

    /// Filter envelope config (None = no filter modulation).
    #[inline]
    #[must_use]
    pub fn fileg(&self) -> Option<&AdsrConfig> {
        self.fileg.as_ref()
    }

    /// Filter envelope depth in cents.
    #[inline]
    #[must_use]
    pub fn fileg_depth(&self) -> f32 {
        self.fileg_depth
    }

    /// Set pitch LFO rate (Hz) and depth (cents). Modulates playback speed.
    pub fn with_pitch_lfo(mut self, rate_hz: f32, depth_cents: f32) -> Self {
        self.pitchlfo_rate = rate_hz.max(0.0);
        self.pitchlfo_depth = depth_cents;
        self
    }

    /// Set filter LFO rate (Hz) and depth (cents). Modulates filter cutoff.
    pub fn with_filter_lfo(mut self, rate_hz: f32, depth_cents: f32) -> Self {
        self.fillfo_rate = rate_hz.max(0.0);
        self.fillfo_depth = depth_cents;
        self
    }

    /// Set filter key tracking (0.0 = none, 1.0 = full tracking from C4/note 60).
    /// At 1.0, each semitone above C4 raises cutoff by 100 cents.
    pub fn with_key_tracking(mut self, amount: f32) -> Self {
        self.fil_keytrack = amount.clamp(0.0, 1.0);
        self
    }

    /// Pitch LFO rate in Hz.
    #[inline]
    #[must_use]
    pub fn pitchlfo_rate(&self) -> f32 {
        self.pitchlfo_rate
    }

    /// Pitch LFO depth in cents.
    #[inline]
    #[must_use]
    pub fn pitchlfo_depth(&self) -> f32 {
        self.pitchlfo_depth
    }

    /// Filter LFO rate in Hz.
    #[inline]
    #[must_use]
    pub fn fillfo_rate(&self) -> f32 {
        self.fillfo_rate
    }

    /// Filter LFO depth in cents.
    #[inline]
    #[must_use]
    pub fn fillfo_depth(&self) -> f32 {
        self.fillfo_depth
    }

    /// Filter key tracking amount.
    #[inline]
    #[must_use]
    pub fn fil_keytrack(&self) -> f32 {
        self.fil_keytrack
    }

    /// Set time-stretch ratio (1.0 = normal, >1.0 = slower, <1.0 = faster).
    pub fn with_time_stretch(mut self, ratio: f32) -> Self {
        self.time_stretch = ratio.clamp(0.0, 4.0);
        self
    }

    /// Time-stretch ratio (0.0 = disabled).
    #[inline]
    #[must_use]
    pub fn time_stretch(&self) -> f32 {
        self.time_stretch
    }

    /// Set output bus index (0 = main, 1+ = aux buses).
    pub fn with_output_bus(mut self, bus: u8) -> Self {
        self.output_bus = bus;
        self
    }

    /// Output bus index.
    #[inline]
    #[must_use]
    pub fn output_bus(&self) -> u8 {
        self.output_bus
    }

    /// Round-robin group (0 = none).
    #[inline]
    #[must_use]
    pub fn group(&self) -> u32 {
        self.group
    }

    /// Filter cutoff in Hz (0.0 = disabled).
    #[inline]
    #[must_use]
    pub fn filter_cutoff(&self) -> f32 {
        self.filter_cutoff
    }

    /// Filter resonance (Q factor).
    #[inline]
    #[must_use]
    pub fn filter_resonance(&self) -> f32 {
        self.filter_resonance
    }

    /// Filter type.
    #[inline]
    #[must_use]
    pub fn filter_type(&self) -> FilterMode {
        self.filter_type
    }

    /// Filter velocity tracking amount.
    #[inline]
    #[must_use]
    pub fn filter_vel_track(&self) -> f32 {
        self.filter_vel_track
    }

    /// Pan position (-1.0 left, 0.0 center, 1.0 right).
    #[inline]
    #[must_use]
    pub fn pan(&self) -> f32 {
        self.pan
    }

    /// Check if a MIDI note and velocity fall within this zone.
    #[inline]
    #[must_use]
    pub fn matches(&self, note: u8, velocity: u8) -> bool {
        note >= self.key_lo
            && note <= self.key_hi
            && velocity >= self.vel_lo
            && velocity <= self.vel_hi
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
    #[must_use]
    pub fn loop_mode(&self) -> LoopMode {
        self.loop_mode
    }
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;

    #[test]
    fn zone_matches() {
        let z = Zone::new(SampleId(0))
            .with_key_range(60, 72)
            .with_vel_range(1, 127);
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

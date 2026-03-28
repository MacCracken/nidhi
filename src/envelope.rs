//! ADSR envelope — lightweight per-voice amplitude envelope.

use serde::{Deserialize, Serialize};

/// Envelope stage.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum EnvState {
    /// Voice is not sounding.
    #[default]
    Idle,
    /// Attack ramp from 0.0 → 1.0.
    Attack,
    /// Decay ramp from 1.0 → sustain level.
    Decay,
    /// Held at sustain level until note-off.
    Sustain,
    /// Release ramp from current level → 0.0.
    Release,
}

/// ADSR configuration — all durations in samples.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[must_use]
pub struct AdsrConfig {
    /// Attack duration in samples.
    pub attack_samples: u32,
    /// Decay duration in samples.
    pub decay_samples: u32,
    /// Sustain level (0.0–1.0).
    pub sustain_level: f32,
    /// Release duration in samples.
    pub release_samples: u32,
}

impl Default for AdsrConfig {
    fn default() -> Self {
        Self {
            attack_samples: 0,
            decay_samples: 0,
            sustain_level: 1.0,
            release_samples: 441, // ~10ms at 44100
        }
    }
}

impl AdsrConfig {
    /// Create an ADSR config from durations in seconds.
    pub fn from_seconds(
        attack: f32,
        decay: f32,
        sustain: f32,
        release: f32,
        sample_rate: f32,
    ) -> Self {
        Self {
            attack_samples: (attack * sample_rate).max(0.0) as u32,
            decay_samples: (decay * sample_rate).max(0.0) as u32,
            sustain_level: sustain.clamp(0.0, 1.0),
            release_samples: (release * sample_rate).max(1.0) as u32,
        }
    }

    /// Convert to seconds given a sample rate.
    #[must_use]
    pub fn to_seconds(&self, sample_rate: f32) -> (f32, f32, f32, f32) {
        (
            self.attack_samples as f32 / sample_rate,
            self.decay_samples as f32 / sample_rate,
            self.sustain_level,
            self.release_samples as f32 / sample_rate,
        )
    }

    /// Check if all ADSR values are at their defaults (no explicit envelope).
    pub fn is_default_sfz(&self, sample_rate: f32) -> bool {
        self.attack_samples == 0
            && self.decay_samples == 0
            && (self.sustain_level - 1.0).abs() < f32::EPSILON
            && self.release_samples <= (sample_rate * 0.001) as u32 // ~0s
    }
}

// ---------------------------------------------------------------------------
// AmpEnvelope — per-voice envelope state
// ---------------------------------------------------------------------------

/// Per-voice amplitude envelope.
///
/// When the `std` feature is enabled (default), this wraps [`naad::envelope::Adsr`]
/// for production-quality envelope generation. Under `no_std`, it uses a built-in
/// linear ADSR implementation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AmpEnvelope {
    #[cfg(feature = "std")]
    inner: naad::envelope::Adsr,

    #[cfg(not(feature = "std"))]
    config: AdsrConfig,
    #[cfg(not(feature = "std"))]
    state: EnvState,
    #[cfg(not(feature = "std"))]
    level: f32,
    #[cfg(not(feature = "std"))]
    pos: u32,
    #[cfg(not(feature = "std"))]
    release_start_level: f32,
}

impl AmpEnvelope {
    /// Create a new envelope from an ADSR config and sample rate.
    #[must_use]
    pub fn new(config: &AdsrConfig, sample_rate: f32) -> Self {
        #[cfg(feature = "std")]
        {
            let (a, d, s, r) = config.to_seconds(sample_rate);
            // naad validates params; clamp to safe ranges to avoid errors.
            let adsr = naad::envelope::Adsr::with_sample_rate(
                a.max(0.0),
                d.max(0.0),
                s.clamp(0.0, 1.0),
                r.max(0.0),
                sample_rate.max(1.0),
            )
            .unwrap_or_else(|_| {
                naad::envelope::Adsr::with_sample_rate(0.0, 0.0, 1.0, 0.01, sample_rate.max(1.0))
                    .expect("default ADSR params are valid")
            });
            Self { inner: adsr }
        }

        #[cfg(not(feature = "std"))]
        {
            let _ = sample_rate;
            Self {
                config: *config,
                state: EnvState::Idle,
                level: 0.0,
                pos: 0,
                release_start_level: 0.0,
            }
        }
    }

    /// Trigger the attack phase (note on).
    #[inline]
    pub fn trigger(&mut self) {
        #[cfg(feature = "std")]
        self.inner.gate_on();

        #[cfg(not(feature = "std"))]
        {
            self.state = EnvState::Attack;
            self.level = 0.0;
            self.pos = 0;
            self.release_start_level = 0.0;
        }
    }

    /// Enter the release phase (note off).
    #[inline]
    pub fn release(&mut self) {
        #[cfg(feature = "std")]
        self.inner.gate_off();

        #[cfg(not(feature = "std"))]
        {
            if self.state != EnvState::Idle {
                self.release_start_level = self.level;
                self.state = EnvState::Release;
                self.pos = 0;
            }
        }
    }

    /// Advance the envelope by one sample, returning the current level (0.0–1.0).
    #[inline]
    pub fn tick(&mut self) -> f32 {
        #[cfg(feature = "std")]
        {
            self.inner.next_value()
        }

        #[cfg(not(feature = "std"))]
        {
            self.tick_no_std()
        }
    }

    /// Whether the envelope is still producing output.
    #[inline]
    pub fn is_active(&self) -> bool {
        #[cfg(feature = "std")]
        {
            self.inner.is_active()
        }

        #[cfg(not(feature = "std"))]
        {
            self.state != EnvState::Idle
        }
    }

    /// Whether the envelope is in the release phase.
    #[inline]
    pub fn is_releasing(&self) -> bool {
        #[cfg(feature = "std")]
        {
            self.inner.state() == naad::envelope::EnvelopeState::Release
        }

        #[cfg(not(feature = "std"))]
        {
            self.state == EnvState::Release
        }
    }

    // -----------------------------------------------------------------------
    // no_std fallback implementation
    // -----------------------------------------------------------------------

    #[cfg(not(feature = "std"))]
    #[inline]
    fn tick_no_std(&mut self) -> f32 {
        match self.state {
            EnvState::Idle => {
                self.level = 0.0;
            }
            EnvState::Attack => {
                if self.config.attack_samples == 0 {
                    self.level = 1.0;
                    self.state = EnvState::Decay;
                    self.pos = 0;
                } else {
                    self.level = (self.pos as f32 + 1.0) / self.config.attack_samples as f32;
                    self.pos += 1;
                    if self.pos >= self.config.attack_samples {
                        self.level = 1.0;
                        self.state = EnvState::Decay;
                        self.pos = 0;
                    }
                }
            }
            EnvState::Decay => {
                if self.config.decay_samples == 0 {
                    self.level = self.config.sustain_level;
                    self.state = EnvState::Sustain;
                    self.pos = 0;
                } else {
                    let t = (self.pos as f32 + 1.0) / self.config.decay_samples as f32;
                    self.level = 1.0 + (self.config.sustain_level - 1.0) * t;
                    self.pos += 1;
                    if self.pos >= self.config.decay_samples {
                        self.level = self.config.sustain_level;
                        self.state = EnvState::Sustain;
                        self.pos = 0;
                    }
                }
            }
            EnvState::Sustain => {
                self.level = self.config.sustain_level;
            }
            EnvState::Release => {
                if self.config.release_samples == 0 {
                    self.level = 0.0;
                    self.state = EnvState::Idle;
                    self.pos = 0;
                } else {
                    let progress = (self.pos as f32 + 1.0) / self.config.release_samples as f32;
                    self.level = self.release_start_level * (1.0 - progress);
                    self.pos += 1;
                    if self.level <= 0.0 || self.pos >= self.config.release_samples {
                        self.level = 0.0;
                        self.state = EnvState::Idle;
                        self.pos = 0;
                    }
                }
            }
        }
        self.level
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adsr_from_seconds() {
        let cfg = AdsrConfig::from_seconds(0.01, 0.05, 0.7, 0.1, 44100.0);
        assert_eq!(cfg.attack_samples, 441);
        assert_eq!(cfg.decay_samples, 2205);
        assert!((cfg.sustain_level - 0.7).abs() < f32::EPSILON);
        assert_eq!(cfg.release_samples, 4410);
    }

    #[test]
    fn amp_envelope_trigger_release_cycle() {
        let cfg = AdsrConfig {
            attack_samples: 4,
            decay_samples: 4,
            sustain_level: 0.5,
            release_samples: 4,
        };
        let mut env = AmpEnvelope::new(&cfg, 44100.0);
        assert!(!env.is_active());

        env.trigger();
        assert!(env.is_active());

        // Attack
        for _ in 0..100 {
            env.tick();
        }
        // Should be at sustain
        let level = env.tick();
        assert!(
            (level - 0.5).abs() < 0.05,
            "expected sustain ~0.5, got {level}"
        );

        // Release
        env.release();
        for _ in 0..10000 {
            env.tick();
            if !env.is_active() {
                break;
            }
        }
        assert!(!env.is_active());
    }

    #[test]
    fn amp_envelope_attack_ramp() {
        let cfg = AdsrConfig {
            attack_samples: 100,
            decay_samples: 0,
            sustain_level: 1.0,
            release_samples: 100,
        };
        let mut env = AmpEnvelope::new(&cfg, 44100.0);
        env.trigger();

        let first = env.tick();
        for _ in 0..49 {
            env.tick();
        }
        let mid = env.tick();
        assert!(
            mid > first,
            "should ramp up during attack: first={first}, mid={mid}"
        );
    }

    #[test]
    fn amp_envelope_smooth_release_from_mid_attack() {
        let cfg = AdsrConfig {
            attack_samples: 1000,
            decay_samples: 0,
            sustain_level: 1.0,
            release_samples: 1000,
        };
        let mut env = AmpEnvelope::new(&cfg, 44100.0);
        env.trigger();

        // Advance partway through attack
        for _ in 0..500 {
            env.tick();
        }
        let level_at_release = env.tick();
        assert!(level_at_release > 0.0 && level_at_release < 1.0);

        // Release should ramp down from current level, not from 1.0
        env.release();
        let first_release = env.tick();
        assert!(
            first_release <= level_at_release,
            "release should start at or below {level_at_release}, got {first_release}"
        );

        for _ in 0..10000 {
            env.tick();
            if !env.is_active() {
                break;
            }
        }
        assert!(!env.is_active());
    }

    #[test]
    fn amp_envelope_idle_stays_zero() {
        let cfg = AdsrConfig::default();
        let mut env = AmpEnvelope::new(&cfg, 44100.0);
        let level = env.tick();
        assert_eq!(level, 0.0);
        assert!(!env.is_active());
    }

    #[test]
    fn amp_envelope_zero_attack() {
        let cfg = AdsrConfig {
            attack_samples: 0,
            decay_samples: 0,
            sustain_level: 0.8,
            release_samples: 100,
        };
        let mut env = AmpEnvelope::new(&cfg, 44100.0);
        env.trigger();

        // Should quickly reach sustain
        for _ in 0..10 {
            env.tick();
        }
        let level = env.tick();
        assert!(
            (level - 0.8).abs() < 0.05,
            "should be at sustain ~0.8, got {level}"
        );
    }
}

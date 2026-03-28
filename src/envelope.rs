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

    /// Tick the envelope forward one sample, returning the new level.
    ///
    /// Mutates `state`, `level`, and `pos` in place.
    #[inline]
    pub fn tick(&self, state: &mut EnvState, level: &mut f32, pos: &mut u32) -> f32 {
        match *state {
            EnvState::Idle => {
                *level = 0.0;
            }
            EnvState::Attack => {
                if self.attack_samples == 0 {
                    *level = 1.0;
                    *state = EnvState::Decay;
                    *pos = 0;
                } else {
                    *level = (*pos as f32 + 1.0) / self.attack_samples as f32;
                    *pos += 1;
                    if *pos >= self.attack_samples {
                        *level = 1.0;
                        *state = EnvState::Decay;
                        *pos = 0;
                    }
                }
            }
            EnvState::Decay => {
                if self.decay_samples == 0 {
                    *level = self.sustain_level;
                    *state = EnvState::Sustain;
                    *pos = 0;
                } else {
                    let t = (*pos as f32 + 1.0) / self.decay_samples as f32;
                    *level = 1.0 + (self.sustain_level - 1.0) * t;
                    *pos += 1;
                    if *pos >= self.decay_samples {
                        *level = self.sustain_level;
                        *state = EnvState::Sustain;
                        *pos = 0;
                    }
                }
            }
            EnvState::Sustain => {
                *level = self.sustain_level;
            }
            EnvState::Release => {
                if self.release_samples == 0 {
                    *level = 0.0;
                    *state = EnvState::Idle;
                    *pos = 0;
                } else {
                    // Store the starting level on first tick of release
                    // We use a linear ramp from current level to 0
                    let remaining = self.release_samples.saturating_sub(*pos);
                    if remaining == 0 {
                        *level = 0.0;
                        *state = EnvState::Idle;
                        *pos = 0;
                    } else {
                        *level *= (remaining as f32 - 1.0) / remaining as f32;
                        *pos += 1;
                        if *pos >= self.release_samples {
                            *level = 0.0;
                            *state = EnvState::Idle;
                            *pos = 0;
                        }
                    }
                }
            }
        }
        *level
    }

    /// Trigger the attack phase.
    #[inline]
    pub fn trigger(state: &mut EnvState, level: &mut f32, pos: &mut u32) {
        *state = EnvState::Attack;
        *level = 0.0;
        *pos = 0;
    }

    /// Enter the release phase.
    #[inline]
    pub fn release(state: &mut EnvState, pos: &mut u32) {
        if *state != EnvState::Idle {
            *state = EnvState::Release;
            *pos = 0;
        }
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
    fn adsr_attack_ramp() {
        let cfg = AdsrConfig {
            attack_samples: 10,
            decay_samples: 0,
            sustain_level: 1.0,
            release_samples: 10,
        };
        let mut state = EnvState::Idle;
        let mut level = 0.0f32;
        let mut pos = 0u32;

        AdsrConfig::trigger(&mut state, &mut level, &mut pos);
        assert_eq!(state, EnvState::Attack);

        // Ramp through attack
        for i in 0..10 {
            let l = cfg.tick(&mut state, &mut level, &mut pos);
            if i < 9 {
                assert!(l > 0.0 && l <= 1.0);
            }
        }
        // After attack completes, state transitions to Decay
        assert_eq!(state, EnvState::Decay);
        assert!((level - 1.0).abs() < f32::EPSILON);

        // One more tick: decay=0 so jumps immediately to Sustain
        cfg.tick(&mut state, &mut level, &mut pos);
        assert_eq!(state, EnvState::Sustain);
        assert!((level - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn adsr_full_cycle() {
        let cfg = AdsrConfig {
            attack_samples: 4,
            decay_samples: 4,
            sustain_level: 0.5,
            release_samples: 4,
        };
        let mut state = EnvState::Idle;
        let mut level = 0.0f32;
        let mut pos = 0u32;

        AdsrConfig::trigger(&mut state, &mut level, &mut pos);

        // Attack
        for _ in 0..4 {
            cfg.tick(&mut state, &mut level, &mut pos);
        }
        assert_eq!(state, EnvState::Decay);

        // Decay
        for _ in 0..4 {
            cfg.tick(&mut state, &mut level, &mut pos);
        }
        assert_eq!(state, EnvState::Sustain);
        assert!((level - 0.5).abs() < 0.01);

        // Release
        AdsrConfig::release(&mut state, &mut pos);
        assert_eq!(state, EnvState::Release);

        for _ in 0..100 {
            cfg.tick(&mut state, &mut level, &mut pos);
            if state == EnvState::Idle {
                break;
            }
        }
        assert_eq!(state, EnvState::Idle);
        assert!((level - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn adsr_idle_stays_zero() {
        let cfg = AdsrConfig::default();
        let mut state = EnvState::Idle;
        let mut level = 0.0f32;
        let mut pos = 0u32;

        let l = cfg.tick(&mut state, &mut level, &mut pos);
        assert_eq!(l, 0.0);
        assert_eq!(state, EnvState::Idle);
    }

    #[test]
    fn adsr_zero_attack() {
        let cfg = AdsrConfig {
            attack_samples: 0,
            decay_samples: 0,
            sustain_level: 0.8,
            release_samples: 10,
        };
        let mut state = EnvState::Idle;
        let mut level = 0.0f32;
        let mut pos = 0u32;

        AdsrConfig::trigger(&mut state, &mut level, &mut pos);
        // First tick: attack=0 → jumps to Decay
        cfg.tick(&mut state, &mut level, &mut pos);
        assert_eq!(state, EnvState::Decay);
        // Second tick: decay=0 → jumps to Sustain
        cfg.tick(&mut state, &mut level, &mut pos);
        assert_eq!(state, EnvState::Sustain);
        assert!((level - 0.8).abs() < f32::EPSILON);
    }
}

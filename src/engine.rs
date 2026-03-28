//! Sampler engine — polyphonic sample playback with voice management.

use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

use crate::envelope::{AdsrConfig, EnvState};
use crate::instrument::Instrument;
use crate::loop_mode::LoopMode;
use crate::sample::SampleBank;

/// A single playback voice — tracks position within a sample.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamplerVoice {
    /// Whether this voice is currently active.
    active: bool,
    /// The zone being played.
    zone_index: usize,
    /// Current playback position (fractional frame).
    position: f64,
    /// Playback speed ratio (pitch shifting).
    speed: f64,
    /// Current amplitude (for velocity scaling).
    amplitude: f32,
    /// MIDI note that triggered this voice.
    note: u8,
    /// Voice age in samples (for steal-oldest).
    age: u64,
    /// Playback direction for ping-pong.
    forward: bool,
    /// ADSR envelope state.
    env_state: EnvState,
    /// Current envelope level (0.0–1.0).
    env_level: f32,
    /// Position within current envelope stage.
    env_pos: u32,
    /// One-pole lowpass filter state.
    filter_state: f32,
    /// Filter coefficient (0.0 = full filter, 1.0 = no filter).
    filter_coeff: f32,
}

impl SamplerVoice {
    fn new() -> Self {
        Self {
            active: false,
            zone_index: 0,
            position: 0.0,
            speed: 1.0,
            amplitude: 1.0,
            note: 0,
            age: 0,
            forward: true,
            env_state: EnvState::Idle,
            env_level: 0.0,
            env_pos: 0,
            filter_state: 0.0,
            filter_coeff: 1.0,
        }
    }

    /// Whether this voice is actively producing audio.
    #[inline]
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// The MIDI note playing.
    #[inline]
    pub fn note(&self) -> u8 {
        self.note
    }

    /// Apply one-pole lowpass filter to stereo.
    /// Uses a single filter state (mono filter on both channels).
    /// For true stereo filtering we'd need two states, but this is
    /// lightweight and adequate for brightness control.
    #[inline]
    fn apply_filter_stereo(&mut self, left: f32, right: f32) -> (f32, f32) {
        if self.filter_coeff >= 1.0 {
            return (left, right);
        }
        // Simple: filter the mid signal, reconstruct
        // Actually, for per-voice brightness, filtering both the same way is fine
        let mono_in = (left + right) * 0.5;
        self.filter_state += self.filter_coeff * (mono_in - self.filter_state);
        let ratio = if mono_in.abs() > 1e-10 {
            self.filter_state / mono_in
        } else {
            self.filter_coeff
        };
        (left * ratio, right * ratio)
    }
}

/// Polyphonic sampler engine with voice stealing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct SamplerEngine {
    voices: Vec<SamplerVoice>,
    instrument: Option<Instrument>,
    bank: SampleBank,
    sample_rate: f32,
    /// ADSR envelope configuration.
    adsr: AdsrConfig,
}

impl SamplerEngine {
    /// Create a new sampler engine with the given voice count.
    pub fn new(max_voices: usize, sample_rate: f32) -> Self {
        Self {
            voices: (0..max_voices).map(|_| SamplerVoice::new()).collect(),
            instrument: None,
            bank: SampleBank::new(),
            sample_rate,
            adsr: AdsrConfig {
                attack_samples: 0,
                decay_samples: 0,
                sustain_level: 1.0,
                release_samples: (sample_rate * 0.01).max(1.0) as u32, // 10ms default
            },
        }
    }

    /// Set the instrument to play.
    pub fn set_instrument(&mut self, instrument: Instrument) {
        self.instrument = Some(instrument);
    }

    /// Set the sample bank.
    pub fn set_bank(&mut self, bank: SampleBank) {
        self.bank = bank;
    }

    /// Get a reference to the sample bank.
    pub fn bank(&self) -> &SampleBank {
        &self.bank
    }

    /// Get a mutable reference to the sample bank.
    pub fn bank_mut(&mut self) -> &mut SampleBank {
        &mut self.bank
    }

    /// Set the ADSR envelope configuration.
    pub fn set_adsr(&mut self, adsr: AdsrConfig) {
        self.adsr = adsr;
    }

    /// Set release time in milliseconds (convenience — updates ADSR release only).
    pub fn set_release_ms(&mut self, ms: f32) {
        self.adsr.release_samples = (self.sample_rate * ms / 1000.0).max(1.0) as u32;
    }

    /// Compute the one-pole filter coefficient from zone settings and velocity.
    #[inline]
    fn compute_filter_coeff(cutoff: f32, vel_track: f32, velocity: u8, sample_rate: f32) -> f32 {
        if cutoff <= 0.0 {
            return 1.0; // disabled
        }
        let vel_norm = velocity as f32 / 127.0;
        // Velocity opens the filter: at max velocity with full tracking, cutoff is used as-is.
        // At zero velocity with full tracking, cutoff is halved.
        let effective_cutoff = cutoff * (1.0 - vel_track * (1.0 - vel_norm));
        // One-pole coefficient: coeff = 1 - e^(-2π * fc / fs)
        let coeff = 1.0 - (-core::f32::consts::TAU * effective_cutoff / sample_rate).exp();
        coeff.clamp(0.0, 1.0)
    }

    /// Trigger a note. Returns the voice index, or None if no voice available.
    pub fn note_on(&mut self, note: u8, velocity: u8) -> Option<usize> {
        let instrument = self.instrument.as_ref()?;
        let zones = instrument.find_zones(note, velocity);
        if zones.is_empty() {
            return None;
        }

        // Find the zone index in the instrument
        let zone_idx = instrument
            .zones()
            .iter()
            .position(|z| core::ptr::eq(z, zones[0]))?;

        let zone = &instrument.zones()[zone_idx];
        let speed = zone.playback_ratio(note);
        let amp = velocity as f32 / 127.0;

        let filter_coeff = Self::compute_filter_coeff(
            zone.filter_cutoff(),
            zone.filter_vel_track(),
            velocity,
            self.sample_rate,
        );

        // Find a free voice, or steal the oldest
        let voice_idx = self
            .voices
            .iter()
            .position(|v| !v.active)
            .or_else(|| {
                self.voices
                    .iter()
                    .enumerate()
                    .max_by_key(|(_, v)| v.age)
                    .map(|(i, _)| i)
            })?;

        let voice = &mut self.voices[voice_idx];
        voice.active = true;
        voice.zone_index = zone_idx;
        voice.position = 0.0;
        voice.speed = speed;
        voice.amplitude = amp;
        voice.note = note;
        voice.age = 0;
        voice.forward = true;
        voice.filter_state = 0.0;
        voice.filter_coeff = filter_coeff;

        // Trigger ADSR
        AdsrConfig::trigger(&mut voice.env_state, &mut voice.env_level, &mut voice.env_pos);

        Some(voice_idx)
    }

    /// Release a note.
    pub fn note_off(&mut self, note: u8) {
        for voice in &mut self.voices {
            if voice.active && voice.note == note && voice.env_state != EnvState::Release {
                AdsrConfig::release(&mut voice.env_state, &mut voice.env_pos);
            }
        }
    }

    /// Release all notes.
    pub fn all_notes_off(&mut self) {
        for voice in &mut self.voices {
            if voice.active && voice.env_state != EnvState::Release && voice.env_state != EnvState::Idle {
                AdsrConfig::release(&mut voice.env_state, &mut voice.env_pos);
            }
        }
    }

    /// Advance a voice's position according to its loop mode. Returns false if voice should deactivate.
    #[inline]
    fn advance_position(voice: &mut SamplerVoice, loop_mode: LoopMode, loop_start: usize, loop_end: usize, frames: usize) -> bool {
        match loop_mode {
            LoopMode::OneShot => {
                voice.position += voice.speed;
                if voice.position >= frames as f64 {
                    return false;
                }
            }
            LoopMode::Forward => {
                voice.position += voice.speed;
                let end = if loop_end > 0 {
                    loop_end as f64
                } else {
                    frames as f64
                };
                if voice.position >= end {
                    voice.position = loop_start as f64;
                }
            }
            LoopMode::PingPong => {
                if voice.forward {
                    voice.position += voice.speed;
                    let end = if loop_end > 0 {
                        loop_end as f64
                    } else {
                        frames as f64
                    };
                    if voice.position >= end {
                        voice.forward = false;
                    }
                } else {
                    voice.position -= voice.speed;
                    if voice.position <= loop_start as f64 {
                        voice.forward = true;
                    }
                }
            }
            LoopMode::Reverse => {
                voice.position -= voice.speed;
                if voice.position < 0.0 {
                    return false;
                }
            }
        }
        true
    }

    /// Generate the next stereo sample pair by mixing all active voices.
    ///
    /// Returns `(left, right)` with pan applied per-zone.
    #[inline]
    pub fn next_sample_stereo(&mut self) -> (f32, f32) {
        let mut out_l = 0.0f32;
        let mut out_r = 0.0f32;

        let instrument = match &self.instrument {
            Some(i) => i,
            None => return (0.0, 0.0),
        };
        let zones = instrument.zones();

        for voice in &mut self.voices {
            if !voice.active {
                continue;
            }
            voice.age += 1;

            let zone = &zones[voice.zone_index];
            let sample = match self.bank.get(zone.sample_id()) {
                Some(s) => s,
                None => {
                    voice.active = false;
                    continue;
                }
            };

            // Read stereo interpolated sample
            let (mut sl, mut sr) = sample.read_stereo_interpolated(voice.position);

            // Apply filter
            let (fl, fr) = voice.apply_filter_stereo(sl, sr);
            sl = fl;
            sr = fr;

            // Tick ADSR
            let env = self.adsr.tick(&mut voice.env_state, &mut voice.env_level, &mut voice.env_pos);

            if voice.env_state == EnvState::Idle {
                voice.active = false;
                continue;
            }

            let amp = voice.amplitude * env;

            // Apply pan (constant-power approximation: linear for simplicity)
            let pan = zone.pan(); // -1..1
            let pan_l = (1.0 - pan) * 0.5;
            let pan_r = (1.0 + pan) * 0.5;

            out_l += sl * amp * pan_l;
            out_r += sr * amp * pan_r;

            // Advance position
            if !Self::advance_position(voice, zone.loop_mode(), zone.loop_start, zone.loop_end, sample.frames()) {
                voice.active = false;
            }
        }

        (out_l, out_r)
    }

    /// Generate the next mono sample by mixing all active voices.
    ///
    /// Convenience method — returns `(L + R) / 2`.
    #[inline]
    pub fn next_sample(&mut self) -> f32 {
        let (l, r) = self.next_sample_stereo();
        (l + r) * 0.5
    }

    /// Fill a buffer with mono sampler output.
    pub fn fill_buffer(&mut self, buffer: &mut [f32]) {
        for s in buffer.iter_mut() {
            *s = self.next_sample();
        }
    }

    /// Fill interleaved stereo buffer.
    pub fn fill_buffer_stereo(&mut self, buffer: &mut [f32]) {
        let mut i = 0;
        while i + 1 < buffer.len() {
            let (l, r) = self.next_sample_stereo();
            buffer[i] = l;
            buffer[i + 1] = r;
            i += 2;
        }
    }

    /// Number of currently active voices.
    #[must_use]
    pub fn active_voice_count(&self) -> usize {
        self.voices.iter().filter(|v| v.active).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sample::Sample;
    use crate::zone::Zone;

    fn make_engine() -> SamplerEngine {
        let mut bank = SampleBank::new();
        let sine: Vec<f32> = (0..44100)
            .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 44100.0).sin())
            .collect();
        let id = bank.add(Sample::from_mono(sine, 44100));

        let mut inst = Instrument::new("test");
        inst.add_zone(Zone::new(id).with_key_range(0, 127).with_root_note(69)); // A4=440

        let mut engine = SamplerEngine::new(8, 44100.0);
        engine.set_bank(bank);
        engine.set_instrument(inst);
        engine
    }

    #[test]
    fn note_on_produces_output() {
        let mut engine = make_engine();
        engine.note_on(69, 100);
        assert_eq!(engine.active_voice_count(), 1);

        let mut sum = 0.0f32;
        for _ in 0..4410 {
            sum += engine.next_sample().abs();
        }
        assert!(sum > 0.0, "Should produce audio output");
    }

    #[test]
    fn note_off_releases() {
        let mut engine = make_engine();
        engine.note_on(69, 100);
        engine.note_off(69);

        // Process through the release
        for _ in 0..44100 {
            engine.next_sample();
        }
        assert_eq!(engine.active_voice_count(), 0);
    }

    #[test]
    fn pitch_shift() {
        let mut engine = make_engine();
        // Playing one octave up should play at 2x speed
        engine.note_on(81, 100); // A5 = 69 + 12

        let mut buf = vec![0.0f32; 4410];
        engine.fill_buffer(&mut buf);
        assert!(buf.iter().any(|&s| s.abs() > 0.1));
    }

    #[test]
    fn no_instrument_silent() {
        let mut engine = SamplerEngine::new(8, 44100.0);
        assert!(engine.note_on(60, 100).is_none());
        assert_eq!(engine.next_sample(), 0.0);
    }

    #[test]
    fn adsr_envelope_shapes_output() {
        let mut engine = make_engine();
        engine.set_adsr(AdsrConfig {
            attack_samples: 100,
            decay_samples: 100,
            sustain_level: 0.5,
            release_samples: 100,
        });

        engine.note_on(69, 127);

        // First sample should be quiet (attack starting)
        let first = engine.next_sample().abs();
        // After 50 samples, should be louder
        for _ in 0..49 {
            engine.next_sample();
        }
        let mid_attack = engine.next_sample().abs();
        assert!(
            mid_attack > first,
            "Output should grow during attack: first={first}, mid={mid_attack}"
        );
    }

    #[test]
    fn stereo_output_with_pan() {
        let mut bank = SampleBank::new();
        let sine: Vec<f32> = (0..44100)
            .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 44100.0).sin())
            .collect();
        let id = bank.add(Sample::from_mono(sine, 44100));

        let mut inst = Instrument::new("test");
        inst.add_zone(
            Zone::new(id)
                .with_key_range(0, 127)
                .with_root_note(69)
                .with_pan(1.0), // hard right
        );

        let mut engine = SamplerEngine::new(8, 44100.0);
        engine.set_bank(bank);
        engine.set_instrument(inst);
        engine.note_on(69, 127);

        // Advance past the first sample to get some signal
        let mut sum_l = 0.0f32;
        let mut sum_r = 0.0f32;
        for _ in 0..1000 {
            let (l, r) = engine.next_sample_stereo();
            sum_l += l.abs();
            sum_r += r.abs();
        }
        // Hard right: left should be near zero, right should have signal
        assert!(sum_l < 0.01, "Left should be near-silent for hard-right pan, got {sum_l}");
        assert!(sum_r > 1.0, "Right should have signal for hard-right pan, got {sum_r}");
    }

    #[test]
    fn filter_reduces_brightness() {
        let mut bank = SampleBank::new();
        // White-ish noise: alternating +1, -1
        let noise: Vec<f32> = (0..44100).map(|i| if i % 2 == 0 { 1.0 } else { -1.0 }).collect();
        let id = bank.add(Sample::from_mono(noise, 44100));

        // No filter
        let mut inst_no_filter = Instrument::new("no_filter");
        inst_no_filter.add_zone(Zone::new(id).with_key_range(0, 127).with_root_note(69));

        let mut engine1 = SamplerEngine::new(1, 44100.0);
        engine1.set_bank(bank.clone());
        engine1.set_instrument(inst_no_filter);
        engine1.note_on(69, 127);

        let mut sum_unfiltered = 0.0f32;
        for _ in 0..1000 {
            sum_unfiltered += engine1.next_sample().abs();
        }

        // With heavy filter (100 Hz cutoff)
        let mut inst_filter = Instrument::new("filtered");
        inst_filter.add_zone(
            Zone::new(id)
                .with_key_range(0, 127)
                .with_root_note(69)
                .with_filter(100.0, 0.0), // 100Hz, no vel track
        );

        let mut engine2 = SamplerEngine::new(1, 44100.0);
        engine2.set_bank(bank);
        engine2.set_instrument(inst_filter);
        engine2.note_on(69, 127);

        let mut sum_filtered = 0.0f32;
        for _ in 0..1000 {
            sum_filtered += engine2.next_sample().abs();
        }

        assert!(
            sum_filtered < sum_unfiltered,
            "Filtered output ({sum_filtered}) should be quieter than unfiltered ({sum_unfiltered})"
        );
    }

    #[test]
    fn fill_buffer_stereo() {
        let mut engine = make_engine();
        engine.note_on(69, 100);

        let mut buf = vec![0.0f32; 200]; // 100 stereo frames
        engine.fill_buffer_stereo(&mut buf);
        assert!(buf.iter().any(|&s| s.abs() > 0.01));
    }

    #[test]
    fn all_notes_off_releases_all() {
        let mut engine = make_engine();
        engine.note_on(69, 100);
        engine.note_on(72, 100);
        assert_eq!(engine.active_voice_count(), 2);

        engine.all_notes_off();

        for _ in 0..44100 {
            engine.next_sample();
        }
        assert_eq!(engine.active_voice_count(), 0);
    }
}

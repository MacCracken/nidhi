//! Sampler engine — polyphonic sample playback with voice management.

use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

use crate::envelope::{AdsrConfig, AmpEnvelope};
use crate::instrument::Instrument;
use crate::loop_mode::LoopMode;
use crate::sample::SampleBank;
use crate::zone::FilterMode;

// ---------------------------------------------------------------------------
// VoiceFilter — per-voice stereo filter (SVF when std, one-pole fallback)
// ---------------------------------------------------------------------------

/// Per-voice stereo filter.
///
/// With `std`: two [`naad::filter::StateVariableFilter`] instances (true stereo).
/// Without `std`: two one-pole lowpass states (lightweight fallback).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct VoiceFilter {
    /// Whether filtering is active (cutoff > 0).
    active: bool,

    #[cfg(feature = "std")]
    filter_l: Option<naad::filter::StateVariableFilter>,
    #[cfg(feature = "std")]
    filter_r: Option<naad::filter::StateVariableFilter>,
    #[cfg(feature = "std")]
    mode: FilterMode,

    #[cfg(not(feature = "std"))]
    state_l: f32,
    #[cfg(not(feature = "std"))]
    state_r: f32,
    #[cfg(not(feature = "std"))]
    coeff: f32,
}

impl VoiceFilter {
    /// Create a disabled filter (bypass).
    fn bypass() -> Self {
        Self {
            active: false,
            #[cfg(feature = "std")]
            filter_l: None,
            #[cfg(feature = "std")]
            filter_r: None,
            #[cfg(feature = "std")]
            mode: FilterMode::LowPass,
            #[cfg(not(feature = "std"))]
            state_l: 0.0,
            #[cfg(not(feature = "std"))]
            state_r: 0.0,
            #[cfg(not(feature = "std"))]
            coeff: 1.0,
        }
    }

    /// Create a filter from zone settings and velocity.
    fn new(
        cutoff: f32,
        resonance: f32,
        mode: FilterMode,
        vel_track: f32,
        velocity: u8,
        sample_rate: f32,
    ) -> Self {
        if cutoff <= 0.0 {
            return Self::bypass();
        }

        let vel_norm = velocity as f32 / 127.0;
        let effective_cutoff = cutoff * (1.0 - vel_track * (1.0 - vel_norm));
        // Clamp to valid range for SVF
        let effective_cutoff = effective_cutoff.clamp(20.0, sample_rate * 0.49);

        #[cfg(feature = "std")]
        {
            let q = resonance.max(0.1);
            let fl = naad::filter::StateVariableFilter::new(effective_cutoff, q, sample_rate).ok();
            let fr = naad::filter::StateVariableFilter::new(effective_cutoff, q, sample_rate).ok();
            Self {
                active: fl.is_some(),
                filter_l: fl,
                filter_r: fr,
                mode,
            }
        }

        #[cfg(not(feature = "std"))]
        {
            let _ = (resonance, mode);
            let coeff = 1.0 - (-core::f32::consts::TAU * effective_cutoff / sample_rate).exp();
            Self {
                active: true,
                state_l: 0.0,
                state_r: 0.0,
                coeff: coeff.clamp(0.0, 1.0),
            }
        }
    }

    /// Update the filter cutoff frequency (for envelope modulation).
    #[inline]
    fn set_cutoff(&mut self, cutoff: f32, sample_rate: f32) {
        if !self.active {
            return;
        }
        let cutoff = cutoff.clamp(20.0, sample_rate * 0.49);

        #[cfg(feature = "std")]
        {
            if let Some(f) = self.filter_l.as_mut() {
                let _ = f.set_params(cutoff, f.q());
            }
            if let Some(f) = self.filter_r.as_mut() {
                let _ = f.set_params(cutoff, f.q());
            }
        }

        #[cfg(not(feature = "std"))]
        {
            self.coeff =
                (1.0 - (-core::f32::consts::TAU * cutoff / sample_rate).exp()).clamp(0.0, 1.0);
        }
    }

    /// Process a stereo sample pair through the filter.
    #[inline]
    fn process_stereo(&mut self, left: f32, right: f32) -> (f32, f32) {
        if !self.active {
            return (left, right);
        }

        #[cfg(feature = "std")]
        {
            let l = self.filter_l.as_mut().map_or(left, |f| {
                let out = f.process_sample(left);
                match self.mode {
                    FilterMode::LowPass => out.low_pass,
                    FilterMode::HighPass => out.high_pass,
                    FilterMode::BandPass => out.band_pass,
                    FilterMode::Notch => out.notch,
                }
            });
            let r = self.filter_r.as_mut().map_or(right, |f| {
                let out = f.process_sample(right);
                match self.mode {
                    FilterMode::LowPass => out.low_pass,
                    FilterMode::HighPass => out.high_pass,
                    FilterMode::BandPass => out.band_pass,
                    FilterMode::Notch => out.notch,
                }
            });
            (l, r)
        }

        #[cfg(not(feature = "std"))]
        {
            self.state_l += self.coeff * (left - self.state_l);
            self.state_r += self.coeff * (right - self.state_r);
            (self.state_l, self.state_r)
        }
    }
}

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
    /// Per-voice amplitude envelope.
    amp_env: AmpEnvelope,
    /// Per-voice filter envelope (modulates cutoff).
    filter_env: Option<AmpEnvelope>,
    /// Filter envelope depth in cents.
    filter_env_depth: f32,
    /// Base filter cutoff (before envelope modulation).
    base_cutoff: f32,
    /// Per-voice stereo filter.
    filter: VoiceFilter,
}

impl SamplerVoice {
    fn new(sample_rate: f32) -> Self {
        Self {
            active: false,
            zone_index: 0,
            position: 0.0,
            speed: 1.0,
            amplitude: 1.0,
            note: 0,
            age: 0,
            forward: true,
            amp_env: AmpEnvelope::new(&AdsrConfig::default(), sample_rate),
            filter_env: None,
            filter_env_depth: 0.0,
            base_cutoff: 0.0,
            filter: VoiceFilter::bypass(),
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
}

/// Polyphonic sampler engine with voice stealing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct SamplerEngine {
    voices: Vec<SamplerVoice>,
    instrument: Option<Instrument>,
    bank: SampleBank,
    sample_rate: f32,
    /// Default ADSR envelope configuration (used when zone has no per-zone ADSR).
    default_adsr: AdsrConfig,
}

impl SamplerEngine {
    /// Create a new sampler engine with the given voice count.
    pub fn new(max_voices: usize, sample_rate: f32) -> Self {
        Self {
            voices: (0..max_voices)
                .map(|_| SamplerVoice::new(sample_rate))
                .collect(),
            instrument: None,
            bank: SampleBank::new(),
            sample_rate,
            default_adsr: AdsrConfig {
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

    /// Set the default ADSR envelope configuration.
    pub fn set_adsr(&mut self, adsr: AdsrConfig) {
        self.default_adsr = adsr;
    }

    /// Set release time in milliseconds (convenience — updates default ADSR release only).
    pub fn set_release_ms(&mut self, ms: f32) {
        self.default_adsr.release_samples = (self.sample_rate * ms / 1000.0).max(1.0) as u32;
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
        let amp = zone.velocity_curve().apply(velocity);

        // Build filter from zone settings
        let voice_filter = VoiceFilter::new(
            zone.filter_cutoff(),
            zone.filter_resonance(),
            zone.filter_type(),
            zone.filter_vel_track(),
            velocity,
            self.sample_rate,
        );

        // Resolve ADSR: per-zone overrides engine default
        let adsr_config = zone.adsr().copied().unwrap_or(self.default_adsr);

        // Find a free voice, or steal the oldest
        let voice_idx = self.voices.iter().position(|v| !v.active).or_else(|| {
            self.voices
                .iter()
                .enumerate()
                .max_by_key(|(_, v)| v.age)
                .map(|(i, _)| i)
        })?;

        let voice = &mut self.voices[voice_idx];
        voice.active = true;
        voice.zone_index = zone_idx;
        voice.position = zone.sample_offset() as f64;
        voice.speed = speed;
        voice.amplitude = amp;
        voice.note = note;
        voice.age = 0;
        voice.forward = true;
        voice.filter = voice_filter;
        voice.base_cutoff = zone.filter_cutoff();

        // Setup filter envelope if zone has one
        if let Some(fileg_config) = zone.fileg() {
            let mut fenv = AmpEnvelope::new(fileg_config, self.sample_rate);
            fenv.trigger();
            voice.filter_env = Some(fenv);
            voice.filter_env_depth = zone.fileg_depth();
        } else {
            voice.filter_env = None;
            voice.filter_env_depth = 0.0;
        }

        // Create and trigger amplitude envelope
        voice.amp_env = AmpEnvelope::new(&adsr_config, self.sample_rate);
        voice.amp_env.trigger();

        Some(voice_idx)
    }

    /// Release a note.
    pub fn note_off(&mut self, note: u8) {
        for voice in &mut self.voices {
            if voice.active && voice.note == note && voice.amp_env.is_active() {
                voice.amp_env.release();
                if let Some(ref mut fenv) = voice.filter_env {
                    fenv.release();
                }
            }
        }
    }

    /// Release all notes.
    pub fn all_notes_off(&mut self) {
        for voice in &mut self.voices {
            if voice.active && voice.amp_env.is_active() {
                voice.amp_env.release();
                if let Some(ref mut fenv) = voice.filter_env {
                    fenv.release();
                }
            }
        }
    }

    /// Advance a voice's position according to its loop mode. Returns false if voice should deactivate.
    #[inline]
    fn advance_position(
        voice: &mut SamplerVoice,
        loop_mode: LoopMode,
        loop_start: usize,
        loop_end: usize,
        frames: usize,
        released: bool,
    ) -> bool {
        let effective_end = if loop_end > 0 {
            loop_end as f64
        } else {
            frames as f64
        };

        match loop_mode {
            LoopMode::OneShot => {
                voice.position += voice.speed;
                if voice.position >= frames as f64 {
                    return false;
                }
            }
            LoopMode::Forward => {
                voice.position += voice.speed;
                if voice.position >= effective_end {
                    voice.position = loop_start as f64;
                }
            }
            LoopMode::PingPong => {
                if voice.forward {
                    voice.position += voice.speed;
                    if voice.position >= effective_end {
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
            LoopMode::LoopSustain => {
                voice.position += voice.speed;
                if released {
                    // Play through to end of sample (like OneShot)
                    if voice.position >= frames as f64 {
                        return false;
                    }
                } else {
                    // Loop while held (like Forward)
                    if voice.position >= effective_end {
                        voice.position = loop_start as f64;
                    }
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

            // Determine effective sample end
            let effective_frames = if zone.sample_end() > 0 {
                zone.sample_end().min(sample.frames())
            } else {
                sample.frames()
            };

            // Read stereo interpolated sample
            let (mut sl, mut sr) = sample.read_stereo_interpolated(voice.position);

            // Crossfade at loop boundary
            let xfade = zone.crossfade_length();
            if xfade > 0 && matches!(zone.loop_mode(), LoopMode::Forward | LoopMode::LoopSustain) {
                let loop_end_f = if zone.loop_end > 0 {
                    zone.loop_end as f64
                } else {
                    effective_frames as f64
                };
                let xfade_f = xfade as f64;
                let dist_to_end = loop_end_f - voice.position;
                if dist_to_end >= 0.0 && dist_to_end < xfade_f {
                    let t = (dist_to_end / xfade_f) as f32; // 1.0 at start of xfade, 0.0 at end
                    let xfade_pos = zone.loop_start as f64 + (xfade_f - dist_to_end);
                    let (xl, xr) = sample.read_stereo_interpolated(xfade_pos);
                    sl = sl * t + xl * (1.0 - t);
                    sr = sr * t + xr * (1.0 - t);
                }
            }

            // Modulate filter cutoff via filter envelope
            if let Some(ref mut fenv) = voice.filter_env {
                let env_val = fenv.tick();
                if voice.base_cutoff > 0.0 {
                    let mod_cents = voice.filter_env_depth * env_val;
                    let mod_ratio = 2.0_f32.powf(mod_cents / 1200.0);
                    let modulated = voice.base_cutoff * mod_ratio;
                    voice.filter.set_cutoff(modulated, self.sample_rate);
                }
            }

            // Apply filter
            let (fl, fr) = voice.filter.process_stereo(sl, sr);
            sl = fl;
            sr = fr;

            // Tick ADSR
            let env = voice.amp_env.tick();

            if !voice.amp_env.is_active() {
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
            if !Self::advance_position(
                voice,
                zone.loop_mode(),
                zone.loop_start,
                zone.loop_end,
                effective_frames,
                voice.amp_env.is_releasing(),
            ) {
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
        assert!(
            sum_l < 0.01,
            "Left should be near-silent for hard-right pan, got {sum_l}"
        );
        assert!(
            sum_r > 1.0,
            "Right should have signal for hard-right pan, got {sum_r}"
        );
    }

    #[test]
    fn filter_reduces_brightness() {
        let mut bank = SampleBank::new();
        // White-ish noise: alternating +1, -1
        let noise: Vec<f32> = (0..44100)
            .map(|i| if i % 2 == 0 { 1.0 } else { -1.0 })
            .collect();
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

    #[test]
    fn per_zone_adsr_overrides_engine_default() {
        let mut bank = SampleBank::new();
        let sine: Vec<f32> = (0..44100)
            .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 44100.0).sin())
            .collect();
        let id = bank.add(Sample::from_mono(sine, 44100));

        // Zone with slow attack (500 samples)
        let slow_adsr = AdsrConfig {
            attack_samples: 500,
            decay_samples: 0,
            sustain_level: 1.0,
            release_samples: 100,
        };

        let mut inst = Instrument::new("test");
        inst.add_zone(
            Zone::new(id)
                .with_key_range(0, 127)
                .with_root_note(69)
                .with_adsr(slow_adsr),
        );

        // Engine default has zero attack
        let mut engine = SamplerEngine::new(8, 44100.0);
        engine.set_bank(bank);
        engine.set_instrument(inst);

        engine.note_on(69, 127);

        // First few samples should be quiet due to slow attack
        let first = engine.next_sample().abs();
        for _ in 0..249 {
            engine.next_sample();
        }
        let mid = engine.next_sample().abs();
        assert!(
            mid > first,
            "Per-zone slow attack: mid={mid} should be louder than first={first}"
        );
    }
}

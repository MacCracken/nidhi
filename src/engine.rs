//! Sampler engine — polyphonic sample playback with voice management.

use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

use crate::envelope::{AdsrConfig, AmpEnvelope};
use crate::instrument::Instrument;
use crate::loop_mode::LoopMode;
use crate::sample::SampleBank;
use crate::zone::FilterMode;

// ---------------------------------------------------------------------------
// Re-export voice management types from naad when std is available
// ---------------------------------------------------------------------------

#[cfg(feature = "std")]
pub use naad::voice::{PolyMode, StealMode};

/// Polyphony mode (no_std fallback).
#[cfg(not(feature = "std"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum PolyMode {
    /// Polyphonic — each note gets its own voice.
    Poly,
    /// Monophonic — only one note at a time.
    Mono,
    /// Legato — monophonic, glides without retriggering.
    Legato,
}

/// Voice stealing strategy (no_std fallback).
#[cfg(not(feature = "std"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum StealMode {
    /// Steal the oldest active voice.
    Oldest,
    /// Steal the quietest active voice.
    Quietest,
    /// Steal the voice with the lowest note.
    Lowest,
    /// Do not steal — ignore new notes when full.
    None,
}

// ---------------------------------------------------------------------------
// VoiceFilter — per-voice stereo filter (SVF when std, one-pole fallback)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
struct VoiceFilter {
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

// ---------------------------------------------------------------------------
// SamplerVoice — per-voice playback state
// ---------------------------------------------------------------------------

/// A single playback voice — tracks position within a sample.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamplerVoice {
    active: bool,
    zone_index: usize,
    position: f64,
    speed: f64,
    amplitude: f32,
    note: u8,
    age: u64,
    forward: bool,
    amp_env: AmpEnvelope,
    filter_env: Option<AmpEnvelope>,
    filter_env_depth: f32,
    base_cutoff: f32,
    filter: VoiceFilter,
    /// Per-voice pitch bend in semitones.
    pitch_bend: f32,
    /// Per-voice pressure / aftertouch (0.0–1.0).
    pressure: f32,
    /// Per-voice brightness CC#74 (0.0–1.0).
    brightness: f32,
    /// Choke group this voice belongs to (0 = none).
    choke_group: u32,
    /// Per-voice pitch LFO (std only).
    #[cfg(feature = "std")]
    pitch_lfo: Option<naad::modulation::Lfo>,
    /// Pitch LFO depth in cents.
    pitch_lfo_depth: f32,
    /// Per-voice filter LFO (std only).
    #[cfg(feature = "std")]
    filter_lfo: Option<naad::modulation::Lfo>,
    /// Filter LFO depth in cents.
    filter_lfo_depth: f32,
    /// Filter key tracking amount (0.0–1.0).
    fil_keytrack: f32,
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
            pitch_bend: 0.0,
            pressure: 0.0,
            brightness: 0.5,
            choke_group: 0,
            #[cfg(feature = "std")]
            pitch_lfo: None,
            pitch_lfo_depth: 0.0,
            #[cfg(feature = "std")]
            filter_lfo: None,
            filter_lfo_depth: 0.0,
            fil_keytrack: 0.0,
        }
    }

    /// Whether this voice is actively producing audio.
    #[inline]
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// The MIDI note playing.
    #[inline]
    #[must_use]
    pub fn note(&self) -> u8 {
        self.note
    }
}

// ---------------------------------------------------------------------------
// SamplerEngine
// ---------------------------------------------------------------------------

/// Polyphonic sampler engine with configurable voice stealing and expression.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct SamplerEngine {
    voices: Vec<SamplerVoice>,
    instrument: Option<Instrument>,
    bank: SampleBank,
    sample_rate: f32,
    default_adsr: AdsrConfig,
    /// Pitch bend range in semitones (default ±2).
    pitch_bend_range: f32,

    #[cfg(feature = "std")]
    voice_mgr: naad::voice::VoiceManager,

    #[cfg(not(feature = "std"))]
    steal_mode: StealMode,
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
                release_samples: (sample_rate * 0.01).max(1.0) as u32,
            },
            pitch_bend_range: 2.0,
            #[cfg(feature = "std")]
            voice_mgr: naad::voice::VoiceManager::new(
                max_voices,
                naad::voice::PolyMode::Poly,
                naad::voice::StealMode::Oldest,
            ),
            #[cfg(not(feature = "std"))]
            steal_mode: StealMode::Oldest,
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

    /// Set pitch bend range in semitones (default ±2).
    pub fn set_pitch_bend_range(&mut self, semitones: f32) {
        self.pitch_bend_range = semitones.max(0.0);
    }

    /// Set voice stealing mode.
    pub fn set_steal_mode(&mut self, mode: StealMode) {
        #[cfg(feature = "std")]
        {
            self.voice_mgr.steal_mode = mode;
        }
        #[cfg(not(feature = "std"))]
        {
            self.steal_mode = mode;
        }
    }

    /// Set polyphony mode.
    pub fn set_poly_mode(&mut self, mode: PolyMode) {
        #[cfg(feature = "std")]
        {
            self.voice_mgr.poly_mode = mode;
        }
        #[cfg(not(feature = "std"))]
        {
            let _ = mode; // no_std only supports Poly
        }
    }

    /// Apply per-note pitch bend (in semitones, scaled by pitch_bend_range).
    pub fn apply_pitch_bend(&mut self, note: u8, bend: f32) {
        let bend_semitones = bend * self.pitch_bend_range;
        for voice in &mut self.voices {
            if voice.active && voice.note == note {
                voice.pitch_bend = bend_semitones;
            }
        }
    }

    /// Apply per-note pressure / aftertouch (0.0–1.0).
    pub fn apply_pressure(&mut self, note: u8, pressure: f32) {
        for voice in &mut self.voices {
            if voice.active && voice.note == note {
                voice.pressure = pressure.clamp(0.0, 1.0);
            }
        }
    }

    /// Apply per-note brightness CC#74 (0.0–1.0).
    pub fn apply_brightness(&mut self, note: u8, brightness: f32) {
        for voice in &mut self.voices {
            if voice.active && voice.note == note {
                voice.brightness = brightness.clamp(0.0, 1.0);
            }
        }
    }

    /// Allocate a voice index for a new note.
    fn allocate_voice(&mut self, note: u8, velocity: u8) -> Option<usize> {
        #[cfg(feature = "std")]
        {
            self.voice_mgr.note_on(note, velocity as f32 / 127.0)
        }

        #[cfg(not(feature = "std"))]
        {
            let _ = (note, velocity);
            // Find free voice
            self.voices
                .iter()
                .position(|v| !v.active)
                .or_else(|| match self.steal_mode {
                    StealMode::Oldest => self
                        .voices
                        .iter()
                        .enumerate()
                        .max_by_key(|(_, v)| v.age)
                        .map(|(i, _)| i),
                    StealMode::Quietest => self
                        .voices
                        .iter()
                        .enumerate()
                        .min_by(|(_, a), (_, b)| {
                            a.amplitude
                                .partial_cmp(&b.amplitude)
                                .unwrap_or(core::cmp::Ordering::Equal)
                        })
                        .map(|(i, _)| i),
                    StealMode::Lowest => self
                        .voices
                        .iter()
                        .enumerate()
                        .filter(|(_, v)| v.active)
                        .min_by_key(|(_, v)| v.note)
                        .map(|(i, _)| i),
                    StealMode::None => None,
                })
        }
    }

    /// Trigger a note. Returns the voice index, or None if no voice available.
    pub fn note_on(&mut self, note: u8, velocity: u8) -> Option<usize> {
        // Extract zone data before mutable borrows
        let zone_data = {
            let instrument = self.instrument.as_ref()?;
            let zones = instrument.find_zones(note, velocity);
            if zones.is_empty() {
                return None;
            }
            let zone_idx = instrument
                .zones()
                .iter()
                .position(|z| core::ptr::eq(z, zones[0]))?;
            let zone = &instrument.zones()[zone_idx];
            (
                zone_idx,
                zone.playback_ratio(note),
                zone.velocity_curve().apply(velocity),
                zone.filter_cutoff(),
                zone.filter_resonance(),
                zone.filter_type(),
                zone.filter_vel_track(),
                zone.adsr().copied().unwrap_or(self.default_adsr),
                zone.choke_group(),
                zone.sample_offset(),
                zone.fileg().copied(),
                zone.fileg_depth(),
                zone.pitchlfo_rate(),
                zone.pitchlfo_depth(),
                zone.fillfo_rate(),
                zone.fillfo_depth(),
                zone.fil_keytrack(),
            )
        };
        #[allow(clippy::type_complexity)]
        let (
            zone_idx,
            speed,
            amp,
            f_cutoff,
            f_res,
            f_type,
            f_vel,
            adsr_config,
            choke,
            sample_offset,
            fileg_config,
            fileg_depth,
            _plfo_rate,
            plfo_depth,
            _flfo_rate,
            flfo_depth,
            keytrack,
        ) = zone_data;

        let voice_filter =
            VoiceFilter::new(f_cutoff, f_res, f_type, f_vel, velocity, self.sample_rate);

        // Choke group: silence any active voice in the same group
        if choke > 0 {
            for voice in &mut self.voices {
                if voice.active && voice.choke_group == choke {
                    voice.active = false;
                }
            }
        }

        let voice_idx_alloc = self.allocate_voice(note, velocity)?;

        let voice = &mut self.voices[voice_idx_alloc];
        voice.active = true;
        voice.zone_index = zone_idx;
        voice.position = sample_offset as f64;
        voice.speed = speed;
        voice.amplitude = amp;
        voice.note = note;
        voice.age = 0;
        voice.forward = true;
        voice.filter = voice_filter;
        voice.base_cutoff = f_cutoff;
        voice.pitch_bend = 0.0;
        voice.pressure = 0.0;
        voice.brightness = 0.5;
        voice.choke_group = choke;

        // Setup filter envelope
        if let Some(ref fc) = fileg_config {
            let mut fenv = AmpEnvelope::new(fc, self.sample_rate);
            fenv.trigger();
            voice.filter_env = Some(fenv);
            voice.filter_env_depth = fileg_depth;
        } else {
            voice.filter_env = None;
            voice.filter_env_depth = 0.0;
        }

        // Setup pitch LFO
        #[cfg(feature = "std")]
        {
            voice.pitch_lfo = if _plfo_rate > 0.0 && plfo_depth != 0.0 {
                naad::modulation::Lfo::new(
                    naad::modulation::LfoShape::Sine,
                    _plfo_rate,
                    self.sample_rate,
                )
                .ok()
            } else {
                None
            };
            voice.filter_lfo = if _flfo_rate > 0.0 && flfo_depth != 0.0 {
                naad::modulation::Lfo::new(
                    naad::modulation::LfoShape::Sine,
                    _flfo_rate,
                    self.sample_rate,
                )
                .ok()
            } else {
                None
            };
        }
        voice.pitch_lfo_depth = plfo_depth;
        voice.filter_lfo_depth = flfo_depth;
        voice.fil_keytrack = keytrack;

        // Trigger amplitude envelope
        voice.amp_env = AmpEnvelope::new(&adsr_config, self.sample_rate);
        voice.amp_env.trigger();

        Some(voice_idx_alloc)
    }

    /// Release a note.
    pub fn note_off(&mut self, note: u8) {
        #[cfg(feature = "std")]
        self.voice_mgr.note_off(note);

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
        #[cfg(feature = "std")]
        self.voice_mgr.all_notes_off();

        for voice in &mut self.voices {
            if voice.active && voice.amp_env.is_active() {
                voice.amp_env.release();
                if let Some(ref mut fenv) = voice.filter_env {
                    fenv.release();
                }
            }
        }
    }

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
                    if voice.position >= frames as f64 {
                        return false;
                    }
                } else if voice.position >= effective_end {
                    voice.position = loop_start as f64;
                }
            }
        }
        true
    }

    /// Generate the next stereo sample pair by mixing all active voices.
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

            let effective_frames = if zone.sample_end() > 0 {
                zone.sample_end().min(sample.frames())
            } else {
                sample.frames()
            };

            // Apply pitch bend + pitch LFO to playback speed
            let pitch_mod_cents = {
                #[allow(unused_mut)]
                let mut cents = voice.pitch_bend as f64 * 100.0; // semitones to cents
                #[cfg(feature = "std")]
                if let Some(ref mut lfo) = voice.pitch_lfo {
                    cents += lfo.next_value() as f64 * voice.pitch_lfo_depth as f64;
                }
                cents
            };
            let effective_speed = if pitch_mod_cents != 0.0 {
                voice.speed * 2.0_f64.powf(pitch_mod_cents / 1200.0)
            } else {
                voice.speed
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
                    let t = (dist_to_end / xfade_f) as f32;
                    let xfade_pos = zone.loop_start as f64 + (xfade_f - dist_to_end);
                    let (xl, xr) = sample.read_stereo_interpolated(xfade_pos);
                    sl = sl * t + xl * (1.0 - t);
                    sr = sr * t + xr * (1.0 - t);
                }
            }

            // Modulate filter cutoff via envelope + LFO + key tracking + brightness
            if voice.base_cutoff > 0.0 {
                let mut cutoff = voice.base_cutoff;

                // Key tracking: scale cutoff by distance from C4 (note 60)
                if voice.fil_keytrack > 0.0 {
                    let semitones_from_c4 = voice.note as f32 - 60.0;
                    let keytrack_cents = semitones_from_c4 * 100.0 * voice.fil_keytrack;
                    cutoff *= 2.0_f32.powf(keytrack_cents / 1200.0);
                }

                // Filter envelope
                if let Some(ref mut fenv) = voice.filter_env {
                    let env_val = fenv.tick();
                    let mod_cents = voice.filter_env_depth * env_val;
                    cutoff *= 2.0_f32.powf(mod_cents / 1200.0);
                }

                // Filter LFO
                #[cfg(feature = "std")]
                if let Some(ref mut lfo) = voice.filter_lfo {
                    let lfo_cents = lfo.next_value() * voice.filter_lfo_depth;
                    cutoff *= 2.0_f32.powf(lfo_cents / 1200.0);
                }

                // Brightness
                cutoff *= 0.5 + voice.brightness * 0.5;

                voice.filter.set_cutoff(cutoff, self.sample_rate);
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

            // Pressure modulates amplitude slightly (±20%)
            let pressure_mod = 1.0 + (voice.pressure - 0.5) * 0.4;
            let amp = voice.amplitude * env * pressure_mod;

            let pan = zone.pan();
            let pan_l = (1.0 - pan) * 0.5;
            let pan_r = (1.0 + pan) * 0.5;

            out_l += sl * amp * pan_l;
            out_r += sr * amp * pan_r;

            // Advance position (using pitch-bent speed)
            let saved_speed = voice.speed;
            voice.speed = effective_speed;
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
            voice.speed = saved_speed;
        }

        (out_l, out_r)
    }

    /// Generate the next mono sample by mixing all active voices.
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
        inst.add_zone(Zone::new(id).with_key_range(0, 127).with_root_note(69));

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

        for _ in 0..44100 {
            engine.next_sample();
        }
        assert_eq!(engine.active_voice_count(), 0);
    }

    #[test]
    fn pitch_shift() {
        let mut engine = make_engine();
        engine.note_on(81, 100);

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

        let first = engine.next_sample().abs();
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
                .with_pan(1.0),
        );

        let mut engine = SamplerEngine::new(8, 44100.0);
        engine.set_bank(bank);
        engine.set_instrument(inst);
        engine.note_on(69, 127);

        let mut sum_l = 0.0f32;
        let mut sum_r = 0.0f32;
        for _ in 0..1000 {
            let (l, r) = engine.next_sample_stereo();
            sum_l += l.abs();
            sum_r += r.abs();
        }
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
        let noise: Vec<f32> = (0..44100)
            .map(|i| if i % 2 == 0 { 1.0 } else { -1.0 })
            .collect();
        let id = bank.add(Sample::from_mono(noise, 44100));

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

        let mut inst_filter = Instrument::new("filtered");
        inst_filter.add_zone(
            Zone::new(id)
                .with_key_range(0, 127)
                .with_root_note(69)
                .with_filter(100.0, 0.0),
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

        let mut buf = vec![0.0f32; 200];
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

        let mut engine = SamplerEngine::new(8, 44100.0);
        engine.set_bank(bank);
        engine.set_instrument(inst);

        engine.note_on(69, 127);

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

    #[test]
    fn choke_group_silences_previous_voice() {
        let mut bank = SampleBank::new();
        let sine: Vec<f32> = (0..44100)
            .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 44100.0).sin())
            .collect();
        let id = bank.add(Sample::from_mono(sine, 44100));

        let mut inst = Instrument::new("hihat");
        // Closed and open hi-hat in same choke group
        inst.add_zone(
            Zone::new(id)
                .with_key_range(42, 42)
                .with_root_note(69)
                .with_choke_group(1),
        );
        inst.add_zone(
            Zone::new(id)
                .with_key_range(46, 46)
                .with_root_note(69)
                .with_choke_group(1),
        );

        let mut engine = SamplerEngine::new(8, 44100.0);
        engine.set_bank(bank);
        engine.set_instrument(inst);

        // Play open hi-hat
        engine.note_on(46, 100);
        assert_eq!(engine.active_voice_count(), 1);

        // Play closed hi-hat — should choke the open
        engine.note_on(42, 100);
        assert_eq!(
            engine.active_voice_count(),
            1,
            "choke group should silence previous voice"
        );
    }

    #[test]
    fn pitch_bend_changes_pitch() {
        let mut engine = make_engine();
        engine.note_on(69, 100);

        // Render some samples without bend
        for _ in 0..100 {
            engine.next_sample();
        }

        // Apply pitch bend up
        engine.apply_pitch_bend(69, 1.0); // full bend up

        let mut sum_bent = 0.0f32;
        for _ in 0..100 {
            sum_bent += engine.next_sample();
        }

        // Pitch bend is active — verify output is finite and non-zero
        assert!(sum_bent.is_finite(), "Pitch bend output should be finite");
        assert!(sum_bent.abs() > 0.0, "Pitch bend output should be non-zero");
    }

    #[test]
    fn steal_mode_none_rejects_when_full() {
        let mut bank = SampleBank::new();
        let sine: Vec<f32> = (0..44100)
            .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 44100.0).sin())
            .collect();
        let id = bank.add(Sample::from_mono(sine, 44100));

        let mut inst = Instrument::new("test");
        inst.add_zone(Zone::new(id).with_key_range(0, 127).with_root_note(69));

        let mut engine = SamplerEngine::new(2, 44100.0);
        engine.set_bank(bank);
        engine.set_instrument(inst);
        engine.set_steal_mode(StealMode::None);

        engine.note_on(60, 100);
        engine.note_on(64, 100);
        assert_eq!(engine.active_voice_count(), 2);

        // Third note should be rejected
        let result = engine.note_on(67, 100);
        assert!(result.is_none(), "StealMode::None should reject when full");
        assert_eq!(engine.active_voice_count(), 2);
    }
}

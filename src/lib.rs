//! # nidhi — Sample Playback Engine
//!
//! **nidhi** (Sanskrit: treasure, storehouse) provides sample-based instrument playback
//! for the AGNOS ecosystem. It handles sample mapping, key/velocity zones,
//! loop modes, time-stretching, and format import (SFZ/SF2).
//!
//! ## Architecture
//!
//! ```text
//! SampleBank (loaded samples)
//!       |
//!       v
//! Instrument (key/velocity zones → sample mapping)
//!       |
//!       v
//! SamplerVoice (playback with interpolation, looping, envelope)
//!       |
//!       v
//! SamplerEngine (polyphonic voice pool)
//! ```
//!
//! ## Key Concepts
//!
//! - **Sample**: A loaded audio waveform (mono or stereo f32 data)
//! - **Zone**: A key/velocity region mapped to a sample with root note, tuning, and loop points
//! - **Instrument**: A collection of zones forming a playable instrument
//! - **SamplerEngine**: Polyphonic playback engine with voice stealing
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use nidhi::prelude::*;
//!
//! // Load a sample and create an instrument
//! let sample = Sample::from_mono(vec![0.0f32; 44100], 44100);
//! let mut bank = SampleBank::new();
//! let id = bank.add(sample);
//!
//! let mut inst = Instrument::new("piano");
//! inst.add_zone(Zone::new(id).with_key_range(60, 72).with_root_note(66));
//!
//! // Create engine and play
//! let mut engine = SamplerEngine::new(16, 44100.0);
//! engine.set_instrument(inst);
//! engine.note_on(66, 100); // middle C#, velocity 100
//! ```

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub mod capture;
pub mod effect_chain;
pub mod engine;
pub mod envelope;
pub mod error;
pub mod instrument;
pub mod loop_mode;
pub mod sample;
pub mod sf2;
pub mod sfz;
pub mod stretch;
pub mod zone;

/// Convenience re-exports for common usage.
pub mod prelude {
    pub use crate::capture::SampleRecorder;
    pub use crate::effect_chain::{EffectChain, EffectType};
    pub use crate::engine::{PolyMode, SamplerEngine, StealMode};
    pub use crate::envelope::{AdsrConfig, AmpEnvelope, EnvState};
    pub use crate::error::{NidhiError, Result};
    pub use crate::instrument::Instrument;
    pub use crate::loop_mode::LoopMode;
    pub use crate::sample::{Sample, SampleBank, SampleId};
    pub use crate::sf2::Sf2Preset;
    pub use crate::sfz::SfzFile;
    pub use crate::zone::{FilterMode, VelocityCurve, Zone};
}

#[cfg(test)]
mod assert_traits {
    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn public_types_are_send_sync() {
        _assert_send_sync::<crate::error::NidhiError>();
        _assert_send_sync::<crate::sample::Sample>();
        _assert_send_sync::<crate::sample::SampleBank>();
        _assert_send_sync::<crate::sf2::Sf2Preset>();
        _assert_send_sync::<crate::zone::FilterMode>();
        _assert_send_sync::<crate::zone::VelocityCurve>();
        _assert_send_sync::<crate::zone::Zone>();
        _assert_send_sync::<crate::instrument::Instrument>();
        _assert_send_sync::<crate::engine::SamplerEngine>();
        _assert_send_sync::<crate::capture::SampleRecorder>();
        _assert_send_sync::<crate::effect_chain::EffectChain>();
        _assert_send_sync::<crate::effect_chain::EffectType>();
        _assert_send_sync::<crate::engine::PolyMode>();
        _assert_send_sync::<crate::engine::StealMode>();
        _assert_send_sync::<crate::engine::SamplerVoice>();
        _assert_send_sync::<crate::envelope::AdsrConfig>();
        _assert_send_sync::<crate::envelope::AmpEnvelope>();
        _assert_send_sync::<crate::envelope::EnvState>();
        _assert_send_sync::<crate::loop_mode::LoopMode>();
        _assert_send_sync::<crate::sample::SampleId>();
        _assert_send_sync::<crate::sfz::SfzFile>();
        _assert_send_sync::<crate::sfz::SfzRegion>();
        _assert_send_sync::<crate::stretch::StretchMode>();
        _assert_send_sync::<crate::stretch::TimeStretcher>();
    }
}

#[cfg(test)]
mod serde_roundtrip {
    fn roundtrip<T: serde::Serialize + serde::de::DeserializeOwned + core::fmt::Debug>(val: &T) {
        let json = serde_json::to_string(val).expect("serialize");
        let _back: T = serde_json::from_str(&json).expect("deserialize");
    }

    #[test]
    fn all_public_types_roundtrip() {
        use crate::prelude::*;

        // Enums
        roundtrip(&EnvState::Idle);
        roundtrip(&LoopMode::Forward);
        roundtrip(&LoopMode::LoopSustain);
        roundtrip(&FilterMode::HighPass);
        roundtrip(&VelocityCurve::Concave);
        roundtrip(&EffectType::Reverb);

        // Config types
        roundtrip(&AdsrConfig::default());
        roundtrip(&AdsrConfig::from_seconds(0.01, 0.1, 0.7, 0.3, 44100.0));

        // Envelope
        roundtrip(&AmpEnvelope::new(&AdsrConfig::default(), 44100.0));

        // Sample types
        let sample = Sample::from_mono(vec![0.1, 0.2, 0.3], 44100);
        roundtrip(&sample);
        roundtrip(&SampleId(42));

        let mut bank = SampleBank::new();
        let _ = bank.add(Sample::from_mono(vec![0.0; 10], 44100));
        roundtrip(&bank);

        // Zone with all fields populated
        let zone = Zone::new(SampleId(0))
            .with_key_range(36, 84)
            .with_vel_range(1, 127)
            .with_root_note(60)
            .with_tune(50.0)
            .with_volume(-3.0)
            .with_pan(0.5)
            .with_loop(LoopMode::Forward, 100, 5000)
            .with_crossfade(64)
            .with_sample_offset(10)
            .with_sample_end(9000)
            .with_filter(2000.0, 0.5)
            .with_filter_resonance(2.0)
            .with_filter_type(FilterMode::LowPass)
            .with_group(1)
            .with_choke_group(1)
            .with_velocity_curve(VelocityCurve::Convex)
            .with_adsr(AdsrConfig::from_seconds(0.01, 0.1, 0.7, 0.3, 44100.0))
            .with_filter_envelope(
                AdsrConfig::from_seconds(0.05, 0.2, 0.5, 0.1, 44100.0),
                2400.0,
            )
            .with_pitch_lfo(5.0, 50.0)
            .with_filter_lfo(3.0, 600.0)
            .with_key_tracking(0.5)
            .with_time_stretch(1.5);
        roundtrip(&zone);

        // Instrument
        let mut inst = Instrument::new("test_piano");
        inst.add_zone(Zone::new(SampleId(0)).with_key_range(0, 127));
        roundtrip(&inst);

        // SFZ
        let sfz = crate::sfz::parse("<region>\nsample=test.wav\n").unwrap();
        roundtrip(&sfz);

        // SF2 preset
        roundtrip(&Sf2Preset {
            name: "Piano".into(),
            bank: 0,
            preset_number: 0,
        });

        // Capture
        let rec = SampleRecorder::new(44100, 1);
        roundtrip(&rec);

        // Effect chain
        let chain = EffectChain::new(44100.0);
        roundtrip(&chain);

        // Engine (full state)
        let mut engine = SamplerEngine::new(8, 44100.0);
        engine.set_adsr(AdsrConfig::from_seconds(0.01, 0.1, 0.7, 0.3, 44100.0));
        roundtrip(&engine);

        // Stretch
        let stretcher = crate::stretch::TimeStretcher::new(vec![0.0; 100], 44100.0);
        roundtrip(&stretcher);
    }
}

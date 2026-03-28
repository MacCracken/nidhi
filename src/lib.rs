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

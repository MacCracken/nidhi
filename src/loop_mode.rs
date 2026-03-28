//! Loop modes for sample playback.

use serde::{Deserialize, Serialize};

/// How a sample loops during sustained playback.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum LoopMode {
    /// No looping — play once and stop.
    #[default]
    OneShot,
    /// Forward loop: play start→end, jump to loop_start, repeat.
    Forward,
    /// Ping-pong loop: play forward then backward within loop region.
    PingPong,
    /// Reverse: play the sample backwards.
    Reverse,
    /// Loop sustain: loop while note is held, play through to end on release.
    LoopSustain,
}

# 0010 — A file with more than two channels loads its front pair

**Status**: Accepted
**Date**: 2026-11-18

## Context

`NSample` holds mono or interleaved stereo. The loaders (`n_load_wav`, `n_load_wav_from_memory`
and the stream reader) built a mono sample for one channel and passed everything else to
`n_sample_from_stereo` unchanged, as the oracle did (`rust-old/src/io.rs:39-42`: "channels == 1
mono, else stereo"). So an N-channel interleave was read as stereo pairs:

| File | Loaded as (2.2.1) |
|---|---|
| 6 channels, 256 frames | "stereo", 768 frames of scrambled channels |
| 4 channels, 256 frames | "stereo", 512 frames |
| 3 channels, 200 frames | "stereo", 300 frames, with L and R drifting across channels |

The sample played at the wrong length with garbled channels, and the stream reader contradicted
itself: it reported 256 frames for the 6-channel file but served 768 through `read_all`. Since
shravan 2.10.3 this also reaches multichannel Opus and AAC through `n_load_wav`, which shravan
2.9.3 refused.

## Decision

**A file with more than two channels loads channels 1 and 2 as a stereo sample of the right
length.** The other channels are dropped. Channels 1 and 2 are front left and front right in WAV
(`WAVE_FORMAT_EXTENSIBLE`'s speaker order) and in the ffmpeg order shravan emits for Opus and AAC.
AIFF mostly agrees, with one exception: its spec defines two 4-channel layouts, quad (FL FR RL RR)
and L-C-R-S, and the second puts the centre in channel 2. This applies to all three loaders. The
stream reader reports 2 channels and serves the front pair chunk by chunk, so `total_frames`, the
chunks and `read_all` agree with `n_load_wav` on the same file. Mono and stereo files are byte-identical to 2.2.1. The maintainer chose this over
refusing such files and over a full downmix.

## Consequences

- **Positive**: a multichannel file loads at its real length with its front image intact, and
  the three loaders agree.
- **Negative**: centre, LFE and surround content is discarded. A 5.1 mix whose dialogue sits only
  in the centre channel loses it. This is a sampler's front pair, not a downmix. A 4-channel
  L-C-R-S AIFF loads as left and centre.
- **Neutral**: the output differs from the oracle for every file with more than two channels, all
  of which it loaded wrongly. A host that wants a downmix can decode with shravan itself and hand
  `n_sample_from_stereo` its own fold.

## Alternatives considered

- **Refuse with `NIDHI_ERR_IMPORT`.** Safe, but it makes every surround stem or ambisonic
  recording unloadable when its front pair is usually exactly what a sampler wants.
- **Standard surround downmix (ITU-R BS.775).** Needs a channel-layout table per format and a gain
  convention (−3 dB centre and surrounds, LFE dropped or not), for layouts up to 22.2. That is more
  code and more decisions than a sampler should own.

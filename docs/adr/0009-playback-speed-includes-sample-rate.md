# 0009 — Playback speed includes the sample's own rate

**Status**: Accepted
**Date**: 2026-11-18

## Context

Every `NSample` records its sample rate: `n_sample_from_mono(data, rate)`, the WAV loaders, and
SF2's `shdr.sampleRate`. Nothing in `src/` read it. A voice's speed was
`2^((note - root + tune/100) / 12)` (`n_zone_playback_ratio`), with no rate term. The oracle's
`Zone::playback_ratio` is the same formula, so the port was parity-faithful.

A voice advanced one sample frame per output frame at the root note, whatever the sample's rate,
so a sample played at the right pitch only when its rate equalled the engine's:

| Sample | Engine | Plays |
|---|---|---|
| 48 kHz WAV | 44.1 kHz | 1.47 semitones flat, 8.1 % slow |
| 22,050 Hz SF2 sample | 44.1 kHz | one octave high, twice as fast |
| 44.1 kHz WAV | 96 kHz | 13.5 semitones high |

SF2 banks routinely mix rates, and SFZ libraries often ship 48 or 96 kHz samples, so this is
the common case rather than an edge.

## Decision

**`note_on` multiplies the speed by `sample_rate / engine_rate`** (`n_engine_rate_scaled`). The
speed is left untouched when:
- the sample is missing or has a rate of 0 or less;
- the two rates are equal, so those voices keep their exact speed bits;
- the engine rate is not finite and positive, or the ratio is not.

## Consequences

- **Positive**: samples play at their recorded pitch at any engine rate.
- **Negative**: output differs from the oracle for every sample whose rate differs from the
  engine's. Those were all at the wrong pitch.
- **Neutral**: `n_zone_playback_ratio` keeps the oracle's formula, and the rate is applied in
  the engine, because a zone does not know its sample. Loop points, offsets and crossfades are in
  sample frames, so they need no change.

## Alternatives considered

- **Resample at load time.** That costs memory and quality, and it is the host's call, not the
  sampler's.
- **Put the rate in `playback_ratio`.** That would change a public function's oracle-pinned
  result, and the zone would need the bank.

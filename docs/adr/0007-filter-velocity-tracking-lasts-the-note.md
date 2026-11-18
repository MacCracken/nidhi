# 0007 — Filter velocity tracking lasts the whole note

**Status**: Accepted
**Date**: 2026-11-18

## Context

A zone's `filter_vel_track` (SFZ `fil_veltrack`) scales the cutoff by
`1 - vel_track * (1 - velocity / 127)`, so soft notes are darker. The oracle applied it once, in
`VoiceFilter::new`, to the SVF's initial cutoff. It set `voice.base_cutoff` to the raw zone
cutoff (`rust-old/src/engine.rs:617`). From the first rendered frame on, the render loop
recomputes the cutoff from `base_cutoff` (key tracking, filter envelope, LFO, brightness, then the
smoother) and overwrites the SVF's. So velocity tracking lasted zero frames.

Measured on 2.2.1, an 8 kHz zone with `vel_track = 1`: at note_on, velocity 16 sat at 1007 Hz and
velocity 127 at 8000 Hz. After 2000 frames both were at 6000 Hz. The opcode had no audible
effect. The port reproduced it faithfully.

## Decision

**Fold the velocity factor into `base_cutoff` at note_on**, next to key tracking, which was
already folded there in 2.0.7 for the same reason: both inputs are fixed for the voice's life.
The factor is skipped when `vel_track` is 0, the default, so those voices are bit-identical.

## Consequences

- **Positive**: `fil_veltrack` works for the whole note, through the envelope, LFO, brightness
  and smoother.
- **Negative**: filtered voices with a non-zero `vel_track` sound different from the oracle (they
  are darker at low velocity). That is a divergence.
- **Neutral**: the cutoff smoother snaps to the velocity-tracked cutoff (see the 2.2.2
  CHANGELOG), so the note does not open with a sweep either.

## Alternatives considered

- **Keep parity.** That keeps an SFZ opcode that parses, inherits and stores, and then does
  nothing: the same shape as `volume` before ADR 0004.

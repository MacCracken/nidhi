# 0008 — A released LoopSustain voice skips the loop crossfade

**Status**: Accepted
**Date**: 2026-11-18

## Context

The loop crossfade blends loop-start material into the last `crossfade_length` frames before
`loop_end`, so the wrap back to `loop_start` has no click ([ADR 0001](0001-loop-crossfade-seam.md)
decides where that material comes from). The oracle applied it to `Forward` and `LoopSustain`
alike (`rust-old/src/engine.rs:814`).

A `LoopSustain` voice loops only while the note is held. After release it plays straight through
`loop_end` into the rest of the sample, with no wrap. The crossfade still ran on that pass: it
faded loop-start material in, then the playhead crossed `loop_end` into the continuation, which
does not follow loop-start material. So the fade made a seam where the audio would otherwise be
continuous.

Measured on 2.2.1 (a 400-frame ramp, loop 100..200, released at frame 50): the worst
frame-to-frame jump on the pass-through was 0.000999 with no crossfade and 0.100996 with a
20-frame one, about 100 times larger.

## Decision

**A `LoopSustain` voice whose amplitude envelope is releasing does not crossfade.** A held
`LoopSustain` voice and every `Forward` voice are unchanged.

## Consequences

- **Positive**: the release pass is continuous, as the sample is.
- **Negative**: a release that lands inside the crossfade window stops the blend mid-fade, a step
  of `(1 - t)` times the difference of the two sources at that frame. It happens at most once per
  note, at the release, and only when the release falls in the window (probability
  `crossfade_length / loop_length`). Today's alternative is a seam on every release pass.
- **Neutral**: the output differs from the oracle for released `LoopSustain` voices with a
  crossfade.

## Alternatives considered

- **Finish an in-progress fade and skip new ones.** That needs per-voice fade state, for a
  residual that the release envelope already attenuates.
- **Keep parity.** That keeps a guaranteed click on the very pass `LoopSustain` exists for.

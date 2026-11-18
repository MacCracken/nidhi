# 0011 — The reverb and chorus slots get the parameters the oracle meant

**Status**: Accepted
**Date**: 2026-11-18

## Context

`n_effect_chain_create_state` builds naad effects with the oracle's literal arguments
(`rust-old/src/effect_chain.rs:202-216`). Two of them did not mean what they said, and the port
reproduced both bug for bug:

- **Reverb**: `Reverb::new(sr, 1.5, 0.3, 0.5, 0.6)` against naad's
  `(decay, damping, pre_delay_ms, mix, sample_rate)`. The sample rate arrived as `decay` (clamped
  to 0.99), `1.5` as damping (clamped to 1), and `0.6` as the sample rate. naad scales every comb
  and all-pass by `sample_rate / 44100`, so all of them were 1 sample long. An impulse left 0.5
  dry and nothing after 10 ms: a resonator, not a reverb. `docs/port/22-mod-dsp.md` recorded this
  as deliberate parity.
- **Chorus**: `Chorus::new(3, 0.02, 0.002, 1.5, 0.5, sr)`. naad's (and naad 1.2.5's) second and
  third parameters are milliseconds ("typically 7-20 ms", "1-5 ms"), so the oracle asked for a
  0.882-sample delay. The delay line was 1 sample long, clamped the read delay to 0, and read
  back the sample just written. The chorus was an exact passthrough: 0 of 4000 samples changed.
  The port notes said these arguments "line up sensibly". They did not.

## Decision

- **Chorus**: 20 ms base delay and 2 ms depth, which is what 0.02 s and 0.002 s were. The voices,
  rate and mix are unchanged.
- **Reverb**: each of the oracle's numbers goes in the slot whose range it fits: decay 0.6,
  damping 0.3, pre-delay 1.5 ms, mix 0.5, and the real sample rate. The user chose this over
  keeping the bug and over picking new defaults.

## Consequences

- **Positive**: both slots now do what they are named for. An impulse through the reverb rings
  past 100 ms, and the chorus changes every sample (`tests/effect_chain.tcyr`).
- **Negative**: the reverb mapping is a reading of intent, not a recovered fact. No record says
  what the oracle's author meant, and the mapping is recorded here so a later choice can
  supersede it with a reason.
- **Neutral**: delay, compressor and limiter are unchanged and bit-identical.

## Alternatives considered

- **Keep both bug for bug.** That ships two effects that do nothing useful.
- **Pick conventional defaults** (for example decay 0.7, damping 0.5, 10 ms, mix 0.3). That is
  equally a guess, and it drops the only numbers the oracle left.

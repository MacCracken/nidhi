# 0002 — A NaN parameter clamps to a bound instead of propagating

**Status**: Accepted
**Date**: 2026-08-31

## Context

Every clamped builder in `src/zone.cyr` (and the equivalents in `src/sfz.cyr` and
`src/envelope.cyr`) is written as `f64_min(f64_max(x, lo), hi)`. `lib/math.cyr` defines
`f64_min` / `f64_max` in terms of `f64_lt` / `f64_gt`, and those return `0` when either operand
is NaN — so a NaN input falls through to the bound rather than being carried.

Measured:

| call | port | Rust `f32::clamp` |
|---|---|---|
| `n_zone_with_tune(z, NaN)` | `-12800` (finite) | `NaN` |
| `n_zone_with_pan(z, NaN)` | `-1.0`, hard left (finite) | `NaN` |

This is a divergence from the oracle and it was neither documented nor tested. Note it applies
only to the two-sided `clamp` form: `.max()`-only sites do **not** diverge, because Rust's
`f32::max` also drops NaN.

## Decision

**Keep the port's behaviour.** A NaN parameter yields a finite value at one of the clamp bounds.

## Consequences

This is the safer of the two behaviours, and the asymmetry is large.

A NaN that reaches the render path does not stay local. It flows into the SVF's integrator
state, and because every subsequent sample is computed from that state, the filter **latches**:
the voice outputs NaN for the rest of its life, and the mix buffer with it. nidhi already treats
non-finite audio as something to stop at the boundary — `n_sample_from_decode` sweeps decoded
frames, `n_normalize_peak` guards the denormal-peak overflow that produced ±inf, and
`n_amp_envelope_new` routes a non-finite envelope time into its fallback. Letting a NaN in
through a *parameter* would undo that work at a different door.

Against that, the cost is a caller who passes NaN getting an odd-but-finite value. `pan = NaN`
landing on hard left is arbitrary, and worth knowing about — but it is audible and debuggable,
where a latched NaN voice is silence-or-worse with no obvious cause.

There is no realistic input that makes this matter for parity: NaN is not a legitimate value for
a tuning offset or a pan position. It arrives from a caller bug, an unvalidated computation, or
an SFZ float that parsed to NaN — and in the last case `sfz_f64` accepts `nan` only because
Rust's `parse::<f32>` does.

Pinned in `tests/zone.tcyr` under "NaN parameters clamp to a bound, never propagate", which
asserts both the finiteness and the specific bound, so neither can drift silently.

## Alternatives rejected

- **Match the oracle and propagate NaN.** Reproduces a defect that costs a voice permanently,
  in service of a parity case that cannot occur in valid input. CLAUDE.md's bar is feature-set
  parity with cleaner routes allowed; this is the cleaner route.
- **Ignore a non-finite argument and leave the field unchanged.** Arguably the most sensible
  behaviour of the three — it is safe *and* non-arbitrary, where clamping to a bound is only
  safe. Rejected for 2.0.6 because it is a third distinct behaviour, invents semantics neither
  implementation has, and would need the same treatment applied consistently across every
  clamped builder to avoid being more confusing than what it replaces. Revisit if a caller ever
  reports the hard-left-pan surprise.

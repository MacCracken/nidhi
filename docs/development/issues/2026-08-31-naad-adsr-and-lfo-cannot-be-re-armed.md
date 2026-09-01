# 2026-08-31 — naad: `Adsr` and `Lfo` cannot be re-armed, so a sampler must allocate one per note

**Component:** `naad` — `src/envelope.cyr` (`Adsr`), `src/modulation.cyr` (`Lfo`).
**naad version seen:** **2.2.2**.
**Severity:** **Low-Medium.** Nothing is wrong; the objects are simply construct-only. The cost is
unreclaimable heap proportional to notes played.
**nidhi impact:** **72 bytes per note-on**, never reclaimed. At a sustained 10 notes/s that is
~2.6 MB/hour, and more for zones that also carry a filter envelope or LFOs.

## Request

Either of these shapes would close it — a setter is the smaller change:

```
fn envelope_adsr_set_params(self, attack, decay, sustain, release)   # + validation, as _new does
fn envelope_adsr_reset(self)                                        # clear state to post-construction
fn modulation_lfo_set_frequency(self, frequency)
fn modulation_lfo_reset(self)                                       # clear phase
```

`filter_svf` already has exactly this pair — `filter_svf_set_params` (validating) and
`filter_svf_reset` (state-clearing) — so the precedent and the validation shape both exist.

## Why a sampler needs it

A voice is a fixed slot reused across notes for the life of the engine, but each note may come
from a **different zone** with different ADSR times and LFO rates. So a voice cannot keep one
`Adsr`: it has to build a new one per note-on, because there is no way to re-point an existing
one at different parameters.

`envelope_adsr_gate_on` restarts the envelope, which handles the *state* half — but not the
*parameter* half, and the parameters are what differ between zones.

## Measurement

Directly measured against naad 2.2.2 with `alloc_used()`:

| construction | bytes / 1000 | per call |
|---|---:|---:|
| `envelope_adsr_with_sample_rate` | 72,000 | **72** |
| `modulation_lfo_new` | 72,000 | **72** |

In nidhi, per note-on/note-off pair, measured end to end (`tests/engine.tcyr`, "note_on does not
allocate a filter per note"): **72 bytes**, on a zone with no filter envelope and no LFOs. A zone
with `ampeg` plus `pitchlfo` plus `fillfo` allocates four such objects.

## What nidhi already did on its own side

nidhi 2.1.0 removed the filter half of this: `nvf_reinit` re-arms the voice's existing
`NVoiceFilter` and its two SVFs in place, using `filter_svf_set_params` + `filter_svf_reset`.
That took per-note allocation from **264 → 72 bytes**. The remaining 72 is precisely the objects
with no re-arm API.

Worth stating plainly: the filter half was only fixable *because* `filter_svf_set_params` and
`filter_svf_reset` exist. This request is to extend the same courtesy to `Adsr` and `Lfo`.

## Why the allocation matters more than the byte count suggests

`lib/alloc.cyr` is a bump allocator whose free is a no-op, so this is unbounded growth for the
life of the process rather than churn a real allocator would absorb. nidhi asserts a zero-byte
delta across a rendered block; note-on is the one remaining path that grows the heap, and it
grows with user activity.

## Acceptance

nidhi gives each voice its `Adsr` and LFOs once in `n_voice_new` and re-arms them in `note_on`,
taking per-note allocation to zero. A `reset` must clear whatever `gate_on` does not — for the
SVF that was `ic1eq`/`ic2eq`; the equivalent here is envelope level/position and LFO phase, so a
retriggered voice does not inherit the previous note's envelope position.

## Related

[`2026-08-31-naad-no-alloc-free-svf-core-for-non-lowpass.md`](2026-08-31-naad-no-alloc-free-svf-core-for-non-lowpass.md)
— the same class of gap on the per-sample path, with a larger measured cost.

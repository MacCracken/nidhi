# nidhi

**nidhi** (Sanskrit: *treasure*) — the sample playback engine for [AGNOS](https://github.com/MacCracken).

A polyphonic sampler written in **Cyrius**: key/velocity zones, ADSR envelopes, loop modes,
time-stretching, SFZ/SF2 import, sample capture, and per-instrument effects. It leans on
[naad](https://github.com/MacCracken/naad) for DSP so it stays a thin, correct sampler.

Originally a Rust crate; ported to Cyrius in 2.0.0 and Rust-free since 2.0.4. The original lives
in git history — see [`docs/port/oracle-build-identity.md`](docs/port/oracle-build-identity.md).

## Features

- **Polyphonic engine** — configurable voice stealing (Oldest/Quietest/Lowest/None), poly/mono/legato
- **Key/velocity zones** — full MIDI range mapping, round-robin groups, choke groups, velocity curves
- **ADSR envelopes** — per-voice and per-zone, with smooth release from any level
- **Filters** — SVF (LP/HP/BP/Notch) via naad, true stereo, with envelope and LFO modulation
- **Loop modes** — OneShot, Forward, PingPong, Reverse, LoopSustain, and seam-closing crossfade loops
- **Expression** — per-note pitch bend, pressure/aftertouch, brightness (CC#74), key tracking
- **SFZ import** — 40+ opcodes, note names, `<control>`/`<curve>`, `#include`, `_onccN` CC modulation
- **SF2 import** — RIFF/SoundFont binary parser, preset and zone extraction
- **Sample capture** — record, auto-trim, normalize, onset detection, loop-point detection
- **Effects** — per-instrument chain (reverb, delay, chorus, compressor, limiter) via naad
- **Audio loading** — WAV and, via shravan's dispatch, AIFF/FLAC/OGG/MP3/ALAC; plus a chunked reader
- **Time-stretching** — WSOLA and OLA (offline)
- **Allocation-free render path** — asserted, not assumed: `tests/engine.tcyr` pins a zero-byte
  `alloc_used()` delta across a rendered block at 8 and 64 voices

Not yet implemented, despite the field existing: **per-zone bus routing**. `output=` is parsed
and stored but nothing reads it — tracked for 2.1.1 in the [roadmap](docs/development/roadmap.md).

## Quick Start

nidhi is a **library**, not a binary. Consumers take `dist/nidhi.cyr`, the bundle built by
`cyrius distlib`.

```sh
cyrius deps                                        # resolve stdlib + naad/shravan/hisab
cyrius build programs/smoke.cyr build/nidhi-smoke   # build-chain smoke test
cyrius test                                        # every tests/*.tcyr
cyrius bench tests/nidhi.bcyr                      # the parity benchmarks
cyrius fuzz                                        # never-crash parser harnesses
cyrius distlib                                     # rebuild dist/nidhi.cyr
```

```cyrius
include "lib/hisab.cyr"
include "lib/goonj.cyr"
include "lib/naad.cyr"
include "dist/nidhi.cyr"

fn main() {
    alloc_init();

    var e = n_engine_new(16, f64_from(44100));
    var bank = n_engine_bank(e);

    var data = vec_new();
    var i = 0;
    while (i < 44100) { vec_push(data, 0); i = i + 1; }
    var sid = n_sample_bank_add(bank, n_sample_from_mono(data, 44100));

    var z = n_zone_with_root_note(n_zone_with_key_range(n_zone_new(sid), 60, 60), 60);
    var inst = n_instrument_new(str_from("piano"));
    n_instrument_add_zone(inst, z);
    n_engine_set_instrument(e, inst);

    n_engine_note_on(e, 60, 100);

    var out = alloc(16);
    n_engine_next_sample_stereo(e, out);        # out[0] = L, out[1] = R
    return 0;
}
```

Everything is `i64`; floats are f64 **bit patterns** built with `f64_from` / `f64_div` and friends.
Errors are negative `NIDHI_ERR_*` codes. See [CONTRIBUTING.md](CONTRIBUTING.md) for the
conventions, and [`docs/guides/getting-started.md`](docs/guides/getting-started.md) to go deeper.

## SFZ / SF2 import

```cyrius
var f = n_sfz_parse("<region> sample=piano_c4.wav pitch_keycenter=60\n");
var inst = n_sfz_to_instrument(f, str_from("piano"), f64_from(44100));

var res = sf2_parse(bytes, len);                # negative NIDHI_ERR_IMPORT on failure
```

## Architecture

```text
              NSamplerEngine
           /       |       |        \
  NInstrument  NSampleBank  NEffectChain  NSampleRecorder
       |            |
    NZone[]      NSample[]
       |
  loop mode, filter, LFO, pan, tune, ADSR, volume
       |
  SFZ / SF2 (import)          NTimeStretcher (offline)
```

Modules are self-contained with **no includes between them** — `dist/nidhi.cyr` is a plain
concatenation in dependency order, so the whole library shares one flat namespace with its
dependencies. That is why every symbol carries an `n_` / `N` prefix.

## Documentation

| | |
|---|---|
| [CHANGELOG.md](CHANGELOG.md) | What shipped, and why |
| [docs/development/roadmap.md](docs/development/roadmap.md) | What is next, and why anything deferred is deferred |
| [docs/development/state.md](docs/development/state.md) | Current version, tests, dependencies, known hazards |
| [docs/adr/](docs/adr/) | Decisions — *why X over Y?* |
| [BENCHMARKS.md](BENCHMARKS.md) · [docs/benchmarks-rust-v-cyrius.md](docs/benchmarks-rust-v-cyrius.md) | Performance |
| [docs/port/](docs/port/) | The port record — historical, not current state |

## Consumers

- [dhvani](https://github.com/MacCracken/dhvani) — audio engine
- [shruti](https://github.com/MacCracken/shruti) — DAW (replaces `shruti-instruments::sampler` via dhvani)

## License

GPL-3.0-only

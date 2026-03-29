# nidhi

**nidhi** (Sanskrit: treasure) — Sample playback engine for [AGNOS](https://github.com/MacCracken).

Polyphonic sampler with key/velocity zones, ADSR envelopes, loop modes, time-stretching, SFZ/SF2 import, sample capture, and per-instrument effects.

## Features

- **Polyphonic engine** — configurable voice stealing (Oldest/Quietest/Lowest/None), poly/mono/legato modes
- **Key/velocity zones** — full MIDI range mapping, round-robin groups, choke groups, velocity curves
- **ADSR envelopes** — per-voice and per-zone, with smooth release from any level
- **Filters** — SVF (LP/HP/BP/Notch) via naad, true stereo, with envelope and LFO modulation
- **Loop modes** — OneShot, Forward, PingPong, Reverse, LoopSustain (release exits loop), crossfade loops
- **Expression** — per-note pitch bend, pressure/aftertouch, brightness (CC#74), key tracking
- **SFZ import** — 40+ opcodes, note names, `<control>`/`<curve>`, `#include`, `_onccN` CC modulation
- **SF2 import** — RIFF binary parser, preset/zone extraction, PCM16 to f32
- **Sample capture** — record audio, auto-trim, normalize, onset detection, loop point detection
- **Effects** — per-instrument chain (reverb, delay, chorus, compressor, limiter) via naad
- **WAV loading** — `io` feature for file and in-memory WAV loading, plus streaming for large instruments
- **Time-stretching** — WSOLA and OLA algorithms (offline)
- **Multi-output** — per-zone bus routing
- **Built on [naad](https://crates.io/crates/naad)** — shares audio synthesis primitives with the AGNOS ecosystem
- **no\_std compatible** — works with `alloc`, no standard library required

## Quick Start

```rust
use nidhi::prelude::*;

let sample = Sample::from_mono(vec![0.0; 44100], 44100);
let mut bank = SampleBank::new();
let id = bank.add(sample);

let zone = Zone::new(id).with_key_range(60, 60).with_root_note(60);
let mut inst = Instrument::new("piano");
inst.add_zone(zone);

let mut engine = SamplerEngine::new(16, 44100.0);
engine.set_bank(bank);
engine.set_instrument(inst);
engine.note_on(60, 100);

let (left, right) = engine.next_sample_stereo();
```

## SFZ Import

```rust
use nidhi::prelude::*;
use nidhi::sfz;

let input = r#"
<control> default_path=samples/
<global> lovel=0 hivel=127
<group> lokey=60 hikey=72
<region> sample=piano_c4.wav pitch_keycenter=60
<region> sample=piano_c5.wav pitch_keycenter=72
"#;

let sfz_file = sfz::parse(input).unwrap();
let (instrument, sample_paths) = sfz_file.to_instrument("piano", 44100.0);
```

## SF2 Import

```rust,no_run
use nidhi::sf2;

let bytes = std::fs::read("soundfont.sf2").unwrap();
let (presets, instruments, bank) = sf2::parse(&bytes).unwrap();
```

## Feature Flags

| Feature   | Default | Description |
|-----------|---------|-------------|
| `std`     | Yes     | Standard library + naad integration. Disable for `no_std` + `alloc` |
| `io`      | No      | WAV file loading and streaming via shravan (implies `std`) |
| `logging` | No      | `tracing-subscriber` for debug logging |
| `full`    | No      | Enables `std` + `io` + `logging` |

## Architecture

```text
              SamplerEngine
           /     |     |     \
  Instrument  SampleBank  EffectChain  SampleRecorder
       |          |
     Zone[]    Sample[]
       |
  LoopMode, filter, LFO, pan, tune, ADSR
       |
  SfzFile / SF2 (import)    TimeStretcher (offline)
```

## Consumers

- [dhvani](https://github.com/MacCracken/dhvani) — Audio engine
- [shruti](https://github.com/MacCracken/shruti) — DAW (replaces `shruti-instruments::sampler` via dhvani)

## License

GPL-3.0-only

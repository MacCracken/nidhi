# nidhi

**nidhi** (Sanskrit: treasure) — Sample playback engine for [AGNOS](https://github.com/MacCracken).

Polyphonic sampler with key/velocity zones, ADSR envelopes, loop modes, time-stretching, and SFZ import.

## Features

- **Polyphonic engine** — voice stealing, per-voice filtering, cubic Hermite interpolation
- **Key/velocity zones** — full MIDI range mapping with round-robin group support
- **ADSR envelopes** — per-voice amplitude envelopes with sample-accurate timing
- **Loop modes** — OneShot, Forward, PingPong, Reverse with configurable loop points
- **Time-stretching** — WSOLA and OLA algorithms for duration changes without pitch shift
- **SFZ import** — parser with global/group/region inheritance, filter and envelope opcodes
- **Stereo** — constant-power pan per zone, stereo sample playback
- **Built on [naad](https://crates.io/crates/naad)** — shares audio synthesis primitives with the AGNOS ecosystem
- **no\_std compatible** — works with `alloc`, no standard library required

## Quick Start

```rust
use nidhi::prelude::*;

// Create a sample and add it to a bank
let sample = Sample::from_mono(vec![0.0; 44100], 44100);
let mut bank = SampleBank::new();
let id = bank.add(sample);

// Build an instrument with one zone
let zone = Zone::new(id).with_key_range(60, 60).with_root_note(60);
let mut inst = Instrument::new("piano".into());
inst.add_zone(zone);

// Play it
let mut engine = SamplerEngine::new(16, 44100);
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
<global> lovel=0 hivel=127
<group> lokey=60 hikey=72
<region> sample=piano_c4.wav pitch_keycenter=60
<region> sample=piano_c5.wav pitch_keycenter=72
"#;

let sfz_file = sfz::parse(input).unwrap();
let (instrument, sample_paths) = sfz_file.to_instrument("piano", 44100);
// Load samples from sample_paths, add to bank, then play
```

## Feature Flags

| Feature   | Default | Description |
|-----------|---------|-------------|
| `std`     | Yes     | Standard library support. Disable for `no_std` + `alloc` |
| `logging` | No      | `tracing-subscriber` for debug logging |
| `full`    | No      | Enables `std` + `logging` |

## Architecture

```text
              SamplerEngine
             /      |      \
     Instrument  SampleBank  AdsrConfig
         |           |
       Zone[]     Sample[]
         |
   LoopMode, filter, pan, tune
         |
      SfzFile (import)     TimeStretcher (offline)
```

## Consumers

- [dhvani](https://github.com/MacCracken/dhvani) — Audio engine
- [shruti](https://github.com/MacCracken/shruti) — DAW (replaces `shruti-instruments::sampler` via dhvani)

## License

GPL-3.0-only

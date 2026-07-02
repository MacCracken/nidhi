//! Criterion benchmarks for nidhi.
//!
//! Covers the hot paths: voice rendering, buffer fill, interpolation,
//! filter processing, and engine scaling with voice count.

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use nidhi::prelude::*;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a minimal engine with N voices, one zone covering all keys, and a
/// 1-second sine wave sample.
fn make_engine(max_voices: usize, sample_rate: f32) -> SamplerEngine {
    let frames = sample_rate as usize;
    let data: Vec<f32> = (0..frames)
        .map(|i| f32::sin(2.0 * core::f32::consts::PI * 440.0 * i as f32 / sample_rate))
        .collect();

    let sample = Sample::from_mono(data, sample_rate as u32);
    let mut bank = SampleBank::new();
    let id = bank.add(sample);

    let zone = Zone::new(id)
        .with_key_range(0, 127)
        .with_root_note(60)
        .with_loop(nidhi::loop_mode::LoopMode::Forward, 0, frames - 1);
    let mut inst = Instrument::new("bench");
    inst.add_zone(zone);

    let mut engine = SamplerEngine::new(max_voices, sample_rate);
    engine.set_bank(bank);
    engine.set_instrument(inst);
    engine
}

/// Trigger N notes spread across the key range.
fn trigger_notes(engine: &mut SamplerEngine, count: usize) {
    let step = if count > 1 { 60 / (count - 1) } else { 0 };
    for i in 0..count {
        let note = (36 + i * step).min(96) as u8;
        engine.note_on(note, 100);
    }
}

// ---------------------------------------------------------------------------
// Benchmarks
// ---------------------------------------------------------------------------

/// Single stereo sample generation with varying active voice counts.
fn voice_count_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("voice_count_scaling");
    for &voices in &[1, 4, 8, 16, 32, 64] {
        group.bench_with_input(BenchmarkId::from_parameter(voices), &voices, |b, &n| {
            let mut engine = make_engine(64, 44100.0);
            trigger_notes(&mut engine, n);
            b.iter(|| black_box(engine.next_sample_stereo()));
        });
    }
    group.finish();
}

/// Fill a 512-frame stereo buffer (1024 samples) — the typical audio callback size.
fn fill_buffer_stereo_512(c: &mut Criterion) {
    let mut group = c.benchmark_group("fill_buffer_stereo");
    for &voices in &[1, 8, 16] {
        group.bench_with_input(BenchmarkId::from_parameter(voices), &voices, |b, &n| {
            let mut engine = make_engine(64, 44100.0);
            trigger_notes(&mut engine, n);
            let mut buf = vec![0.0f32; 1024];
            b.iter(|| {
                buf.fill(0.0);
                engine.fill_buffer_stereo(black_box(&mut buf));
            });
        });
    }
    group.finish();
}

/// Per-sample rendering baseline for comparison with block rendering.
fn fill_buffer_per_sample(c: &mut Criterion) {
    let mut group = c.benchmark_group("fill_buffer_per_sample");
    for &voices in &[1, 8, 16] {
        group.bench_with_input(BenchmarkId::from_parameter(voices), &voices, |b, &n| {
            let mut engine = make_engine(64, 44100.0);
            trigger_notes(&mut engine, n);
            let mut buf = vec![0.0f32; 1024];
            b.iter(|| {
                let mut i = 0;
                while i + 1 < buf.len() {
                    let (l, r) = engine.next_sample_stereo();
                    buf[i] = l;
                    buf[i + 1] = r;
                    i += 2;
                }
                black_box(&buf);
            });
        });
    }
    group.finish();
}

/// Cubic Hermite interpolation throughput — mono read_cubic.
fn interpolation_cubic(c: &mut Criterion) {
    let frames = 44100;
    let data: Vec<f32> = (0..frames)
        .map(|i| f32::sin(2.0 * core::f32::consts::PI * 440.0 * i as f32 / 44100.0))
        .collect();
    let sample = Sample::from_mono(data, 44100);

    c.bench_function("interpolation_cubic_1k", |b| {
        b.iter(|| {
            let mut sum = 0.0f32;
            for i in 0..1000 {
                let pos = i as f64 * 0.73 + 100.0;
                sum += sample.read_cubic(black_box(pos));
            }
            black_box(sum)
        });
    });
}

/// Stereo interpolation throughput — read_stereo_interpolated.
fn interpolation_stereo(c: &mut Criterion) {
    let frames = 44100;
    let data: Vec<f32> = (0..frames * 2)
        .map(|i| f32::sin(2.0 * core::f32::consts::PI * 440.0 * i as f32 / 88200.0))
        .collect();
    let sample = Sample::from_stereo(data, 44100);

    c.bench_function("interpolation_stereo_1k", |b| {
        b.iter(|| {
            let mut sum = (0.0f32, 0.0f32);
            for i in 0..1000 {
                let pos = i as f64 * 0.73 + 100.0;
                let (l, r) = sample.read_stereo_interpolated(black_box(pos));
                sum.0 += l;
                sum.1 += r;
            }
            black_box(sum)
        });
    });
}

/// Engine with filter enabled — measures filter processing cost.
fn engine_with_filter(c: &mut Criterion) {
    let frames = 44100;
    let data: Vec<f32> = (0..frames)
        .map(|i| f32::sin(2.0 * core::f32::consts::PI * 440.0 * i as f32 / 44100.0))
        .collect();

    let sample = Sample::from_mono(data, 44100);
    let mut bank = SampleBank::new();
    let id = bank.add(sample);

    let zone = Zone::new(id)
        .with_key_range(0, 127)
        .with_root_note(60)
        .with_filter(2000.0, 0.7)
        .with_filter_type(nidhi::zone::FilterMode::LowPass);
    let mut inst = Instrument::new("filtered");
    inst.add_zone(zone);

    let mut engine = SamplerEngine::new(16, 44100.0);
    engine.set_bank(bank);
    engine.set_instrument(inst);
    trigger_notes(&mut engine, 8);

    let mut buf = vec![0.0f32; 1024];

    c.bench_function("fill_buffer_stereo_filtered_8v", |b| {
        b.iter(|| {
            buf.fill(0.0);
            engine.fill_buffer_stereo(black_box(&mut buf));
        });
    });
}

/// Time-stretching throughput — WSOLA on 1 second of audio.
fn time_stretch_wsola(c: &mut Criterion) {
    use nidhi::stretch::TimeStretcher;

    let frames = 44100;
    let data: Vec<f32> = (0..frames)
        .map(|i| f32::sin(2.0 * core::f32::consts::PI * 440.0 * i as f32 / 44100.0))
        .collect();

    let ts = TimeStretcher::new(data, 44100.0);

    c.bench_function("wsola_1sec_2x", |b| {
        b.iter(|| black_box(ts.stretch(2.0)));
    });
}

criterion_group!(
    benches,
    voice_count_scaling,
    fill_buffer_stereo_512,
    fill_buffer_per_sample,
    interpolation_cubic,
    interpolation_stereo,
    engine_with_filter,
    time_stretch_wsola,
);
criterion_main!(benches);

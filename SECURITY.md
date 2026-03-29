# Security Policy

## Scope

nidhi is a sample playback engine library. It performs no network access and contains no `unsafe` code. File I/O is limited to the optional `io` feature (WAV loading via shravan).

## Attack Surface

| Area | Risk | Mitigation |
|------|------|------------|
| SFZ parser | Crafted input with deep nesting or extreme values | All numeric fields clamped on parse; unknown opcodes ignored |
| SF2 parser | Malformed RIFF chunks, truncated data | Bounds-checked reads; all offsets validated before access |
| WAV loading (`io`) | Crafted WAV with extreme sample counts | shravan handles format validation; allocation bounded by file size |
| Serde deserialization | Crafted JSON with extreme values | Enum validation via serde derive; parameters clamped on use |
| Sample rate validation | Division by zero, NaN propagation | Clamped to safe ranges in constructors |
| Buffer lengths | Over-allocation from large voice counts | Voice count clamped to 1–128 by naad VoiceManager |
| `alloc::format!` in errors | Allocation in error paths | Only in constructors and parsers, not in render hot path |

## Reporting Vulnerabilities

Report security issues to the repository maintainer via GitHub Security Advisories. Do not file public issues for security vulnerabilities.

## Dependencies

| Dependency | Purpose | Risk |
|---|---|---|
| `naad` (optional) | DSP primitives — filters, envelopes, effects | No I/O; pure audio processing |
| `shravan` (optional) | Audio codecs (WAV, FLAC, etc.) | File I/O only when `io` feature enabled |
| `serde` | Serialization | Widely audited, no unsafe in derive |
| `thiserror` | Error derive | Proc macro only, no runtime code |
| `hisab` | Math primitives | Pure computation, no I/O |
| `tracing` (optional) | Structured logging | No I/O; subscriber is caller's responsibility |

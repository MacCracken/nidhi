# Contributing to nidhi

Thank you for your interest in contributing to nidhi.

## Development Workflow

1. Fork and clone the repository
2. Create a feature branch from `main`
3. Make your changes
4. Run the cleanliness check (see below)
5. Open a pull request

## Prerequisites

- Rust stable (MSRV 1.89)
- Components: `rustfmt`, `clippy`
- Optional: `cargo-audit`, `cargo-deny`

## Cleanliness Check

Every change must pass:

```bash
cargo fmt --check
cargo clippy --all-features --all-targets -- -D warnings
cargo test --all-features
cargo test --no-default-features
RUSTDOCFLAGS="-D warnings" cargo doc --all-features --no-deps
cargo audit
cargo deny check
```

Or simply run `make check` for the core checks.

## Code Conventions

- `#[non_exhaustive]` on all public enums
- `#[must_use]` on all pure functions and accessors
- `#[inline]` on hot-path render functions
- Serde (`Serialize + Deserialize`) on all public types
- Zero `unwrap`/`panic` in library code (`.expect()` only on provably infallible paths)
- `no_std` compatible — use `alloc` not `std` collections
- Feature-gate `naad` usage behind `#[cfg(feature = "std")]`
- Feature-gate `hound` usage behind `#[cfg(feature = "io")]`
- All new fields on Zone must have `#[serde(default)]` for backward compatibility

## Adding a New Module

1. Create `src/my_module.rs` following existing patterns
2. Register in `lib.rs`: module declaration, prelude export if public
3. Add `Send + Sync` assertion in `lib.rs::assert_traits`
4. Add serde roundtrip test in `lib.rs::serde_roundtrip`
5. Add unit tests in the module

## License

By contributing, you agree that your contributions will be licensed under GPL-3.0-only.

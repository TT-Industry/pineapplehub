# Contributing to PineappleHub

## Prerequisites

- [Rust](https://rustup.rs/) (edition 2024)
- `wasm32-unknown-unknown` target: `rustup target add wasm32-unknown-unknown`
- [Trunk](https://trunkrs.dev/): `cargo install trunk`

## Development

Start the dev server:

```bash
trunk serve -a 0.0.0.0
```

With release optimizations (recommended for testing performance-sensitive image processing):

```bash
trunk serve --release
```

> **Note:** The app requires `SharedArrayBuffer` for Rayon-based parallel processing. Trunk is configured to serve the necessary COOP/COEP headers via `Trunk.toml`.

## Project Structure

```
src/
├── main.rs              # Application entry, UI, and message loop
├── pipeline/
│   ├── mod.rs            # Pipeline types (FruitletMetrics, Step, etc.)
│   ├── fruitlet_counting.rs  # Interactive pipeline (with UI previews)
│   ├── fast.rs           # Headless pipeline (Web Worker parallel processing)
│   ├── scale_calibration.rs
│   └── roi_extraction.rs
├── export.rs             # CSV export
├── history/              # IndexedDB-backed analysis history
└── ui/                   # UI components
docs/
├── algorithms/           # Algorithm documentation (EN + ZH)
└── user_guide/           # Debug image interpretation guides
```

## Architecture Notes

- **Dual pipeline:** `fruitlet_counting.rs` is the interactive pipeline with iced UI types and `log` output; `fast.rs` is a pure-computation mirror (`Send + Sync`, no browser APIs) used by rayon Web Workers. **Changes to measurement logic must be synced between both files.**
- **WASM-only:** The crate targets `wasm32-unknown-unknown` exclusively. All dependencies must be WASM-compatible.
- **Lints:** `clippy::pedantic` and `clippy::all` are enabled as warnings. Run `cargo clippy --target wasm32-unknown-unknown` before submitting.

## Documentation

When modifying algorithms, update the corresponding documentation:

- [algorithm.md](docs/algorithms/algorithm.md) (English)
- [algorithm_zh.md](docs/algorithms/algorithm_zh.md) (Chinese)
- [debug_interpretation.md](docs/user_guide/debug_interpretation.md) (English)
- [debug_interpretation_zh.md](docs/user_guide/debug_interpretation_zh.md) (Chinese)

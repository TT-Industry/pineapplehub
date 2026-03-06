# PineappleHub

A browser-based pineapple fruit quality measurement tool built with Rust + WebAssembly.

Upload a photo of a bisected pineapple with a 1 Yuan coin for scale, and PineappleHub automatically measures fruit geometry and fruitlet eye count.

## Features

- **Automatic Scale Calibration** — Detects a 1 Yuan coin (Ø 25 mm) via two-tier shape analysis (aspect ratio, fill ratio, circularity) to establish pixel-to-millimetre mapping; no manual calibration needed.
- **Texture-Driven ROI Selection** — Distinguishes the textured skin surface from the smooth flesh cut-face using an edge-density × √area score; no colour-space assumptions required.
- **Dual-Axis Cylindrical Perspective Correction** — Independently corrects horizontal and vertical foreshortening via two orthogonal inverse cylindrical projections, recovering physically accurate Height (ℓ_H) and Width (ℓ_W).
- **Volume Estimation** — Computes whole-fruit volume via disk-method integration on the perspective-corrected contour profile with dual-view axial fusion.
- **Surface Area Estimation** — Integrates the surface-of-revolution formula on the contour profile with envelope-binned smoothing to suppress pixel-level noise inflation.
- **Equatorial Fruitlet Eye Sizing** — Segments and measures the representative fruitlet eye at the equator (long axis a_eq, short axis b_eq, orientation angle α).
- **Whole-Fruit Eye Count Estimation** — Estimates total fruitlet eye count N_total by dividing the effective surface area (after polar cap subtraction for crown/peduncle plates) by the per-eye footprint area.

## Documentation

- [Algorithm Documentation (EN)](docs/algorithms/algorithm.md)
- [算法文档（中文）](docs/algorithms/algorithm_zh.md)
- [Debug Image Interpretation](docs/user_guide/debug_interpretation.md) · [调试图解读（中文）](docs/user_guide/debug_interpretation_zh.md)

## Development

On the public network:

```bash
trunk serve -a 0.0.0.0
```

Or with release optimizations:

```bash
trunk serve --release
```

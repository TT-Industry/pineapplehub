# ROI Extraction: Known Issues & Improvement Directions

> Source: edge cases discovered during `fix/algo` branch development, 2026-03-14

## Background

The ROI extraction pipeline in [`roi_extraction.rs`](../src/pipeline/roi_extraction.rs) uses a **low-threshold guided scoring** design:

1. **Low-threshold binarization** — Threshold the smoothed grayscale image at `τ_low = 25` to obtain complete, unfragmented fruit silhouettes. Each low-threshold outer contour defines a natural object boundary.
2. **Area filtering** — Discard low-threshold contours with geometric area below `0.2 × coin_area`.
3. **Otsu membership gating** — Only score low-threshold contours that contain at least one Otsu candidate centroid within their AABB.
4. **Texture scoring** — Score each qualifying contour by `edge_density × √(contour_area)`, where `edge_density` is the mean gradient over the contour's AABB, and `contour_area` is the low-threshold contour's own geometric area (NOT the sum of Otsu fragment areas).
5. **Selection & Bounding** — The highest-scoring contour wins. Its `min_area_rect` directly yields the ROI parameters.

## Resolved Edge Case

### Peel-side fragmentation causing flesh selection (Fixed 2026-03-14)

**Symptom**: On images where the peel side has especially deep inter-fruitlet crevices, the Otsu binarization splits the peel contour into many small fragments. The old per-candidate scoring formula (`edge_density × √area`) assigned each fragment a low score due to its small `area`, allowing the flesh side (one large solid contour) to win despite lower `edge_density`.

**Fix**: Low-threshold guided grouping (described above). By grouping fragments under their parent low-threshold contour before scoring, the peel side's full area is correctly represented as `merged_area`, restoring the peel side's dominance in the texture score.

### Round cross-section misidentified as coin (Fixed 2026-03-14)

**Symptom**: When the pineapple cross-section is unusually round, its convex hull passes all three coin shape criteria (aspect > 0.95, fill ∈ [0.70, 0.88], circularity > 0.85). Since the old Tier 1 selection chose the **largest** passing candidate, the cross-section (much larger than the actual coin) won.

**Fix**: Two-step selection in both Tier 1 and Tier 2. (1) **Relative-size gating**: when ≥2 candidates pass shape thresholds, exclude any with `hull_area > max_area_all × 0.25`, where `max_area_all` is the largest hull area among *all* contour candidates (including non-round fruit halves). (2) **Circularity scoring**: among surviving candidates, select the one closest to an ideal circle; ties broken by preferring smaller area.

## Known Edge Case

### Peel and cross-section merging at low threshold

**Symptom**: On certain images where the peel-side fruit and the cross-section are placed very close together (< 5 mm gap), the smoothed grayscale pixels in the gap are raised close to threshold 25. After thresholding, both objects become one connected component.

**Impact**: `find_contours` returns a single large contour spanning both objects → both peel and flesh Otsu candidates are grouped together → `min_area_rect` computes a ROI covering both → downstream unwrap fails.

**Status**: Observed on 1 image so far. Mitigations under consideration:

- **Adaptive threshold**: Replace fixed threshold (25) with `otsu_level(smoothed) / 3`, clamped to [15, 50].
- **Area validation fallback**: After grouping, if the winning low-threshold contour is disproportionately large (> 2× the merged Otsu area), suspect a merge and fall back to the largest member candidate's contour.

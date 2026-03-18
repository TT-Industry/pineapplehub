# Debug Visualization Guide

This guide explains how to interpret the intermediate visualizations shown in the PineappleHub debug UI. Understanding these outputs is crucial for diagnosing issues with lighting, focus, or algorithm parameters.

---

## 1. Smoothing
**What it is:** The original image after applying a Gaussian Blur ($\sigma=1.0$), followed by a median filter to suppress salt-and-pepper noise.
*   **Normal:** Slightly blurry, but edges of fruitlets remain visible and sensor noise is suppressed.
*   **Debug:** If too blurry, the original image may be out of focus. If still noisy, the camera ISO may be too high.

## 2. Scale Calibration
**What it is:** Automatic detection of the physical reference object (1 Yuan Coin, radius 12.5 mm) to derive the pixel-to-millimeter scale $S$ (unit: px/mm).

**Algorithm (`perform_scale_calibration`):**
1.  **Contour Extraction (`extract_robust_contours`):** Applies Otsu thresholding on the grayscale image, followed by morphological close ($r=2$, L2 norm) and open ($r=3$, L2 norm). Outer contours are extracted, and straight-edge artifacts (rulers, etc.) are removed via `remove_hypotenuse`.
2.  **Convex Hull Repair:** For each candidate contour with area $> 100$ px, its **Convex Hull** (Graham scan) is computed. This bridges concavities caused by edge stains or occlusions, recovering the true underlying shape.
3.  **Rotation-Invariant Metrics:**
    *   **Aspect Ratio** $ar = d_{\min} / d_{\max}$: Short-to-long edge ratio of `min_area_rect`, constrained to $(0, 1]$. Ideal circle $= 1.0$. **Crucially, calculated from rect edges — not from the axis-aligned bounding box (AABB), which distorts under rotation.**
    *   **Fill Ratio** $\phi = A_{hull} / (d_0 \cdot d_1)$: Convex hull area divided by min-area-rect area. Ideal circle $\approx \pi/4 \approx 0.785$.
    *   **Circularity** $C = 4\pi A_{hull} / P_{hull}^2$: Ideal circle $= 1.0$.
4.  **Two-Tier Detection:**
    *   **Tier 1 (Strict):** $ar > 0.95$, $\phi \in (0.70, 0.88)$, $C > 0.85$. Selects the largest-hull-area candidate passing all constraints.
    *   **Tier 2 (Relaxed + Scoring, triggers only if Tier 1 finds nothing):** $ar > 0.85$, $\phi \in (0.60, 0.92)$, $C > 0.70$. Among candidates passing the relaxed gate, selects by score: $score = -(10|ar-1| + 5|\phi - \pi/4| + 5|C-1|)$.
5.  **Scale Derivation:** Pixel radius is back-computed from hull area: $r_{px} = \sqrt{A_{hull}/\pi}$, giving $S = r_{px} / 12.5$.

*   **Normal:** The detected coin is highlighted with a red bounding box and a red circle overlay.
*   **Debug:** If no coin is detected (no red marks), check the browser console for `[CoinCandidate]` log lines. Examine whether any metric is near the threshold. If `Tier 1 failed` is logged, inspect `[CoinDetect T2]` scoring lines. Common causes: heavy occlusion of the coin edge (>~30% missing), strong specular highlights splitting the interior, or insufficient contrast against the background.

## 3. Texture Patch (Global Binary)
**What it is:** The binary image produced by global Otsu thresholding.
*   **Normal:** A noisy "star map". Fruitlet centers appear as white blobs; gaps appear black.
*   **Debug:**
    *   **All White:** Contrast too low; lighting may be too uniform.
    *   **All Black:** Contrast too high, or image underexposed.

## 4. Binary Fusion (Morphological Closing + Opening)
**What it is:** The "star map" from Step 3 processed by close ($r=2$) then open ($r=3$). Small noise dots merge into solid, distinct blobs.
*   **Normal:** Clearly separated, roughly circular blobs, each representing an individual fruitlet.
*   **Debug:**
    *   **Blobs fused into large regions:** The morphology radius $R_{morph}$ is too large (often caused by an inaccurate scale calibration inflating `pixels_per_mm`).
    *   **Over-fragmented:** $R_{morph}$ is too small.

## 5. Morphology / ROI Extraction
**What it is:** The three-panel visualization of the final extracted "skin region" (ROI), from left to right:
1.  **Vertical Unwrap (`VERT_UNWRAP`):** Cylindrical projection unwrapped along the vertical axis; used to extract physically accurate **Height** ($Major\_Length$).
2.  **Rotated ROI:** High-resolution crop rotated so the pineapple's principal axis is vertical.
3.  **Horizontal Unwrap (`HORIZ_UNWRAP`):** The ROI first rotated 90° then unwrapped; the fruit's poles are flattened to extract physically accurate **Width** ($Minor\_Length$) and **Volume**.

**ROI Selection Algorithm (Two-Stage):**
1.  **Identify** which candidate is the peel side via texture scoring (edge density × √area) on the Otsu-derived contours.
2.  **Bound** the full fruit silhouette by low-thresholding the smoothed grayscale image ($\tau = 25$) and matching the contour at the scored target's centroid. This avoids the contour fragmentation inherent in Otsu binarization.

Red annotation lines (solid edges + dashed center line) mark the minimal-area bounding rectangle used for metric extraction.

*   **Normal:**
    *   The pineapple axis is vertical in the center panel.
    *   The crop compactly contains most fruitlet area.
    *   Red annotation lines follow the fruit contour closely.
*   **Debug:**
    *   **Tilted:** `min_area_rect` computed an incorrect angle.
    *   **Blank crop:** ROI scoring failed — likely another object in the scene with a larger area than the pineapple surface.
    *   **ROI encompasses both peel and cross-section:** The low-threshold ($\tau=25$) merged both objects into one contour. This can happen when the two halves are placed very close together (gap < 5 mm). Try spacing the objects further apart.
    *   **Annotation lines misaligned:** The tight-bounds detection on the unwrapped image failed; dimension estimates may be inaccurate.

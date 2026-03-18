# Pineapple Measurement Algorithm Pipeline

*Surface Texture Analysis via Inverse Cylindrical Perspective Correction*

This document provides a mathematically rigorous description of the computer vision pipeline implemented in PineappleHub for measuring pineapple fruitlet geometry. The pipeline is designed to be robust against lighting variations, fruit orientation, and camera distance by combining **physical-scale calibration**, **surface-texture-driven ROI selection**, and **dual-axis cylindrical perspective correction**.

## Core Assumptions

1.  **Physical Scale Invariance**: A 1 Yuan coin (nominal diameter 25 mm, radius $R_{coin} = 12.5$ mm) serves as the reference object. By detecting it, we establish a pixel-to-millimetre mapping $\rho$ (px/mm). All subsequent spatial parameters are derived from $\rho$, eliminating manual calibration.
2.  **Imaging Geometry**: The pineapple is modelled as a convex cylindrical surface. Perspective foreshortening compresses the apparent width of the lateral extremities and the apparent height of the top and bottom poles. Correcting both requires two independent cylindrical reprojections (see Step 3).
3.  **Morphological Contrast**: The pineapple skin surface is rich in high-frequency texture (individual fruitlet mounds with sharp edges), whereas the flesh cut surface is smooth and nearly constant in luminance. This textural difference is the sole discriminant used for ROI selection.

---

## Algorithm Pipeline

### Step 1: Scale Calibration & Pre-processing

**Objective**: Establish the physical scale $\rho$ (px/mm), suppress sensor noise, and produce a binarised representation for downstream contour analysis.

#### 1.1 Noise Suppression

A two-stage filter is applied to the raw luminance image $I_{raw}$. First, a $3\times 3$ median filter removes salt-and-pepper noise without blurring edges:

$$I_{med} = \text{median}_{3\times 3}(I_{raw})$$

Then a Gaussian filter with $\sigma = 1.0$ pixel smooths residual high-frequency sensor noise while preserving structural edges:

$$I_{smooth} = I_{med} * G_\sigma$$

#### 1.2 Robust Contour Extraction

To obtain reliable shape candidates for coin detection and subsequent ROI selection, a common pre-processing sequence is applied to $I_{smooth}$:

1.  **Global Otsu Thresholding**: A threshold level $\tau^*$ is selected to minimise intra-class luminance variance:

$$B = \mathbf{1}[I_{smooth} > \tau^*]$$

2.  **Morphological Closing** (radius 2 px, $L_2$ structuring element): Bridges small gaps caused by specular highlights:

$$B_{closed} = B \oplus \text{disk}(2) \ominus \text{disk}(2)$$

3.  **Morphological Opening** (radius 3 px, $L_2$ structuring element): Removes thin protrusions and isolated noise:

$$B_{open} = B_{closed} \ominus \text{disk}(3) \oplus \text{disk}(3)$$

> The $L_2$ (Euclidean) structuring element produces an isotropic circular disk, which is essential for coin detection — an anisotropic element would systematically distort the circularity metric $\kappa$.

4.  **Contour Finding with Straight-Edge Rejection** (`remove_hypotenuse`): Contours whose boundary contains long straight segments (indicative of rulers or other rectilinear objects) are discarded. The detection threshold is 5.0 pixels.

#### 1.3 Scale Calibration (Coin Detection)

For each surviving contour, the algorithm extracts three rotation-invariant metrics computed on the **convex hull** of the contour (convex hull repair eliminates the effect of dirt or small edge defects that introduce concavities):

- **Convex Hull Area** $A_{hull}$ and **Hull Perimeter** $P_{hull}$.
- **Minimum-area bounding rectangle** (`min_area_rect`): yields edge lengths $d_0, d_1$.
- **Aspect Ratio**: $\alpha = d_{short} / d_{long} \in (0,1]$ — equals 1.0 for a square/circle; immune to rotation.
- **Fill Ratio**: $\phi = A_{hull} / (d_0 \cdot d_1)$ — for an ideal circle, $\phi_{ideal} = \pi/4 \approx 0.785$.
- **Circularity**: $\kappa = 4\pi A_{hull} / P_{hull}^2$ — equals 1.0 for a perfect circle.

**Two-Tier Detection**:

*Tier 1 (Strict)*: Candidates satisfying all three constraints simultaneously are collected:
$$\alpha > 0.95, \quad \phi \in [0.70,\,0.88], \quad \kappa > 0.85$$

When two or more candidates pass, a **relative-size gate** excludes fruit-sized objects: any candidate with $A_{hull} > 0.25 \times A_{max}$ is discarded, where $A_{max}$ is the largest hull area among *all* contour candidates (including non-round fruit halves that failed the shape test). A 1-Yuan coin (25 mm dia) has area $\approx \frac{1}{6}$ to $\frac{1}{20}$ of a fruit half (60–120 mm dia), so the 25% cutoff safely separates coin from fruit. Among surviving candidates, the one with the best circularity score wins (smallest deviation from ideal circle); ties are broken by preferring the smaller candidate:
$$s = -\bigl(10\,|\alpha - 1| + 5\,|\phi - \tfrac{\pi}{4}| + 5\,|1 - \kappa|\bigr)$$

*Tier 2 (Relaxed Fallback)*: If Tier 1 yields no result, candidates passing relaxed thresholds ($\alpha > 0.85$, $\phi \in [0.60, 0.92]$, $\kappa > 0.70$) are filtered by the same relative-size gate when multiple candidates exist, then ranked by the penalty score $s$. The candidate with maximum $s$ is selected; ties prefer smaller area.

**Scale Derivation**: For the winning hull with area $A_{hull}$, the equivalent radius is:
$$R_{hull} = \sqrt{A_{hull} / \pi}$$

and the physical scale is:
$$\rho = \frac{R_{hull}}{R_{coin}} \quad [\text{px/mm}]$$

---

### Step 2: Texture-Driven ROI Extraction

**Objective**: Identify the pineapple skin half of the bisected fruit (avoiding flesh and background objects) and extract an upright, rotation-corrected crop suitable for the unwrapping stage.

#### 2.1 Physical Area Filter

From the contours obtained in Step 1.2, all candidates with area below a minimum physical size are discarded:

$$A_{min} = 0.2 \times \pi R_{coin}^2 \,\rho^2 \quad [\text{px}^2]$$

*Rationale*: Any region substantially smaller than a coin is too small to be a valid fruit surface patch at any plausible camera distance.

#### 2.2 Low-Threshold Guided Grouping & Texture Scoring

The Otsu-derived contours from Step 1.2 may be **fragmented**: the dark inter-fruitlet crevices often fall below the Otsu threshold, splitting the fruit's outline into multiple disconnected white regions. If each fragment is scored independently, their individually small areas may cause the peel side to lose to the flesh side (which remains one large, solid contour). Furthermore, even aggregating fragment areas is insufficient: the Otsu mask only captures the bright fruitlet mounds — the dark gaps between them are excluded — so the summed fragment areas drastically underrepresent the peel side's true physical extent.

To solve this, the algorithm uses **low-threshold contours as natural object boundaries**, with the low-threshold contour's own geometric area as the size term:

1.  **Low-threshold binarization**: The smoothed grayscale image $I_{smooth}$ is globally thresholded at a low fixed level $\tau_{low} = 25$:

$$B_{low} = \mathbf{1}[I_{smooth} > \tau_{low}]$$

At this threshold, inter-fruitlet gaps (smoothed intensity ≈ 30–50) remain above threshold, keeping each fruit half as a single connected component. The background (intensity ≈ 0–15) stays below threshold, naturally separating distinct objects.

2.  **Area filtering**: Outer contours are extracted from $B_{low}$. Low-threshold contours with geometric area below $A_{min}$ (same threshold as Step 2.1) are discarded as noise specks.

3.  **Otsu membership check**: Each surviving low-threshold contour $\mathcal{L}_j$ is checked for the presence of at least one Otsu candidate whose centroid falls within $\mathcal{L}_j$'s AABB. This prevents non-fruit objects (rulers, background artifacts) — whose Otsu contours were already removed by the straight-edge filter in Step 1.2 — from being scored.

4.  **Per-contour texture scoring**: For each qualifying low-threshold contour $\mathcal{L}_j$:

    -  **Edge density** $\bar{g}_j$ is computed over $\mathcal{L}_j$'s AABB (clamped to image bounds): for each non-background pixel (luminance $> 15$), the first-order finite-difference gradient magnitude is calculated:

    $$\nabla I(x,y) = |I(x,y) - I(x+1,y)| + |I(x,y) - I(x,y+1)|$$

    and averaged over all $N_{fg}$ non-background pixels.

    -  **Contour area** $A_j$: the geometric area enclosed by $\mathcal{L}_j$ itself (from `contour_area`), **not** the sum of Otsu fragment areas. This correctly represents the full physical size of each object, including inter-fruitlet gaps that are invisible in the Otsu mask.

    -  **Combined score**: $\mathcal{S}_j = \bar{g}_j \cdot \sqrt{A_j}$

5.  **Selection**: The contour $\mathcal{L}^* = \arg\max_j \mathcal{S}_j$ wins. Its `min_area_rect` directly yields the final ROI parameters.

*Physical rationale*: Both fruit halves have similar physical sizes, so their low-threshold contour areas $A_j$ are approximately equal. This makes edge density $\bar{g}$ the sole effective discriminator. The pineapple skin is covered with raised fruitlet mounds separated by narrow dark crevices, producing high $\bar{g}$. The cut flesh surface is optically smooth, producing $\bar{g} \approx 0$. The coin, though high in edge contrast, is small in contour area, making $\sqrt{A}$ an effective size penalty.

> This design uses low-threshold contours in three roles simultaneously: (1) **area measurement** — correctly representing each object's full physical extent, (2) **membership gating** — ensuring only regions containing known fruit candidates are scored, and (3) **bounding** — providing the winning object's complete silhouette for `min_area_rect`.

#### 2.3 Rotated ROI Extraction

Given the bounding contour's minimum-area rectangle with centroid $(c_x, c_y)$, upright dimensions $(W_{roi}, H_{roi})$ — where the longer axis is assigned as height — and tilt angle $\theta_{tilt}$:

1.  A square padded buffer of side $d = \lceil\sqrt{W_{roi}^2 + H_{roi}^2}\rceil$ is centred at $(c_x, c_y)$ (zero-padded where out-of-bounds).
2.  The buffer is rotated by $-\theta_{tilt}$ about its centre using bilinear interpolation, aligning the fruit's long axis with the vertical.
3.  A tight $(W_{roi} \times H_{roi})$ crop is extracted from the centre of the rotated buffer.

If a high-resolution original image is available, the above procedure is repeated at the full-resolution scale (with coordinates scaled by $\text{scale} = W_{orig} / W_{preview}$) to preserve maximum detail for the metric computation.

---

### Step 3: Geometric Depth Reconstruction & Dual-Axis Unwrapping

**Objective**: Eliminate the perspective foreshortening introduced by the pineapple's convex surface curvature. The algorithm applies an inverse cylindrical projection independently along two orthogonal axes to recover physically accurate **Height** ($\ell_H$), **Width** ($\ell_W$), and **Volume** ($V$).

#### 3.1 Inverse Perspective Cylindrical Projection Model

**Physical model**: The pineapple is approximated as a finite cylinder of radius $r$. A pinhole camera at focal length $f$ images it from the front. Pixels near the lateral edges appear compressed because they image surface points that are physically farther from the camera than the central axis.

![Perspective Cylindrical Projection Geometry](perspective_projection.svg)

**Auto-scaling geometry**: To achieve a correction magnitude appropriate for a convex biological surface (real camera focal lengths typically produce undercorrection), the model parameters are set to equal the pixel width $W$ of the ROI crop:

$$f = W, \qquad r = W, \qquad \omega = W/2$$

where $\omega$ is the cylinder's half-width in the image plane.

**Cylinder reference distance**:

$$z_0 = f - \sqrt{r^2 - \omega^2}$$

**Per-column depth recovery**: For a destination pixel at column $x$ (centred coordinate $p_c^x = x - W/2$), the depth $z_c$ at which a ray from the pinhole intersects the cylinder surface is found by solving the quadratic ray–cylinder intersection equation. Defining:

$$a = \frac{(p_c^x)^2}{f^2} + 1, \qquad \Delta = 4z_0^2 - 4a(z_0^2 - r^2)$$

If $\Delta < 0$ the ray misses the cylinder and the destination pixel is left black. Otherwise:

$$z_c = \frac{2z_0 + \sqrt{\Delta}}{2a}$$

**Texture back-projection**: The source coordinates in the input image corresponding to destination pixel $(x, y)$ are:

$$x_{src} = p_c^x \cdot \frac{z_c}{f} + \frac{W}{2}, \quad y_{src} = p_c^y \cdot \frac{z_c}{f} + \frac{H}{2}$$

where $p_c^y = y - H/2$. Note that $z_c$ depends only on $x$, so the per-column computation is hoisted outside the inner loop (O(W) evaluations of $\sqrt{\cdot}$ rather than O(WH)).

Source pixels lying outside $[0, W) \times [0, H)$ are discarded. For source pixels at the very edge, the $2\times 2$ bilinear neighbourhood is clamped to valid indices to avoid a one-pixel black border:

$$I_{dst}(x,y) = \text{bilinear}\bigl(I_{src},\, x_{src},\, y_{src}\bigr)$$

#### 3.2 Dual-Axis Orthogonal Unwrapping

A single vertical cylindrical model corrects horizontal foreshortening but not the vertical curvature of the top and bottom poles. Two independent unwraps are performed:

**Vertical Unwrap** (`VERT_UNWRAP`): The upright ROI crop of dimensions $(W_{roi} \times H_{roi})$ is unwrapped directly. This projection expands the laterally foreshortened edges, recovering the true vertical extent of the fruit:

$$I_{vert} = \texttt{unwrap}(I_{roi}) \qquad [f = r = W_{roi}]$$

The `VERT_UNWRAP` image provides the physically accurate representation of the fruit's **true height**.

**Horizontal Unwrap** (`HORIZ_UNWRAP`): The ROI is first rotated 90° clockwise ($I_{rot}$, dimensions $H_{roi} \times W_{roi}$), then unwrapped:

$$I_{horiz} = \texttt{unwrap}(\texttt{rot90}(I_{roi})) \qquad [f = r = H_{roi}]$$

After rotation, the fruit's rotation axis (originally vertical) now lies along the horizontal direction of $I_{rot}$. The poles — originally at the top and bottom — are repositioned to the lateral extremities. The unwrapper, acting with $f = r = H_{roi} \geq W_{roi}$, applies a proportionally stronger horizontal stretch that eliminates the foreshortening along the fruit's rotation axis.

The `HORIZ_UNWRAP` image provides the physically accurate representation of the fruit's **true width**.

#### 3.3 Contour Extraction & Metric Computation

For each of the two unwrapped images, the following pipeline is applied to extract the minimal bounding geometry:

1.  **Global Otsu threshold** → binary mask.
2.  **0.25× downscale** (nearest-neighbour), followed by morphological Close (radius 2, $L_\infty$) then Open (radius 3, $L_\infty$), then **4× upscale** back to original resolution. This multi-scale approach suppresses internal noise while preserving the overall fruit outline. The $L_\infty$ (Chebyshev / square) structuring element is chosen here for computational efficiency on the downscaled image; at 0.25× resolution the distinction between $L_2$ and $L_\infty$ kernels is negligible relative to the fruit's overall outline scale.
3.  **Largest contour** by perimeter length is selected.
4.  **Minimum-area rectangle** (`min_area_rect`) of the largest contour: yields major axis length $\ell_{major}$ and minor axis length $\ell_{minor}$, and major-axis orientation $\varphi$.

**Dimension assignment**:

| Source | Quantity used | Physical interpretation |
|:---:|:---:|:---:|
| `VERT_UNWRAP` rect | $\ell_{major}$ | **Height** $\ell_H$ |
| `HORIZ_UNWRAP` rect | $\ell_{minor}$ | **Width** $\ell_W$ |

#### 3.4 Volume Integration (Disk Method with Dual-View Fusion)

The solid-of-revolution volume is computed from the `HORIZ_UNWRAP` contour using the **disk integration method**, with axial coordinates corrected using the `VERT_UNWRAP` major-axis length.

##### Coordinate Decomposition

Each `HORIZ_UNWRAP` contour point $\{(x_k, y_k)\}$ is decomposed relative to the rectangle centroid $(c_x, c_y)$ into two orthogonal components along the rotation axis (major-axis direction $\varphi$):

- **Along-axis coordinate** (slice position): $t_k = (x_k - c_x)\cos\varphi + (y_k - c_y)\sin\varphi$
- **Perpendicular distance** (cross-section radius): $r_k = -(x_k - c_x)\sin\varphi + (y_k - c_y)\cos\varphi$

##### Dual-View Axial Fusion

`HORIZ_UNWRAP` corrects **width-direction** foreshortening, so $r_k$ values are physically accurate cross-section radii. However, the **axial direction** remains uncorrected, leaving $t_k$ values foreshortened. To recover the true axial scale, $t_k$ is linearly rescaled using the major-axis length from `VERT_UNWRAP` (which has corrected the height direction):

$$t'_k = t_k \times \frac{\ell_{major}^{V}}{\ell_{major}^{H}}$$

where $\ell_{major}^{H}$ is the `HORIZ_UNWRAP` rectangle's major axis. This ratio captures the magnitude of axial perspective compression.

##### Single-Profile Integration

Only the **upper half** of the contour ($r_k \geq 0$) is retained for integration. A single profile is sufficient to define a body of revolution; using both halves would interleave upper and lower profiles when sorted by $t'_k$, producing incorrect slab interpolation between alternating profiles. After sorting the upper-half points by $t'_k$ in ascending order, consecutive point pairs contribute trapezoidal slabs:

$$V_{px} = \sum_{k} \pi \frac{r_k^2 + r_{k+1}^2}{2} \Delta t'_k, \qquad \Delta t'_k = t'_{k+1} - t'_k$$

The trapezoidal interpolation assumes that cross-section **area** varies linearly between adjacent sample points, which is more accurate than the outer-envelope approximation $\max(r_k, r_{k+1})$. The sum is accumulated in double precision (`f64`) to suppress rounding errors, then converted to physical units:

$$V = V_{px} \cdot \rho_{hr}^{-3} \quad [\text{mm}^3]$$

where $\rho_{hr} = \rho \cdot \text{scale}$ is the high-resolution pixel-to-millimetre ratio.

---

### Step 4: Fruitlet Eye Sizing & Whole-Fruit Count Estimation

**Objective**: Measure the representative equatorial fruitlet eye and estimate the total number of eyes on the whole fruit.

#### 4.1 Equatorial Representative Eye Measurement

A single representative eye is selected from the **equatorial zone** of the skin ROI and measured to obtain the fruitlet eye geometry.

##### Segmentation Strategy

Pineapple eyes are irregular hexagons/rhombi with diameters comparable to a 1 Yuan coin (~20–30 mm). In the `filled` binary (see below), adjacent eyes merge into large connected components via thin groove bridges. The algorithm separates them using **progressive morphological opening** and selects the best individual eye by structural scoring — without attempting to locate an eye centre first.

**Binary preparation** (on high-resolution upright ROI grayscale):

1.  Adaptive thresholding (block radius $= \lfloor R_{coin} \times \rho_{hr} \rceil$, $\delta = 0$).
2.  Morphological **closing** ($L_\infty$ norm, radius 2 px) to bridge hairline cracks within fragmented eyes.
3.  **Hole filling**: connected-component analysis on the inverted binary identifies interior dark regions. Regions not connected to the image border with area $\leq \lfloor (\text{block\_radius})^2 / 20 \rfloor$ px² (minimum 100 px²) are filled to white. This removes internal noise spots without filling the large enclosed grooves between eyes.

The resulting image is denoted `filled`.

**Progressive open CC search**:

The `filled` binary typically has all visible eyes merged into one or a few large connected components. To separate them, morphological **opening** is applied at progressively increasing radii $r \in \{0, 2, 4, \ldots, r_{max}\}$, where $r_{max} = \min\bigl(\max(\lfloor \text{block\_radius} / 10 \rfloor, 8),\, 25\bigr)$. At each radius:

1.  $B_r = B_\text{filled} \ominus \text{sq}(r) \oplus \text{sq}(r)$ ($L_\infty$ structuring element).
2.  Connected-component labelling (4-connectivity) of $B_r$; for each component $C_i$, the area $A_i$ and centroid $(\bar{x}_i, \bar{y}_i)$ are computed.
3.  **Candidate filtering** — a component $C_i$ passes if all three conditions hold:
    -  **Area**: $0.15\,A_{coin} \leq A_i \leq 1.8\,A_{coin}$, where $A_{coin} = \pi R_{coin}^2 \rho_{hr}^2$.
    -  **Equator band**: $|\bar{y}_i - H/2| \leq R_{coin} \cdot \rho_{hr}$, ensuring the eye straddles the equator.
    -  **Inner circle**: $\sqrt{(\bar{x}_i - W/2)^2 + (\bar{y}_i - H/2)^2} \leq 0.4\,W$, excluding peripheral eyes that may be clipped or distorted.
4.  **Scoring** — each surviving candidate is scored:
    $$s_i = \underbrace{\bigl(1 - \min\bigl(|A_i / A_{coin} - 0.7|,\; 1\bigr)\bigr)}_{\text{area match}} \;-\; \underbrace{\frac{\sqrt{(\bar{x}_i - W/2)^2 + (\bar{y}_i - H/2)^2}}{0.4\,W}}_{\text{position penalty}}$$
    The area-match target is set to $0.7 \times A_{coin}$ rather than $1.0$ because morphological opening erodes the eye boundary, systematically reducing its area below the coin reference. The best-scoring candidate at the current radius is selected.
5.  **Early termination**: the first radius that yields at least one valid candidate terminates the search. This ensures the minimum structurally necessary opening is used, preserving maximum boundary fidelity for the subsequent `min_area_rect` measurement.

##### Measurement

Because a single eye subtends only ±5°–10° on the fruit surface, perspective distortion is < 1%. Inverse cylindrical unwrapping at this scale would introduce more interpolation error than it corrects. Therefore, eye-level measurements are taken **directly** from the high-resolution image:

- **Long axis** $a_{eq}$ and **short axis** $b_{eq}$: the two edge lengths of the eye's minimum-area rectangle, converted to mm via $\rho_{hr}$.
- **Orientation angle** $\alpha$: the included angle between the eye's long axis and the fruit's vertical axis, normalised to $[0, \pi/2]$.

#### 4.2 Total Surface Area (Contour Integration)

The whole-fruit surface area $S$ is computed alongside volume in `unwrap_metrics.rs`, using the **surface-of-revolution formula** on the same `HORIZ_UNWRAP` upper-half $(t, r)$ profile:

$$S = \int 2\pi \, r(t) \sqrt{1 + \left(\frac{dr}{dt}\right)^2} \,dt$$

##### Envelope Smoothing

Unlike volume integration (where $\Delta t \approx 0 \Rightarrow \pi r^2 \Delta t \approx 0$), the surface area integral accumulates arc-length $ds = \sqrt{\Delta t^2 + \Delta r^2}$, which is highly sensitive to pixel-level zigzags in the sorted contour. To suppress this noise inflation:

1.  The $t$-axis is divided into equal-width bins (width $\approx t\_scale$, i.e. ~1 original pixel in the scaled coordinate system).
2.  Each bin retains only the **maximum** $r$ value (outer envelope).
3.  Empty bins are **linearly interpolated** from their neighbours; leading/trailing empties are filled with the nearest valid value.

The integration then proceeds over the smoothed envelope profile $\hat{r}(t)$:

$$S \approx \sum_i 2\pi \cdot \frac{\hat{r}_i + \hat{r}_{i+1}}{2} \cdot \sqrt{\Delta t_i^2 + \Delta\hat{r}_i^2}$$

#### 4.3 Polar Cap Area Subtraction

Pineapples have flat, eye-free areas at both poles — the **crown plate** (top) and **peduncle plate** (bottom). The surface area integral includes these regions; their area must be subtracted.

**Method**: the `HORIZ_UNWRAP` contour is projected to $(t, |r|)$ space (absolute $r$ for full half-widths). At each pole tip, the average $|r|$ within a window of depth $a_{eq}/2$ mm (converted to pixels via $\rho_{hr}$) is computed, yielding pole radii $r_{top}$ and $r_{bot}$. Each pole cap is modelled as a flat disc:

$$S_{cap} = \pi \, r_{top}^2 + \pi \, r_{bot}^2$$

> **Window rationale**: Within $a_{eq}/2$ (half one eye-height) of the fruit tip, no complete eye can fit; this zone is safely classified as eye-free.

#### 4.4 Per-Eye Footprint Area

$$A_{eye} = a_{eq} \times b_{eq}$$

> **Why not** $d_v \times d_h$? The projections $d_v = a_{eq}|\cos\alpha| + b_{eq}|\sin\alpha|$ and $d_h = a_{eq}|\sin\alpha| + b_{eq}|\cos\alpha|$ produce the **axis-aligned bounding box** of the rotated eye. This was meaningful for row/column counting (how many eyes fit per row), but for surface tiling, each eye's physical footprint is its own bounding rectangle $a_{eq} \times b_{eq}$, regardless of orientation. At $\alpha = 0.358\,\text{rad}$, $d_v \times d_h$ overestimates $A_{eye}$ by 66%.

#### 4.5 Total Eye Count

$$N_{total} = \left\lfloor \frac{S - S_{cap}}{A_{eye}} \right\rfloor = \left\lfloor \frac{S - S_{cap}}{a_{eq} \times b_{eq}} \right\rfloor$$

> Floor is used because the tight-packing assumption ignores inter-eye grooves, making the estimate a slight upper bound; flooring partially compensates.

---

## Reported Metrics

| Symbol | Name | Unit | Source |
|:---:|:---:|:---:|:---:|
| $\ell_H$ | Physical Height (major length) | mm | `VERT_UNWRAP` major axis |
| $\ell_W$ | Physical Width (minor length) | mm | `HORIZ_UNWRAP` minor axis |
| $V$ | Authentic Volume | mm³ | `HORIZ_UNWRAP` disc integration |
| $S$ | Surface Area | mm² | `HORIZ_UNWRAP` envelope integration |
| $a_{eq}$ | Equatorial eye long axis | mm | Eye min-area rectangle |
| $b_{eq}$ | Equatorial eye short axis | mm | Eye min-area rectangle |
| $\alpha$ | Eye orientation angle | rad | Eye long axis vs. fruit vertical |
| $N_{total}$ | Estimated whole-fruit eye count | — | $\lfloor (S - S_{cap}) / (a_{eq} \times b_{eq}) \rfloor$ |

---

## Summary of Advantages

- **Physical exactness**: The dual-axis unwrapping strategy explicitly accounts for both horizontal and vertical perspective foreshortening without heuristic bounding boxes.
- **Scale invariance**: All spatial parameters (area thresholds, morphology radii) are derived from the coin calibration and remain consistent across camera distances.
- **Texture-discriminated ROI selection**: Low-threshold guided grouping ensures peel-side fragment areas are correctly aggregated before texture scoring, making the edge-density × √area metric robust against Otsu contour fragmentation.
- **Computational efficiency**: Column-invariant depth values are precomputed in O(W) rather than O(WH), reducing the dominant square-root cost by a factor of H.
- **Noise-robust surface integration**: Envelope binning eliminates pixel-level zigzag inflation while preserving the fruit's true profile shape.
- **Anatomically-aware eye counting**: Polar cap subtraction accounts for the crown and peduncle plates that bear no fruitlet eyes.

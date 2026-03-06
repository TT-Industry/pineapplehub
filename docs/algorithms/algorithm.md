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

*Tier 1 (Strict)*: Selects the largest hull-area candidate satisfying all three constraints simultaneously:
$$\alpha > 0.95, \quad \phi \in [0.70,\,0.88], \quad \kappa > 0.85$$

*Tier 2 (Relaxed Fallback)*: If Tier 1 yields no result, candidates passing relaxed thresholds ($\alpha > 0.85$, $\phi \in [0.60, 0.92]$, $\kappa > 0.70$) are ranked by a penalty score that penalises deviation from the ideal circle:
$$s = -\bigl(10\,|\alpha - 1| + 5\,|\phi - \tfrac{\pi}{4}| + 5\,|1 - \kappa|\bigr)$$
The candidate with maximum $s$ is selected.

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

#### 2.2 Texture Richness Scoring

Each surviving candidate $\mathcal{C}_i$ is scored by a **texture richness** measure $\mathcal{S}_i$ that exploits the high-frequency surface structure of the pineapple skin:

1.  **Axis-aligned bounding box** $[x_0, x_1) \times [y_0, y_1)$ of the candidate's contour is computed; coordinates are clamped to the image boundary.

2.  **Local gradient magnitude**: For each non-background pixel $(x,y)$ inside the bounding box (background defined as luminance $\leq 15$), the first-order finite-difference gradient magnitude is computed:
$$\nabla I(x,y) = |I(x,y) - I(x+1,y)| + |I(x,y) - I(x,y+1)|$$

3.  **Mean edge density**: Averaged over all $N_{fg}$ non-background pixels in the region:
$$\bar{g}_i = \frac{1}{N_{fg}} \sum_{(x,y) \in \mathcal{C}_i} \nabla I(x,y)$$

4.  **Combined score** (balances texture richness with region size, using $\sqrt{A}$ rather than $A$ to prevent size dominance):
$$\mathcal{S}_i = \bar{g}_i \cdot \sqrt{A_i}$$

The candidate $\mathcal{C}^* = \arg\max_i \mathcal{S}_i$ is selected as the skin ROI.

*Physical rationale*: The pineapple skin is covered with raised fruitlet mounds separated by narrow dark crevices, producing high $\bar{g}$. The cut flesh surface is optically smooth, producing $\bar{g} \approx 0$. The coin, though high in edge contrast, is small in area, making $\sqrt{A}$ an effective size penalty.

#### 2.3 Rotated ROI Extraction

Given the selected candidate's minimum-area rectangle with centroid $(c_x, c_y)$, upright dimensions $(W_{roi}, H_{roi})$ — where the longer axis is assigned as height — and tilt angle $\theta_{tilt}$:

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

Pineapple eyes are irregular hexagons/rhombi with diameters comparable to a 1 Yuan coin (~20–30 mm). Using this geometric prior, a **strict → progressively relaxed** strategy is applied:

1.  Adaptive thresholding on the upright skin ROI grayscale (block radius derived from coin radius, δ = 0).
2.  Morphological opening (`erode` then `dilate`, $L_\infty$ norm, radius 2) to separate touching eyes.
3.  Connected-component labelling (4-connectivity); candidates are filtered by area (between 0.2× and 2.0× coin area).
4.  From remaining candidates whose bounding boxes intersect the equatorial line, each candidate's minimum-area rectangle aspect ratio is checked against a three-tier threshold (strict: [0.4, 1.0], then [0.3, 1.0], then [0.2, 1.0]). The first tier to yield candidates is used.
5.  Among passing candidates, the one whose centroid is closest to the fruit's longitudinal axis is selected.

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
- **Texture-discriminated ROI selection**: The edge-density × √area score reliably selects the textured skin surface over the smooth flesh with no colour-space assumptions.
- **Computational efficiency**: Column-invariant depth values are precomputed in O(W) rather than O(WH), reducing the dominant square-root cost by a factor of H.
- **Noise-robust surface integration**: Envelope binning eliminates pixel-level zigzag inflation while preserving the fruit's true profile shape.
- **Anatomically-aware eye counting**: Polar cap subtraction accounts for the crown and peduncle plates that bear no fruitlet eyes.

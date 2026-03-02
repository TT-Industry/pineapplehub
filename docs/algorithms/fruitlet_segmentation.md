# Pineapple Fruitlet Segmentation Pipeline

*Scale-Invariant Feature Density Analysis*

This document provides a comprehensive technical overview of the computer vision pipeline used in PineappleHub to count fruitlets on a pineapple. The pipeline is designed to be robust against lighting variations, fruit orientation, and distance by leveraging **Physical Scale Calibration** and **Spatial Feature Analysis**.

## Core Philosophy

The pineapple surface is modeled as a textured surface with specific physical feature sizes.

1.  **Physical Scale Invariance**: By detecting a physical reference (1 Yuan Coin, 25mm), we establish a `pixels_per_mm` scale. This allows all subsequent parameters (kernel sizes, thresholds, search windows) to be deterministically calculated, removing the need for fragile heuristics or manual tuning.
2.  **Skin vs. Flesh**: The skin contains dense, fruitlet-sized blobs. The flesh is smooth or contains only large cracks/fine noise.
3.  **Fruitlet Detection**: Fruitlets are distinct "blobs" with a specific physical size determined by the scale.

---

## Algorithm Pipeline

### Step 1: Scale Calibration & Pre-processing

**Objective**: Establish physical scale, suppress noise, and derive morphological parameters.

1.  **Gaussian Smoothing**:
    Apply a Gaussian kernel ($\sigma = 1.0$) to remove sensor noise while preserving edge structure.
    $$ I_{smooth} = I_{raw} * G_\sigma $$

2.  **Scale Calibration (Coin Detection)**:
    Identify the reference object to establish the mapping from pixels to millimeters.
    *   **Detection**: Apply Otsu's thresholding and contour analysis.
    *   **Selection**: Identify the candidate with the **Highest Circularity** (> 0.85).
    *   **Derivation**:
        $$ \text{pixels\_per\_mm} = \frac{\text{Radius}_{coin\_px}}{12.5mm} $$
        (Assuming 1 Yuan Coin diameter = 25mm).

3.  **Parameter Derivation (CV-Based)**:
    All morphological and spatial parameters are derived from the physical scale:
    *   **Patch Size**: $3.0 \times R_{coin}$ (Approx 37.5mm).
        *   *Rationale*: Large enough to capture a full fruitlet (foreground) plus surrounding gaps (background) to ensure valid contrast calculation.
    *   **Adaptive Threshold Radius**: $1.0 \times R_{coin}$ (Approx 12.5mm).
        *   *Rationale*: Matches the structural scale of a half-fruitlet, filtering out internal texture details while preserving the overall mound shape.
    *   **Morphology Radius**: $0.15 \times R_{coin}$ (Approx 1.8mm).
        *   *Rationale*: Conservative size to close small specular highlights/gaps without merging adjacent fruitlets.
    *   **Contrast (Threshold C)**: $C = -0.5 \times \sigma_{global}$.
        *   *Rationale*: Dynamically adapts to global image contrast, ensuring only peaks significantly brighter than the local neighborhood are retained.

---

### Step 2: Adaptive Thresholding & ROI Extraction

**Objective**: Segmentation of the "Skin" surface using the deterministically derived parameters.

1.  **Adaptive Thresholding**:
    Use a local adaptive threshold (Bernsen/Mean) parameterized by the derived $R$ and Contrast $C$ (from patch variance).
    $$ B(x,y) = \begin{cases} 1 & \text{if } I(x,y) > \mu_{R}(x,y) + 0.5 \times \sigma_{global} \\ 0 & \text{otherwise} \end{cases} $$

2.  **Morphological Closing**:
    Fuse fragmented binary features using the derived radius.
    $$ B_{fused} = \text{Close}(B, R_{morph}) $$

3.  **Physical Area Filter**:
    *   Remove blobs where $\text{Area} < 0.2 \times \text{Area}_{coin}$.
    *   *Rationale*: Objects significantly smaller than a coin are physically too small to be valid fruitlets or skin patches, regardless of camera distance.

4.  **ROI Selection (Feature & Color Fusion)**:
    Distinguish between Skin (Target) and Flesh (Background).
    *   Iterate through top candidate regions (passed area filter).
    *   **Feature Density Score** ($S_{feature}$):
        1.  Crop the candidate ROI.
        2.  **Adaptive Thresholding**: Apply local thresholding with a smaller radius ($R \approx 6mm$) to detect individual fruitlets.
        3.  **Blob Filtering**: Count contours with an area consistent with a fruitlet:
            $$ Area \in [0.2 \times A_{target}, 2.0 \times A_{target}] $$
            (Target Area $A_{target} \approx \pi \times (12.5mm)^2$).
        4.  **Score**: The count of valid fruitlet blobs is used as the positive signal.
        $$ S_{feature} = N_{valid\_blobs} $$
    *   **Color Penalty** ($P_{flesh}$):
        1.  **Flesh Detection**: Calculate ratio of pixels with H $\in [35, 85]$ (Yellow/White) and moderate S/V.
        2.  **Shadow Detection**: Calculate ratio of dark pixels (Luma < 60).
        3.  **Penalty Logic**: If a region is predominantly Yellow AND lacks dark gaps (Dark Ratio < 2%), it is classified as Flesh and heavily penalized ($P_{flesh} = 0.9$).
    *   **Combined Score**:
        $$ S_{final} = S_{feature} \times (1.0 - P_{flesh}) $$
    *   **Selection**: The region with the highest Score $S_{final}$ is selected as the Skin ROI.



---

---

### Step 3: Geometric Depth Reconstruction & Unwrapping

**Objective**: Eliminate the severe perspective foreshortening caused by the pineapple's convex surface curvature near the image boundaries. By unwrapping the 2D projection back onto a 3D cylindrical model, we extract dimensionally accurate physical baseline lengths (Major and Minor axes) and compute an un-biased authentic volume.

1.  **Inverse Perspective Cylindrical Projection Model**:
    The pineapple is mathematically approximated as a cylindrical surface. From the camera's perspective, as the deviation angle from the central axis increases across the Cartesian pixel plane, the physical object depth $z_c$ increases drastically, causing severe visual compression (foreshortening) at the fruit's lateral edges.

    *Projection Physical Parameters:*
    To induce a sufficiently strong distortion-canceling effect required for such a convex object, we omit the camera's actual hardware focal length (which often yields a projection that is too flat). Instead, we establish a robust auto-scaling geometric model:
    - **Focal Length ($f$)** and **Cylinder Radius ($r$)** are dynamically assigned to equal the pixel width of the localized fruitlet bounding box on the image plane: $f = W$, $r = W$.
    - The cylinder half-width $\omega = W/2$.

    *Inverse Depth Calculation ($z_c$):*
    For any pixel on the 2D projection plane deviating from the center by coordinates $(pc_x, pc_y)$, its original object distance $z_c$ in the 3D camera coordinate system is analytically derived by solving the ray-cylinder intersection quadratic equation:
    $$ z_c = \frac{2z_0 + \sqrt{4z_0^2 - 4(pc_x^2/f^2 + 1)(z_0^2 - r^2)}}{2(pc_x^2/f^2 + 1)} $$
    Where $z_0 = f - \sqrt{r^2 - \omega^2}$ defines the distance to the cylinder's closest surface point.
    *(Note: if the discriminant is negative, the ray misses the cylinder and the pixel is discarded).*

    *Perspective Un-projection (Texture Mapping)*:
    The recovered un-foreshortened 3D coordinates $(X, Y)$ are mapped by scaling the image coordinates with the calculated depth ratio:
    $$ X = pc_x \frac{z_c}{f}, \quad Y = pc_y \frac{z_c}{f} $$
    Since $z_c$ increases non-linearly towards the cylinder's curved lateral horizons, the spatial coordinates at the left and right edges are significantly stretched, actively expanding and canceling the perspective roll-off effect. Bilinear interpolation is then applied to reconstruct the discrete pixel grid of the unwrapped image.

2.  **Dual-Axis Orthogonal Unwrapping Scheme**:
    Because a single vertical cylindrical approximation only corrects horizontal foreshortening (stretching the left and right peripheral edges) and fails to correct the vertical longitudinal curvature (the top and bottom poles), our pipeline employs a Dual-Axis Independent Unwrapping algorithm to extract the mathematically exact Physical Height and Width.

    *   **Vertical Cylinder Assumption (`VERT_UNWRAP`)**:
        - **Operation**: The standard cylindrical unwrap is applied to the upright (or software-aligned) image.
        - **Physical Effect**: The depth $z_c$ calculation compensates for the horizontal curvature. The side edges are flattened.
        - **Dimension Extraction**: Because the fruit naturally stands upright with its longitudinal axis parallel to the cylinder's $Y$-axis, this projection provides the most physically accurate, un-distorted representation of the fruit's **True Height**.
        - **Assignment**: The major bounding axis (length) extracted from the `VERT_UNWRAP` contour's minimal area rectangle is strictly assigned as the **Physical Height ($Major\_Length$)**.

    *   **Horizontal Cylinder Assumption (`HORIZ_UNWRAP`)**:
        - **Operation**: The source image is physically rotated by 90° ($W_{rot} = H_{orig}$, $H_{rot} = W_{orig}$), and the identical cylindrical unwrap algorithm is applied.
        - **Parameter Shift**: Due to the rotation, the algorithm dynamically adopts properties of the newly oriented image, establishing $f_{horiz} = H_{orig}$ and $r_{horiz} = H_{orig}$. Since the fruit is typically taller than it is wide, this artificially larger radius imposes an intensive lateral stretch that forcefully flattens the fruit's longitudinal poles (which now lie along the left and right edges of the rotated image).
        - **Dimension Extraction**: The depth compensation perfectly counteract foreshortening along the fruit's true equator (now aligned with the vertical axis of the rotated canvas). This provides the mathematically exact un-distorted representation of the fruit's **True Width**.
        - **Assignment**: The minor bounding axis (width) extracted from the `HORIZ_UNWRAP` contour's minimal area rectangle is strictly assigned as the **Physical Width ($Minor\_Length$)**.

3.  **Volume Integration Calculation**:
    The Authentic Volume of the fruitlet is calculated by performing a 1D integral (method of cylindrical shells/disks) across the flattened slice profile generated by the `HORIZ_UNWRAP`.
    - The discrete contour points belonging to the unwrapped geometry are sorted along the horizontal axis.
    - For each sequential pair of points representing a vertical slice of width $dx$, the volume contribution is calculated utilizing the rotational volume formula $dV = \frac{\pi}{3} (R_{i}^3 - R_{i-1}^3)$, where $R$ is the distance from the central axis to the contour edge.
    - These pixel-space volumes are then converted to physical cubic millimeters ($mm^3$) using the calibrated `pixels_per_mm` scale established in Step 1.

## Advantages

*   **Physical Exactness**: The dual unwrapping strategy explicitly matches the perspective geometry of convex biological shapes without relying on heuristic bounding boxes.
*   **Omni-Directional Robustness**: The generalized model handles both vertical and horizontal perspective foreshortening, more accurate than a single-axis cylindrical model.
*   **Shape Adaptation**: The multi-scale competition mechanism "lets the data speak," locking onto features via local contrast maximization rather than enforcing a perfect geometric shape.
*   **Efficiency**: Removes complex IFFT and geometric interpolation, operating directly in the physical coordinate domain.
*   **Scale Invariance**: Remains fully grounded in the physical coin calibration.

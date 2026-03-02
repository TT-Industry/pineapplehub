use ::image::{DynamicImage, EncodableLayout, GrayImage, Luma, Rgba, imageops};
use iced::{
    Color, ContentFit, Element, Fill, Shadow,
    time::Instant,
    widget::{button, container, float, image, mouse_area, space, stack},
};
use image_debug_utils::{contours::remove_hypotenuse_owned, rect::to_axis_aligned_bounding_box};

// ... (imports)
use imageproc::{
    contours::{self, BorderType},
    distance_transform::Norm,
    drawing::{draw_hollow_circle_mut, draw_line_segment_mut},
    filter::{gaussian_blur_f32, median_filter},
    geometric_transformations::{Interpolation, rotate_about_center},
    geometry::min_area_rect,
};
// Removed rustfft usage
use sipper::{Straw, sipper};
// ...

// ...

fn extract_best_roi(
    fused: &GrayImage,
    smoothed: &GrayImage,
    _color_image: &DynamicImage,
    px_per_mm: f32,
) -> Result<(GrayImage, Option<RotatedRect>), Error> {
    // 1. Find Contours in Low-Res Fused Image
    let contours = contours::find_contours::<i32>(fused);

    // Filter out rulers/straight edges
    let contours = remove_hypotenuse_owned(contours, 5.0, Some(BorderType::Outer));

    // 2. Filter by Physical Area (Doc Step 2.3)
    // Area > 0.2 * Area_coin
    let coin_area_px = std::f32::consts::PI * (COIN_RADIUS_MM * px_per_mm).powi(2);
    let min_area = 0.2 * coin_area_px;

    let mut candidates = Vec::with_capacity(contours.len());
    for contour in contours {
        if contour_area(&contour) > min_area {
            candidates.push(contour);
        }
    }

    // 3. Score Candidates (Feature Density + Color Penalty)
    // Doc Step 2.4
    let mut stats = Vec::with_capacity(candidates.len());

    for (i, contour) in candidates.iter().enumerate() {
        let rect = min_area_rect(&contour.points);
        let combined_score = contour_area(&contour); // Simple Area tracking fits Python logic

        // Store: (index, area, score, contour)

        // Store: (index, area, score, contour)
        // We actually want Rotated Rect info for final crop
        let r_rect = get_rotated_rect_info(&rect);
        stats.push((i, r_rect, combined_score));
    }

    // Sort by Score Descending
    stats.sort_by(|a, b| b.2.total_cmp(&a.2));

    if let Some((_, r_rect, score)) = stats.first() {
        use web_sys::console;
        console::log_1(
            &format!("[Step 5] Best ROI Score: {:.2}, Rect: {:?}", score, r_rect).into(),
        );

        // Extract the BEST Rotated ROI from SMOOTHED
        // We want the Texture for unwrapping.

        // Rotation Logic (Same as before)
        // ... (reuse existing rotation logic or simplify)
        // The existing function had complex rotation logic. I should keep it or rewrite it.
        // To be safe, let's keep the rotation logic from the original file if possible, or re-implement cleanly.

        // Re-implementing rotation extraction for the Best ROI:
        let (w, h) = smoothed.dimensions();
        // Expand ROI to ensure we capture the corners after rotation
        let diag = (r_rect.width.powi(2) + r_rect.height.powi(2)).sqrt().ceil();
        let cx = r_rect.cx;
        let cy = r_rect.cy;

        // 1. Calculate an exact bounding box symmetrically around cx, cy
        // even if it extends out of the image bounds
        let safe_x = (cx - diag / 2.0).round() as i32;
        let safe_y = (cy - diag / 2.0).round() as i32;
        let safe_w = diag.round() as u32;
        let safe_h = diag.round() as u32;

        // Create a padded image buffer to hold the crop safely
        use ::image::ImageBuffer;
        let mut padded_crop: ::image::GrayImage = ImageBuffer::new(safe_w, safe_h);

        // Copy pixels from the original image into the padded buffer
        // where coordinates overlap
        for y in 0..safe_h {
            for x in 0..safe_w {
                let src_x = safe_x + x as i32;
                let src_y = safe_y + y as i32;
                if src_x >= 0 && src_x < w as i32 && src_y >= 0 && src_y < h as i32 {
                    padded_crop.put_pixel(x, y, *smoothed.get_pixel(src_x as u32, src_y as u32));
                } else {
                    padded_crop.put_pixel(x, y, Luma([0]));
                }
            }
        }

        // 2. Rotate around the EXACT center of our padded symmetrical box
        let rotated_full = rotate_about_center(
            &padded_crop,
            r_rect.angle_rad,
            Interpolation::Bilinear,
            Luma([0]),
        );

        // 3. Crop the upright rectangle from the exact center of rotated image
        let center_x = rotated_full.width() as f32 / 2.0;
        let center_y = rotated_full.height() as f32 / 2.0;

        let extract_x = (center_x - r_rect.width / 2.0).round() as i32;
        let extract_y = (center_y - r_rect.height / 2.0).round() as i32;

        let best_crop = imageops::crop_imm(
            &rotated_full,
            extract_x.max(0) as u32,
            extract_y.max(0) as u32,
            r_rect.width.round() as u32,
            r_rect.height.round() as u32,
        )
        .to_image();

        Ok((best_crop, Some(*r_rect)))
    } else {
        Err(Error::General("No valid ROI found".into()))
    }
}

use std::sync::Arc;

use crate::{Message, Preview, error::Error, utils::dynamic_image_to_handle};

pub(crate) type EncodedImage = Vec<u8>;

/// Matches `docs/user_guide/debug_interpretation_zh.md`
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Step {
    Original,
    Smoothing,        // Step 1
    ScaleCalibration, // Step 2 (Replacs ExclusionMap)
    Binary,           // Step 3 (Texture Patch)
    BinaryFusion,     // Step 4 (Morphology Closing)
    RoiExtraction,    // Step 5 (Morphology / ROI Extraction)
}

#[derive(Clone, Debug)]
pub(crate) struct FruitletMetrics {
    pub major_length: f32,
    pub minor_length: f32,
    pub volume: f32,
}

#[derive(Clone, Debug)]
pub(crate) struct Intermediate {
    pub(crate) current_step: Step,
    pub(crate) preview: Preview,
    /// Derived from Step 2: Scale Calibration
    pub(crate) pixels_per_mm: Option<f32>,
    /// Carried over context image (e.g., Reconstructed Surface)
    pub(crate) context_image: Option<Arc<DynamicImage>>,
    /// ROI Image (Color, High-Res if available) - Persisted for Step 7 Viz
    pub(crate) roi_image: Option<Arc<DynamicImage>>,
    /// Original High Resolution Image (for FFT)
    pub(crate) original_high_res: Option<Arc<DynamicImage>>,
    /// Extracted from EXIF (FocalLength * Px/Unit). Required for Perspective Correction.
    pub(crate) focal_length_px: Option<f32>,
    /// Persisted coordinate transform for mapping points back to original image
    pub(crate) transform: Option<CoordinateTransform>,
    /// Calculated metrics: major length, minor length, volume
    pub(crate) metrics: Option<FruitletMetrics>,
}

#[derive(Clone, Debug)]
pub(crate) struct CoordinateTransform {
    pub bbox_x: u32,
    pub bbox_y: u32,
    pub extract_x: i32,
    pub extract_y: i32,
    pub local_width: u32,
    pub local_height: u32,
    pub angle_rad: f32,
    pub radius: f32,
    pub focal_length_px: f32,
}

const COIN_RADIUS_MM: f32 = 12.5;

impl Intermediate {
    pub(crate) fn process(self) -> impl Straw<Self, EncodedImage, Error> {
        sipper(async move |mut sender| {
            let image: DynamicImage = self.preview.clone().into();

            // Generate Blurhash for UI transition
            if let Ok(blurhash) = blurhash::encode(
                4,
                3,
                image.width(),
                image.height(),
                image.to_rgba8().as_bytes(),
            ) {
                if let Ok(decoded) = blurhash::decode(&blurhash, 20, 20, 1.0) {
                    let _ = sender.send(decoded).await;
                } else {
                    use web_sys::console;
                    console::error_1(&"Blurhash decode failed".into());
                }
            }

            match self.current_step {
                Step::Original => {
                    // Step 1: Smoothing
                    // Doc: Gaussian Smoothing (sigma = 1.0)
                    let smoothed = gaussian_blur_f32(&median_filter(&image.to_rgba8(), 1, 1), 1.0);

                    Ok(Intermediate {
                        current_step: Step::Smoothing,
                        preview: Preview::ready(smoothed.into(), Instant::now()),
                        pixels_per_mm: None, // Not calculated yet
                        context_image: None,
                        roi_image: None,
                        original_high_res: self.original_high_res.clone(),
                        focal_length_px: self.focal_length_px,
                        transform: None,
                        metrics: None,
                    })
                }
                Step::Smoothing => {
                    // Step 2: Scale Calibration
                    // Doc: Detect coin (Circularity > 0.85). Calculate pixels_per_mm.
                    let smoothed_luma = image.to_luma8();
                    let (vis_img, px_per_mm) = perform_scale_calibration(&smoothed_luma);

                    Ok(Intermediate {
                        current_step: Step::ScaleCalibration,
                        preview: Preview::ready(vis_img.into(), Instant::now()),
                        pixels_per_mm: px_per_mm,

                        context_image: Some(Arc::new(DynamicImage::ImageLuma8(smoothed_luma))),
                        roi_image: None,
                        original_high_res: self.original_high_res.clone(),
                        focal_length_px: self.focal_length_px,
                        transform: None,
                        metrics: None,
                    })
                }
                Step::ScaleCalibration => {
                    // Step 3: Global Otsu Thresholding
                    let smoothed = self
                        .context_image
                        .as_ref()
                        .ok_or(Error::General("Missing context".into()))?
                        .as_luma8()
                        .ok_or(Error::General("Invalid context image format".into()))?;

                    use imageproc::contrast::{ThresholdType, otsu_level, threshold};

                    // Calculate the optimal global threshold using Otsu's method
                    let level = otsu_level(smoothed);

                    // Create binary image. threshold() marks pixels >= level with 255
                    let binary = threshold(smoothed, level, ThresholdType::Binary);

                    // Debug Logs
                    use web_sys::console;
                    console::log_1(&format!("[Step 3] Otsu Threshold Level: {}", level).into());

                    Ok(Intermediate {
                        current_step: Step::Binary,
                        preview: Preview::ready(binary.into(), Instant::now()),
                        pixels_per_mm: self.pixels_per_mm,
                        context_image: self.context_image.clone(),
                        roi_image: None,
                        original_high_res: self.original_high_res.clone(),
                        focal_length_px: self.focal_length_px,
                        transform: None,
                        metrics: None,
                    })
                }
                Step::Binary => {
                    // Step 4: Binary Fusion (Morphological Closing followed by Opening)
                    // Replicating Python structure: cv2.MORPH_CLOSE (r=2) then cv2.MORPH_OPEN (r=3)
                    let binary = image.to_luma8(); // Previous step output
                    use imageproc::morphology::{close, open};

                    // Exact equivalent of Python's OpenCV processing on scaled down images
                    let closed = close(&binary, Norm::L2, 2);
                    let opened = open(&closed, Norm::L2, 3);

                    Ok(Intermediate {
                        current_step: Step::BinaryFusion,
                        preview: Preview::ready(opened.into(), Instant::now()),
                        pixels_per_mm: self.pixels_per_mm,
                        context_image: self.context_image.clone(), // Keep smoothed image for Step 5 crop
                        roi_image: None,
                        original_high_res: self.original_high_res.clone(),
                        focal_length_px: self.focal_length_px,
                        transform: None,
                        metrics: None,
                    })
                }
                Step::BinaryFusion => {
                    // Step 5: ROI Extraction (Morphology / ROI Extraction) & Unwrapping
                    let fused = image.to_luma8();
                    let smoothed = self
                        .context_image
                        .as_ref()
                        .ok_or(Error::General("Missing context image".into()))?
                        .as_luma8()
                        .ok_or(Error::General("Context image is not Luma8".into()))?;
                    let px_per_mm = self
                        .pixels_per_mm
                        .ok_or(Error::General("Missing scale".into()))?;

                    let (roi_img_low_res, roi_rect_low_res) =
                        extract_best_roi(&fused, smoothed, &image, px_per_mm)?;

                    // Extract ROI
                    if let Some(roi_rect_low_res) = roi_rect_low_res {
                        let roi_arc_low_res = Arc::new(DynamicImage::ImageLuma8(roi_img_low_res));

                        let mut best_roi = roi_arc_low_res.clone();
                        let mut transform = None;

                        let focal_length = self.focal_length_px.ok_or(Error::General(
                            "Missing EXIF Focal Length. Cannot perform perspective correction."
                                .into(),
                        ))?;

                        let gray_original = if let Some(ref hr) = self.original_high_res {
                            hr.to_luma8()
                        } else {
                            image.to_luma8()
                        };

                        let scale = gray_original.width() as f32 / image.width() as f32;

                        let hr_cx = roi_rect_low_res.cx * scale;
                        let hr_cy = roi_rect_low_res.cy * scale;
                        let hr_w = (roi_rect_low_res.width * scale).round() as u32;
                        let hr_h = (roi_rect_low_res.height * scale).round() as u32;

                        // Unrotated Context ROI for center panel preview
                        use ::image::{DynamicImage, ImageBuffer, Luma, Rgba, RgbaImage, imageops};
                        use imageops::FilterType;

                        let diag = ((hr_w as f32).powi(2) + (hr_h as f32).powi(2)).sqrt();
                        let safe_x = (hr_cx - diag / 2.0).round() as i32;
                        let safe_y = (hr_cy - diag / 2.0).round() as i32;
                        let safe_w = diag.ceil() as u32;
                        let safe_h = diag.ceil() as u32;

                        let bbox_x = safe_x.max(0) as u32;
                        let bbox_y = safe_y.max(0) as u32;
                        let bbox_w = safe_w;
                        let bbox_h = safe_h;

                        let mut padded_crop: ImageBuffer<Luma<u8>, Vec<u8>> =
                            ImageBuffer::new(safe_w, safe_h);

                        for y in 0..safe_h {
                            for x in 0..safe_w {
                                let src_x = safe_x + x as i32;
                                let src_y = safe_y + y as i32;
                                if src_x >= 0
                                    && src_x < gray_original.width() as i32
                                    && src_y >= 0
                                    && src_y < gray_original.height() as i32
                                {
                                    padded_crop.put_pixel(
                                        x,
                                        y,
                                        *gray_original.get_pixel(src_x as u32, src_y as u32),
                                    );
                                } else {
                                    padded_crop.put_pixel(x, y, Luma([0]));
                                }
                            }
                        }

                        // To replicate cv2.warpPerspective which inherently rotates the ROI to upright:
                        // 1. Rotate `center_panel` around its center by the box angle.
                        // `min_area_rect` gives us an angle where width/height are aligned.
                        use imageproc::geometric_transformations::{
                            Interpolation, rotate_about_center,
                        };
                        let rotated_panel = rotate_about_center(
                            &padded_crop,
                            roi_rect_low_res.angle_rad,
                            Interpolation::Bilinear,
                            Luma([0]),
                        );

                        // 2. Crop exactly the (hr_w, hr_h) region from the center of this rotated panel
                        let rot_center_x = rotated_panel.width() as f32 / 2.0;
                        let rot_center_y = rotated_panel.height() as f32 / 2.0;
                        let crop_x = (rot_center_x - hr_w as f32 / 2.0).round() as i32;
                        let crop_y = (rot_center_y - hr_h as f32 / 2.0).round() as i32;

                        let warped = imageops::crop_imm(
                            &rotated_panel,
                            crop_x.max(0) as u32,
                            crop_y.max(0) as u32,
                            hr_w,
                            hr_h,
                        )
                        .to_image();

                        best_roi = Arc::new(DynamicImage::ImageLuma8(warped.clone()));

                        transform = Some(CoordinateTransform {
                            bbox_x,
                            bbox_y,
                            extract_x: 0,
                            extract_y: 0,
                            local_width: bbox_w,
                            local_height: bbox_h,
                            angle_rad: roi_rect_low_res.angle_rad,
                            radius: best_roi.width() as f32 / 2.0,
                            focal_length_px: focal_length,
                        });

                        // Direct tilt-aware Unwrapping
                        // Direct tilt-aware Unwrapping
                        use crate::correction::{unwrap, unwrap_with_radius};

                        // ---- 6. Mathematical Forward Mapping for Exact Metrics & Unwrapped Views ----
                        // Panel 1 & 2: vertical unwrap
                        let hr_w = warped.width();
                        let hr_h = warped.height();

                        // Left panel: vertical_unwrapped (Vertical Cylinder)
                        let vert_unwrapped = unwrap(&warped);

                        // Right panel: horizontal_unwrapped (Horizontal Cylinder)
                        let horiz_rotated = ::image::imageops::rotate90(&warped);
                        // Reverting to `unwrap` because Python's `unwrap(cv2.rotate(warped))` implicitly uses
                        // the rotated image's width (which is `hr_h`) as `f` and `r`.
                        // While physically "distorted", this is the exact projection Python uses for volume integration.
                        let horiz_unwrapped = unwrap(&horiz_rotated);

                        // Build 3 panel image: vert_unwrapped (w x h) | warped | horiz_unwrapped (h x w)
                        let padding = 10;
                        let panel1_w = hr_w;
                        let panel2_w = hr_w; // warped width
                        let panel3_w = hr_h;

                        let max_h = hr_h.max(hr_w);
                        let total_w = panel1_w + panel2_w + panel3_w + padding * 2;

                        let mut combined_preview: ImageBuffer<Luma<u8>, Vec<u8>> =
                            ImageBuffer::new(total_w, max_h);

                        for p in combined_preview.pixels_mut() {
                            *p = Luma([255]);
                        }

                        // Offsets
                        let x1 = 0;
                        let x2 = panel1_w + padding;
                        let x3 = panel1_w + panel2_w + padding * 2;

                        // Center them vertically
                        let y1 = (max_h - hr_h) / 2;
                        let y2 = (max_h - hr_h) / 2; // warped height
                        let y3 = (max_h - hr_w) / 2;

                        imageops::replace(
                            &mut combined_preview,
                            &vert_unwrapped,
                            x1 as i64,
                            y1 as i64,
                        );
                        imageops::replace(&mut combined_preview, &warped, x2 as i64, y2 as i64);
                        imageops::replace(
                            &mut combined_preview,
                            &horiz_unwrapped,
                            x3 as i64,
                            y3 as i64,
                        );

                        let mut color_preview: RgbaImage =
                            DynamicImage::ImageLuma8(combined_preview).to_rgba8();

                        // --- DRAW TIGHT BOUNDING BASELINES ---
                        use imageproc::contours::find_contours_with_threshold;
                        use imageproc::distance_transform::Norm;
                        use imageproc::drawing::draw_line_segment_mut;
                        use imageproc::morphology::{dilate, erode};

                        let red = Rgba([255, 0, 0, 255]);

                        // Helper to find minAreaRect bounds for an unwrapped image
                        let get_tight_bounds = |img: &ImageBuffer<Luma<u8>, Vec<u8>>| -> Option<(
                            [imageproc::point::Point<i32>; 4],
                            Vec<imageproc::point::Point<i32>>,
                        )> {
                            let otsu = imageproc::contrast::otsu_level(img);
                            let binary = imageproc::contrast::threshold(
                                img,
                                otsu,
                                imageproc::contrast::ThresholdType::Binary,
                            );

                            // Python matching morphology: resize to 0.25x
                            let w = binary.width();
                            let h = binary.height();
                            let small_w = (w as f32 * 0.25).max(1.0) as u32;
                            let small_h = (h as f32 * 0.25).max(1.0) as u32;

                            let small = imageops::resize(
                                &binary,
                                small_w,
                                small_h,
                                imageops::FilterType::Nearest,
                            );

                            // Morphology close (dilate then erode) then open (erode then dilate)
                            // 5x5 equivalent = radius 2
                            let d1 = dilate(&small, Norm::LInf, 2);
                            let closed = erode(&d1, Norm::LInf, 2);
                            // 7x7 equivalent = radius 3
                            let e1 = erode(&closed, Norm::LInf, 3);
                            let opened = dilate(&e1, Norm::LInf, 3);

                            // Restore size
                            let restored =
                                imageops::resize(&opened, w, h, imageops::FilterType::Nearest);

                            let contours = find_contours_with_threshold(&restored, 127);
                            if let Some(longest) =
                                contours.into_iter().max_by_key(|c| c.points.len())
                            {
                                let corners = min_area_rect(&longest.points);
                                Some((corners, longest.points))
                            } else {
                                None
                            }
                        };

                        let dash_length = 10.0;
                        let gap_length = 5.0;

                        let mut calculated_metrics = None;

                        // Panel 1: vert_unwrapped (Vertical Cylinder)
                        // Left panel: top/bottom edges solid, vertical centerline dashed
                        if let Some((mut box_points, contour)) = get_tight_bounds(&vert_unwrapped) {
                            // --- Python Volume Calculation Port ---
                            // We must compute lengths and angles BEFORE sorting the points.
                            // Sorting by Y ruins the perimeter topology, turning edges into diagonals.
                            let dx1 = (box_points[0].x - box_points[1].x) as f32;
                            let dy1 = (box_points[0].y - box_points[1].y) as f32;
                            let l1 = (dx1 * dx1 + dy1 * dy1).sqrt();

                            let dx2 = (box_points[1].x - box_points[2].x) as f32;
                            let dy2 = (box_points[1].y - box_points[2].y) as f32;
                            let l2 = (dx2 * dx2 + dy2 * dy2).sqrt();

                            let (major, minor, dx_major, dy_major) = if l1 > l2 {
                                (l1, l2, dx1, dy1)
                            } else {
                                (l2, l1, dx2, dy2)
                            };

                            let angle = dy_major.atan2(dx_major);
                            // To match distances, center is average of all 4 corners
                            let cx = (box_points[0].x
                                + box_points[1].x
                                + box_points[2].x
                                + box_points[3].x) as f32
                                / 4.0;
                            let cy = (box_points[0].y
                                + box_points[1].y
                                + box_points[2].y
                                + box_points[3].y) as f32
                                / 4.0;

                            box_points.sort_by_key(|p| p.y);
                            // Top edge = 0 and 1, Bottom edge = 2 and 3
                            let top_mid_x =
                                (box_points[0].x as f32 + box_points[1].x as f32) / 2.0 + x1 as f32;
                            let top_mid_y =
                                (box_points[0].y as f32 + box_points[1].y as f32) / 2.0 + y1 as f32;

                            let bot_mid_x =
                                (box_points[2].x as f32 + box_points[3].x as f32) / 2.0 + x1 as f32;
                            let bot_mid_y =
                                (box_points[2].y as f32 + box_points[3].y as f32) / 2.0 + y1 as f32;

                            draw_line_segment_mut(
                                &mut color_preview,
                                (
                                    box_points[0].x as f32 + x1 as f32,
                                    box_points[0].y as f32 + y1 as f32,
                                ),
                                (
                                    box_points[1].x as f32 + x1 as f32,
                                    box_points[1].y as f32 + y1 as f32,
                                ),
                                red,
                            );
                            draw_line_segment_mut(
                                &mut color_preview,
                                (
                                    box_points[2].x as f32 + x1 as f32,
                                    box_points[2].y as f32 + y1 as f32,
                                ),
                                (
                                    box_points[3].x as f32 + x1 as f32,
                                    box_points[3].y as f32 + y1 as f32,
                                ),
                                red,
                            );

                            let dx = bot_mid_x - top_mid_x;
                            let dy = bot_mid_y - top_mid_y;
                            let dist = (dx * dx + dy * dy).sqrt();
                            if dist > 0.1 {
                                let (ux, uy) = (dx / dist, dy / dist);

                                let mut curr = 0.0;
                                while curr < dist {
                                    let start = (top_mid_x + ux * curr, top_mid_y + uy * curr);
                                    let mut end_curr = curr + dash_length;
                                    if end_curr > dist {
                                        end_curr = dist;
                                    }
                                    let end =
                                        (top_mid_x + ux * end_curr, top_mid_y + uy * end_curr);
                                    draw_line_segment_mut(&mut color_preview, start, end, red);
                                    curr += dash_length + gap_length;
                                }
                            }

                            let mut valid_points = Vec::with_capacity(contour.len());
                            for pt in &contour {
                                let lx = pt.x as f32 - cx;
                                let ly = pt.y as f32 - cy;
                                // Project point onto shifted major axis directly
                                let rot_x = lx * angle.cos() + ly * angle.sin();
                                if rot_x >= 0.0 {
                                    valid_points.push(rot_x.abs());
                                }
                            }

                            valid_points.sort_by(|a, b| {
                                a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)
                            });

                            let mut vol = 0.0;
                            for w in valid_points.windows(2) {
                                vol += std::f32::consts::PI / 3.0 * (w[1].powi(3) - w[0].powi(3));
                            }

                            if let Some(px_per_mm) = self.pixels_per_mm {
                                let hr_px_per_mm = px_per_mm * scale;
                                let mm_per_px = 1.0 / hr_px_per_mm;
                                let v_major = major * mm_per_px;
                                let v_minor = minor * mm_per_px;
                                let v_vol = vol * mm_per_px.powi(3);

                                log::info!(
                                    "VERT_UNWRAP (Corrects Width): V_major(Height)={}, V_minor(Width)={}, V_vol={}",
                                    v_major,
                                    v_minor,
                                    v_vol
                                );

                                calculated_metrics = Some(FruitletMetrics {
                                    major_length: v_major, // Temp, will replace with H
                                    minor_length: v_minor, // Authentic Width
                                    volume: v_vol,         // Temp, will recalculate
                                });
                            }
                        }

                        // Panel 3: horiz_unwrapped (Horizontal Cylinder)
                        // Right panel: left/right edges solid, horizontal centerline dashed
                        if let Some((mut box_points, contour)) = get_tight_bounds(&horiz_unwrapped)
                        {
                            let dx1 = (box_points[0].x - box_points[1].x) as f32;
                            let dy1 = (box_points[0].y - box_points[1].y) as f32;
                            let l1 = (dx1 * dx1 + dy1 * dy1).sqrt();

                            let dx2 = (box_points[1].x - box_points[2].x) as f32;
                            let dy2 = (box_points[1].y - box_points[2].y) as f32;
                            let l2 = (dx2 * dx2 + dy2 * dy2).sqrt();

                            let (major, minor, dx_major, dy_major) = if l1 > l2 {
                                (l1, l2, dx1, dy1)
                            } else {
                                (l2, l1, dx2, dy2)
                            };

                            let angle = dy_major.atan2(dx_major);
                            let cx = (box_points[0].x
                                + box_points[1].x
                                + box_points[2].x
                                + box_points[3].x) as f32
                                / 4.0;
                            let cy = (box_points[0].y
                                + box_points[1].y
                                + box_points[2].y
                                + box_points[3].y) as f32
                                / 4.0;

                            // In horiz_unwrapped (fruit rotated 90 deg), the fruit's true width is laying along the Y axis.
                            // The Y axis limits are thus the authentic Minor length (H_minor).
                            // Python identically sorts by Y and draws top and bottom bounds for both unrotated and rotated unwraps.
                            box_points.sort_by_key(|p| p.y);
                            // Top edge = 0 and 1, Bottom edge = 2 and 3
                            let top_mid_x =
                                (box_points[0].x as f32 + box_points[1].x as f32) / 2.0 + x3 as f32;
                            let top_mid_y =
                                (box_points[0].y as f32 + box_points[1].y as f32) / 2.0 + y3 as f32;

                            let bot_mid_x =
                                (box_points[2].x as f32 + box_points[3].x as f32) / 2.0 + x3 as f32;
                            let bot_mid_y =
                                (box_points[2].y as f32 + box_points[3].y as f32) / 2.0 + y3 as f32;

                            draw_line_segment_mut(
                                &mut color_preview,
                                (
                                    box_points[0].x as f32 + x3 as f32,
                                    box_points[0].y as f32 + y3 as f32,
                                ),
                                (
                                    box_points[1].x as f32 + x3 as f32,
                                    box_points[1].y as f32 + y3 as f32,
                                ),
                                red,
                            );
                            draw_line_segment_mut(
                                &mut color_preview,
                                (
                                    box_points[2].x as f32 + x3 as f32,
                                    box_points[2].y as f32 + y3 as f32,
                                ),
                                (
                                    box_points[3].x as f32 + x3 as f32,
                                    box_points[3].y as f32 + y3 as f32,
                                ),
                                red,
                            );

                            // Draw vertical dashed centerline linking top_mid to bot_mid
                            let dx = bot_mid_x - top_mid_x;
                            let dy = bot_mid_y - top_mid_y;
                            let dist = (dx * dx + dy * dy).sqrt();
                            if dist > 0.1 {
                                let (ux, uy) = (dx / dist, dy / dist);

                                let mut curr = 0.0;
                                while curr < dist {
                                    let start = (top_mid_x + ux * curr, top_mid_y + uy * curr);
                                    let mut end_curr = curr + dash_length;
                                    if end_curr > dist {
                                        end_curr = dist;
                                    }
                                    let end =
                                        (top_mid_x + ux * end_curr, top_mid_y + uy * end_curr);
                                    draw_line_segment_mut(&mut color_preview, start, end, red);
                                    curr += dash_length + gap_length;
                                }
                            }

                            let mut valid_points = Vec::with_capacity(contour.len());
                            for pt in &contour {
                                let lx = pt.x as f32 - cx;
                                let ly = pt.y as f32 - cy;
                                let rot_x = lx * angle.cos() + ly * angle.sin();
                                if rot_x >= 0.0 {
                                    valid_points.push(rot_x.abs());
                                }
                            }
                            valid_points.sort_by(|a, b| {
                                a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)
                            });
                            let mut vol = 0.0;
                            for w in valid_points.windows(2) {
                                vol += std::f32::consts::PI / 3.0 * (w[1].powi(3) - w[0].powi(3));
                            }

                            if let Some(metrics) = calculated_metrics.as_mut() {
                                if let Some(px_per_mm) = self.pixels_per_mm {
                                    let hr_px_per_mm = px_per_mm * scale;
                                    let mm_per_px = 1.0 / hr_px_per_mm;

                                    let h_major = major * mm_per_px;
                                    let h_minor = minor * mm_per_px;
                                    let h_vol = vol * mm_per_px.powi(3);

                                    log::info!(
                                        "HORIZ_UNWRAP (Corrects Height): H_major(Height)={}, H_minor(Width)={}, H_vol={}",
                                        h_major,
                                        h_minor,
                                        h_vol
                                    );

                                    // True logic (as explained by user):
                                    // VERT_UNWRAP assumes vertical cylinder -> Corrects Height curvature -> Use its Major for Height.
                                    // HORIZ_UNWRAP assumes horizontal cylinder -> Corrects Width curvature -> Use its Minor for Width.
                                    metrics.minor_length = h_minor;
                                    metrics.volume = h_vol;

                                    log::info!(
                                        "FINAL METRICS: Authentic Height(V_Major)={}, Authentic Width(Final Minor)={}, Authentic Vol(H_Vol)={}",
                                        metrics.major_length,
                                        metrics.minor_length,
                                        metrics.volume
                                    );
                                }
                            }
                        }

                        // Downscale for preview (keep consistent UI)
                        let preview_img = if scale > 1.1 {
                            DynamicImage::ImageRgba8(color_preview).resize(
                                total_w.min(1000), // cap width to avoid massive rendering hang
                                max_h,
                                FilterType::Lanczos3,
                            )
                        } else {
                            DynamicImage::ImageRgba8(color_preview)
                        };

                        Ok(Intermediate {
                            current_step: Step::RoiExtraction,
                            preview: Preview::ready(preview_img.into(), Instant::now()),
                            pixels_per_mm: self.pixels_per_mm,
                            context_image: Some(best_roi.clone()),
                            roi_image: Some(best_roi),
                            original_high_res: self.original_high_res.clone(),
                            focal_length_px: self.focal_length_px,
                            transform,
                            metrics: calculated_metrics,
                        })
                    } else {
                        Err(Error::General("No ROI found in Step 5".into()))
                    }
                }
                _ => Ok(self),
            }
        })
    }

    // UI Card rendering remains largely same but updated for Step enum
    pub(crate) fn card(&self, now: Instant) -> Element<'_, Message> {
        let image = {
            let thumbnail: Element<'_, _> = if let Preview::Ready { result_img, .. } = &self.preview
            {
                float(
                    image(dynamic_image_to_handle(&result_img.img))
                        .width(Fill)
                        .content_fit(ContentFit::Contain)
                        .opacity(result_img.fade_in.interpolate(0.0, 1.0, now)),
                )
                .scale(result_img.zoom.interpolate(1.0, 1.1, now))
                .translate(move |bounds, viewport| {
                    bounds.zoom(1.1).offset(&viewport.shrink(10))
                        * result_img.zoom.interpolate(0.0, 1.0, now)
                })
                .style(move |_theme| float::Style {
                    shadow: Shadow {
                        color: Color::BLACK.scale_alpha(result_img.zoom.interpolate(0.0, 1.0, now)),
                        blur_radius: result_img.zoom.interpolate(0.0, 20.0, now),
                        ..Shadow::default()
                    },
                    ..float::Style::default()
                })
                .into()
            } else {
                space::horizontal().into()
            };

            if let Some(blurhash) = self.preview.blurhash(now) {
                let blurhash = image(&blurhash.handle)
                    .width(Fill)
                    .height(Fill)
                    .content_fit(ContentFit::Fill)
                    .opacity(blurhash.fade_in.interpolate(0.0, 1.0, now));

                stack![blurhash, thumbnail].into()
            } else {
                thumbnail
            }
        };

        let card = mouse_area(container(image).style(container::dark))
            .on_enter(Message::ThumbnailHovered(self.current_step.clone(), true))
            .on_exit(Message::ThumbnailHovered(self.current_step.clone(), false));

        let decorated_card: Element<'_, Message> =
            if matches!(self.current_step, Step::RoiExtraction) {
                use iced::widget::{row, text};
                let title_bar = container(
                    row![
                        text("Vertical Unwrap").width(Fill).center(),
                        text("Original Rect").width(Fill).center(),
                        text("Horizontal Unwrap").width(Fill).center(),
                    ]
                    .width(Fill),
                )
                .padding(10)
                .style(container::dark);

                iced::widget::column![title_bar, card].into()
            } else {
                card.into()
            };

        let is_result = matches!(self.preview, Preview::Ready { .. });

        button(decorated_card)
            .on_press_maybe(is_result.then_some(Message::Open(self.current_step.clone())))
            .padding(0)
            .style(button::text)
            .into()
    }
}

// ================= Algorithm Implementations =================

fn perform_scale_calibration(image: &GrayImage) -> (DynamicImage, Option<f32>) {
    // 1. Scale Calibration
    // Doc: Otsu Edge -> Find Contours -> Highest Circularity (> 0.85)
    let binarized = imageproc::contrast::threshold(
        image,
        imageproc::contrast::otsu_level(image),
        imageproc::contrast::ThresholdType::Binary,
    );

    // Find contours
    let contours = ::imageproc::contours::find_contours::<i32>(&binarized);

    // Filter & Select
    // Doc: Highest Circularity (> 0.85)
    let mut best_coin: Option<(f32, contours::Contour<i32>)> = None;

    for contour in &contours {
        let area = contour_area(contour);
        if area < 100.0 {
            continue;
        }

        let rect = min_area_rect(&contour.points);
        let bbox = to_axis_aligned_bounding_box(&rect);

        let bbox_area = (bbox.width * bbox.height) as f32;
        if bbox_area == 0.0 {
            continue;
        }

        let aspect_ratio = bbox.width as f32 / bbox.height as f32;

        // A circle has a bounding box area ratio of roughly PI / 4 (~0.785)
        let fill_ratio = area / bbox_area;

        // Accept near-circles (aspect_ratio 0.9 to 1.1) and area ratio roughly 0.70 to 0.85
        if aspect_ratio > 0.9 && aspect_ratio < 1.1 && fill_ratio > 0.70 && fill_ratio < 0.88 {
            // Prefer the largest coin found
            if let Some((best_area, _)) = best_coin {
                if area > best_area {
                    best_coin = Some((area, contour.clone()));
                }
            } else {
                best_coin = Some((area, contour.clone()));
            }
        }
    }

    let mut vis_img = DynamicImage::ImageLuma8(image.clone()).to_rgba8();
    let mut px_per_mm = None;

    if let Some((_, contour)) = best_coin {
        // Derive pixels_per_mm
        // Doc: pixels_per_mm = Radius_coin_px / 12.5mm
        let area = contour_area(&contour);
        let radius_px = (area / std::f32::consts::PI).sqrt();
        px_per_mm = Some(radius_px / COIN_RADIUS_MM);
        log::info!(
            "Coin detection: area={}, radius_px={}, px_per_mm={}",
            area,
            radius_px,
            px_per_mm.unwrap()
        );

        // Visual: Red Box/Circle
        let rect = min_area_rect(&contour.points);
        let bbox = to_axis_aligned_bounding_box(&rect);
        // Draw 4 lines for rect
        let color = Rgba([255, 0, 0, 255]);
        let (x, y, w, h) = (
            bbox.x as f32,
            bbox.y as f32,
            bbox.width as f32,
            bbox.height as f32,
        );
        draw_line_segment_mut(&mut vis_img, (x, y), (x + w, y), color);
        draw_line_segment_mut(&mut vis_img, (x + w, y), (x + w, y + h), color);
        draw_line_segment_mut(&mut vis_img, (x + w, y + h), (x, y + h), color);
        draw_line_segment_mut(&mut vis_img, (x, y + h), (x, y), color);

        // Also draw simple circle approximation
        let cx = bbox.x + bbox.width / 2;
        let cy = bbox.y + bbox.height / 2;
        draw_hollow_circle_mut(
            &mut vis_img,
            (cx as i32, cy as i32),
            radius_px as i32,
            Rgba([255, 0, 0, 255]),
        );
    }

    (vis_img.into(), px_per_mm)
}

#[derive(Clone, Copy, Debug)]
struct RotatedRect {
    cx: f32,
    cy: f32,
    width: f32,     // Upright Width
    height: f32,    // Upright Height
    angle_rad: f32, // Rotation applied to image to make it upright
}

use imageproc::point::Point;

fn get_rotated_rect_info(points: &[Point<i32>]) -> RotatedRect {
    // We expect 4 points.
    if points.len() != 4 {
        return RotatedRect {
            cx: 0.0,
            cy: 0.0,
            width: 0.0,
            height: 0.0,
            angle_rad: 0.0,
        };
    }

    // Convert to float for simpler math
    let pts: Vec<(f32, f32)> = points.iter().map(|p| (p.x as f32, p.y as f32)).collect();

    // Calculate Edge Lengths
    // Edge 0: 0-1
    // Edge 1: 1-2
    let d0 = ((pts[1].0 - pts[0].0).powi(2) + (pts[1].1 - pts[0].1).powi(2)).sqrt();
    let d1 = ((pts[2].0 - pts[1].0).powi(2) + (pts[2].1 - pts[1].1).powi(2)).sqrt();

    let cx = (pts[0].0 + pts[1].0 + pts[2].0 + pts[3].0) / 4.0;
    let cy = (pts[0].1 + pts[1].1 + pts[2].1 + pts[3].1) / 4.0;

    // Identify Long Axis
    // Pineapple is usually Taller than Wide.
    // We want the Long Axis to be Vertical (Y).

    let (width, height, theta) = if d0 > d1 {
        // Edge 0 is Height
        // Angle of Edge 0
        let dx = pts[1].0 - pts[0].0;
        let dy = pts[1].1 - pts[0].1;
        let theta = dy.atan2(dx);
        (d1, d0, theta)
    } else {
        // Edge 1 is Height
        let dx = pts[2].0 - pts[1].0;
        let dy = pts[2].1 - pts[1].1;
        let theta = dy.atan2(dx);
        (d0, d1, theta)
    };

    // Calculate minimal rotation to vertical
    // We want to rotate such that the long axis becomes vertical.
    // This could be -PI/2 (Up) or PI/2 (Down).
    // We choose the rotation with smallest magnitude to avoid flipping the image upside down
    // if it is already mostly upright.

    let pi = std::f32::consts::PI;
    let normalize = |mut r: f32| {
        while r <= -pi {
            r += 2.0 * pi;
        }
        while r > pi {
            r -= 2.0 * pi;
        }
        r
    };

    let rot_up = normalize(-std::f32::consts::FRAC_PI_2 - theta);
    let rot_down = normalize(std::f32::consts::FRAC_PI_2 - theta);

    let angle = if rot_up.abs() < rot_down.abs() {
        rot_up
    } else {
        rot_down
    };

    RotatedRect {
        cx,
        cy,
        width,
        height,
        angle_rad: angle,
    }
}

#[allow(clippy::cast_precision_loss)]
pub(crate) fn contour_area(contour: &imageproc::contours::Contour<i32>) -> f32 {
    let points = &contour.points;
    if points.is_empty() {
        return 0.0;
    }
    let mut area = 0.0;
    for i in 0..points.len() {
        let p1 = points[i];
        let p2 = points[(i + 1) % points.len()];
        area += (p1.x * p2.y - p2.x * p1.y) as f32;
    }
    (area / 2.0).abs()
}

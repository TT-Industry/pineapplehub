use image::{DynamicImage, ImageBuffer, Luma, Rgba, RgbaImage, imageops};
use imageops::FilterType;
use imageproc::{
    contrast::adaptive_threshold,
    distance_transform::Norm,
    drawing::{draw_hollow_polygon_mut, draw_line_segment_mut},
    geometry::min_area_rect,
    morphology::{dilate, erode},
    point::Point,
    region_labelling::{Connectivity, connected_components},
};

use std::collections::HashMap;

use iced::time::Instant;

use crate::{Preview, error::Error};

use super::{COIN_RADIUS_MM, FruitletMetrics, Intermediate, Step};

/// Draws a dashed line from `start` to `end` on the preview image.
fn draw_dashed_line(img: &mut RgbaImage, start: (f32, f32), end: (f32, f32), color: Rgba<u8>) {
    let dash_length = 10.0;
    let gap_length = 5.0;

    let dx = end.0 - start.0;
    let dy = end.1 - start.1;
    let dist = (dx * dx + dy * dy).sqrt();
    if dist <= 0.1 {
        return;
    }
    let (ux, uy) = (dx / dist, dy / dist);

    let mut curr = 0.0;
    while curr < dist {
        let s = (start.0 + ux * curr, start.1 + uy * curr);
        let mut end_curr = curr + dash_length;
        if end_curr > dist {
            end_curr = dist;
        }
        let e = (start.0 + ux * end_curr, start.1 + uy * end_curr);
        draw_line_segment_mut(img, s, e, color);
        curr += dash_length + gap_length;
    }
}

/// Extracts rect metrics (major/minor lengths, angle) from min_area_rect box points.
fn compute_rect_metrics(box_points: &[Point<i32>; 4]) -> (f32, f32, f32) {
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
    (major, minor, angle)
}

/// A connected component region with precomputed properties.
struct Region {
    points: Vec<Point<i32>>,
    area: u32,
    centroid_x: f32,
    centroid_y: f32,
    bbox_min_y: i32,
    bbox_max_y: i32,
}

/// Collects connected component regions from a label image.
fn collect_regions(labels: &ImageBuffer<Luma<u32>, Vec<u32>>) -> HashMap<u32, Region> {
    let mut regions: HashMap<u32, Region> = HashMap::new();

    for (x, y, pixel) in labels.enumerate_pixels() {
        let label = pixel.0[0];
        if label == 0 {
            continue; // skip background
        }
        let entry = regions.entry(label).or_insert_with(|| Region {
            points: Vec::new(),
            area: 0,
            centroid_x: 0.0,
            centroid_y: 0.0,
            bbox_min_y: i32::MAX,
            bbox_max_y: i32::MIN,
        });
        entry.points.push(Point::new(x as i32, y as i32));
        entry.area += 1;
        entry.centroid_x += x as f32;
        entry.centroid_y += y as f32;
        entry.bbox_min_y = entry.bbox_min_y.min(y as i32);
        entry.bbox_max_y = entry.bbox_max_y.max(y as i32);
    }

    // Finalize centroids
    for region in regions.values_mut() {
        let n = region.area as f32;
        region.centroid_x /= n;
        region.centroid_y /= n;
    }

    regions
}



/// Map a label to a pseudo-color for visualization.
fn label_to_color(label: u32) -> Rgba<u8> {
    if label == 0 {
        return Rgba([0, 0, 0, 255]);
    }
    // Simple hash-based coloring
    let r = ((label.wrapping_mul(127) + 80) % 256) as u8;
    let g = ((label.wrapping_mul(83) + 120) % 256) as u8;
    let b = ((label.wrapping_mul(199) + 40) % 256) as u8;
    Rgba([r, g, b, 255])
}

/// Process the `RoiExtraction` step: fruitlet eye segmentation, counting,
/// and row count estimation.
pub(crate) fn process_fruitlet_counting(
    inter: &Intermediate,
    _image: &DynamicImage,
) -> Result<Intermediate, Error> {
    // Retrieve the upright skin ROI grayscale image
    let roi_gray = inter
        .context_image
        .as_ref()
        .ok_or(Error::General("Missing context image (warped ROI)".into()))?
        .to_luma8();

    let px_per_mm = inter
        .pixels_per_mm
        .ok_or(Error::General("Missing scale".into()))?;
    let scale = inter.scale_factor.unwrap_or(1.0);
    let hr_px_per_mm = px_per_mm * scale;
    let mm_per_px = 1.0 / hr_px_per_mm;

    let roi_w = roi_gray.width();
    let roi_h = roi_gray.height();

    // --- Step 1: Adaptive threshold on the ROI ---
    // block_radius ≈ fruitlet radius ≈ coin radius in high-res pixels
    let block_radius = (COIN_RADIUS_MM * hr_px_per_mm).round() as u32;
    // delta = 0: threshold = local mean. Grooves (darker) → black, fruitlet surfaces → white.
    let delta = 0_i32;
    let binary = adaptive_threshold(&roi_gray, block_radius, delta);

    // --- Step 2: Morphological opening (erode then dilate) to separate touching fruitlets ---
    let opened = dilate(&erode(&binary, Norm::LInf, 2), Norm::LInf, 2);

    // --- Step 3: Connected components ---
    let labels = connected_components(&opened, Connectivity::Four, Luma([0u8]));

    // Collect regions
    let regions = collect_regions(&labels);

    log::info!(
        "[FruitletCounting] ROI {}x{}, adaptive_threshold(block_radius={}, delta={}), {} connected components found",
        roi_w,
        roi_h,
        block_radius,
        delta,
        regions.len()
    );

    // --- Step 4: Area filtering using coin area as reference ---
    let coin_area_px = std::f32::consts::PI * (COIN_RADIUS_MM * hr_px_per_mm).powi(2);
    let area_min = (0.2 * coin_area_px) as u32;
    let area_max = (2.0 * coin_area_px) as u32;

    // Three-tier aspect ratio filtering
    let aspect_tiers = [(0.4_f32, 1.0_f32), (0.3, 1.0), (0.2, 1.0)];

    let equator_y = roi_h as f32 / 2.0;
    let center_x = roi_w as f32 / 2.0;

    // Find the best fruitlet candidate
    let mut selected_region: Option<(u32, [Point<i32>; 4], f32, f32, f32)> = None; // (label, box_points, major, minor, angle)

    for &(ar_min, ar_max) in &aspect_tiers {
        let mut candidates: Vec<(u32, [Point<i32>; 4], f32, f32, f32, f32, f32)> = Vec::new();

        for (&label, region) in &regions {
            // Area filter
            if region.area < area_min || region.area > area_max {
                continue;
            }

            // Check if bbox intersects equator line
            if region.bbox_max_y < equator_y as i32 || region.bbox_min_y > equator_y as i32 {
                continue;
            }

            // Compute min_area_rect for aspect ratio
            let rect = min_area_rect(&region.points);
            let (major, minor, angle) = compute_rect_metrics(&rect);

            if major <= 0.0 {
                continue;
            }
            let aspect = minor / major;
            if aspect < ar_min || aspect > ar_max {
                continue;
            }

            // Distance from centroid to center vertical axis
            let dist_to_center = (region.centroid_x - center_x).abs();
            candidates.push((
                label,
                rect,
                major,
                minor,
                angle,
                dist_to_center,
                region.centroid_y,
            ));
        }

        log::info!(
            "[FruitletCounting] Tier AR[{:.1},{:.1}]: {} candidates at equator",
            ar_min,
            ar_max,
            candidates.len()
        );

        if !candidates.is_empty() {
            // Select the candidate closest to the vertical center axis
            candidates.sort_by(|a, b| a.5.partial_cmp(&b.5).unwrap_or(std::cmp::Ordering::Equal));
            let best = &candidates[0];
            selected_region = Some((best.0, best.1, best.2, best.3, best.4));
            break;
        }
    }

    // --- Compute metrics ---
    let prev_metrics = inter.metrics.clone();
    let mut new_metrics = prev_metrics.unwrap_or(FruitletMetrics {
        major_length: 0.0,
        minor_length: 0.0,
        volume: 0.0,
        a_eq: None,
        b_eq: None,
        alpha: None,
        surface_area: None,
        n_total: None,
    });

    if let Some((_label, _box_points, major_px, minor_px, angle_raw)) = &selected_region {
        let a_eq_mm = major_px * mm_per_px;
        let b_eq_mm = minor_px * mm_per_px;

        // α = angle of fruitlet long axis relative to the fruit's vertical axis.
        // The fruit's vertical axis is the Y axis (upright ROI).
        // angle_raw is atan2(dy, dx) of the major axis direction.
        // Vertical axis direction = π/2 rad.
        // α = |angle_raw - π/2| normalized to [0, π/2]
        let pi = std::f32::consts::PI;
        let alpha = {
            let diff = (angle_raw - std::f32::consts::FRAC_PI_2).abs();
            if diff > pi / 2.0 { pi - diff } else { diff }
        };

        new_metrics.a_eq = Some(a_eq_mm);
        new_metrics.b_eq = Some(b_eq_mm);
        new_metrics.alpha = Some(alpha);

        // --- Feature 2: Whole-fruit fruitlet count estimation ---
        // Per-eye footprint = a_eq × b_eq (the eye's own bounding rectangle area).
        // NOT d_v × d_h: that axis-aligned projection inflates area when α ≠ 0
        // and was only meaningful for row/column counting, not surface tiling.
        let a_eye = a_eq_mm * b_eq_mm;

        if a_eye > 0.0 {
            if let Some(surface_area) = new_metrics.surface_area {
                // Subtract polar cap areas (crown plate + stalk plate)
                // Pineapples have flat eye-free areas at both poles.
                // Estimate each cap as a disc with radius = average half-width
                // in a window of a_eq/2 depth from each pole tip.
                let cap_area = if let (Some(horiz_contour), Some((_h_major, _h_minor, h_angle, h_cx, h_cy))) =
                    (&inter.horiz_contour, inter.horiz_rect_metrics)
                {
                    let cos_a = h_angle.cos();
                    let sin_a = h_angle.sin();

                    // Project to (t, |r|) — use absolute r for half-widths
                    let mut tr: Vec<(f32, f32)> = horiz_contour
                        .iter()
                        .map(|pt| {
                            let lx = pt.x as f32 - h_cx;
                            let ly = pt.y as f32 - h_cy;
                            let t = lx * cos_a + ly * sin_a;
                            let r = (-lx * sin_a + ly * cos_a).abs();
                            (t, r)
                        })
                        .collect();
                    tr.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

                    let t_min = tr.first().map(|p| p.0).unwrap_or(0.0);
                    let t_max = tr.last().map(|p| p.0).unwrap_or(0.0);

                    // Window = a_eq/2 in pixels (convert from mm)
                    let window_px = (a_eq_mm / mm_per_px) / 2.0;

                    let front: Vec<f32> = tr.iter()
                        .filter(|(t, _)| *t <= t_min + window_px)
                        .map(|(_, r)| *r)
                        .collect();
                    let back: Vec<f32> = tr.iter()
                        .filter(|(t, _)| *t >= t_max - window_px)
                        .map(|(_, r)| *r)
                        .collect();

                    let r_front = if front.is_empty() { 0.0 }
                        else { front.iter().sum::<f32>() / front.len() as f32 };
                    let r_back = if back.is_empty() { 0.0 }
                        else { back.iter().sum::<f32>() / back.len() as f32 };

                    let r_front_mm = r_front * mm_per_px;
                    let r_back_mm = r_back * mm_per_px;
                    let cap = std::f32::consts::PI * (r_front_mm.powi(2) + r_back_mm.powi(2));

                    log::info!(
                        "[FruitletCounting] Polar caps: r_front={:.2}mm, r_back={:.2}mm, cap_area={:.1}mm²",
                        r_front_mm, r_back_mm, cap
                    );
                    cap
                } else {
                    0.0
                };

                let s_effective = (surface_area - cap_area).max(0.0);
                let n_total_raw = s_effective / a_eye;
                let n_total = n_total_raw.floor() as u32;
                log::info!(
                    "[FruitletCounting] N_total_raw = {:.3} (S_eff={:.2}mm² / A_eye={:.2}mm²) → floor = {}",
                    n_total_raw,
                    s_effective,
                    a_eye,
                    n_total
                );
                new_metrics.n_total = Some(n_total);
            }
        }

        log::info!(
            "[FruitletCounting] a_eq={:.2}mm, b_eq={:.2}mm, α={:.3}rad, A_eye={:.2}mm², S={:.2}mm², N_total={}",
            a_eq_mm,
            b_eq_mm,
            alpha,
            a_eye,
            new_metrics.surface_area.unwrap_or(0.0),
            new_metrics.n_total.unwrap_or(0),
        );
    } else {
        log::warn!("[FruitletCounting] No suitable fruitlet candidate found at equator");
    }

    // --- Build 4-panel visualization ---
    let padding = 10;
    let panel_w = roi_w;
    let panel_h = roi_h;
    let total_w = panel_w * 4 + padding * 3;

    let mut color_preview: RgbaImage = ImageBuffer::new(total_w, panel_h);
    // Fill with white
    for p in color_preview.pixels_mut() {
        *p = Rgba([255, 255, 255, 255]);
    }

    let x_offsets = [
        0,
        panel_w + padding,
        panel_w * 2 + padding * 2,
        panel_w * 3 + padding * 3,
    ];

    // Panel 1: Adaptive threshold binary
    for (x, y, pixel) in binary.enumerate_pixels() {
        let val = pixel.0[0];
        color_preview.put_pixel(x_offsets[0] + x, y, Rgba([val, val, val, 255]));
    }

    // Panel 2: Morphological opening result
    for (x, y, pixel) in opened.enumerate_pixels() {
        let val = pixel.0[0];
        color_preview.put_pixel(x_offsets[1] + x, y, Rgba([val, val, val, 255]));
    }

    // Panel 3: Connected components pseudo-color
    for (x, y, pixel) in labels.enumerate_pixels() {
        let label = pixel.0[0];
        let c = label_to_color(label);
        color_preview.put_pixel(x_offsets[2] + x, y, c);
    }

    // Panel 4: Original ROI + equator line + selected fruitlet rect
    let roi_rgba = DynamicImage::ImageLuma8(roi_gray).to_rgba8();
    for (x, y, pixel) in roi_rgba.enumerate_pixels() {
        color_preview.put_pixel(x_offsets[3] + x, y, *pixel);
    }

    // Draw equator line (green dashed)
    let green = Rgba([0, 200, 0, 255]);
    let eq_y = roi_h as f32 / 2.0;
    draw_dashed_line(
        &mut color_preview,
        (x_offsets[3] as f32, eq_y),
        ((x_offsets[3] + panel_w) as f32, eq_y),
        green,
    );

    // Draw selected fruitlet bounding rect (red)
    if let Some((_label, box_points, _major, _minor, _angle)) = &selected_region {
        let red = Rgba([255, 0, 0, 255]);
        let offset_points: Vec<Point<f32>> = box_points
            .iter()
            .map(|p| Point::new(p.x as f32 + x_offsets[3] as f32, p.y as f32))
            .collect();
        draw_hollow_polygon_mut(&mut color_preview, &offset_points, red);
    }

    // Downscale for preview if needed
    let preview_img = if total_w > 2000 {
        DynamicImage::ImageRgba8(color_preview).resize(
            total_w.min(2000),
            panel_h,
            FilterType::Lanczos3,
        )
    } else {
        DynamicImage::ImageRgba8(color_preview)
    };

    Ok(Intermediate {
        current_step: Step::FruitletCounting,
        preview: Preview::ready(preview_img.into(), Instant::now()),
        pixels_per_mm: inter.pixels_per_mm,
        binary_image: inter.binary_image.clone(),
        fused_image: inter.fused_image.clone(),
        contours: inter.contours.clone(),
        context_image: inter.context_image.clone(),
        roi_image: inter.roi_image.clone(),
        original_high_res: inter.original_high_res.clone(),
        transform: inter.transform.clone(),
        metrics: Some(new_metrics),
        horiz_contour: inter.horiz_contour.clone(),
        horiz_rect_metrics: inter.horiz_rect_metrics,
        scale_factor: inter.scale_factor,
    })
}

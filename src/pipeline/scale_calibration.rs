use image::{DynamicImage, GrayImage, Rgba};
use imageproc::{
    contours::{self, BorderType},
    distance_transform::Norm,
    drawing::{draw_hollow_circle_mut, draw_line_segment_mut},
    geometry::{arc_length, contour_area as geometry_contour_area, convex_hull, min_area_rect},
    point::Point,
};

use image_debug_utils::{contours::remove_hypotenuse_owned, rect::to_axis_aligned_bounding_box};

use super::COIN_RADIUS_MM;

pub(crate) fn extract_robust_contours(
    image: &GrayImage,
) -> (GrayImage, GrayImage, Vec<imageproc::contours::Contour<i32>>) {
    use imageproc::contrast::{ThresholdType, otsu_level, threshold};
    use imageproc::morphology::{close, open};

    // 1. Otsu Edge
    let level = otsu_level(image);
    let binary = threshold(image, level, ThresholdType::Binary);

    // 2. Morphology: cv2.MORPH_CLOSE (r=2) then cv2.MORPH_OPEN (r=3)
    let closed = close(&binary, Norm::L2, 2);
    let opened = open(&closed, Norm::L2, 3);

    // 3. Find contours
    let contours = contours::find_contours::<i32>(&opened);

    // 4. Filter out rulers/straight edges
    let filtered_contours = remove_hypotenuse_owned(contours, 5.0, Some(BorderType::Outer));

    (binary, opened, filtered_contours)
}

pub(crate) fn perform_scale_calibration(
    image: &GrayImage,
) -> (
    DynamicImage,
    Option<f32>,
    GrayImage,
    GrayImage,
    Vec<imageproc::contours::Contour<i32>>,
) {
    let (binary, fused, contours) = extract_robust_contours(image);

    let mut best_coin: Option<(f32, Vec<Point<i32>>)> = None;

    // Collect all candidates with their metrics for two-tier detection
    struct CoinCandidate {
        hull_area: f32,
        hull_points: Vec<Point<i32>>,
        aspect_ratio: f32,
        fill_ratio: f32,
        circularity: f32,
    }
    let mut coin_candidates: Vec<CoinCandidate> = Vec::new();

    for contour in &contours {
        let area = geometry_contour_area(&contour.points).abs() as f32;
        if area < 100.0 {
            continue;
        }

        // Convex hull repairs edge defects (stains, dirt) by bridging concavities
        let hull = convex_hull(contour.points.clone());
        let hull_area = geometry_contour_area(&hull).abs() as f32;

        let rect = min_area_rect(&hull);

        // Compute rotation-invariant aspect ratio from min_area_rect edge lengths
        // (NOT from axis-aligned bounding box, which distorts under rotation)
        let d0 = {
            let dx = (rect[1].x - rect[0].x) as f32;
            let dy = (rect[1].y - rect[0].y) as f32;
            (dx * dx + dy * dy).sqrt()
        };
        let d1 = {
            let dx = (rect[2].x - rect[1].x) as f32;
            let dy = (rect[2].y - rect[1].y) as f32;
            (dx * dx + dy * dy).sqrt()
        };
        let rect_area = d0 * d1;
        if rect_area < 1.0 {
            continue;
        }
        let (short, long) = if d0 < d1 { (d0, d1) } else { (d1, d0) };
        let aspect_ratio = short / long; // Always in (0, 1], 1.0 = square/circle
        let fill_ratio = hull_area / rect_area;

        let perimeter = arc_length(&hull, true) as f32;
        let circularity = if perimeter > 0.0 {
            4.0 * std::f32::consts::PI * hull_area / (perimeter * perimeter)
        } else {
            0.0
        };

        log::info!(
            "[CoinCandidate] area={:.1}, hull_area={:.1}, aspect={:.4}, fill={:.4}, circ={:.4}",
            area,
            hull_area,
            aspect_ratio,
            fill_ratio,
            circularity
        );

        coin_candidates.push(CoinCandidate {
            hull_area,
            hull_points: hull,
            aspect_ratio,
            fill_ratio,
            circularity,
        });
    }

    // Tier 1: Strict thresholds (preserves existing behavior for clean coins)
    for c in &coin_candidates {
        if c.aspect_ratio > 0.95
            && c.fill_ratio > 0.70
            && c.fill_ratio < 0.88
            && c.circularity > 0.85
        {
            if let Some((best_area, _)) = best_coin {
                if c.hull_area > best_area {
                    best_coin = Some((c.hull_area, c.hull_points.clone()));
                }
            } else {
                best_coin = Some((c.hull_area, c.hull_points.clone()));
            }
        }
    }

    // Tier 2: Relaxed thresholds + scoring (fallback for stained/damaged coins)
    if best_coin.is_none() {
        log::info!("[CoinDetect] Tier 1 (strict) failed, trying Tier 2 (relaxed + scoring)");

        let mut best_score = f32::NEG_INFINITY;
        for c in &coin_candidates {
            if c.aspect_ratio > 0.85
                && c.fill_ratio > 0.60
                && c.fill_ratio < 0.92
                && c.circularity > 0.70
            {
                // Score: penalize deviation from ideal circle metrics
                // Ideal: aspect=1.0, fill=PI/4≈0.785, circularity=1.0
                let aspect_dev = (c.aspect_ratio - 1.0).abs();
                let fill_dev = (c.fill_ratio - std::f32::consts::FRAC_PI_4).abs();
                let circ_dev = (1.0 - c.circularity).abs();
                let score = -(aspect_dev * 10.0 + fill_dev * 5.0 + circ_dev * 5.0);

                log::info!(
                    "[CoinDetect T2] score={:.4} (aspect_dev={:.4}, fill_dev={:.4}, circ_dev={:.4})",
                    score,
                    aspect_dev,
                    fill_dev,
                    circ_dev
                );

                if score > best_score {
                    best_score = score;
                    best_coin = Some((c.hull_area, c.hull_points.clone()));
                }
            }
        }
    }

    let mut vis_img = DynamicImage::ImageLuma8(image.clone()).to_rgba8();
    let mut px_per_mm = None;

    if let Some((hull_area, hull_points)) = best_coin {
        // Derive pixels_per_mm from convex hull area
        // Doc: pixels_per_mm = Radius_coin_px / 12.5mm
        let radius_px = (hull_area / std::f32::consts::PI).sqrt();
        px_per_mm = Some(radius_px / COIN_RADIUS_MM);
        log::info!(
            "Coin detection: hull_area={}, radius_px={}, px_per_mm={}",
            hull_area,
            radius_px,
            px_per_mm.unwrap()
        );

        // Visual: Red Box/Circle
        let rect = min_area_rect(&hull_points);
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

    (vis_img.into(), px_per_mm, binary, fused, contours)
}

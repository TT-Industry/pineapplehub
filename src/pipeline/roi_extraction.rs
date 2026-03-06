use image::GrayImage;
use imageproc::{
    geometry::{contour_area as geometry_contour_area, min_area_rect},
    point::Point,
};

use crate::error::Error;

use super::COIN_RADIUS_MM;

#[derive(Clone, Copy, Debug)]
pub(crate) struct RotatedRect {
    pub(crate) cx: f32,
    pub(crate) cy: f32,
    pub(crate) width: f32,     // Upright Width
    pub(crate) height: f32,    // Upright Height
    pub(crate) angle_rad: f32, // Rotation applied to image to make it upright
}

pub(crate) fn get_rotated_rect_info(points: &[Point<i32>]) -> RotatedRect {
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

pub(crate) fn extract_best_roi(
    smoothed: &GrayImage,
    px_per_mm: f32,
    contours: Vec<imageproc::contours::Contour<i32>>,
) -> Result<Option<RotatedRect>, Error> {
    // 2. Filter by Physical Area (Doc Step 2.3)
    // Area > 0.2 * Area_coin
    let coin_area_px = std::f32::consts::PI * (COIN_RADIUS_MM * px_per_mm).powi(2);
    let min_area = 0.2 * coin_area_px;

    let mut candidates = Vec::with_capacity(contours.len());
    for contour in contours {
        let area = geometry_contour_area(&contour.points).abs() as f32;
        if area > min_area {
            candidates.push((contour, area));
        }
    }

    // 3. Score Candidates by Texture Richness (edge density × area)
    // Skin side → bumpy fruitlet eyes → high local gradient magnitudes → high edge density.
    // Flesh side → smooth cut surface → low gradients → low edge density.
    // Coin → small area → penalized by area factor.
    let mut stats = Vec::with_capacity(candidates.len());

    for (i, (contour, area)) in candidates.iter().enumerate() {
        let rect = min_area_rect(&contour.points);
        let r_rect = get_rotated_rect_info(&rect);

        // Compute axis-aligned bounding box for this contour
        let (mut bx_min, mut by_min) = (i32::MAX, i32::MAX);
        let (mut bx_max, mut by_max) = (i32::MIN, i32::MIN);
        for pt in &contour.points {
            bx_min = bx_min.min(pt.x);
            by_min = by_min.min(pt.y);
            bx_max = bx_max.max(pt.x);
            by_max = by_max.max(pt.y);
        }

        // Clamp to image bounds
        let (img_w, img_h) = smoothed.dimensions();
        let bx0 = (bx_min.max(0) as u32).min(img_w.saturating_sub(1));
        let by0 = (by_min.max(0) as u32).min(img_h.saturating_sub(1));
        let bx1 = (bx_max.max(0) as u32).min(img_w.saturating_sub(1));
        let by1 = (by_max.max(0) as u32).min(img_h.saturating_sub(1));

        // Compute edge density: average |dI/dx| + |dI/dy| over non-background pixels
        let bg_threshold = 15u8; // pixels below this are considered black background
        let mut gradient_sum: f64 = 0.0;
        let mut pixel_count: u32 = 0;

        for y in by0..by1.min(img_h - 1) {
            for x in bx0..bx1.min(img_w - 1) {
                let p = smoothed.get_pixel(x, y).0[0];
                if p <= bg_threshold {
                    continue; // skip black background
                }
                let px_right = smoothed.get_pixel(x + 1, y).0[0];
                let py_down = smoothed.get_pixel(x, y + 1).0[0];
                let dx = (p as i16 - px_right as i16).unsigned_abs() as f64;
                let dy = (p as i16 - py_down as i16).unsigned_abs() as f64;
                gradient_sum += dx + dy;
                pixel_count += 1;
            }
        }

        let edge_density = if pixel_count > 0 {
            gradient_sum / pixel_count as f64
        } else {
            0.0
        };

        // Score = edge_density × sqrt(area)
        // sqrt(area) rather than area to avoid extreme dominance by size
        let score = edge_density as f32 * area.sqrt();

        log::info!(
            "[ROI Score] Candidate {}: area={:.0}, edge_density={:.2}, score={:.1}, rect={:?}",
            i,
            area,
            edge_density,
            score,
            r_rect
        );

        stats.push((i, r_rect, score));
    }

    // Sort by Score Descending
    stats.sort_by(|a, b| b.2.total_cmp(&a.2));

    if let Some((_, r_rect, score)) = stats.first() {
        log::info!("[Step 5] Best ROI Score: {:.2}, Rect: {:?}", score, r_rect);

        Ok(Some(*r_rect))
    } else {
        Err(Error::General("No valid ROI found".into()))
    }
}

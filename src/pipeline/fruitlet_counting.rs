use image::{DynamicImage, ImageBuffer, Luma, Rgba, RgbaImage, imageops};
use imageops::FilterType;
use imageproc::{
    contrast::adaptive_threshold,
    distance_transform::Norm,
    drawing::{draw_hollow_polygon_mut, draw_line_segment_mut},
    geometry::min_area_rect,
    morphology::{close, open},
    point::Point,
    region_labelling::{Connectivity, connected_components},
};

use std::collections::{HashMap, VecDeque};

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
/// Uses golden-ratio hue spacing in HSV (S=0.85, V=0.9) so every label
/// gets a vivid, maximally-spaced colour — never near-white or near-black.
fn label_to_color(label: u32) -> Rgba<u8> {
    if label == 0 {
        return Rgba([0, 0, 0, 255]);
    }
    // Golden-ratio hue spacing: each successive label rotates ~222° in hue
    let hue = ((label as f64 * 0.618033988749895) % 1.0) * 360.0;
    let s: f64 = 0.85;
    let v: f64 = 0.90;

    let c = v * s;
    let h_prime = hue / 60.0;
    let x = c * (1.0 - ((h_prime % 2.0) - 1.0).abs());
    let m = v - c;

    let (r, g, b) = if h_prime < 1.0 { (c, x, 0.0) }
        else if h_prime < 2.0 { (x, c, 0.0) }
        else if h_prime < 3.0 { (0.0, c, x) }
        else if h_prime < 4.0 { (0.0, x, c) }
        else if h_prime < 5.0 { (x, 0.0, c) }
        else { (c, 0.0, x) };

    Rgba([
        ((r + m) * 255.0) as u8,
        ((g + m) * 255.0) as u8,
        ((b + m) * 255.0) as u8,
        255,
    ])
}

/// Fill **small** internal holes in a binary image.
///
/// 1. BFS from border marks all background-connected black pixels.
/// 2. Remaining unvisited black pixels are clustered into connected components.
/// 3. Only CCs with area < `max_hole_area` are filled white.
///
/// This removes tiny noise spots inside eyes while preserving enclosed grooves
/// that may have been disconnected from the border by `close()`.
fn fill_holes(binary: &image::GrayImage, max_hole_area: u32) -> image::GrayImage {
    let (w, h) = binary.dimensions();
    let mut filled = binary.clone();
    let mut visited = vec![false; (w * h) as usize];
    let mut queue: VecDeque<(u32, u32)> = VecDeque::new();

    // Seed BFS from all border black pixels
    let seed = |x: u32, y: u32, v: &mut Vec<bool>, q: &mut VecDeque<(u32, u32)>| {
        let idx = (y * w + x) as usize;
        if binary.get_pixel(x, y).0[0] == 0 && !v[idx] {
            v[idx] = true;
            q.push_back((x, y));
        }
    };
    for x in 0..w {
        seed(x, 0, &mut visited, &mut queue);
        if h > 1 { seed(x, h - 1, &mut visited, &mut queue); }
    }
    for y in 1..h.saturating_sub(1) {
        seed(0, y, &mut visited, &mut queue);
        if w > 1 { seed(w - 1, y, &mut visited, &mut queue); }
    }

    // BFS: mark all background-connected black pixels
    while let Some((x, y)) = queue.pop_front() {
        for (dx, dy) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)] {
            let nx = x as i32 + dx;
            let ny = y as i32 + dy;
            if nx >= 0 && nx < w as i32 && ny >= 0 && ny < h as i32 {
                let idx = (ny as u32 * w + nx as u32) as usize;
                if !visited[idx] && binary.get_pixel(nx as u32, ny as u32).0[0] == 0 {
                    visited[idx] = true;
                    queue.push_back((nx as u32, ny as u32));
                }
            }
        }
    }

    // Flood-fill each unvisited black CC; only fill if small
    let mut n_filled: u32 = 0;
    let mut n_preserved: u32 = 0;
    for y in 0..h {
        for x in 0..w {
            let idx = (y * w + x) as usize;
            if visited[idx] || binary.get_pixel(x, y).0[0] != 0 {
                continue;
            }
            // BFS to find this hole's CC
            let mut hole_pixels: Vec<(u32, u32)> = Vec::new();
            let mut hq: VecDeque<(u32, u32)> = VecDeque::new();
            visited[idx] = true;
            hq.push_back((x, y));
            while let Some((hx, hy)) = hq.pop_front() {
                hole_pixels.push((hx, hy));
                for (ddx, ddy) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)] {
                    let nx = hx as i32 + ddx;
                    let ny = hy as i32 + ddy;
                    if nx >= 0 && nx < w as i32 && ny >= 0 && ny < h as i32 {
                        let nidx = (ny as u32 * w + nx as u32) as usize;
                        if !visited[nidx] && binary.get_pixel(nx as u32, ny as u32).0[0] == 0 {
                            visited[nidx] = true;
                            hq.push_back((nx as u32, ny as u32));
                        }
                    }
                }
            }
            if (hole_pixels.len() as u32) < max_hole_area {
                for &(hx, hy) in &hole_pixels {
                    filled.put_pixel(hx, hy, Luma([255u8]));
                }
                n_filled += hole_pixels.len() as u32;
            } else {
                n_preserved += 1;
            }
        }
    }
    log::info!(
        "[FruitletCounting] fill_holes: filled {} pixels in small holes, preserved {} large holes (max_area={})",
        n_filled, n_preserved, max_hole_area,
    );
    filled
}

/// Count white and total pixels inside a circle of radius `r` centred at `(cx, cy)`.
fn circular_fill(img: &image::GrayImage, cx: i32, cy: i32, r: i32) -> (u32, u32) {
    let (w, h) = img.dimensions();
    let r2 = r * r;
    let mut white = 0u32;
    let mut total = 0u32;
    for dy in -r..=r {
        for dx in -r..=r {
            if dx * dx + dy * dy > r2 { continue; }
            let x = cx + dx;
            let y = cy + dy;
            if x >= 0 && x < w as i32 && y >= 0 && y < h as i32 {
                total += 1;
                if img.get_pixel(x as u32, y as u32).0[0] > 0 {
                    white += 1;
                }
            }
        }
    }
    (white, total)
}

/// A scored candidate from the multi-attempt search.
struct ScoredCandidate {
    open_r: u8,
    box_points: [Point<i32>; 4],
    major: f32,
    minor: f32,
    angle: f32,
    area: f32,
    score: f32,
}

/// Collect ALL valid equatorial candidates from a given binary+open_radius,
/// computing a quality score for each.  Higher score = more likely a single eye.
fn collect_candidates(
    binary: &image::GrayImage,
    equator_y: f32,
    equator_band: f32,
    center_x: f32,
    roi_w: f32,
    area_min: u32,
    area_max: u32,
    coin_area_px: f32,
    open_radius: u8,
) -> Vec<ScoredCandidate> {
    let processed = if open_radius > 0 {
        open(binary, Norm::LInf, open_radius)
    } else {
        binary.clone()
    };

    let labels = connected_components(&processed, Connectivity::Four, Luma([0u8]));
    let regions = collect_regions(&labels);

    let mut out = Vec::new();
    let coin_diam_px_local = 2.0 * (coin_area_px / std::f32::consts::PI).sqrt();
    if open_radius == 0 {
        log::info!(
            "[FruitletCounting] filter params: coin_diam={:.0}px, area=[{},{}], major=[{:.0},{:.0}], {} total CCs",
            coin_diam_px_local, area_min, area_max,
            coin_diam_px_local * 0.3, coin_diam_px_local * 2.0,
            regions.len(),
        );
    }

    for (_label, region) in &regions {
        if region.area < area_min || region.area > area_max {
            continue;
        }
        // Must overlap the equator band [equator_y - band, equator_y + band]
        let band_top = (equator_y - equator_band) as i32;
        let band_bottom = (equator_y + equator_band) as i32;
        if region.bbox_max_y < band_top || region.bbox_min_y > band_bottom {
            continue;
        }

        let rect = min_area_rect(&region.points);
        let (major, minor, angle) = compute_rect_metrics(&rect);
        if major <= 0.0 {
            continue;
        }

        let aspect = minor / major;
        // Hard cutoff: single eye physical constraints
        if aspect < 0.15 || aspect > 1.0 {
            continue;
        }
        // Hard cutoff: major axis must be ≤ 2× coin diameter
        let coin_diam_px = 2.0 * (coin_area_px / std::f32::consts::PI).sqrt();
        if major > coin_diam_px * 2.0 || major < coin_diam_px * 0.3 {
            continue;
        }

        // --- Quality score ---
        // 1) Area: prefer ≈ 1× coin area.  ln(1.0) = 0 → best.
        //    Weight ×2 to strongly penalize merged blobs.
        let area_ratio = region.area as f32 / coin_area_px;
        let area_score = -(area_ratio.ln().abs()) * 2.0;     // 0 at 1×, -1.4 at 2×, -3.2 at 0.2×

        // 2) Aspect: prefer ≈ 0.6 (typical pineapple eye)
        let aspect_score = -((aspect - 0.6).abs()) * 3.0;   // 0 at 0.6, -0.6 at 0.4 or 0.8

        // 3) Position: prefer centroid close to (center_x, equator_y)
        let dx = (region.centroid_x - center_x) / (roi_w / 2.0);
        let dy = (region.centroid_y - equator_y) / (roi_w / 2.0);
        let pos_score = -(dx * dx + dy * dy).sqrt() * 0.5;  // 0 at centre

        // 4) Fill ratio: a real eye is roughly elliptical → fills ~π/4 ≈ 0.785
        //    of its bounding rect.  Irregular merged fragments fill much less.
        let rect_area = major * minor;
        let fill_ratio = if rect_area > 0.0 { region.area as f32 / rect_area } else { 0.0 };
        let fill_score = -((fill_ratio - 0.75).abs()) * 4.0; // 0 at 0.75, -1.0 at 0.50

        // 5) Dimension match: major axis should be ≈ coin diameter (25 mm).
        //    This is the KEY prior: pineapple eyes are coin-sized.
        let coin_diam_px = 2.0 * (coin_area_px / std::f32::consts::PI).sqrt();
        let major_ratio = major / coin_diam_px;
        let dim_score = -(major_ratio.ln().abs()) * 3.0;     // 0 when major = coin_diam

        let score = area_score + aspect_score + pos_score + fill_score + dim_score;

        out.push(ScoredCandidate {
            open_r: open_radius,
            box_points: rect,
            major,
            minor,
            angle,
            area: region.area as f32,
            score,
        });
    }

    out
}

/// Process the `FruitletCounting` step: fruitlet eye segmentation, counting,
/// and row count estimation.
///
/// Uses a **multi-attempt, score-based** connected-component strategy:
/// tries several morphological open radii, collects ALL valid candidates
/// across all attempts, scores each by how closely it resembles a single
/// eye (area ≈ 1× coin, aspect ≈ 0.6, centroid near equator centre),
/// and selects the highest-scoring candidate.
pub(crate) fn process_fruitlet_counting(
    inter: &Intermediate,
    _image: &DynamicImage,
) -> Result<Intermediate, Error> {
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
    let block_radius = (COIN_RADIUS_MM * hr_px_per_mm).round() as u32;
    let delta = 0_i32;
    let binary = adaptive_threshold(&roi_gray, block_radius, delta);

    // --- Step 2: Morphological close + hole filling ---
    // Close bridges hairline cracks; fill_holes removes internal noise spots
    // (< max_hole_area px²) without filling enclosed grooves.
    let closed = close(&binary, Norm::LInf, 2);
    let max_hole_area = (block_radius * block_radius / 20).max(100);
    let filled = fill_holes(&closed, max_hole_area);

    // --- Step 3: Eye detection via CC search ---
    // Instead of locating an eye centre first (which fails at junctions),
    // progressively open the `filled` binary until individual eyes separate,
    // then pick the best CC by area match + equator proximity + centrality.
    let coin_diam_px = 2.0 * COIN_RADIUS_MM * hr_px_per_mm;
    let coin_radius_px = coin_diam_px / 2.0;
    let coin_area_px = std::f32::consts::PI * coin_radius_px * coin_radius_px;
    let equator_y = roi_h as f32 / 2.0;
    let center_x = roi_w as f32 / 2.0;

    let max_open = (block_radius / 10).max(8).min(25) as u8;
    let open_attempts: Vec<u8> = {
        let mut v = vec![0u8];
        let mut r = 2u8;
        while r <= max_open {
            v.push(r);
            r += 2;
        }
        v
    };

    let mut eye_rect: Option<([Point<i32>; 4], f32, f32, f32)> = None;
    let mut eye_centroid: (u32, u32) = (center_x as u32, equator_y as u32);

    for open_r in &open_attempts {
        let opened = if *open_r == 0 { filled.clone() } else { open(&filled, Norm::LInf, *open_r) };
        let labels = connected_components(&opened, Connectivity::Four, Luma([0u8]));

        // Collect region stats: area, sum_x, sum_y
        let mut regions: std::collections::HashMap<u32, (u32, f64, f64)> = std::collections::HashMap::new();
        for (x, y, px) in labels.enumerate_pixels() {
            let l = px.0[0];
            if l == 0 { continue; }
            let e = regions.entry(l).or_insert((0, 0.0, 0.0));
            e.0 += 1;
            e.1 += x as f64;
            e.2 += y as f64;
        }

        // Score each region
        let mut best_open_score = f32::NEG_INFINITY;
        let mut best_label: Option<u32> = None;

        for (&label, &(area, sx, sy)) in &regions {
            let area_f = area as f32;
            if area_f < 0.15 * coin_area_px || area_f > 1.8 * coin_area_px { continue; }

            let cx = (sx / area as f64) as f32;
            let cy = (sy / area as f64) as f32;

            // Equator band: centroid within ± coin_radius of equator_y
            if (cy - equator_y).abs() > coin_radius_px { continue; }

            // Inner circle: centroid within 40% of roi_w from center
            let max_r = 0.4 * roi_w as f32;
            let dx = cx - center_x;
            let dy = cy - equator_y;
            if dx * dx + dy * dy > max_r * max_r { continue; }

            // Prefer area close to 0.7×coin (eyes shrink with opening)
            let area_ratio = area_f / coin_area_px;
            let area_score = 1.0 - (area_ratio - 0.7).abs().min(1.0);
            let pos_dist = (dx * dx + dy * dy).sqrt() / max_r;
            let score = area_score - pos_dist;

            if score > best_open_score {
                best_open_score = score;
                best_label = Some(label);
            }
        }

        if let Some(label) = best_label {
            let (area, sx, sy) = regions[&label];
            let mut pts: Vec<Point<i32>> = Vec::new();
            for (x, y, px) in labels.enumerate_pixels() {
                if px.0[0] == label {
                    pts.push(Point::new(x as i32, y as i32));
                }
            }
            let rect = min_area_rect(&pts);
            let (major, minor, angle) = compute_rect_metrics(&rect);
            if major > 0.0 {
                let cx = (sx / area as f64) as f32;
                let cy = (sy / area as f64) as f32;
                log::info!(
                    "[FruitletCounting] Eye CC found at open_r={}: centroid=({:.0},{:.0}), major={:.1}px ({:.1}mm), minor={:.1}px ({:.1}mm), area={} ({:.2}× coin), score={:.2}",
                    open_r, cx, cy, major, major * mm_per_px, minor, minor * mm_per_px, area, area as f32 / coin_area_px, best_open_score,
                );
                eye_rect = Some((rect, major, minor, angle));
                eye_centroid = (cx as u32, cy as u32);
                break;
            }
        } else {
            let n_valid = regions.values().filter(|(a,_,_)| {
                let af = *a as f32;
                af >= 0.15 * coin_area_px && af <= 1.8 * coin_area_px
            }).count();
            log::info!(
                "[FruitletCounting] open_r={}: {} CCs total, {} in area range, none passed position filter",
                open_r, regions.len(), n_valid,
            );
        }
    }

    if eye_rect.is_none() {
        log::warn!("[FruitletCounting] CC search found no valid eye across all open_r levels");
    }

    // For CC visualization, use a light open to show structure
    let viz_open_r = 2u8;
    let viz_binary = open(&filled, Norm::LInf, viz_open_r);

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

    if let Some((_rect_pts, major, minor, angle)) = &eye_rect {
        let a_eq_mm = major * mm_per_px;
        let b_eq_mm = minor * mm_per_px;

        let pi = std::f32::consts::PI;
        let alpha = {
            let diff = (angle - std::f32::consts::FRAC_PI_2).abs();
            if diff > pi / 2.0 { pi - diff } else { diff }
        };

        new_metrics.a_eq = Some(a_eq_mm);
        new_metrics.b_eq = Some(b_eq_mm);
        new_metrics.alpha = Some(alpha);

        let a_eye = a_eq_mm * b_eq_mm;

        if a_eye > 0.0 {
            if let Some(surface_area) = new_metrics.surface_area {
                let cap_area = if let (Some(horiz_contour), Some((_h_major, _h_minor, h_angle, h_cx, h_cy))) =
                    (&inter.horiz_contour, inter.horiz_rect_metrics)
                {
                    let cos_a = h_angle.cos();
                    let sin_a = h_angle.sin();

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
        log::warn!("[FruitletCounting] No suitable fruitlet candidate found at equator after all attempts");
    }

    // --- Build 4-panel visualization ---
    let labels = connected_components(&viz_binary, Connectivity::Four, Luma([0u8]));

    let padding = 10;
    let panel_w = roi_w;
    let panel_h = roi_h;
    let total_w = panel_w * 4 + padding * 3;

    let mut color_preview: RgbaImage = ImageBuffer::new(total_w, panel_h);
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

    // Panel 2: Filled binary (close + fill_holes)
    for (x, y, pixel) in filled.enumerate_pixels() {
        let val = pixel.0[0];
        color_preview.put_pixel(x_offsets[1] + x, y, Rgba([val, val, val, 255]));
    }

    // Panel 3: Connected components pseudo-color (at viz_open_r for structure)
    for (x, y, pixel) in labels.enumerate_pixels() {
        let label = pixel.0[0];
        let c = label_to_color(label);
        color_preview.put_pixel(x_offsets[2] + x, y, c);
    }

    // Draw selected eye centroid circle on panel 3 (cyan)
    let cyan = Rgba([0, 255, 255, 200]);
    {
        let cr = (coin_radius_px / 4.0) as i32;  // small marker circle
        for angle_step in 0..360 {
            let a = (angle_step as f32).to_radians();
            let px = (eye_centroid.0 as f32 + cr as f32 * a.cos()).round() as i32;
            let py = (eye_centroid.1 as f32 + cr as f32 * a.sin()).round() as i32;
            if px >= 0 && px < panel_w as i32 && py >= 0 && py < panel_h as i32 {
                color_preview.put_pixel(x_offsets[2] + px as u32, py as u32, cyan);
            }
        }
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
    if let Some((rect_pts, _major, _minor, _angle)) = &eye_rect {
        let red = Rgba([255, 0, 0, 255]);
        let offset_points: Vec<Point<f32>> = rect_pts
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

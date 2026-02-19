/// Maps a point (x, y) from the unwrapped image back to the source image coordinates.
///
/// This is the inverse of the geometric transformation in `cylindrical_unwrap`.
pub fn map_point_back(
    x: f32,
    y: f32,
    width: u32,
    height: u32,
    radius: f32,
    focal_length_px: f32,
) -> Option<(f32, f32)> {
    let center_x = width as f32 / 2.0;
    let center_y = height as f32 / 2.0;
    let f = focal_length_px;
    let r = radius;

    let pc_x = x - center_x;
    let pc_y = y - center_y;

    let omega = width as f32 / 2.0;
    let term1 = (r * r - omega * omega).max(0.0);
    let z0 = f - term1.sqrt();

    // The forward transform was:
    // u = pc.x * zc / f  => pc.x corresponds to x in unwrapped image (target)
    // v = pc.y * zc / f
    // Wait, let's re-read cylindrical_unwrap.
    // Loop iterates x, y in OUTPUT (Target).
    // pc_x = x - center_x (Target centered)
    // zc = depth at target x, y
    // src_u = pc_x * zc / f (Source centered)
    // src_x = src_u + center_x

    // So if we have a point (x, y) in the OUTPUT (Unwrapped),
    // we just need to re-calculate zc and then src_u, src_v.
    // It's effectively the same math as the forward loop, just for a single point!

    let a_term = (pc_x * pc_x) / (f * f) + 1.0;
    let c_term = z0 * z0 - r * r;
    let discriminant = 4.0 * z0 * z0 - 4.0 * a_term * c_term;

    if discriminant < 0.0 {
        return None;
    }

    let zc = (2.0 * z0 + discriminant.sqrt()) / (2.0 * a_term);

    let src_u = pc_x * zc / f;
    let src_v = pc_y * zc / f;

    let src_x = src_u + center_x;
    let src_y = src_v + center_y;

    Some((src_x, src_y))
}

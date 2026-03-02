use image::{GrayImage, ImageBuffer, Luma};

/// Unwraps an already-upright cropped cylinder image exactly like Python's `convert_pt`.
/// Assumes f = w and r = w
pub fn unwrap(img: &GrayImage) -> GrayImage {
    let w = img.width() as f32;
    unwrap_with_radius(img, w, w)
}

/// Unwraps an arbitrarily rotated cylinder where focal length and radius
/// might not be equal to the current image width.
pub fn unwrap_with_radius(img: &GrayImage, f: f32, r: f32) -> GrayImage {
    let w = img.width() as f32;
    let h = img.height() as f32;

    let mut output = ImageBuffer::new(img.width(), img.height());

    let omega = w / 2.0;

    let term1 = (r * r - omega * omega).max(0.0);
    let z0 = f - term1.sqrt();
    let c_term = z0 * z0 - r * r;

    for y in 0..img.height() {
        for x in 0..img.width() {
            let pc_x = x as f32 - w / 2.0;
            let pc_y = y as f32 - h / 2.0;

            let a_term = (pc_x * pc_x) / (f * f) + 1.0;
            let discriminant = 4.0 * z0 * z0 - 4.0 * a_term * c_term;

            if discriminant < 0.0 {
                continue;
            }

            let zc = (2.0 * z0 + discriminant.sqrt()) / (2.0 * a_term);

            let src_x = pc_x * zc / f + w / 2.0;
            let src_y = pc_y * zc / f + h / 2.0;

            let x0 = src_x.floor() as i32;
            let y0 = src_y.floor() as i32;
            let x1 = x0 + 1;
            let y1 = y0 + 1;

            if x0 >= 0 && x1 < img.width() as i32 && y0 >= 0 && y1 < img.height() as i32 {
                let wx1 = src_x - x0 as f32;
                let wx0 = 1.0 - wx1;
                let wy1 = src_y - y0 as f32;
                let wy0 = 1.0 - wy1;

                let p00 = img.get_pixel(x0 as u32, y0 as u32).0[0] as f32;
                let p10 = img.get_pixel(x1 as u32, y0 as u32).0[0] as f32;
                let p01 = img.get_pixel(x0 as u32, y1 as u32).0[0] as f32;
                let p11 = img.get_pixel(x1 as u32, y1 as u32).0[0] as f32;

                let val = p00 * wx0 * wy0 + p10 * wx1 * wy0 + p01 * wx0 * wy1 + p11 * wx1 * wy1;
                output.put_pixel(x, y, Luma([val as u8]));
            }
        }
    }

    output
}

use image::RgbaImage;

pub fn unpremultiply(pixels: &mut [u8]) {
    for pixel in pixels.as_chunks_mut::<4>().0 {
        unpremultiply_pixel(pixel);
    }
}

fn unpremultiply_pixel(pixel: &mut [u8; 4]) {
    let alpha = u32::from(pixel[3]);
    if alpha == 0 {
        pixel[..3].fill(0);
    } else if alpha < 255 {
        for channel in &mut pixel[..3] {
            *channel = ((u32::from(*channel) * 255 + alpha / 2) / alpha).min(255) as u8;
        }
    }
}

pub fn captured_pixels(
    bytes: &[u8],
    pitch: usize,
    width: u32,
    height: u32,
) -> Result<RgbaImage, String> {
    let stride = (width as usize)
        .checked_mul(4)
        .ok_or("The capture is too large.")?;
    let length = stride
        .checked_mul(height as usize)
        .ok_or("The capture is too large.")?;
    let source_length = pitch
        .checked_mul(height as usize)
        .ok_or("The capture is too large.")?;
    if length == 0 || pitch < stride || bytes.len() < source_length {
        return Err("The captured pixel buffer is incomplete.".into());
    }
    let mut pixels = Vec::new();
    pixels
        .try_reserve_exact(length)
        .map_err(|_| "There is not enough memory for the capture.")?;
    for row in bytes.chunks_exact(pitch).take(height as usize) {
        pixels.extend_from_slice(&row[..stride]);
    }
    for pixel in pixels.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
        unpremultiply_pixel(pixel);
    }
    RgbaImage::from_raw(width, height, pixels)
        .ok_or_else(|| "The captured image is invalid.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alpha_conversion_handles_transparency_rounding_and_saturation() {
        for (input, expected) in [
            ([10, 20, 30, 255], [10, 20, 30, 255]),
            ([50, 25, 0, 128], [100, 50, 0, 128]),
            ([1, 0, 1, 1], [255, 0, 255, 1]),
            ([255, 100, 0, 128], [255, 199, 0, 128]),
            ([22, 33, 44, 0], [0, 0, 0, 0]),
        ] {
            let mut actual = input;
            unpremultiply(&mut actual);
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn capture_decodes_padded_bgra_rows_without_flipping_or_losing_alpha() {
        let image = captured_pixels(
            &[
                10, 20, 30, 255, 0, 0, 0, 255, 99, 99, 99, 99, 16, 32, 64, 128, 10, 20, 30, 0, 88,
                88, 88, 88,
            ],
            12,
            2,
            2,
        )
        .unwrap();
        assert_eq!(image.dimensions(), (2, 2));
        assert_eq!(
            image.as_raw(),
            &[30, 20, 10, 255, 0, 0, 0, 255, 128, 64, 32, 128, 0, 0, 0, 0]
        );
    }

    #[test]
    fn capture_rejects_empty_truncated_and_overflowing_buffers() {
        for (len, pitch, width, height) in [
            (0, 0, 0, 0),
            (7, 8, 2, 1),
            (15, 8, 1, 2),
            (8, 3, 1, 2),
            (8, 8, 0, 1),
            (0, usize::MAX, 1, 2),
        ] {
            assert!(
                captured_pixels(&vec![0; len], pitch, width, height).is_err(),
                "{len} {pitch} {width} {height}"
            );
        }
    }
}

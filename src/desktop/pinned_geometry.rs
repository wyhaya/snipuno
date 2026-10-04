pub(super) fn fitted_size(
    logical_size: (f64, f64),
    available: (f64, f64),
    zoom: f64,
    minimum_width: f64,
) -> (f64, f64) {
    let maximum = (available.0 / logical_size.0).min(available.1 / logical_size.1);
    let minimum = (minimum_width / logical_size.0)
        .max(68.0 / logical_size.1)
        .max(0.1)
        .min(maximum);
    let zoom = zoom.clamp(minimum, maximum);
    (logical_size.0 * zoom, logical_size.1 * zoom)
}

#[cfg(test)]
mod tests {
    use super::fitted_size;

    #[test]
    fn zoom_limits_keep_image_proportions_and_fit_the_screen() {
        let available = (1440.0, 900.0);
        for minimum_width in [208.0, 288.0] {
            for logical_size in [
                (800.0, 600.0),
                (20.0, 10.0),
                (6000.0, 100.0),
                (100.0, 6000.0),
            ] {
                for zoom in [0.0, 0.5, 1.0, 100.0] {
                    let size = fitted_size(logical_size, available, zoom, minimum_width);
                    assert!(size.0 > 0.0 && size.1 > 0.0);
                    assert!(size.0 <= available.0 && size.1 <= available.1);
                    assert!((size.0 / size.1 - logical_size.0 / logical_size.1).abs() < 1e-9);
                }
            }
        }
    }

    #[test]
    fn ordinary_images_keep_requested_zoom_and_room_for_controls() {
        let available = (1440.0, 900.0);
        let logical_size = (800.0, 600.0);
        for minimum_width in [208.0, 288.0] {
            assert_eq!(
                fitted_size(logical_size, available, 0.5, minimum_width),
                (400.0, 300.0)
            );
            let minimum = fitted_size(logical_size, available, 0.0, minimum_width);
            assert!(minimum.0 >= minimum_width && minimum.1 >= 68.0);
        }
    }
}

use super::*;
use crate::test_support::bounds;
use image::Rgba;

#[test]
fn measurement_stops_at_color_boundaries_and_uses_outer_pixel_edges() {
    for axis in [MeasurementAxis::X, MeasurementAxis::Y] {
        let image = RgbaImage::from_fn(7, 7, |x, y| {
            Rgba(if (2..5).contains(&x) && (2..5).contains(&y) {
                [255; 4]
            } else {
                [0, 0, 0, 255]
            })
        });
        let crop = bounds(0.0, 0.0, 7.0, 7.0);
        let interval = measure(&image, crop, Point::new(3.8, 3.2), axis).unwrap();
        let expected = match axis {
            MeasurementAxis::X => Interval {
                start: Point::new(2.0, 3.5),
                end: Point::new(5.0, 3.5),
            },
            MeasurementAxis::Y => Interval {
                start: Point::new(3.5, 2.0),
                end: Point::new(3.5, 5.0),
            },
        };
        assert_eq!(interval, expected);
    }
    for (colors, seed, expected) in [
        (vec![100], 0.0, (0.0, 1.0)),
        (vec![100; 5], 4.0, (0.0, 5.0)),
        (vec![100, 100, 0, 100], 2.0, (2.0, 3.0)),
        (vec![100, 106, 112, 113, 118], 0.0, (0.0, 3.0)),
    ] {
        let image = RgbaImage::from_fn(colors.len() as u32, 1, |x, _| {
            Rgba([colors[x as usize], 0, 0, 255])
        });
        let result = measure(
            &image,
            bounds(0.0, 0.0, colors.len() as f32, 1.0),
            Point::new(seed, 0.0),
            MeasurementAxis::X,
        )
        .unwrap();
        assert_eq!((result.start.x, result.end.x), expected);
    }
}

#[test]
fn measurement_uses_composited_alpha_and_crop_coordinates() {
    let image = RgbaImage::from_fn(6, 2, |x, _| {
        Rgba(match x {
            2 => [255; 4],
            3 => [0, 0, 0, 0],
            _ => [0, 0, 0, 255],
        })
    });
    let crop = bounds(2.0, 1.0, 3.0, 1.0);
    assert_eq!(
        measure(&image, crop, Point::default(), MeasurementAxis::X),
        Some(Interval {
            start: Point::new(0.0, 0.5),
            end: Point::new(2.0, 0.5)
        })
    );
    for point in [
        Point::new(-0.1, 0.0),
        Point::new(3.0, 0.0),
        Point::new(0.0, 1.0),
        Point::new(f32::NAN, 0.0),
    ] {
        assert!(measure(&image, crop, point, MeasurementAxis::X).is_none());
    }
    for invalid in [
        bounds(-1.0, 0.0, 2.0, 1.0),
        bounds(5.0, 0.0, 2.0, 1.0),
        bounds(0.0, 0.0, 0.0, 1.0),
        bounds(0.0, 0.0, 1.0, f32::INFINITY),
    ] {
        assert!(measure(&image, invalid, Point::default(), MeasurementAxis::X).is_none());
    }
}

#[test]
fn cached_measurements_match_fresh_results_when_any_input_changes() {
    let images = [
        Arc::new(RgbaImage::from_pixel(8, 3, Rgba([255; 4]))),
        Arc::new(RgbaImage::from_fn(8, 3, |x, _| {
            Rgba([100 + (x / 2) as u8 * 12, 100, 100, 255])
        })),
    ];
    let mut cache = MeasurementCache::default();
    for image in &images {
        for crop in [bounds(0.0, 0.0, 8.0, 3.0), bounds(2.0, 1.0, 5.0, 2.0)] {
            for axis in [MeasurementAxis::X, MeasurementAxis::Y] {
                for point in [
                    Point::new(0.1, 0.1),
                    Point::new(0.9, 0.9),
                    Point::new(1.0, 0.0),
                    Point::new(2.0, 0.0),
                    Point::new(4.0, 1.0),
                    Point::new(8.0, 0.0),
                ] {
                    assert_eq!(
                        cache.measure(image, crop, point, axis),
                        measure(image, crop, point, axis),
                        "{crop:?} {axis:?} {point:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn manual_measurement_has_a_display_threshold_and_stable_axis_switching() {
    let crop = bounds(0.0, 0.0, 100.0, 80.0);
    let mut drag = MeasurementDrag::new(Point::new(20.0, 20.0));
    drag.update(Point::new(23.0, 20.0), crop, 1.0);
    assert_eq!(drag.interval(), None);
    drag.update(Point::new(22.0, 20.0), crop, 2.0);
    assert_eq!(drag.interval().unwrap().1.end, Point::new(22.0, 20.0));
    for (end, axis, projected) in [
        (
            Point::new(60.0, 30.0),
            MeasurementAxis::X,
            Point::new(60.0, 20.0),
        ),
        (
            Point::new(60.0, 61.0),
            MeasurementAxis::X,
            Point::new(60.0, 20.0),
        ),
        (
            Point::new(60.0, 66.0),
            MeasurementAxis::Y,
            Point::new(20.0, 66.0),
        ),
        (
            Point::new(-100.0, 20.0),
            MeasurementAxis::X,
            Point::new(0.0, 20.0),
        ),
    ] {
        drag.update(end, crop, 1.0);
        assert_eq!(
            drag.interval(),
            Some((
                axis,
                Interval {
                    start: Point::new(20.0, 20.0),
                    end: projected
                }
            ))
        );
    }
    drag.update(Point::new(20.0, 20.0), crop, 1.0);
    assert_eq!(drag.interval(), None);
}

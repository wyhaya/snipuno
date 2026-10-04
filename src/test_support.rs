use crate::editor::{Bounds, EffectStrength, Point, Stroke, TailDirection, TextAnnotation, Tool};
use gpui::SvgRenderer;
use image::{Frame, Rgba, RgbaImage, codecs::gif::GifEncoder};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
use ui::IconAssets;

pub fn bounds(x: f32, y: f32, width: f32, height: f32) -> Bounds {
    Bounds {
        x,
        y,
        width,
        height,
    }
}

pub fn stroke(tool: Tool, rect: Bounds) -> Stroke {
    Stroke {
        tool,
        start: Point::new(rect.x, rect.y),
        end: Point::new(rect.x + rect.width, rect.y + rect.height),
        color: 0xffe02040,
        brush_auto_color: false,
        width: 3.0,
        points: Arc::default(),
        effect_strength: EffectStrength::Low,
        arrow_control: None,
        arrow_double_headed: false,
        fill_color: None,
        fill_opacity: 1.0,
        text: None,
        measurement_label: None,
        magnifier: None,
        number_tail: TailDirection::default(),
    }
}

pub fn text(content: &str) -> TextAnnotation {
    TextAnnotation {
        content: content.into(),
        font_size: 20.0,
        wrap_width: 120.0,
        bubble: false,
        tail: TailDirection::default(),
    }
}

pub fn renderer() -> SvgRenderer {
    SvgRenderer::new(Arc::new(IconAssets))
}

pub fn animated_gif() -> Vec<u8> {
    let first = RgbaImage::from_fn(3, 2, |x, _| {
        if x == 0 {
            Rgba([0, 0, 0, 0])
        } else {
            Rgba([255, 0, 0, 255])
        }
    });
    let second = RgbaImage::from_pixel(3, 2, Rgba([0, 0, 255, 255]));
    let mut bytes = Vec::new();
    GifEncoder::new(&mut bytes)
        .encode_frames([Frame::new(first), Frame::new(second)])
        .unwrap();
    bytes
}

pub struct TestDir(PathBuf);

impl TestDir {
    pub fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        loop {
            let path = std::env::temp_dir().join(format!(
                "snipuno-test-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("Unable to create test directory: {error}"),
            }
        }
    }

    pub fn join(&self, name: impl AsRef<Path>) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

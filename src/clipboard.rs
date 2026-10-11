use crate::image_processing::{decode_overlay, load_overlay};
use arboard::{Clipboard, ImageData};
use gpui::{App, ClipboardEntry, ClipboardItem, RenderImage, Task};
use image::{ImageReader, RgbaImage};
use std::{
    io::Cursor,
    path::PathBuf,
    sync::{Arc, Mutex},
};

static WRITE_LOCK: Mutex<()> = Mutex::new(());

pub fn init(cx: &mut App) {
    cx.on_app_quit(|cx| {
        cx.background_executor().spawn(async {
            drop(WRITE_LOCK.lock());
        })
    })
    .detach();
}

pub fn read_image(cx: &App) -> Task<Result<Option<Arc<RenderImage>>, String>> {
    let clipboard = cx.read_from_clipboard_async();
    cx.background_executor().spawn(async move {
        if let Some(item) = clipboard.await.map_err(|error| error.to_string())?
            && let Some(image) = decode_item(item)?
        {
            return Ok(Some(image));
        }
        Ok(None)
    })
}

pub fn write_image(pixels: Arc<RgbaImage>, cx: &App) -> Task<Result<(), String>> {
    write_image_with(move || Ok(pixels), cx)
}

pub fn write_image_with(
    prepare: impl FnOnce() -> Result<Arc<RgbaImage>, String> + Send + 'static,
    cx: &App,
) -> Task<Result<(), String>> {
    cx.background_executor().spawn(async move {
        let pixels = prepare()?;
        if pixels.width() == 0 || pixels.height() == 0 {
            return Err("Cannot copy an empty image.".into());
        }
        let _guard = WRITE_LOCK.lock().map_err(|error| error.to_string())?;
        let mut clipboard = Clipboard::new().map_err(|error| error.to_string())?;
        clipboard
            .set_image(ImageData {
                width: pixels.width() as usize,
                height: pixels.height() as usize,
                bytes: pixels.as_raw().as_slice().into(),
            })
            .map_err(|error| error.to_string())
    })
}

fn decode_item(item: ClipboardItem) -> Result<Option<Arc<RenderImage>>, String> {
    if let Some(image) = item.entries().iter().find_map(|entry| match entry {
        ClipboardEntry::Image(image) => Some(image),
        _ => None,
    }) {
        return decode_overlay(ImageReader::new(Cursor::new(image.bytes())))
            .map(|(image, _)| Some(image));
    }
    item.entries()
        .iter()
        .find_map(|entry| match entry {
            ClipboardEntry::ExternalPaths(paths) => paths.0.first(),
            _ => None,
        })
        .map(|path| read_file(path.clone()))
        .transpose()
}

fn read_file(path: PathBuf) -> Result<Arc<RenderImage>, String> {
    load_overlay(path).map(|(image, _)| image)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{TestDir, animated_gif};
    use gpui::{ExternalPaths, Image, ImageFormat};
    use image::{DynamicImage, ImageFormat as FileFormat, Rgba};
    use std::fs;

    #[test]
    fn clipboard_images_decode_supported_raster_formats() {
        for (format, clipboard_format) in [
            (FileFormat::Png, ImageFormat::Png),
            (FileFormat::Jpeg, ImageFormat::Jpeg),
            (FileFormat::WebP, ImageFormat::Webp),
            (FileFormat::Bmp, ImageFormat::Bmp),
            (FileFormat::Ico, ImageFormat::Ico),
        ] {
            let source =
                DynamicImage::ImageRgba8(RgbaImage::from_pixel(2, 3, Rgba([20, 40, 80, 128])));
            let source = if format == FileFormat::Jpeg {
                DynamicImage::ImageRgb8(source.into_rgb8())
            } else {
                source
            };
            let mut bytes = Cursor::new(Vec::new());
            source.write_to(&mut bytes, format).unwrap();
            let item =
                ClipboardItem::new_image(&Image::from_bytes(clipboard_format, bytes.into_inner()));
            let image = decode_item(item).unwrap().unwrap();
            assert_eq!(image.size(0).width.0, 2);
            assert_eq!(image.size(0).height.0, 3);
            if format != FileFormat::Jpeg {
                assert_eq!(&image.as_bytes(0).unwrap()[..4], &[80, 40, 20, 128]);
            }
        }
    }

    #[test]
    fn clipboard_gifs_import_only_the_first_frame() {
        let item = ClipboardItem::new_image(&Image::from_bytes(ImageFormat::Gif, animated_gif()));
        let image = decode_item(item).unwrap().unwrap();
        assert_eq!(image.size(0).width.0, 3);
        assert_eq!(image.size(0).height.0, 2);
        assert_eq!(image.frame_count(), 1);
        assert_eq!(
            image.as_bytes(0).unwrap(),
            [0, 0, 0, 0, 0, 0, 255, 255, 0, 0, 255, 255].repeat(2)
        );
    }

    #[test]
    fn svg_clipboard_data_and_copied_files_are_unsupported() {
        let dir = TestDir::new();
        let path = dir.join("copied.svg");
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="6" height="4"/>"#;
        fs::write(&path, svg).unwrap();
        let encoded = ClipboardItem::new_image(&Image::from_bytes(ImageFormat::Svg, svg.to_vec()));
        let file = ClipboardItem {
            entries: vec![ClipboardEntry::ExternalPaths(ExternalPaths(
                vec![path].into(),
            ))],
        };
        for item in [encoded, file] {
            assert_eq!(decode_item(item).unwrap_err(), "Unsupported image format.");
        }
    }

    #[test]
    fn text_is_ignored_and_invalid_images_report_errors() {
        assert!(
            decode_item(ClipboardItem::new_string("hello".into()))
                .unwrap()
                .is_none()
        );
        let item = ClipboardItem::new_image(&Image::from_bytes(ImageFormat::Png, vec![0, 1, 2]));
        assert!(decode_item(item).is_err());
    }

    #[test]
    fn copied_image_files_preserve_pixels_and_transparency() {
        let dir = TestDir::new();
        let path = dir.join("copied.png");
        let image = RgbaImage::from_pixel(4, 5, Rgba([10, 20, 30, 128]));
        image.save(&path).unwrap();
        let item = ClipboardItem {
            entries: vec![ClipboardEntry::ExternalPaths(ExternalPaths(
                vec![path.clone()].into(),
            ))],
        };
        let result = decode_item(item);
        fs::remove_file(&path).unwrap();
        let decoded = result.unwrap().unwrap();
        assert_eq!(decoded.size(0).width.0, 4);
        assert_eq!(decoded.size(0).height.0, 5);
        assert_eq!(decoded.as_bytes(0).unwrap(), [30, 20, 10, 128].repeat(20));
        assert!(read_file(path).is_err());
    }
}

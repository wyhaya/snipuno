use crate::image_processing::{decode_overlay, load_overlay};
use arboard::{Clipboard, ImageData};
use gpui::{App, ClipboardEntry, ClipboardItem, Global, RenderImage, Task};
use image::{ImageReader, RgbaImage};
use std::{
    io::Cursor,
    path::PathBuf,
    sync::{Arc, Mutex},
};

#[derive(Clone, Default)]
struct State(Arc<Mutex<Option<Clipboard>>>);

impl Global for State {}

pub fn init(cx: &mut App) {
    cx.set_global(State::default());
    cx.on_app_quit(|cx| {
        let state = cx.global::<State>().clone();
        cx.background_executor().spawn(async move {
            if let Ok(mut clipboard) = state.0.lock() {
                clipboard.take();
            }
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
        #[cfg(target_os = "linux")]
        return wayland::read_image_file();
        #[cfg(not(target_os = "linux"))]
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
    let state = cx.global::<State>().clone();
    cx.background_executor().spawn(async move {
        let pixels = prepare()?;
        if pixels.width() == 0 || pixels.height() == 0 {
            return Err("Cannot copy an empty image.".into());
        }
        let mut clipboard = state.0.lock().map_err(|error| error.to_string())?;
        if clipboard.is_none() {
            *clipboard = Some(Clipboard::new().map_err(|error| error.to_string())?);
        }
        clipboard
            .as_mut()
            .unwrap()
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

#[cfg(any(target_os = "linux", all(test, unix)))]
mod wayland {
    use super::*;
    use url::Url;

    // GPUI's Wayland clipboard reads image MIME types but does not expose file lists.
    #[cfg(target_os = "linux")]
    pub(super) fn read_image_file() -> Result<Option<Arc<RenderImage>>, String> {
        use std::io::Read;
        use wl_clipboard_rs::paste::{self, ClipboardType, Error, MimeType, Seat};

        let (mut pipe, _) = match paste::get_contents(
            ClipboardType::Regular,
            Seat::Unspecified,
            MimeType::Specific("text/uri-list"),
        ) {
            Ok(contents) => contents,
            Err(Error::ClipboardEmpty | Error::NoMimeType | Error::NoSeats) => return Ok(None),
            // Ordinary GPUI image reads still work without data-control support.
            Err(Error::MissingProtocol { .. }) => return Ok(None),
            Err(error) => return Err(error.to_string()),
        };
        let mut uris = String::new();
        pipe.read_to_string(&mut uris)
            .map_err(|error| error.to_string())?;
        decode_file_list(&uris)
    }

    fn decode_file_list(uris: &str) -> Result<Option<Arc<RenderImage>>, String> {
        first_file(uris).map(read_file).transpose()
    }

    fn first_file(uris: &str) -> Option<PathBuf> {
        uris.lines()
            .filter(|line| !line.starts_with('#'))
            .filter_map(|line| Url::parse(line).ok())
            .find_map(|url| url.to_file_path().ok())
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::test_support::TestDir;
        use image::Rgba;

        #[test]
        fn uri_lists_handle_line_endings_comments_and_escaped_paths() {
            for newline in ["\r\n", "\n"] {
                let uris = format!(
                    "# copied files{newline}{newline}file:///tmp/a%20b%23c.png{newline}file:///tmp/second.png{newline}"
                );
                assert_eq!(first_file(&uris), Some(PathBuf::from("/tmp/a b#c.png")));
            }
            assert_eq!(
                first_file("file:///tmp/a.png%0D\r\n"),
                Some(PathBuf::from("/tmp/a.png\r"))
            );
            assert_eq!(
                first_file("file://localhost/tmp/a.png\r\n"),
                Some(PathBuf::from("/tmp/a.png"))
            );
            assert!(first_file("file://remote/tmp/a.png\r\n").is_none());
            assert!(first_file("# empty\r\nhttps://example.com/a.png\r\nnot a uri").is_none());
        }

        #[test]
        fn crlf_file_list_loads_the_actual_image() {
            let dir = TestDir::new();
            let path = dir.join("a b#中文.png");
            RgbaImage::from_pixel(2, 3, Rgba([10, 20, 30, 128]))
                .save(&path)
                .unwrap();
            let uris = format!("{}\r\n", Url::from_file_path(&path).unwrap());
            let result = decode_file_list(&uris);
            let image = result.unwrap().unwrap();
            assert_eq!(&image.as_bytes(0).unwrap()[..4], &[30, 20, 10, 128]);
            assert!(decode_file_list("").unwrap().is_none());
        }
    }
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

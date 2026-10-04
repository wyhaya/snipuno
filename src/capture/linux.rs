use super::{
    CaptureBounds, CaptureDisplayId, CapturedImage, WindowTarget, placement::CapturePlacement,
};
use gpui::{App, Context, Point, Window};

const UNSUPPORTED: &str = "The screenshot portal does not provide independent window capture.";

pub fn has_permission() -> bool {
    true
}

pub fn request_permission() -> bool {
    true
}

pub fn open_permission_settings(_: &App) {}

pub fn restart(cx: &mut App) {
    cx.restart();
}

pub async fn capture_window(_: WindowTarget) -> Result<CapturedImage, String> {
    Err(UNSUPPORTED.into())
}

pub fn configure_overlay<T: 'static>(
    window: &mut Window,
    _: CaptureBounds,
    _: &mut Context<T>,
) -> Result<(), String> {
    window.set_window_title("Snipuno · Drag to select · Esc to cancel");
    Ok(())
}

pub fn display_work_area(_: CaptureDisplayId) -> Option<CaptureBounds> {
    None
}

pub fn position_editor(
    _: &Window,
    _: CapturePlacement,
    _: CaptureBounds,
    _: (u32, u32),
) -> Result<Point<f32>, String> {
    Err(UNSUPPORTED.into())
}

pub fn set_window_visible(window: &mut Window, visible: bool) -> Result<(), String> {
    if visible {
        window.activate_window();
    }
    Ok(())
}

pub async fn portal_screenshot() -> Result<Option<image::RgbaImage>, String> {
    use ashpd::desktop::{ResponseError, screenshot::Screenshot};

    let result = async {
        Screenshot::request()
            .interactive(false)
            .modal(false)
            .send()
            .await?
            .response()
    }
    .await;
    let screenshot = match result {
        Ok(screenshot) => screenshot,
        Err(ashpd::Error::Response(ResponseError::Cancelled))
        | Err(ashpd::Error::Portal(ashpd::PortalError::Cancelled(_))) => return Ok(None),
        Err(error) => {
            return Err(format!(
                "Unable to request a screenshot from the desktop portal: {error}"
            ));
        }
    };
    let path = url::Url::parse(screenshot.uri().as_str())
        .map_err(|error| format!("Invalid screenshot URI: {error}"))?
        .to_file_path()
        .map_err(|_| "The screenshot portal returned a non-local image URI.".to_string())?;
    let pixels = image::open(&path)
        .map_err(|error| format!("Unable to read the portal screenshot: {error}"))?
        .into_rgba8();
    if pixels.width() == 0 || pixels.height() == 0 {
        return Err("The screenshot portal returned an empty image.".into());
    }
    Ok(Some(pixels))
}

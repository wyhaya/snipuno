use super::{CaptureDisplayId, WindowTarget, error, validate_window, worker::Cancellation};
use crate::image_processing::captured_pixels;
use image::RgbaImage;
use std::{sync::mpsc, thread, time::Duration};
use windows::{
    Foundation::TypedEventHandler,
    Graphics::{
        Capture::{
            Direct3D11CaptureFrame, Direct3D11CaptureFramePool, GraphicsCaptureItem,
            GraphicsCaptureSession,
        },
        DirectX::{Direct3D11::IDirect3DDevice, DirectXPixelFormat},
    },
    Win32::{
        Foundation::{HMODULE, HWND},
        Graphics::{
            Direct3D::D3D_DRIVER_TYPE_HARDWARE,
            Direct3D11::{
                D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                D3D11_MAP_FLAG_DO_NOT_WAIT, D3D11_MAP_READ, D3D11_MAPPED_SUBRESOURCE,
                D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING, D3D11CreateDevice,
                ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D,
            },
            Dxgi::{Common::DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_ERROR_WAS_STILL_DRAWING, IDXGIDevice},
            Gdi::HMONITOR,
        },
        System::WinRT::{
            Direct3D11::{CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess},
            Graphics::Capture::IGraphicsCaptureItemInterop,
        },
    },
    core::{Interface, factory},
};

pub(super) struct Device {
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    direct3d: IDirect3DDevice,
}

impl Device {
    pub fn new() -> Result<Self, String> {
        let (mut device, mut context) = (None, None);
        unsafe {
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut context),
            )
        }
        .map_err(error)?;
        let device = device.ok_or("Windows returned no capture device.")?;
        let context = context.ok_or("Windows returned no capture context.")?;
        let dxgi: IDXGIDevice = device.cast().map_err(error)?;
        let direct3d = unsafe { CreateDirect3D11DeviceFromDXGIDevice(&dxgi) }
            .and_then(|device| device.cast())
            .map_err(error)?;
        Ok(Self {
            device,
            context,
            direct3d,
        })
    }

    pub fn monitor(
        &self,
        id: CaptureDisplayId,
        cancel: &Cancellation,
    ) -> Result<RgbaImage, String> {
        let interop: IGraphicsCaptureItemInterop =
            factory::<GraphicsCaptureItem, _>().map_err(error)?;
        let item = unsafe { interop.CreateForMonitor(HMONITOR(id.0 as usize as *mut _)) }
            .map_err(error)?;
        self.capture(item, None, cancel)
    }

    pub fn window(
        &self,
        target: &WindowTarget,
        cancel: &Cancellation,
    ) -> Result<RgbaImage, String> {
        let interop: IGraphicsCaptureItemInterop =
            factory::<GraphicsCaptureItem, _>().map_err(error)?;
        let item = unsafe { interop.CreateForWindow(HWND(target.id.0 as usize as *mut _)) }
            .map_err(error)?;
        self.capture(item, Some(target), cancel)
    }

    fn capture(
        &self,
        item: GraphicsCaptureItem,
        target: Option<&WindowTarget>,
        cancel: &Cancellation,
    ) -> Result<RgbaImage, String> {
        cancel.check()?;
        let size = item.Size().map_err(error)?;
        if size.Width <= 0 || size.Height <= 0 {
            return Err("The capture target has no visible content.".into());
        }
        let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
            &self.direct3d,
            DirectXPixelFormat::B8G8R8A8UIntNormalized,
            1,
            size,
        )
        .map_err(error)?;
        let mut capture = Capture {
            pool,
            session: None,
            item,
            arrived: None,
            closed: None,
        };
        capture.session = Some(
            capture
                .pool
                .CreateCaptureSession(&capture.item)
                .map_err(error)?,
        );
        let session = capture.session.as_ref().unwrap();
        session.SetIsCursorCaptureEnabled(false).map_err(error)?;
        let (sender, receiver) = mpsc::channel();
        let closed_sender = sender.clone();
        capture.closed = Some(
            capture
                .item
                .Closed(&TypedEventHandler::new(move |_, _| {
                    let _ = closed_sender.send(Event::Closed);
                    Ok(())
                }))
                .map_err(error)?,
        );
        capture.arrived = Some(
            capture
                .pool
                .FrameArrived(&TypedEventHandler::new(move |_, _| {
                    let _ = sender.send(Event::Frame);
                    Ok(())
                }))
                .map_err(error)?,
        );
        session.StartCapture().map_err(error)?;
        loop {
            cancel.check()?;
            if let Some(target) = target {
                validate_window(target)?;
            }
            match receiver.recv_timeout(Duration::from_millis(10)) {
                Ok(Event::Closed) => {
                    return Err("The capture target closed. Start a new screenshot.".into());
                }
                Ok(Event::Frame) => {
                    let frame = Frame(capture.pool.TryGetNextFrame().map_err(error)?);
                    let content = frame.0.ContentSize().map_err(error)?;
                    if content.Width != size.Width || content.Height != size.Height {
                        return Err(
                            "The capture target changed size. Start a new screenshot.".into()
                        );
                    }
                    let pixels =
                        self.read_frame(&frame.0, size.Width as u32, size.Height as u32, cancel)?;
                    cancel.check()?;
                    return Ok(pixels);
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err("Windows stopped the capture session.".into());
                }
            }
        }
    }

    fn read_frame(
        &self,
        frame: &Direct3D11CaptureFrame,
        width: u32,
        height: u32,
        cancel: &Cancellation,
    ) -> Result<RgbaImage, String> {
        let access: IDirect3DDxgiInterfaceAccess = frame
            .Surface()
            .and_then(|surface| surface.cast())
            .map_err(error)?;
        let source: ID3D11Texture2D = unsafe { access.GetInterface() }.map_err(error)?;
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        unsafe { source.GetDesc(&mut desc) };
        if desc.Format != DXGI_FORMAT_B8G8R8A8_UNORM || width > desc.Width || height > desc.Height {
            return Err("Windows returned an unexpected capture texture.".into());
        }
        desc.Usage = D3D11_USAGE_STAGING;
        desc.BindFlags = 0;
        desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
        desc.MiscFlags = 0;
        let mut staging = None;
        unsafe { self.device.CreateTexture2D(&desc, None, Some(&mut staging)) }.map_err(error)?;
        let staging = staging.ok_or("Windows returned no readable capture texture.")?;
        unsafe { self.context.CopyResource(&staging, &source) };
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        unsafe { self.context.Flush() };
        loop {
            cancel.check()?;
            match unsafe {
                self.context.Map(
                    &staging,
                    0,
                    D3D11_MAP_READ,
                    D3D11_MAP_FLAG_DO_NOT_WAIT.0 as u32,
                    Some(&mut mapped),
                )
            } {
                Ok(()) => break,
                Err(error) if error.code() == DXGI_ERROR_WAS_STILL_DRAWING => {
                    thread::sleep(Duration::from_millis(2));
                }
                Err(failure) => return Err(error(failure)),
            }
        }
        let _mapping = Mapping {
            context: &self.context,
            texture: &staging,
        };
        let length = (mapped.RowPitch as usize)
            .checked_mul(height as usize)
            .filter(|length| *length <= isize::MAX as usize)
            .ok_or("The captured image is too large.")?;
        if mapped.pData.is_null() || length == 0 {
            return Err("Windows returned an empty capture buffer.".into());
        }
        // The staging texture stays mapped until conversion completes.
        let bytes = unsafe { std::slice::from_raw_parts(mapped.pData.cast::<u8>(), length) };
        captured_pixels(bytes, mapped.RowPitch as usize, width, height)
    }
}

enum Event {
    Frame,
    Closed,
}

struct Capture {
    pool: Direct3D11CaptureFramePool,
    session: Option<GraphicsCaptureSession>,
    item: GraphicsCaptureItem,
    arrived: Option<i64>,
    closed: Option<i64>,
}

impl Drop for Capture {
    fn drop(&mut self) {
        if let Some(token) = self.arrived.take() {
            let _ = self.pool.RemoveFrameArrived(token);
        }
        if let Some(token) = self.closed.take() {
            let _ = self.item.RemoveClosed(token);
        }
        if let Some(session) = self.session.take() {
            let _ = session.Close();
        }
        let _ = self.pool.Close();
    }
}

struct Frame(Direct3D11CaptureFrame);

impl Drop for Frame {
    fn drop(&mut self) {
        let _ = self.0.Close();
    }
}

struct Mapping<'a> {
    context: &'a ID3D11DeviceContext,
    texture: &'a ID3D11Texture2D,
}

impl Drop for Mapping<'_> {
    fn drop(&mut self) {
        unsafe { self.context.Unmap(self.texture, 0) };
    }
}

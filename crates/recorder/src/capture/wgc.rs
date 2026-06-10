//! Windows.Graphics.Capture session for a single HWND (spec §9).
//!
//! Uses a **free-threaded** frame pool so no DispatcherQueue / message pump is
//! needed and the `FrameArrived` callback can run on a worker thread (mitigates
//! windows-rs #1409). Each frame's `ID3D11Texture2D` + `SystemRelativeTime` is
//! pushed onto a channel for the grid/encoder to consume.

use crossbeam_channel::{bounded, Receiver, Sender};
use std::sync::{Arc, Mutex};
use windows::core::{IInspectable, Interface};
use windows::Foundation::TypedEventHandler;
use windows::Graphics::Capture::{
    Direct3D11CaptureFramePool, GraphicsCaptureItem, GraphicsCaptureSession,
};
use windows::Graphics::DirectX::Direct3D11::IDirect3DDevice;
use windows::Graphics::DirectX::DirectXPixelFormat;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Direct3D11::{ID3D11Texture2D, D3D11_TEXTURE2D_DESC};
use windows::Win32::Graphics::Gdi::HMONITOR;
use windows::Win32::System::WinRT::Direct3D11::IDirect3DDxgiInterfaceAccess;
use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;

/// One captured GPU frame.
pub struct RawFrame {
    pub texture: ID3D11Texture2D,
    /// QPC-based capture time in 100ns units (`SystemRelativeTime`).
    pub time_100ns: i64,
    pub width: u32,
    pub height: u32,
}
// SAFETY: D3D11 textures are free-threaded; we move them between our own threads.
unsafe impl Send for RawFrame {}

/// An active capture session. Dropping it stops capture.
pub struct WgcCapture {
    _item: GraphicsCaptureItem,
    pool: Direct3D11CaptureFramePool,
    session: GraphicsCaptureSession,
    rx: Receiver<RawFrame>,
    size: (u32, u32),
}

impl WgcCapture {
    /// The capture item's size (window or monitor), used to size the encoder.
    pub fn size(&self) -> (u32, u32) {
        self.size
    }

    /// Start capturing `hwnd`, rendering into `winrt_device`.
    pub fn for_hwnd(hwnd: HWND, winrt_device: &IDirect3DDevice) -> anyhow::Result<Self> {
        let interop: IGraphicsCaptureItemInterop =
            windows::core::factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()?;
        let item: GraphicsCaptureItem = unsafe { interop.CreateForWindow(hwnd)? };
        Self::from_item(item, winrt_device)
    }

    /// Start capturing a monitor (delivers continuous frames; spec §28 note).
    pub fn for_monitor(monitor: HMONITOR, winrt_device: &IDirect3DDevice) -> anyhow::Result<Self> {
        let interop: IGraphicsCaptureItemInterop =
            windows::core::factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()?;
        let item: GraphicsCaptureItem = unsafe { interop.CreateForMonitor(monitor)? };
        Self::from_item(item, winrt_device)
    }

    fn from_item(item: GraphicsCaptureItem, winrt_device: &IDirect3DDevice) -> anyhow::Result<Self> {
        let size = item.Size()?;

        const FORMAT: DirectXPixelFormat = DirectXPixelFormat::B8G8R8A8UIntNormalized;
        let pool =
            Direct3D11CaptureFramePool::CreateFreeThreaded(winrt_device, FORMAT, 4, size)?;

        // Bounded so a stalled consumer drops frames rather than growing unbounded.
        let (tx, rx): (Sender<RawFrame>, Receiver<RawFrame>) = bounded(8);

        // WGC/D3D objects are agile but windows-rs types aren't `Send`; the
        // callback runs on an MTA worker, so moving them in is sound.
        struct Agile(windows::Graphics::DirectX::Direct3D11::IDirect3DDevice);
        unsafe impl Send for Agile {}
        unsafe impl Sync for Agile {}
        let device = Agile(winrt_device.clone());
        let last_size = Arc::new(Mutex::new((size.Width, size.Height)));

        let handler = TypedEventHandler::<Direct3D11CaptureFramePool, IInspectable>::new(
            move |sender, _| {
                let _ = &device; // capture the whole Agile (not just the !Send field)
                if let Some(pool) = sender.as_ref() {
                    if let Ok(frame) = pool.TryGetNextFrame() {
                        // Recreate the pool if the window resized, or frames stop.
                        if let Ok(cs) = frame.ContentSize() {
                            let mut last = last_size.lock().unwrap();
                            if cs.Width > 0 && cs.Height > 0 && (cs.Width, cs.Height) != *last {
                                *last = (cs.Width, cs.Height);
                                let _ = pool.Recreate(&device.0, FORMAT, 4, cs);
                            }
                        }
                        if let Ok(raw) = extract_frame(&frame) {
                            let _ = tx.try_send(raw); // best-effort; drop if full
                        }
                    }
                }
                Ok(())
            },
        );
        pool.FrameArrived(&handler)?;

        let session = pool.CreateCaptureSession(&item)?;
        // Remove the yellow capture border (Win11 22000+). Best-effort: older
        // builds lack IGraphicsCaptureSession3 and return an error we ignore.
        let _ = session.SetIsBorderRequired(false);
        session.StartCapture()?;

        Ok(WgcCapture {
            _item: item,
            pool,
            session,
            rx,
            size: (size.Width.max(0) as u32, size.Height.max(0) as u32),
        })
    }

    /// Receiver of captured frames.
    pub fn frames(&self) -> Receiver<RawFrame> {
        self.rx.clone()
    }
}

impl Drop for WgcCapture {
    fn drop(&mut self) {
        let _ = self.session.Close();
        let _ = self.pool.Close();
    }
}

fn extract_frame(
    frame: &windows::Graphics::Capture::Direct3D11CaptureFrame,
) -> anyhow::Result<RawFrame> {
    let surface = frame.Surface()?;
    let access: IDirect3DDxgiInterfaceAccess = surface.cast()?;
    let texture: ID3D11Texture2D = unsafe { access.GetInterface()? };
    let mut desc = D3D11_TEXTURE2D_DESC::default();
    unsafe { texture.GetDesc(&mut desc) };
    let time_100ns = frame.SystemRelativeTime()?.Duration;
    Ok(RawFrame {
        texture,
        time_100ns,
        width: desc.Width,
        height: desc.Height,
    })
}

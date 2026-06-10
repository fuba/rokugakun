//! BGRA -> NV12 color conversion via the D3D11 Video Processor (spec §4 note).
//!
//! WGC delivers BGRA textures; the HEVC encoder MFT wants NV12. This wraps
//! `ID3D11VideoProcessor` to blit+convert (and scale) on the GPU.

use super::D3dDevice;
use anyhow::{anyhow, Result};
use windows::core::Interface;
use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_NV12, DXGI_RATIONAL, DXGI_SAMPLE_DESC};

/// Converts BGRA capture textures into NV12 encoder-input textures.
pub struct Nv12Converter {
    device: ID3D11Device,
    video_device: ID3D11VideoDevice,
    video_context: ID3D11VideoContext,
    enumerator: ID3D11VideoProcessorEnumerator,
    processor: ID3D11VideoProcessor,
    out_w: u32,
    out_h: u32,
    /// Source crop (client area within the captured window frame); None = full.
    crop: Option<RECT>,
}
// SAFETY: owned + used only by the single encode-feeding thread.
unsafe impl Send for Nv12Converter {}

impl Nv12Converter {
    pub fn new(
        d3d: &D3dDevice,
        in_w: u32,
        in_h: u32,
        out_w: u32,
        out_h: u32,
        crop: Option<RECT>,
    ) -> Result<Self> {
        let video_device: ID3D11VideoDevice = d3d.device.cast()?;
        let video_context: ID3D11VideoContext = d3d.context.cast()?;

        let content_desc = D3D11_VIDEO_PROCESSOR_CONTENT_DESC {
            InputFrameFormat: D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE,
            InputFrameRate: DXGI_RATIONAL { Numerator: 60, Denominator: 1 },
            InputWidth: in_w,
            InputHeight: in_h,
            OutputFrameRate: DXGI_RATIONAL { Numerator: 60, Denominator: 1 },
            OutputWidth: out_w,
            OutputHeight: out_h,
            Usage: D3D11_VIDEO_USAGE_PLAYBACK_NORMAL,
        };

        let enumerator = unsafe { video_device.CreateVideoProcessorEnumerator(&content_desc)? };
        let processor = unsafe { video_device.CreateVideoProcessor(&enumerator, 0)? };

        Ok(Nv12Converter {
            device: d3d.device.clone(),
            video_device,
            video_context,
            enumerator,
            processor,
            out_w,
            out_h,
            crop,
        })
    }

    /// Convert a BGRA source texture into a fresh NV12 texture.
    pub fn convert(&self, src: &ID3D11Texture2D) -> Result<ID3D11Texture2D> {
        let dst = self.alloc_nv12()?;

        let in_view = unsafe {
            let desc = D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC {
                FourCC: 0,
                ViewDimension: D3D11_VPIV_DIMENSION_TEXTURE2D,
                Anonymous: D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC_0 {
                    Texture2D: D3D11_TEX2D_VPIV { MipSlice: 0, ArraySlice: 0 },
                },
            };
            let mut view = None;
            self.video_device
                .CreateVideoProcessorInputView(src, &self.enumerator, &desc, Some(&mut view))?;
            view.ok_or_else(|| anyhow!("null input view"))?
        };

        let out_view = unsafe {
            let desc = D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC {
                ViewDimension: D3D11_VPOV_DIMENSION_TEXTURE2D,
                Anonymous: D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC_0 {
                    Texture2D: D3D11_TEX2D_VPOV { MipSlice: 0 },
                },
            };
            let mut view = None;
            self.video_device
                .CreateVideoProcessorOutputView(&dst, &self.enumerator, &desc, Some(&mut view))?;
            view.ok_or_else(|| anyhow!("null output view"))?
        };

        let stream = D3D11_VIDEO_PROCESSOR_STREAM {
            Enable: true.into(),
            OutputIndex: 0,
            InputFrameOrField: 0,
            PastFrames: 0,
            FutureFrames: 0,
            ppPastSurfaces: std::ptr::null_mut(),
            pInputSurface: std::mem::ManuallyDrop::new(Some(in_view)),
            ppFutureSurfaces: std::ptr::null_mut(),
            ppPastSurfacesRight: std::ptr::null_mut(),
            pInputSurfaceRight: std::mem::ManuallyDrop::new(None),
            ppFutureSurfacesRight: std::ptr::null_mut(),
        };

        unsafe {
            // Crop to the client area (exclude title bar/borders) when requested.
            self.video_context.VideoProcessorSetStreamSourceRect(
                &self.processor,
                0,
                self.crop.is_some(),
                self.crop.as_ref().map(|r| r as *const RECT),
            );
            self.video_context
                .VideoProcessorBlt(&self.processor, &out_view, 0, &[stream])?;
        }
        Ok(dst)
    }

    fn alloc_nv12(&self) -> Result<ID3D11Texture2D> {
        let desc = D3D11_TEXTURE2D_DESC {
            Width: self.out_w,
            Height: self.out_h,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_NV12,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: (D3D11_BIND_RENDER_TARGET.0 | D3D11_BIND_SHADER_RESOURCE.0) as u32,
            CPUAccessFlags: 0,
            MiscFlags: 0,
        };
        let mut tex = None;
        unsafe { self.device.CreateTexture2D(&desc, None, Some(&mut tex))? };
        tex.ok_or_else(|| anyhow!("null NV12 texture"))
    }
}

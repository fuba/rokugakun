//! Window capture (spec §9): Windows.Graphics.Capture + Direct3D11.

mod convert;
mod d3d;
mod grid;
mod wgc;

pub use convert::Nv12Converter;
pub use d3d::D3dDevice;
pub use grid::{FrameGrid, GridDecision};
pub use wgc::{RawFrame, WgcCapture};

/// A capture frame snapped onto the 60fps grid, ready for the encoder.
pub struct GridFrame {
    pub texture: windows::Win32::Graphics::Direct3D11::ID3D11Texture2D,
    pub pts_100ns: i64,
    pub width: u32,
    pub height: u32,
}

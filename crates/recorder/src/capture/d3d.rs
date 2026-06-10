//! D3D11 device creation + WinRT bridging (spec §9).

use anyhow::Result;
use windows::core::Interface;
use windows::Graphics::DirectX::Direct3D11::IDirect3DDevice;
use windows::Win32::Foundation::HMODULE;
use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL_11_0};
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, D3D11_CREATE_DEVICE_BGRA_SUPPORT,
    D3D11_SDK_VERSION,
};
use windows::Win32::Graphics::Dxgi::IDXGIDevice;
use windows::Win32::System::WinRT::Direct3D11::CreateDirect3D11DeviceFromDXGIDevice;

/// A hardware D3D11 device + immediate context used for capture and encode input.
#[derive(Clone)]
pub struct D3dDevice {
    pub device: ID3D11Device,
    pub context: ID3D11DeviceContext,
}

impl D3dDevice {
    /// Create a BGRA-capable hardware device (required by WGC).
    pub fn create() -> Result<Self> {
        let mut device: Option<ID3D11Device> = None;
        let mut context: Option<ID3D11DeviceContext> = None;
        unsafe {
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                Some(&[D3D_FEATURE_LEVEL_11_0]),
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut context),
            )?;
        }
        Ok(D3dDevice {
            device: device.expect("D3D11CreateDevice returned null device"),
            context: context.expect("D3D11CreateDevice returned null context"),
        })
    }

    /// Bridge to the WinRT `IDirect3DDevice` that the capture API consumes.
    pub fn to_winrt(&self) -> Result<IDirect3DDevice> {
        let dxgi: IDXGIDevice = self.device.cast()?;
        let inspectable = unsafe { CreateDirect3D11DeviceFromDXGIDevice(&dxgi)? };
        Ok(inspectable.cast()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires a GPU/display; run with --ignored"]
    fn create_device_and_bridge_to_winrt() {
        crate::win::init_mta();
        let d = D3dDevice::create().expect("create D3D11 device");
        let _winrt = d.to_winrt().expect("bridge to WinRT IDirect3DDevice");
    }
}

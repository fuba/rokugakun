//! Small Windows/COM helpers shared by the pipeline.

use windows::Win32::Media::MediaFoundation::{MFStartup, MFSTARTUP_FULL};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};

/// `MF_VERSION` = (MF_SDK_VERSION << 16) | MF_API_VERSION.
const MF_VERSION: u32 = 0x0002_0070;

/// Start the Media Foundation platform. Idempotent per process.
pub fn init_mf() {
    unsafe {
        let _ = MFStartup(MF_VERSION, MFSTARTUP_FULL);
    }
}

/// Initialise the calling thread's COM apartment as multithreaded (MTA).
///
/// WGC's free-threaded frame pool, WASAPI, and Media Foundation are all happy in
/// an MTA. Safe to call repeatedly; an already-initialised apartment is ignored.
pub fn init_mta() {
    unsafe {
        // RPC_E_CHANGED_MODE if the thread is already STA — harmless for our use.
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
}

/// Wrapper that asserts a COM interface is safe to move across threads.
///
/// D3D11 device/context/textures are free-threaded, so handing a texture pointer
/// to the encode thread is sound even though windows-rs interfaces aren't `Send`.
pub struct SendPtr<T>(pub T);
// SAFETY: only used for free-threaded D3D11 objects moved between our own threads.
unsafe impl<T> Send for SendPtr<T> {}

impl<T> SendPtr<T> {
    pub fn into_inner(self) -> T {
        self.0
    }
}

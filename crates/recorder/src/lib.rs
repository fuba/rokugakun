//! Recorder library: the media pipeline.
//!
//! Hardware-independent modules (`mux`) build everywhere; the Windows capture /
//! encode / audio modules are gated on `cfg(windows)`.

pub mod mux;

#[cfg(windows)]
pub mod win;

#[cfg(windows)]
pub mod capture;

#[cfg(windows)]
pub mod encode;

#[cfg(windows)]
pub mod audio;

#[cfg(windows)]
pub mod demo;

#[cfg(windows)]
pub mod pipeline;

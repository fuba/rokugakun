//! Audio capture (spec §10). Per-process WASAPI loopback.

mod convert;
mod loopback;

pub use convert::f32_to_i16;
pub use loopback::LoopbackCapture;

/// A buffer of captured audio (interleaved float32), spec §10.
#[derive(Debug, Clone)]
pub struct AudioFrame {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u16,
    /// QPC-based capture time in 100ns units.
    pub time_100ns: i64,
}

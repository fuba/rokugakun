//! Media timebase helpers (spec §11).
//!
//! Internally everything is kept in 100ns ticks anchored to a session base, and
//! converted to a 33-bit 90kHz clock when written into MPEG-TS.

/// 100ns ticks per second.
pub const TICKS_PER_SEC: i64 = 10_000_000;
/// MPEG-TS PTS/DTS clock rate.
pub const PTS_HZ: i64 = 90_000;
/// One 60fps frame interval, in 100ns ticks (1/60s). Used by the capture grid.
pub const FRAME_INTERVAL_60_100NS: i64 = TICKS_PER_SEC / 60; // 166_666 (rounded down)

/// Convert a raw QueryPerformanceCounter delta into 100ns ticks.
///
/// `qpc` and `qpc_base` are raw counter values; `qpc_freq` is the counter
/// frequency (ticks/sec) from `QueryPerformanceFrequency`.
#[inline]
pub fn qpc_to_100ns(qpc: i64, qpc_base: i64, qpc_freq: i64) -> i64 {
    debug_assert!(qpc_freq > 0);
    // Use i128 to avoid overflow on the multiply.
    (((qpc - qpc_base) as i128) * TICKS_PER_SEC as i128 / qpc_freq as i128) as i64
}

/// Convert 100ns ticks to a 90kHz PTS value (spec §11).
#[inline]
pub fn ns100_to_pts90k(ticks_100ns: i64) -> i64 {
    ((ticks_100ns as i128) * PTS_HZ as i128 / TICKS_PER_SEC as i128) as i64
}

/// Convert a sample index at a given rate into 100ns ticks. Audio PTS advances
/// by accumulated sample count rather than wall clock (spec §11).
#[inline]
pub fn samples_to_100ns(sample_index: i64, sample_rate: u32) -> i64 {
    debug_assert!(sample_rate > 0);
    ((sample_index as i128) * TICKS_PER_SEC as i128 / sample_rate as i128) as i64
}

/// A session clock: anchors media timestamps to a common QPC base.
#[derive(Debug, Clone, Copy)]
pub struct SessionClock {
    pub qpc_base: i64,
    pub qpc_freq: i64,
}

impl SessionClock {
    pub fn new(qpc_base: i64, qpc_freq: i64) -> Self {
        Self { qpc_base, qpc_freq }
    }

    /// 100ns ticks elapsed since the session base for a given QPC reading.
    #[inline]
    pub fn ticks(&self, qpc: i64) -> i64 {
        qpc_to_100ns(qpc, self.qpc_base, self.qpc_freq)
    }

    /// 90kHz PTS for a given QPC reading.
    #[inline]
    pub fn pts90k(&self, qpc: i64) -> i64 {
        ns100_to_pts90k(self.ticks(qpc))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pts_conversion_one_second() {
        // 1s in 100ns ticks -> 90000 in 90kHz.
        assert_eq!(ns100_to_pts90k(TICKS_PER_SEC), PTS_HZ);
    }

    #[test]
    fn qpc_base_is_zero() {
        // At the base instant, elapsed ticks are zero regardless of frequency.
        assert_eq!(qpc_to_100ns(5_000, 5_000, 10_000_000), 0);
    }

    #[test]
    fn qpc_half_second() {
        // freq = 10MHz, delta = 5M ticks => 0.5s => 5,000,000 100ns ticks.
        assert_eq!(qpc_to_100ns(5_000_000, 0, 10_000_000), 5_000_000);
        assert_eq!(ns100_to_pts90k(5_000_000), 45_000);
    }

    #[test]
    fn audio_pts_from_samples() {
        // 48000 samples at 48kHz == 1 second.
        assert_eq!(samples_to_100ns(48_000, 48_000), TICKS_PER_SEC);
    }
}

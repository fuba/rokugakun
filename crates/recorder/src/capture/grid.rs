//! 60fps frame grid (spec §9).
//!
//! Capture frames arrive irregularly; the encoder wants a constant cadence. This
//! snaps each frame's timestamp to the nearest 1/60s slot, **drops** frames that
//! land in an already-used slot (capture ran ahead), and reports how many slots
//! were skipped so the caller can **duplicate** the previous frame to fill a gap.

use rec_core::timebase::FRAME_INTERVAL_60_100NS;

/// What to do with an arrived frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GridDecision {
    /// Grid-snapped presentation timestamp (100ns).
    pub pts_100ns: i64,
    /// Number of preceding empty slots to fill by duplicating the prior frame.
    pub duplicates: u32,
}

/// Snaps capture timestamps onto a fixed-rate grid.
pub struct FrameGrid {
    interval: i64,
    base: Option<i64>,
    last_slot: i64,
}

impl Default for FrameGrid {
    fn default() -> Self {
        Self::with_interval(FRAME_INTERVAL_60_100NS)
    }
}

impl FrameGrid {
    pub fn with_interval(interval: i64) -> Self {
        FrameGrid {
            interval,
            base: None,
            last_slot: -1,
        }
    }

    /// The grid slot duration in 100ns units (one frame at the grid's rate).
    pub fn interval(&self) -> i64 {
        self.interval
    }

    /// Feed a capture timestamp (100ns). `None` means drop this frame.
    pub fn tick(&mut self, time_100ns: i64) -> Option<GridDecision> {
        let base = match self.base {
            Some(b) => b,
            None => {
                self.base = Some(time_100ns);
                self.last_slot = 0;
                return Some(GridDecision { pts_100ns: 0, duplicates: 0 });
            }
        };

        let rel = (time_100ns - base).max(0);
        // round to nearest slot
        let slot = (rel + self.interval / 2) / self.interval;
        if slot <= self.last_slot {
            return None; // capture outran the grid; drop
        }
        let duplicates = (slot - self.last_slot - 1) as u32;
        self.last_slot = slot;
        Some(GridDecision {
            pts_100ns: slot * self.interval,
            duplicates,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const I: i64 = FRAME_INTERVAL_60_100NS;

    #[test]
    fn first_frame_is_pts_zero() {
        let mut g = FrameGrid::default();
        let d = g.tick(123_456).unwrap();
        assert_eq!(d.pts_100ns, 0);
        assert_eq!(d.duplicates, 0);
    }

    #[test]
    fn steady_60fps_advances_one_slot() {
        let mut g = FrameGrid::default();
        let t0 = 1_000_000;
        g.tick(t0).unwrap();
        let d = g.tick(t0 + I).unwrap();
        assert_eq!(d.pts_100ns, I);
        assert_eq!(d.duplicates, 0);
    }

    #[test]
    fn too_fast_frame_is_dropped() {
        let mut g = FrameGrid::default();
        g.tick(0).unwrap();
        // a frame only 1/4 interval later lands in slot 0 again -> drop
        assert!(g.tick(I / 4).is_none());
    }

    #[test]
    fn slow_frame_reports_duplicates() {
        let mut g = FrameGrid::default();
        g.tick(0).unwrap();
        // ~3 intervals later -> slot 3, two empty slots (1,2) to fill
        let d = g.tick(3 * I).unwrap();
        assert_eq!(d.pts_100ns, 3 * I);
        assert_eq!(d.duplicates, 2);
    }
}

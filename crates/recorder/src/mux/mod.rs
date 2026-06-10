//! Muxing + segment writing (spec §15). The `ts-mux` crate produces the bytes;
//! [`SegmentWriter`] owns rotation, temp-file safety, and DB bookkeeping.

mod segment_writer;

pub use segment_writer::{ClosedSegment, SegmentParams, SegmentWriter};

/// Which elementary stream a packet belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamKind {
    Video,
    Audio,
}

/// An encoded access unit flowing from an encoder to the segment writer.
///
/// Owned (unlike [`ts_mux::Packet`]) because packets are queued across threads.
#[derive(Debug, Clone)]
pub struct EncodedPacket {
    pub data: Vec<u8>,
    pub pts_90k: i64,
    pub dts_90k: i64,
    pub keyframe: bool,
    pub kind: StreamKind,
}

impl EncodedPacket {
    fn as_ts(&self) -> ts_mux::Packet<'_> {
        ts_mux::Packet {
            data: &self.data,
            pts_90k: self.pts_90k,
            dts_90k: self.dts_90k,
            keyframe: self.keyframe,
        }
    }
}

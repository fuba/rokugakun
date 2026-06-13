//! A small, dependency-free MPEG-TS muxer (spec §14).
//!
//! Produces single-playable `.ts` segments: every [`TsMuxer::begin_segment`]
//! re-emits PAT + PMT, and HEVC parameter sets (VPS/SPS/PPS) are re-inserted
//! ahead of each keyframe access unit. Video and audio are carried as PES with a
//! 33-bit 90kHz PTS/DTS clock; PCR rides the video PID.
//!
//! Fixed layout (spec §14):
//! ```text
//! PAT PID 0x0000   PMT PID 0x1000   Video PID 0x0100   Audio PID 0x0101
//! PCR PID = Video PID   HEVC stream_type 0x24   AAC(ADTS) stream_type 0x0F
//! ```

use std::io::{self, Write};

pub const TS_PACKET_SIZE: usize = 188;
const SYNC_BYTE: u8 = 0x47;

pub const PAT_PID: u16 = 0x0000;
pub const PMT_PID: u16 = 0x1000;
pub const VIDEO_PID: u16 = 0x0100;
pub const AUDIO_PID: u16 = 0x0101;

pub const STREAM_TYPE_HEVC: u8 = 0x24;
pub const STREAM_TYPE_AAC_ADTS: u8 = 0x0F;

const PES_STREAM_ID_VIDEO: u8 = 0xE0;
const PES_STREAM_ID_AUDIO: u8 = 0xC0;
const PROGRAM_NUMBER: u16 = 1;
const TRANSPORT_STREAM_ID: u16 = 1;

/// AAC sampling-frequency-index table (ISO/IEC 14496-3).
const SAMPLE_RATES: [u32; 13] = [
    96000, 88200, 64000, 48000, 44100, 32000, 24000, 22050, 16000, 12000, 11025, 8000, 7350,
];

/// Per-stream configuration needed to build segment headers.
#[derive(Debug, Clone)]
pub struct StreamConfig {
    /// HEVC VPS/SPS/PPS as Annex B (with start codes), re-emitted per keyframe.
    pub hevc_vps_sps_pps: Vec<u8>,
    /// Audio sample rate (Hz) — selects the ADTS frequency index.
    pub aac_sample_rate: u32,
    /// Audio channel count — ADTS channel configuration.
    pub aac_channels: u8,
}

impl StreamConfig {
    fn sample_rate_index(&self) -> u8 {
        SAMPLE_RATES
            .iter()
            .position(|&r| r == self.aac_sample_rate)
            .unwrap_or(3) as u8 // default 48kHz
    }
}

/// One encoded access unit handed to the muxer.
#[derive(Debug, Clone, Copy)]
pub struct Packet<'a> {
    /// Video: Annex B access unit. Audio: raw AAC frame (no ADTS — added here).
    pub data: &'a [u8],
    pub pts_90k: i64,
    pub dts_90k: i64,
    pub keyframe: bool,
}

/// Stateful MPEG-TS muxer. One instance spans a whole session (continuity
/// counters persist across segments); call [`Self::begin_segment`] per file.
pub struct TsMuxer {
    cfg: StreamConfig,
    cc_pat: u8,
    cc_pmt: u8,
    cc_video: u8,
    cc_audio: u8,
}

impl TsMuxer {
    pub fn new(cfg: StreamConfig) -> Self {
        TsMuxer {
            cfg,
            cc_pat: 0,
            cc_pmt: 0,
            cc_video: 0,
            cc_audio: 0,
        }
    }

    /// Replace the HEVC parameter sets (VPS/SPS/PPS) emitted before each keyframe.
    /// Used when the source resolution changes mid-session.
    pub fn set_hevc_params(&mut self, vps_sps_pps: Vec<u8>) {
        self.cfg.hevc_vps_sps_pps = vps_sps_pps;
    }

    /// Emit PAT + PMT at the start of a new segment (spec §14 single-playable).
    pub fn begin_segment(&mut self, w: &mut impl Write) -> io::Result<()> {
        let pat = build_pat();
        write_psi(w, PAT_PID, &mut self.cc_pat, &pat)?;
        let pmt = build_pmt();
        write_psi(w, PMT_PID, &mut self.cc_pmt, &pmt)?;
        Ok(())
    }

    /// Write a video access unit. Keyframes are prefixed with parameter sets and
    /// carry PCR + random-access indicator.
    pub fn write_video(&mut self, w: &mut impl Write, pkt: &Packet) -> io::Result<()> {
        let mut es: Vec<u8> = Vec::with_capacity(pkt.data.len() + self.cfg.hevc_vps_sps_pps.len());
        if pkt.keyframe {
            es.extend_from_slice(&self.cfg.hevc_vps_sps_pps);
        }
        es.extend_from_slice(pkt.data);

        let pes = build_pes(PES_STREAM_ID_VIDEO, pkt.pts_90k, Some(pkt.dts_90k), &es, true);
        // PCR on the video PID, value = decode timestamp.
        write_pes_packets(
            w,
            VIDEO_PID,
            &mut self.cc_video,
            &pes,
            Some(pkt.dts_90k),
            pkt.keyframe,
        )
    }

    /// Write an AAC frame: an ADTS header is synthesized and prepended.
    pub fn write_audio(&mut self, w: &mut impl Write, pkt: &Packet) -> io::Result<()> {
        let adts = build_adts(
            pkt.data.len(),
            self.cfg.sample_rate_index(),
            self.cfg.aac_channels,
        );
        let mut es = Vec::with_capacity(adts.len() + pkt.data.len());
        es.extend_from_slice(&adts);
        es.extend_from_slice(pkt.data);

        let pes = build_pes(PES_STREAM_ID_AUDIO, pkt.pts_90k, None, &es, false);
        write_pes_packets(w, AUDIO_PID, &mut self.cc_audio, &pes, None, false)
    }
}

// ---------------------------------------------------------------------------
// PES
// ---------------------------------------------------------------------------

/// Encode a 33-bit timestamp field with the given 4-bit prefix (PTS=0b0010,
/// PTS-of-pair=0b0011, DTS=0b0001).
fn encode_ts_field(prefix: u8, v: i64) -> [u8; 5] {
    let v = (v as u64) & 0x1_FFFF_FFFF;
    [
        (prefix << 4) | (((v >> 30) & 0x07) as u8) << 1 | 0x01,
        ((v >> 22) & 0xFF) as u8,
        ((((v >> 15) & 0x7F) as u8) << 1) | 0x01,
        ((v >> 7) & 0xFF) as u8,
        (((v & 0x7F) as u8) << 1) | 0x01,
    ]
}

/// Build a complete PES packet. `unbounded_length` (video) writes a length field
/// of 0; audio writes the real length.
fn build_pes(stream_id: u8, pts: i64, dts: Option<i64>, payload: &[u8], unbounded_length: bool) -> Vec<u8> {
    let (pts_dts_flags, header_data_len) = match dts {
        Some(d) if d != pts => (0b11u8, 10usize),
        _ => (0b10u8, 5usize),
    };

    let after_len = 3 + header_data_len + payload.len();
    let length_field: u16 = if unbounded_length || after_len > 0xFFFF {
        0
    } else {
        after_len as u16
    };

    let mut v = Vec::with_capacity(9 + header_data_len + payload.len());
    v.extend_from_slice(&[0x00, 0x00, 0x01, stream_id]);
    v.extend_from_slice(&length_field.to_be_bytes());
    v.push(0x80); // '10' marker, no scrambling/priority/alignment
    v.push(pts_dts_flags << 6);
    v.push(header_data_len as u8);
    if pts_dts_flags == 0b10 {
        v.extend_from_slice(&encode_ts_field(0b0010, pts));
    } else {
        v.extend_from_slice(&encode_ts_field(0b0011, pts));
        v.extend_from_slice(&encode_ts_field(0b0001, dts.unwrap()));
    }
    v.extend_from_slice(payload);
    v
}

// ---------------------------------------------------------------------------
// TS packetization
// ---------------------------------------------------------------------------

fn push_ts_header(pkt: &mut Vec<u8>, pusi: bool, pid: u16, afc: u8, cc: u8) {
    pkt.push(SYNC_BYTE);
    pkt.push(((pusi as u8) << 6) | ((pid >> 8) as u8 & 0x1F));
    pkt.push((pid & 0xFF) as u8);
    pkt.push((afc << 4) | (cc & 0x0F));
}

/// 6-byte PCR field: base at 90kHz (extension 0).
fn encode_pcr(pcr_90k: i64) -> [u8; 6] {
    let base = (pcr_90k as u64) & 0x1_FFFF_FFFF;
    let ext: u64 = 0;
    [
        (base >> 25) as u8,
        (base >> 17) as u8,
        (base >> 9) as u8,
        (base >> 1) as u8,
        (((base & 0x1) as u8) << 7) | 0x7E | ((ext >> 8) as u8 & 0x01),
        (ext & 0xFF) as u8,
    ]
}

/// Split a PES into 188-byte TS packets on `pid`, advancing `cc`. The first
/// packet may carry PCR and/or the random-access indicator in its adaptation
/// field; the last packet is stuffed to a full 188 bytes.
fn write_pes_packets(
    w: &mut impl Write,
    pid: u16,
    cc: &mut u8,
    pes: &[u8],
    pcr: Option<i64>,
    random_access: bool,
) -> io::Result<()> {
    let mut offset = 0usize;
    let mut first = true;

    while offset < pes.len() {
        let remaining = pes.len() - offset;
        let use_pcr = first && pcr.is_some();
        let use_ra = first && random_access;
        let want_af = use_pcr || use_ra || remaining < 184;

        let mut pkt = Vec::with_capacity(TS_PACKET_SIZE);

        if !want_af {
            push_ts_header(&mut pkt, first, pid, 0b01, *cc);
            pkt.extend_from_slice(&pes[offset..offset + 184]);
            offset += 184;
        } else {
            let base_af_payload = 1 + if use_pcr { 6 } else { 0 }; // flags(+pcr)
            let max_payload = 184 - (1 + base_af_payload); // minus length byte
            let payload_len = remaining.min(max_payload);
            let afc = if payload_len == 0 { 0b10 } else { 0b11 };
            push_ts_header(&mut pkt, first, pid, afc, *cc);

            let adaptation_field_length = 183 - payload_len; // bytes after length byte
            pkt.push(adaptation_field_length as u8);
            let mut flags = 0u8;
            if use_ra {
                flags |= 0x40;
            }
            if use_pcr {
                flags |= 0x10;
            }
            pkt.push(flags);
            if use_pcr {
                pkt.extend_from_slice(&encode_pcr(pcr.unwrap()));
            }
            let stuffing = adaptation_field_length - base_af_payload;
            pkt.extend(std::iter::repeat_n(0xFFu8, stuffing));
            pkt.extend_from_slice(&pes[offset..offset + payload_len]);
            offset += payload_len;
        }

        debug_assert_eq!(pkt.len(), TS_PACKET_SIZE);
        w.write_all(&pkt)?;
        *cc = (*cc + 1) & 0x0F;
        first = false;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// PSI: PAT / PMT
// ---------------------------------------------------------------------------

/// Wrap a PSI section (with pointer_field) into a single TS packet.
fn write_psi(w: &mut impl Write, pid: u16, cc: &mut u8, section: &[u8]) -> io::Result<()> {
    let mut pkt = Vec::with_capacity(TS_PACKET_SIZE);
    push_ts_header(&mut pkt, true, pid, 0b01, *cc);
    pkt.push(0x00); // pointer_field
    pkt.extend_from_slice(section);
    assert!(pkt.len() <= TS_PACKET_SIZE, "PSI section too large for one packet");
    pkt.resize(TS_PACKET_SIZE, 0xFF);
    w.write_all(&pkt)?;
    *cc = (*cc + 1) & 0x0F;
    Ok(())
}

/// Finish a PSI section: prepend table header + section_length, append CRC32.
fn finish_section(table_id: u8, body: &[u8]) -> Vec<u8> {
    let section_length = body.len() + 4; // body + CRC32
    let mut s = Vec::with_capacity(3 + section_length);
    s.push(table_id);
    // syntax(1)=1, '0', reserved '11', then high nibble of length
    s.push(0xB0 | ((section_length >> 8) & 0x0F) as u8);
    s.push((section_length & 0xFF) as u8);
    s.extend_from_slice(body);
    let crc = crc32_mpeg2(&s);
    s.extend_from_slice(&crc.to_be_bytes());
    s
}

fn build_pat() -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(&TRANSPORT_STREAM_ID.to_be_bytes());
    body.push(0xC1); // reserved '11', version 0, current_next_indicator 1
    body.push(0x00); // section_number
    body.push(0x00); // last_section_number
    body.extend_from_slice(&PROGRAM_NUMBER.to_be_bytes());
    body.extend_from_slice(&(0xE000u16 | (PMT_PID & 0x1FFF)).to_be_bytes());
    finish_section(0x00, &body)
}

fn build_pmt() -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(&PROGRAM_NUMBER.to_be_bytes());
    body.push(0xC1); // version/current
    body.push(0x00); // section_number
    body.push(0x00); // last_section_number
    body.extend_from_slice(&(0xE000u16 | (VIDEO_PID & 0x1FFF)).to_be_bytes()); // PCR_PID
    body.extend_from_slice(&0xF000u16.to_be_bytes()); // program_info_length = 0

    // ES: HEVC video
    body.push(STREAM_TYPE_HEVC);
    body.extend_from_slice(&(0xE000u16 | (VIDEO_PID & 0x1FFF)).to_be_bytes());
    body.extend_from_slice(&0xF000u16.to_be_bytes()); // ES_info_length = 0

    // ES: AAC audio (ADTS)
    body.push(STREAM_TYPE_AAC_ADTS);
    body.extend_from_slice(&(0xE000u16 | (AUDIO_PID & 0x1FFF)).to_be_bytes());
    body.extend_from_slice(&0xF000u16.to_be_bytes());

    finish_section(0x02, &body)
}

/// MPEG-2 CRC32 (poly 0x04C11DB7, init 0xFFFFFFFF, no reflection, no final xor).
fn crc32_mpeg2(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &b in data {
        crc ^= (b as u32) << 24;
        for _ in 0..8 {
            crc = if crc & 0x8000_0000 != 0 {
                (crc << 1) ^ 0x04C1_1DB7
            } else {
                crc << 1
            };
        }
    }
    crc
}

// ---------------------------------------------------------------------------
// ADTS
// ---------------------------------------------------------------------------

/// Build a 7-byte ADTS header (AAC-LC, no CRC) for an `aac_len`-byte frame.
fn build_adts(aac_len: usize, sr_index: u8, channels: u8) -> [u8; 7] {
    let frame_len = (7 + aac_len) as u32; // 13 bits
    let profile = 1u8; // AAC-LC (object type 2 -> profile 1)
    let chan = channels & 0x07;
    [
        0xFF,
        0xF1, // syncword high + MPEG-4, layer 0, protection_absent=1
        (profile << 6) | ((sr_index & 0x0F) << 2) | ((chan >> 2) & 0x01),
        ((chan & 0x03) << 6) | (((frame_len >> 11) & 0x03) as u8),
        ((frame_len >> 3) & 0xFF) as u8,
        (((frame_len & 0x07) as u8) << 5) | 0x1F, // + buffer fullness high
        0xFC, // buffer fullness low (0x7FF) + 0 raw blocks
    ]
}

#[cfg(test)]
mod tests;

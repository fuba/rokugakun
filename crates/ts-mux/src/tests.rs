//! Self-contained validation of the TS byte format: a tiny demuxer parses what
//! the muxer wrote and we assert on structure, CRCs, and timestamps. No ffmpeg.

use super::*;

fn cfg() -> StreamConfig {
    StreamConfig {
        // fake but plausible Annex B parameter sets (VPS/SPS/PPS start codes).
        hevc_vps_sps_pps: vec![
            0x00, 0x00, 0x00, 0x01, 0x40, 0x01, 0x0c, // VPS-ish
            0x00, 0x00, 0x00, 0x01, 0x42, 0x01, 0x01, // SPS-ish
            0x00, 0x00, 0x00, 0x01, 0x44, 0x01, 0xc0, // PPS-ish
        ],
        aac_sample_rate: 48_000,
        aac_channels: 2,
    }
}

/// Split a TS byte stream into 188-byte packets, asserting sync bytes.
fn packets(data: &[u8]) -> Vec<&[u8]> {
    assert_eq!(data.len() % TS_PACKET_SIZE, 0, "stream not packet-aligned");
    let pkts: Vec<&[u8]> = data.chunks(TS_PACKET_SIZE).collect();
    for p in &pkts {
        assert_eq!(p[0], 0x47, "missing sync byte");
    }
    pkts
}

fn pid_of(p: &[u8]) -> u16 {
    (((p[1] & 0x1F) as u16) << 8) | p[2] as u16
}
fn pusi(p: &[u8]) -> bool {
    p[1] & 0x40 != 0
}
fn afc(p: &[u8]) -> u8 {
    (p[3] >> 4) & 0x03
}

/// Extract the payload region of a packet (after any adaptation field).
fn payload(p: &[u8]) -> &[u8] {
    let mut start = 4;
    if afc(p) & 0b10 != 0 {
        let af_len = p[4] as usize;
        start = 5 + af_len;
    }
    if afc(p) & 0b01 != 0 {
        &p[start..]
    } else {
        &[]
    }
}

/// Reassemble all PES/PSI units on `pid`: each Vec begins at a PUSI packet.
fn units_for(data: &[u8], pid: u16) -> Vec<Vec<u8>> {
    let mut units: Vec<Vec<u8>> = Vec::new();
    for p in packets(data) {
        if pid_of(p) != pid {
            continue;
        }
        if pusi(p) {
            units.push(Vec::new());
        }
        if let Some(last) = units.last_mut() {
            last.extend_from_slice(payload(p));
        }
    }
    units
}

fn decode_ts_field(b: &[u8]) -> i64 {
    ((((b[0] >> 1) & 0x07) as i64) << 30)
        | ((b[1] as i64) << 22)
        | (((b[2] >> 1) as i64) << 15)
        | ((b[3] as i64) << 7)
        | ((b[4] >> 1) as i64)
}

/// (pts, dts, es_payload) from a reassembled PES unit.
fn parse_pes(unit: &[u8]) -> (i64, Option<i64>, Vec<u8>) {
    assert_eq!(&unit[0..3], &[0x00, 0x00, 0x01], "bad PES start code");
    let pts_dts_flags = (unit[7] >> 6) & 0x03;
    let header_data_len = unit[8] as usize;
    let pts = decode_ts_field(&unit[9..14]);
    let dts = if pts_dts_flags == 0b11 {
        Some(decode_ts_field(&unit[14..19]))
    } else {
        None
    };
    let es = unit[9 + header_data_len..].to_vec();
    (pts, dts, es)
}

#[test]
fn crc32_mpeg2_check_value() {
    // Canonical CRC-32/MPEG-2 check over b"123456789".
    assert_eq!(crc32_mpeg2(b"123456789"), 0x0376_E6E7);
}

#[test]
fn segment_starts_with_valid_pat_and_pmt() {
    let mut m = TsMuxer::new(cfg());
    let mut buf = Vec::new();
    m.begin_segment(&mut buf).unwrap();

    let pkts = packets(&buf);
    assert_eq!(pkts.len(), 2, "PAT + PMT");
    assert_eq!(pid_of(pkts[0]), PAT_PID);
    assert_eq!(pid_of(pkts[1]), PMT_PID);

    // pointer_field is 0, section follows; validate CRC by recomputing to 0.
    for (pkt, table_id) in [(pkts[0], 0x00u8), (pkts[1], 0x02u8)] {
        let pl = payload(pkt);
        let ptr = pl[0] as usize;
        let section = &pl[1 + ptr..];
        assert_eq!(section[0], table_id);
        let section_length = (((section[1] & 0x0F) as usize) << 8) | section[2] as usize;
        let total = 3 + section_length;
        assert_eq!(crc32_mpeg2(&section[..total]), 0, "section CRC must verify");
    }
}

#[test]
fn pmt_advertises_hevc_and_aac() {
    let mut m = TsMuxer::new(cfg());
    let mut buf = Vec::new();
    m.begin_segment(&mut buf).unwrap();
    let pmt = payload(packets(&buf)[1]);
    // crude scan: both stream types must appear in the PMT section bytes.
    assert!(pmt.contains(&STREAM_TYPE_HEVC));
    assert!(pmt.contains(&STREAM_TYPE_AAC_ADTS));
}

#[test]
fn video_keyframe_pts_dts_and_param_sets() {
    let mut m = TsMuxer::new(cfg());
    let mut buf = Vec::new();
    m.begin_segment(&mut buf).unwrap();

    let au = vec![0x00, 0x00, 0x00, 0x01, 0x26, 0x01, 0xAB, 0xCD]; // fake IDR slice
    m.write_video(
        &mut buf,
        &Packet {
            data: &au,
            pts_90k: 9000,
            dts_90k: 6000,
            keyframe: true,
        },
    )
    .unwrap();

    // first video packet must carry PCR + random-access in its adaptation field.
    let first_vid = packets(&buf)
        .into_iter()
        .find(|p| pid_of(p) == VIDEO_PID && pusi(p))
        .unwrap();
    assert!(afc(first_vid) & 0b10 != 0, "expected adaptation field");
    let af_flags = first_vid[5];
    assert!(af_flags & 0x40 != 0, "random access indicator");
    assert!(af_flags & 0x10 != 0, "PCR flag");

    let units = units_for(&buf, VIDEO_PID);
    assert_eq!(units.len(), 1);
    let (pts, dts, es) = parse_pes(&units[0]);
    assert_eq!(pts, 9000);
    assert_eq!(dts, Some(6000));
    // parameter sets are prepended, followed by the access unit.
    assert!(es.starts_with(&cfg().hevc_vps_sps_pps));
    assert!(es.ends_with(&au));
}

#[test]
fn non_keyframe_has_no_param_sets_and_pts_only() {
    let mut m = TsMuxer::new(cfg());
    let mut buf = Vec::new();
    let au = vec![0x00, 0x00, 0x00, 0x01, 0x02, 0x01, 0x11];
    m.write_video(
        &mut buf,
        &Packet {
            data: &au,
            pts_90k: 12000,
            dts_90k: 12000, // equal -> PTS only
            keyframe: false,
        },
    )
    .unwrap();
    let (pts, dts, es) = parse_pes(&units_for(&buf, VIDEO_PID)[0]);
    assert_eq!(pts, 12000);
    assert_eq!(dts, None);
    assert_eq!(es, au, "no parameter sets on non-keyframe");
}

#[test]
fn audio_frame_gets_adts_and_pts() {
    let mut m = TsMuxer::new(cfg());
    let mut buf = Vec::new();
    let aac = vec![0x21, 0x33, 0x55, 0x77, 0x99]; // raw AAC payload
    m.write_audio(
        &mut buf,
        &Packet {
            data: &aac,
            pts_90k: 4500,
            dts_90k: 4500,
            keyframe: true,
        },
    )
    .unwrap();

    let (pts, dts, es) = parse_pes(&units_for(&buf, AUDIO_PID)[0]);
    assert_eq!(pts, 4500);
    assert_eq!(dts, None);
    // ADTS sync 0xFFF, then the original payload.
    assert_eq!(es[0], 0xFF);
    assert_eq!(es[1] & 0xF6, 0xF0);
    let frame_len = (((es[3] & 0x03) as usize) << 11) | ((es[4] as usize) << 3) | ((es[5] as usize) >> 5);
    assert_eq!(frame_len, 7 + aac.len());
    assert_eq!(&es[7..], &aac[..]);
}

#[test]
fn large_frame_spans_multiple_packets_with_continuity() {
    let mut m = TsMuxer::new(cfg());
    let mut buf = Vec::new();
    m.begin_segment(&mut buf).unwrap();
    let big = vec![0xAAu8; 5000]; // forces ~28 TS packets
    m.write_video(
        &mut buf,
        &Packet {
            data: &big,
            pts_90k: 90000,
            dts_90k: 90000,
            keyframe: true,
        },
    )
    .unwrap();

    // continuity counters on the video PID must increment by 1 (mod 16).
    let vids: Vec<&[u8]> = packets(&buf).into_iter().filter(|p| pid_of(p) == VIDEO_PID).collect();
    assert!(vids.len() > 1);
    for w in vids.windows(2) {
        let a = w[0][3] & 0x0F;
        let b = w[1][3] & 0x0F;
        assert_eq!(b, (a + 1) & 0x0F);
    }
    // only the first video packet is a unit start.
    assert!(pusi(vids[0]));
    assert!(vids[1..].iter().all(|p| !pusi(p)));

    // reassembled ES still ends with the payload.
    let (_, _, es) = parse_pes(&units_for(&buf, VIDEO_PID)[0]);
    assert!(es.ends_with(&big));
}

#[test]
fn continuity_persists_across_segments() {
    let mut m = TsMuxer::new(cfg());
    let mut buf = Vec::new();
    m.begin_segment(&mut buf).unwrap();
    m.begin_segment(&mut buf).unwrap();
    // two PATs -> PAT continuity counter goes 0 then 1.
    let pats: Vec<&[u8]> = packets(&buf).into_iter().filter(|p| pid_of(p) == PAT_PID).collect();
    assert_eq!(pats.len(), 2);
    assert_eq!(pats[0][3] & 0x0F, 0);
    assert_eq!(pats[1][3] & 0x0F, 1);
}

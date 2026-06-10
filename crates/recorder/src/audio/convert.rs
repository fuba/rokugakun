//! Audio sample-format conversion (spec §10): capture is float32, the MF AAC
//! encoder wants PCM16.

/// Convert interleaved float32 samples to interleaved 16-bit PCM (clamped).
pub fn f32_to_i16(samples: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(samples.len() * 2);
    for &s in samples {
        let clamped = s.clamp(-1.0, 1.0);
        let v = (clamped * i16::MAX as f32) as i16;
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_and_clamps() {
        let pcm = f32_to_i16(&[0.0, 1.0, -1.0, 2.0, -2.0]);
        let vals: Vec<i16> = pcm
            .chunks_exact(2)
            .map(|c| i16::from_le_bytes([c[0], c[1]]))
            .collect();
        assert_eq!(vals[0], 0);
        assert_eq!(vals[1], i16::MAX);
        assert_eq!(vals[2], -i16::MAX);
        assert_eq!(vals[3], i16::MAX); // clamped
        assert_eq!(vals[4], -i16::MAX); // clamped
    }
}

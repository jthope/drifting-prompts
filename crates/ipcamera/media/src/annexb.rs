//! H.264 and H.265 access units in Annex-B form: their NAL units, parameter sets and codec string.
//!
//! Some cameras send parameter sets only now and then, so [`ParameterSets`] keeps the newest.

use g2g_core::VideoCodec;

/// The parameter sets' NAL unit types, in the order a decoder needs them.
pub(crate) fn parameter_set_types(codec: VideoCodec) -> &'static [u8] {
    match codec {
        VideoCodec::H265 => &[32, 33, 34],
        _ => &[7, 8],
    }
}

pub(crate) fn nal_type(codec: VideoCodec, nal: &[u8]) -> Option<u8> {
    let first = *nal.first()?;
    Some(match codec {
        VideoCodec::H265 => (first >> 1) & 0x3f,
        _ => first & 0x1f,
    })
}

pub(crate) fn nal_units(au: &[u8]) -> Vec<&[u8]> {
    let mut starts = Vec::new();
    let mut at = 0;
    while at + 3 <= au.len() {
        if au[at] == 0 && au[at + 1] == 0 && au[at + 2] == 1 {
            starts.push(at + 3);
            at += 3;
        } else {
            at += 1;
        }
    }
    starts
        .iter()
        .enumerate()
        .map(|(index, &start)| {
            let end = starts.get(index + 1).map_or(au.len(), |&next| next - 3);
            // A four-byte start code leaves a zero at the end of the unit before it.
            let mut end = end.max(start);
            while index + 1 < starts.len() && end > start && au[end - 1] == 0 {
                end -= 1;
            }
            &au[start..end]
        })
        .collect()
}

#[derive(Default)]
pub(crate) struct ParameterSets {
    /// In [`parameter_set_types`] order.
    latest: Vec<Option<Vec<u8>>>,
}

impl ParameterSets {
    /// Keeps the parameter sets `au` carries; true when it carries all of them.
    pub(crate) fn refresh(&mut self, codec: VideoCodec, au: &[u8]) -> bool {
        let types = parameter_set_types(codec);
        self.latest.resize(types.len(), None);
        let mut carried = vec![false; types.len()];
        for nal in nal_units(au) {
            if let Some(slot) =
                nal_type(codec, nal).and_then(|kind| types.iter().position(|&t| t == kind))
            {
                self.latest[slot] = Some(nal.to_vec());
                carried[slot] = true;
            }
        }
        carried.iter().all(|&carries| carries)
    }

    pub(crate) fn complete(&self, codec: VideoCodec) -> bool {
        self.latest.len() == parameter_set_types(codec).len()
            && self.latest.iter().all(Option::is_some)
    }

    pub(crate) fn prepend(&self, au: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(au.len() + 256);
        for nal in self.latest.iter().flatten() {
            bytes.extend_from_slice(&[0, 0, 0, 1]);
            bytes.extend_from_slice(nal);
        }
        bytes.extend_from_slice(au);
        bytes
    }

    fn sps(&self, codec: VideoCodec) -> Option<&[u8]> {
        let slot = match codec {
            VideoCodec::H265 => 1,
            _ => 0,
        };
        self.latest.get(slot)?.as_deref()
    }
}

/// The WebCodecs codec string from the stream's SPS: `hev1`, not `hvc1`, since the parameter sets
/// travel in the stream.
pub(crate) fn codec_string(codec: VideoCodec, sets: &ParameterSets) -> Option<String> {
    let sps = sets.sps(codec)?;
    match codec {
        VideoCodec::H265 => hevc_codec_string(sps),
        _ => {
            // These three bytes can't hold an emulation-prevention byte.
            let [_, profile, constraints, level, ..] = *sps else {
                return None;
            };
            Some(format!("avc1.{profile:02X}{constraints:02X}{level:02X}"))
        }
    }
}

fn hevc_codec_string(sps: &[u8]) -> Option<String> {
    // Fixed offsets after the two-byte NAL header (H.265 7.3.2.2 and 7.3.3).
    let rbsp = without_emulation_prevention(sps.get(2..)?);
    let head: &[u8; 13] = rbsp.get(..13)?.try_into().ok()?;
    let space = match head[1] >> 6 {
        0 => "",
        1 => "A",
        2 => "B",
        _ => "C",
    };
    let tier = if head[1] & 0x20 == 0 { "L" } else { "H" };
    let profile = head[1] & 0x1f;
    let compatibility = u32::from_be_bytes([head[2], head[3], head[4], head[5]]).reverse_bits();
    let level = head[12];
    let mut codec = format!("hev1.{space}{profile}.{compatibility:X}.{tier}{level}");
    let constraints = &head[6..12];
    if let Some(last) = constraints.iter().rposition(|&byte| byte != 0) {
        for byte in &constraints[..=last] {
            codec.push_str(&format!(".{byte:02X}"));
        }
    }
    Some(codec)
}

fn without_emulation_prevention(payload: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(payload.len());
    let mut zeros = 0;
    for &byte in payload {
        if zeros >= 2 && byte == 3 {
            zeros = 0;
            continue;
        }
        zeros = if byte == 0 { zeros + 1 } else { 0 };
        bytes.push(byte);
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn annex_b(nals: &[&[u8]]) -> Vec<u8> {
        nals.iter()
            .flat_map(|nal| [&[0u8, 0, 0, 1][..], nal].concat())
            .collect()
    }

    #[test]
    fn nal_units_split_on_both_start_codes() {
        let au = [
            &[0u8, 0, 0, 1, 0x67, 1, 2][..],
            &[0, 0, 1, 0x68, 3],
            &[0, 0, 0, 1, 0x65, 4, 0],
        ]
        .concat();
        let units = nal_units(&au);
        assert_eq!(units, vec![&[0x67, 1, 2][..], &[0x68, 3], &[0x65, 4, 0]]);
    }

    #[test]
    fn an_h264_codec_string_comes_from_the_sps() {
        let sps: &[u8] = &[
            0x67, 0x42, 0xc0, 0x0a, 0xda, 0x0f, 0xd0, 0x80, 0x00, 0x00, 0x03, 0x00, 0x80, 0x00,
            0x00, 0x1e, 0x47, 0x89, 0x16, 0xa8,
        ];
        let pps: &[u8] = &[0x68, 0xce, 0x3c, 0x80];
        let mut sets = ParameterSets::default();
        assert_eq!(codec_string(VideoCodec::H264, &sets), None, "no SPS yet");

        assert!(sets.refresh(VideoCodec::H264, &annex_b(&[sps, pps, &[0x65, 0x88]])));
        assert_eq!(
            codec_string(VideoCodec::H264, &sets).as_deref(),
            Some("avc1.42C00A")
        );
    }

    #[test]
    fn an_h265_codec_string_reads_past_emulation_prevention() {
        let vps: &[u8] = &[0x40, 0x01, 0x0c];
        let sps: &[u8] = &[
            0x42, 0x01, 0x01, 0x01, 0x60, 0x00, 0x00, 0x03, 0x00, 0xb0, 0x00, 0x00, 0x03, 0x00,
            0x00, 0x03, 0x00, 0x5d,
        ];
        let pps: &[u8] = &[0x44, 0x01, 0xc1];
        let mut sets = ParameterSets::default();
        assert!(sets.refresh(VideoCodec::H265, &annex_b(&[vps, sps, pps])));
        assert_eq!(
            codec_string(VideoCodec::H265, &sets).as_deref(),
            Some("hev1.1.6.L93.B0")
        );
    }

    #[test]
    fn a_truncated_sps_gives_no_codec_string() {
        let mut sets = ParameterSets::default();
        sets.refresh(VideoCodec::H265, &annex_b(&[&[0x42, 0x01, 0x01, 0x01]]));
        assert_eq!(codec_string(VideoCodec::H265, &sets), None);
    }
}

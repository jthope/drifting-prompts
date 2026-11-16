//! Reading the compressed video the camera sends: H.264 and H.265 access units in Annex-B form.
//!
//! An Annex-B access unit is a run of NAL units, each after a start code (`00 00 01`, or
//! `00 00 00 01`). A decoder needs the stream's parameter sets before its first keyframe: SPS and
//! PPS for H.264, and VPS, SPS and PPS for H.265. Cameras send them either in front of every
//! keyframe or only now and then, so [`ParameterSets`] keeps the newest of each. A recording or a
//! viewer that starts at a keyframe without them gets them from there.
//!
//! [`codec_string`] reads the SPS for the codec string a browser's WebCodecs decoder is configured
//! with, such as `avc1.42C00A` or `hev1.1.6.L93.B0`.

use g2g_core::VideoCodec;

/// The NAL unit types of the parameter sets a decoder needs, in the order they go: SPS and PPS
/// for H.264, VPS, SPS and PPS for H.265.
pub(crate) fn parameter_set_types(codec: VideoCodec) -> &'static [u8] {
    match codec {
        VideoCodec::H265 => &[32, 33, 34],
        _ => &[7, 8],
    }
}

/// The type of a NAL unit, from its header.
pub(crate) fn nal_type(codec: VideoCodec, nal: &[u8]) -> Option<u8> {
    let first = *nal.first()?;
    Some(match codec {
        VideoCodec::H265 => (first >> 1) & 0x3f,
        _ => first & 0x1f,
    })
}

/// The NAL units of an Annex-B access unit, without their start codes.
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

/// The newest parameter sets the stream has sent, so a file or a viewer can start at a keyframe
/// that came without them.
#[derive(Default)]
pub(crate) struct ParameterSets {
    /// The newest unit of each type, in [`parameter_set_types`] order.
    latest: Vec<Option<Vec<u8>>>,
}

impl ParameterSets {
    /// Keeps the parameter sets `au` carries, and says whether it carries all of them.
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

    /// Whether every parameter set the codec needs has arrived.
    pub(crate) fn complete(&self, codec: VideoCodec) -> bool {
        self.latest.len() == parameter_set_types(codec).len()
            && self.latest.iter().all(Option::is_some)
    }

    /// `au` with the parameter sets ahead of it.
    pub(crate) fn prepend(&self, au: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(au.len() + 256);
        for nal in self.latest.iter().flatten() {
            bytes.extend_from_slice(&[0, 0, 0, 1]);
            bytes.extend_from_slice(nal);
        }
        bytes.extend_from_slice(au);
        bytes
    }

    /// The newest SPS, NAL header included.
    fn sps(&self, codec: VideoCodec) -> Option<&[u8]> {
        let slot = match codec {
            VideoCodec::H265 => 1,
            _ => 0,
        };
        self.latest.get(slot)?.as_deref()
    }
}

/// The codec string a WebCodecs decoder is configured with, read from the stream's SPS, or `None`
/// before an SPS has arrived.
///
/// H.264 gives `avc1.` and the SPS's profile, constraint flags and level, as hex. H.265 gives
/// `hev1.` and its general profile, tier and level (ISO/IEC 14496-15 Annex E): the profile space
/// letter and profile, the compatibility flags in reverse bit order, the tier letter and level,
/// then the constraint bytes up to the last one that isn't zero. `hev1` rather than `hvc1`,
/// because the parameter sets travel in the stream.
pub(crate) fn codec_string(codec: VideoCodec, sets: &ParameterSets) -> Option<String> {
    let sps = sets.sps(codec)?;
    match codec {
        VideoCodec::H265 => hevc_codec_string(sps),
        _ => {
            // After the one-byte header come profile_idc, the constraint flags and level_idc,
            // none of which can hold an emulation-prevention byte.
            let [_, profile, constraints, level, ..] = *sps else {
                return None;
            };
            Some(format!("avc1.{profile:02X}{constraints:02X}{level:02X}"))
        }
    }
}

/// The `hev1.` codec string of an H.265 SPS, NAL header included.
fn hevc_codec_string(sps: &[u8]) -> Option<String> {
    // The general profile, tier and level sit at fixed places after the two-byte header: a byte of
    // ids and flags, then profile space (2 bits), tier (1) and profile (5), 32 compatibility
    // flags, 6 constraint bytes and the level (H.265 7.3.2.2 and 7.3.3).
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

/// A NAL unit's payload with the emulation-prevention bytes taken out: the `03` an encoder puts
/// after two zero bytes so the payload never looks like a start code.
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
        // The SPS and PPS of a 64 x 64 Constrained Baseline stream, level 1.0.
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
        // Main profile, main tier, level 3.1, progressive and frame only. The payload is
        // 01 01 60 00 00 00 B0 00 00 00 00 00 5D, and an encoder writes its runs of zeros with an
        // emulation-prevention byte after every second one.
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

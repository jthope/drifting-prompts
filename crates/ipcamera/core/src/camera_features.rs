//! What a camera can do beyond its stream, in no brand's terms: what it tells apart in its picture,
//! its spotlight, its streams and what it says it is. Each brand's protocol fills these in.

use serde::{Deserialize, Serialize};

/// Something a camera can tell apart in its picture, beyond plain motion.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Detection {
    /// A person.
    Person,
    /// A car, a truck or another vehicle.
    Vehicle,
    /// A dog, a cat or another animal.
    Animal,
    /// A face.
    Face,
    /// A parcel left in view.
    Package,
}

impl Detection {
    /// Every detection, in order.
    pub const ALL: [Detection; 5] = [
        Detection::Person,
        Detection::Vehicle,
        Detection::Animal,
        Detection::Face,
        Detection::Package,
    ];

    /// The name it is passed by.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Person => "person",
            Self::Vehicle => "vehicle",
            Self::Animal => "animal",
            Self::Face => "face",
            Self::Package => "package",
        }
    }

    /// The detection passed as `key`, if there is one.
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|detection| detection.key() == key)
    }

    /// Its display name.
    pub const fn title(self) -> &'static str {
        match self {
            Self::Person => "Person",
            Self::Vehicle => "Vehicle",
            Self::Animal => "Animal",
            Self::Face => "Face",
            Self::Package => "Package",
        }
    }
}

/// What a camera says it is.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceInfo {
    /// The model, as the maker names it: `RLC-811A`.
    pub model: String,
    /// The firmware's version.
    pub firmware: String,
    /// The hardware's version.
    pub hardware: String,
}

/// A camera found on the network without a login.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FoundCamera {
    /// Its address on the network.
    pub host: String,
    /// Its ONVIF device service's port, when ONVIF's discovery found it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub onvif_port: Option<u16>,
    /// Its ONVIF device service's path, when ONVIF's discovery found it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub onvif_device_path: Option<String>,
    /// The port it answers RTSP on, when found there.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rtsp_port: Option<u16>,
    /// The name it gives itself; empty when it gives none.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// Its model, as it says it; empty when it says none.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub model: String,
    /// Whatever else it says of itself, each as `Kind: what it says` (`MAC: aa:bb:cc:00:11:22`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub details: Vec<String>,
}

/// What starts a saved stream path that names a Reolink FLV stream: `flv:channel0_ext.bcs`.
pub const FLV_PREFIX: &str = "flv:";

/// A saved stream path: a Reolink FLV stream, sent on the camera's API port, or else RTSP.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StreamPath<'a> {
    /// An RTSP path, query included: `/Preview_01_main`.
    Rtsp(&'a str),
    /// The name a Reolink sends an FLV stream under: `channel0_ext.bcs`.
    ReolinkFlv(&'a str),
}

impl<'a> StreamPath<'a> {
    /// What the saved `path` is.
    pub fn of(path: &'a str) -> Self {
        match path.trim().strip_prefix(FLV_PREFIX) {
            Some(name) => Self::ReolinkFlv(name.trim()),
            None => Self::Rtsp(path),
        }
    }
}

/// One of the streams a camera says it has, in its own words.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CameraStream {
    /// What the camera calls it.
    pub name: String,
    /// Its RTSP path, query included, or `flv:` and a Reolink FLV stream's name.
    pub path: String,
    /// What the camera says of its picture (`3840×2160, H265`); empty when it says nothing.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub detail: String,
}

/// A camera's lens beside its main one, as its streams' names say.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Lens {
    /// A lens that pans and zooms to follow what moves.
    Tracking,
    /// A lens that sees farther, beside a wide one.
    Telephoto,
}

impl Lens {
    /// Its display name.
    pub const fn title(self) -> &'static str {
        match self {
            Self::Tracking => "Tracking lens",
            Self::Telephoto => "Telephoto lens",
        }
    }
}

impl CameraStream {
    /// The picture's width and height, as `detail` says them.
    pub fn size(&self) -> Option<(u32, u32)> {
        self.detail.split(',').find_map(size_of)
    }

    /// The video's codec, as `detail` says it (`H.264`, `H.265`, or as the camera wrote it).
    pub fn codec(&self) -> Option<String> {
        let codec = self
            .detail
            .split(',')
            .map(str::trim)
            .find(|part| !part.is_empty() && size_of(part).is_none())?;
        let plain: String = codec
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .collect::<String>()
            .to_ascii_uppercase();
        Some(match plain.as_str() {
            "H264" | "AVC" => "H.264".to_string(),
            "H265" | "HEVC" => "H.265".to_string(),
            _ => codec.to_string(),
        })
    }

    /// The lens it is of, as its name or path says; `None` for the main lens.
    pub fn lens(&self) -> Option<Lens> {
        let said = format!("{} {}", self.name, self.path).to_ascii_lowercase();
        if said.contains("autotrack") {
            Some(Lens::Tracking)
        } else if said.contains("telephoto") {
            Some(Lens::Telephoto)
        } else {
            None
        }
    }

    /// Adds each of `more` whose path (any case) isn't there; one there gains a missing detail.
    pub fn merge(streams: &mut Vec<Self>, more: Vec<Self>) {
        for stream in more {
            match streams
                .iter_mut()
                .find(|known| known.path.eq_ignore_ascii_case(&stream.path))
            {
                Some(known) if known.detail.is_empty() => known.detail = stream.detail,
                Some(_) => {}
                None => streams.push(stream),
            }
        }
    }
}

fn size_of(part: &str) -> Option<(u32, u32)> {
    let (width, height) = part.trim().split_once(['\u{d7}', 'x'])?;
    let size = (width.trim().parse().ok()?, height.trim().parse().ok()?);
    (size.0 > 0 && size.1 > 0).then_some(size)
}

#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub(crate) fn rtsp_path(uri: &str) -> Option<String> {
    let rest = uri
        .trim()
        .split_once("://")
        .map_or(uri.trim(), |(_, rest)| rest);
    let path = &rest[rest.find('/')?..];
    (path.len() > 1).then(|| path.to_string())
}

/// What a camera can do beyond its stream, as it says itself.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CameraFeatures {
    /// Whether it reports motion.
    pub motion: bool,
    /// What it tells apart in its picture, in [`Detection::ALL`]'s order.
    pub detections: Vec<Detection>,
    /// Whether it has a spotlight the app can switch.
    pub spotlight: bool,
    /// Whether it pans and tilts.
    pub pan_tilt: bool,
    /// Whether it zooms.
    pub zoom: bool,
}

impl CameraFeatures {
    /// What it has, in words: `Motion`, `Person`, `Spotlight`, `Pan and tilt`, `Zoom`.
    pub fn titles(&self) -> Vec<&'static str> {
        let mut titles = Vec::new();
        if self.motion {
            titles.push("Motion");
        }
        titles.extend(self.detections.iter().map(|detection| detection.title()));
        for (has, title) in [
            (self.spotlight, "Spotlight"),
            (self.pan_tilt, "Pan and tilt"),
            (self.zoom, "Zoom"),
        ] {
            if has {
                titles.push(title);
            }
        }
        titles
    }
}

/// What a camera sees right now.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CameraActivity {
    /// Whether something moves in its picture.
    pub motion: bool,
    /// What it sees now, in [`Detection::ALL`]'s order.
    pub detected: Vec<Detection>,
    /// Its spotlight now, for a camera that has one.
    pub spotlight: Option<Spotlight>,
}

/// A camera's spotlight.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Spotlight {
    /// Whether it is on.
    pub on: bool,
    /// How bright it is when on, 0 to 100.
    pub brightness: u8,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_detection_is_passed_by_its_key() {
        for detection in Detection::ALL {
            assert_eq!(Detection::from_key(detection.key()), Some(detection));
            assert_eq!(
                serde_json::to_string(&detection).unwrap(),
                format!("\"{}\"", detection.key()),
                "the JSON says the key too"
            );
        }
        assert_eq!(Detection::from_key("unicorn"), None);
    }

    #[test]
    fn the_features_read_as_words_in_a_steady_order() {
        let features = CameraFeatures {
            motion: true,
            detections: vec![Detection::Person, Detection::Animal],
            spotlight: true,
            pan_tilt: false,
            zoom: true,
        };
        assert_eq!(
            features.titles(),
            ["Motion", "Person", "Animal", "Spotlight", "Zoom"]
        );
        assert!(CameraFeatures::default().titles().is_empty());
    }

    #[test]
    fn a_streams_detail_says_its_size_and_codec_and_its_name_its_lens() {
        let stream = |name: &str, path: &str, detail: &str| CameraStream {
            name: name.into(),
            path: path.into(),
            detail: detail.into(),
        };
        let main = stream(
            "Profile000_MainStream",
            "/Preview_01_main",
            "3840\u{d7}2160, H265",
        );
        assert_eq!(main.size(), Some((3840, 2160)));
        assert_eq!(main.codec().as_deref(), Some("H.265"));
        assert_eq!(main.lens(), None);

        let tracking = stream(
            "autotrack_sub",
            "flv:channel0_autotrack_sub.bcs",
            "896x512, H264",
        );
        assert_eq!(tracking.size(), Some((896, 512)));
        assert_eq!(tracking.codec().as_deref(), Some("H.264"));
        assert_eq!(tracking.lens(), Some(Lens::Tracking));
        assert_eq!(Lens::Tracking.title(), "Tracking lens");

        let said_little = stream("Profile_2", "/telephoto", "JPEG");
        assert_eq!(said_little.size(), None);
        assert_eq!(said_little.codec().as_deref(), Some("JPEG"));
        assert_eq!(said_little.lens(), Some(Lens::Telephoto));
        assert_eq!(stream("", "/a", "").codec(), None);
    }
}

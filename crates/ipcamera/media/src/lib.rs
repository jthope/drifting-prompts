//! A camera's stream on glass2glass, over RTSP or a Reolink's FLV: live view, recording, relaying.
//!
//! Only the newest decoded frame is kept ([`FrameSlot`]): a slow frame is dropped, never queued.

mod annexb;
mod avc;
mod error;
mod flv;
mod frame;
mod gate;
mod hevc;
mod pipeline;
#[cfg(all(windows, any(test, feature = "pretend-camera")))]
pub mod pretend;
mod record;
mod relay;
mod session;
mod sink;
mod sizes;
mod still;
mod tap;

pub use error::MediaError;
pub use frame::{DecodedFrame, FrameSlot};
pub use record::{PREROLL_MOST_BYTES, RecordRules, RecordStatus};
pub use relay::{Feed, Picture, Relayed, VideoConfig, Viewer};
pub use session::{MediaSession, MediaStatus};
pub use sizes::sized_streams;

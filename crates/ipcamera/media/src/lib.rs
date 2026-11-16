//! RTSP live view: a glass2glass pipeline and the newest decoded frame.
//!
//! Nothing here touches the UI. The pipeline runs on its own thread, publishes the newest
//! decoded frame into a [`FrameSlot`], and the view paints whatever is in the slot when it
//! repaints. That keeps decode rate and repaint rate independent: a slow frame is dropped rather
//! than queued. Sound goes straight from the pipeline to the playback device.
//!
//! The frames leave this crate as [`DecodedFrame`] — bytes and a size, nothing GPU-shaped — so
//! the pipeline has no opinion about how or whether anything is drawn.
//!
//! A live view can stay connected while nothing shows it ([`MediaSession::set_shown`]): its stream
//! keeps arriving and recording, and nothing is decoded until it is shown again.
//!
//! A session can also record what it plays ([`MediaSession::start_recording`]): the compressed
//! stream, as the camera sent it, into an MP4.
//!
//! A session can instead relay a camera ([`MediaSession::relay`]), for the server: it decodes
//! nothing, and passes the compressed video on to any number of viewers through its [`Feed`]. It
//! records as a live view does, and decodes one picture only when asked ([`Feed::picture`]), for a
//! snapshot or a recording's thumbnail.
//!
//! A session plays RTSP, or one of a Reolink's FLV streams (`flv`), which its web server sends some
//! streams only as.
//!
//! Detect asks here how large a camera's RTSP streams are ([`sized_streams`]), from what each says
//! to DESCRIBE, for those nothing else sized.

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

//! The video branch's own two elements: one that notes the stream's picture size, and the last
//! one, which copies each BGRA picture into the [`FrameSlot`](crate::FrameSlot), unless nothing
//! shows it ([`crate::gate`]).
//!
//! [`SlotSink`] is where pictures leave the pipeline, with no pull loop or global channel: the
//! pipeline hands it a picture, it packs the rows and replaces the slot's frame, and the UI paints
//! whatever is in the slot when it repaints.
//!
//! [`SizeTap`] sits between the parser and the decoder. A decoder may hand back the coded picture,
//! which is rounded up to whole 16-pixel macroblocks: Media Foundation's, which decoded H.264 here
//! once, turned a 1080-line camera into 1088 lines, the last eight of them junk. The parser reads
//! the real size from the stream's SPS and announces it, so the tap keeps it and the sink crops
//! each picture to it. The Rust decoders crop their pictures themselves, so for them it changes
//! nothing.

use std::{
    future::{Ready, ready},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use g2g_core::{
    AsyncElement, Caps, ConfigureOutcome, Dim, ElementMetadata, Frame, G2gError, OutputSink,
    PipelinePacket, RawVideoFormat,
};
use ipcamera_core::sync::LockExt;

use crate::{
    DecodedFrame,
    gate::Visibility,
    tap::{Tap, Watch},
};

/// The picture size the stream declares, shared by the [`SizeTap`] that hears it and the
/// [`SlotSink`] that crops to it.
#[derive(Clone, Default)]
pub(crate) struct DisplaySize(Arc<Mutex<Option<(u32, u32)>>>);

impl DisplaySize {
    fn set(&self, size: (u32, u32)) {
        *self.0.locked() = Some(size);
    }

    fn get(&self) -> Option<(u32, u32)> {
        *self.0.locked()
    }
}

/// The fixed width and height `caps` give, raw or compressed.
pub(crate) fn fixed_size(caps: &Caps) -> Option<(u32, u32)> {
    match caps {
        Caps::RawVideo {
            width: Dim::Fixed(width),
            height: Dim::Fixed(height),
            ..
        }
        | Caps::CompressedVideo {
            width: Dim::Fixed(width),
            height: Dim::Fixed(height),
            ..
        } => Some((*width, *height)),
        _ => None,
    }
}

/// Notes each picture size the parser announces, as soon as it is offered, passing everything on.
pub(crate) struct SizeTap {
    size: DisplaySize,
}

impl SizeTap {
    /// The tap, as the element the graph takes.
    pub(crate) fn new(size: DisplaySize) -> Tap<Self> {
        Tap(Self { size })
    }

    fn note(&self, caps: &Caps) {
        if let Some(size) = fixed_size(caps) {
            self.size.set(size);
        }
    }
}

impl Watch for SizeTap {
    fn metadata(&self) -> ElementMetadata {
        ElementMetadata::new(
            "IP Camera picture size",
            "Filter/Video",
            "Notes the picture size the stream declares, so the sink can crop the decoded picture.",
            "Drifting Prompts",
        )
    }

    fn offered(&self, caps: &Caps) {
        self.note(caps);
    }

    fn caps(&self, caps: &Caps) {
        self.note(caps);
    }
}

/// Where decoded pictures leave the pipeline.
pub(crate) struct SlotSink {
    /// The frame slot, and whether anything shows it ([`crate::gate`]).
    visibility: Arc<Visibility>,
    /// Set by the first picture, so the session can report that the camera is live.
    live: Arc<AtomicBool>,
    /// The size the stream declares, which the picture is cropped to.
    display: DisplaySize,
    /// The decoded picture's size, from the caps. A decoder announces a new one before the first
    /// frame of a new size, so this is always the size of the next frame.
    decoded: Option<(u32, u32)>,
}

impl SlotSink {
    pub(crate) fn new(
        visibility: Arc<Visibility>,
        live: Arc<AtomicBool>,
        display: DisplaySize,
    ) -> Self {
        Self {
            visibility,
            live,
            display,
            decoded: None,
        }
    }

    fn take_size(&mut self, caps: &Caps) {
        if let Some(size) = fixed_size(caps) {
            self.decoded = Some(size);
        }
    }

    fn show(&mut self, frame: &Frame) -> Result<(), G2gError> {
        let Some((width, height)) = self.decoded else {
            return Ok(());
        };
        // A picture the decoder is catching up through is not worth copying.
        if self.visibility.skips(frame.timing.pts_ns) {
            return Ok(());
        }
        // The declared size, where it fits inside the decoded one; the decoded size otherwise.
        let (shown_width, shown_height) = self
            .display
            .get()
            .filter(|&(w, h)| w <= width && h <= height)
            .unwrap_or((width, height));
        let bytes = frame.domain.require_system_bytes("ipcamera slot sink")?;
        if let Some(picture) =
            DecodedFrame::from_bgra_rows(&bytes, height, shown_width, shown_height)
        {
            self.visibility.store(Arc::new(picture));
            self.live.store(true, Ordering::Relaxed);
        }
        Ok(())
    }
}

impl AsyncElement for SlotSink {
    type ProcessFuture<'a>
        = Ready<Result<(), G2gError>>
    where
        Self: 'a;

    /// Only BGRA, so the converter in front of this sink settles on the order the UI uploads.
    fn intercept_caps(&self, upstream: &Caps) -> Result<Caps, G2gError> {
        match upstream {
            Caps::RawVideo {
                format: RawVideoFormat::Bgra8,
                ..
            } => Ok(upstream.clone()),
            _ => Err(G2gError::CapsMismatch),
        }
    }

    fn configure_pipeline(&mut self, caps: &Caps) -> Result<ConfigureOutcome, G2gError> {
        self.take_size(caps);
        Ok(ConfigureOutcome::Accepted)
    }

    fn process<'a>(
        &'a mut self,
        packet: PipelinePacket,
        _out: &'a mut dyn OutputSink,
    ) -> Self::ProcessFuture<'a> {
        ready(match packet {
            PipelinePacket::CapsChanged(caps) => {
                self.take_size(&caps);
                Ok(())
            }
            PipelinePacket::DataFrame(frame) => self.show(&frame),
            _ => Ok(()),
        })
    }

    fn metadata(&self) -> ElementMetadata {
        ElementMetadata::new(
            "IP Camera frame slot",
            "Sink/Video",
            "Keeps the newest BGRA picture for the UI to paint.",
            "Drifting Prompts",
        )
    }
}

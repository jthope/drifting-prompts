//! The video branch's own elements: [`SizeTap`] notes the size the stream declares, and
//! [`SlotSink`] stores each BGRA picture cropped to it, since a decoder may hand back the coded
//! size (1088 lines for 1080).

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

pub(crate) struct SizeTap {
    size: DisplaySize,
}

impl SizeTap {
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

pub(crate) struct SlotSink {
    visibility: Arc<Visibility>,
    live: Arc<AtomicBool>,
    display: DisplaySize,
    /// From the caps, which a decoder announces before the first frame of a new size.
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
        if self.visibility.skips(frame.timing.pts_ns) {
            return Ok(());
        }
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

    /// Only BGRA, so the converter in front settles on the order the UI uploads.
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

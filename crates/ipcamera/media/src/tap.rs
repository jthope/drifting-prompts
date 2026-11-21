//! An element that passes everything on untouched, showing it to a [`Watch`] first.

use std::{future::Future, pin::Pin};

use g2g_core::{
    AsyncElement, Caps, ConfigureOutcome, ElementBound, ElementMetadata, Frame, G2gError,
    OutputSink, PipelinePacket,
};

pub(crate) trait Watch {
    fn metadata(&self) -> ElementMetadata;

    /// Caps offered from upstream, before the pipeline settles on them.
    fn offered(&self, _caps: &Caps) {}

    /// The caps the pipeline settled on, at the start and whenever they change.
    fn caps(&self, caps: &Caps);

    fn frame(&self, _frame: &Frame) {}
}

pub(crate) struct Tap<W>(pub(crate) W);

impl<W: Watch> AsyncElement for Tap<W>
where
    Self: ElementBound,
{
    type ProcessFuture<'a>
        = Pin<Box<dyn Future<Output = Result<(), G2gError>> + 'a>>
    where
        Self: 'a;

    fn intercept_caps(&self, upstream: &Caps) -> Result<Caps, G2gError> {
        self.0.offered(upstream);
        Ok(upstream.clone())
    }

    fn configure_pipeline(&mut self, caps: &Caps) -> Result<ConfigureOutcome, G2gError> {
        self.0.caps(caps);
        Ok(ConfigureOutcome::Accepted)
    }

    fn process<'a>(
        &'a mut self,
        packet: PipelinePacket,
        out: &'a mut dyn OutputSink,
    ) -> Self::ProcessFuture<'a> {
        Box::pin(async move {
            match &packet {
                // The runner sends the one end of stream itself, so a transform must not.
                PipelinePacket::Eos => return Ok(()),
                PipelinePacket::CapsChanged(caps) => self.0.caps(caps),
                PipelinePacket::DataFrame(frame) => self.0.frame(frame),
                _ => {}
            }
            out.push(packet).await.map(|_| ())
        })
    }

    fn metadata(&self) -> ElementMetadata {
        self.0.metadata()
    }
}

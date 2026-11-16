//! The recording's file: the fragmented MP4 the muxer hands over, written as it comes, and its
//! length filled in once it ends.
//!
//! A fragmented MP4's header is written before anything is recorded, so it cannot say how long the
//! recording runs, and a player then shows no length and cannot scale its scrubber. [`FileOut`]
//! adds the box made for this, `mehd` (movie extends header), to the header as it goes out, and
//! fills it in, with the header's own duration, when the file closes. A file cut short keeps a
//! length of zero, and plays as before.

use std::{
    fs::File,
    io::{Seek, SeekFrom, Write},
    path::Path,
    task::{Context, Poll},
};

use g2g_core::{G2gError, HardwareError, OutputSink, PipelinePacket, PushOutcome};

/// Where the muxer's output goes: straight to the file, a fragment at a time.
pub(super) struct FileOut {
    /// The file.
    pub(super) file: File,
    /// How much has been written to it.
    pub(super) bytes: u64,
    /// Why the last write failed, which the muxer's own error doesn't say.
    pub(super) error: Option<String>,
    /// Where the header's length fields are, once it has been written.
    length: Option<LengthFields>,
}

impl FileOut {
    /// Makes the file at `path`, empty, or empties it.
    pub(super) fn create(path: &Path) -> Result<Self, String> {
        let file = File::create(path).map_err(|error| format!("{}: {error}", path.display()))?;
        Ok(Self {
            file,
            bytes: 0,
            error: None,
            length: None,
        })
    }

    /// Fills in how long the recording runs, `length_ns`, now that it has ended.
    pub(super) fn write_length(&mut self, length_ns: u64) -> std::io::Result<()> {
        let Some(fields) = self.length else {
            return Ok(());
        };
        let units =
            u64::try_from(u128::from(length_ns) * u128::from(fields.timescale) / 1_000_000_000)
                .unwrap_or(u64::MAX);
        self.file.seek(SeekFrom::Start(fields.mehd))?;
        self.file.write_all(&units.to_be_bytes())?;
        self.file.seek(SeekFrom::Start(fields.mvhd))?;
        if fields.mvhd_wide {
            self.file.write_all(&units.to_be_bytes())?;
        } else {
            let narrow = u32::try_from(units).unwrap_or(u32::MAX);
            self.file.write_all(&narrow.to_be_bytes())?;
        }
        self.file.seek(SeekFrom::End(0))?;
        Ok(())
    }
}

/// Where the length of a fragmented MP4 is written: offsets in the file, and the units.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LengthFields {
    /// The `mehd`'s fragment duration, eight bytes.
    mehd: u64,
    /// The `mvhd`'s duration, eight bytes when `mvhd_wide`, else four.
    mvhd: u64,
    mvhd_wide: bool,
    /// The movie's timescale, which both are counted in.
    timescale: u32,
}

/// The boxes from `from` to `to` in `bytes`: where each starts, its size, and its type.
fn boxes(bytes: &[u8], from: usize, to: usize) -> Vec<(usize, usize, [u8; 4])> {
    let mut found = Vec::new();
    let mut at = from;
    while at + 8 <= to {
        let size = u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]);
        let size = size as usize;
        let kind = [bytes[at + 4], bytes[at + 5], bytes[at + 6], bytes[at + 7]];
        if size < 8 || at + size > to {
            break;
        }
        found.push((at, size, kind));
        at += size;
    }
    found
}

/// `packet` with an empty `mehd` at the start of its header's `mvex`, and where, counted from the
/// packet's start, the length goes. `None` when the packet holds no fragmented header.
fn with_length_box(packet: &[u8]) -> Option<(Vec<u8>, LengthFields)> {
    const MEHD_SIZE: usize = 20;
    let (moov, moov_size, _) = boxes(packet, 0, packet.len())
        .into_iter()
        .find(|(_, _, kind)| kind == b"moov")?;
    let children = boxes(packet, moov + 8, moov + moov_size);
    let (mvhd, _, _) = *children.iter().find(|(_, _, kind)| kind == b"mvhd")?;
    let (mvex, mvex_size, _) = *children.iter().find(|(_, _, kind)| kind == b"mvex")?;

    let wide = *packet.get(mvhd + 8)? == 1;
    let (timescale_at, duration_at) = if wide {
        (mvhd + 28, mvhd + 32)
    } else {
        (mvhd + 20, mvhd + 24)
    };
    let timescale = u32::from_be_bytes(
        packet
            .get(timescale_at..timescale_at + 4)?
            .try_into()
            .ok()?,
    );

    let grow = |at: usize, size: usize, bytes: &mut Vec<u8>| {
        let grown = u32::try_from(size + MEHD_SIZE).unwrap_or(u32::MAX);
        bytes[at..at + 4].copy_from_slice(&grown.to_be_bytes());
    };
    let inside = mvex + 8;
    let mut patched = Vec::with_capacity(packet.len() + MEHD_SIZE);
    patched.extend_from_slice(&packet[..inside]);
    // Version 1, so the length has eight bytes, which no recording outgrows.
    patched.extend_from_slice(&[0, 0, 0, MEHD_SIZE as u8]);
    patched.extend_from_slice(b"mehd");
    patched.extend_from_slice(&[1, 0, 0, 0]);
    patched.extend_from_slice(&[0; 8]);
    patched.extend_from_slice(&packet[inside..]);
    grow(moov, moov_size, &mut patched);
    grow(mvex, mvex_size, &mut patched);

    Some((
        patched,
        LengthFields {
            mehd: (inside + 12) as u64,
            mvhd: duration_at as u64,
            mvhd_wide: wide,
            timescale,
        },
    ))
}

impl OutputSink for FileOut {
    fn poll_push(
        &mut self,
        _cx: &mut Context<'_>,
        packet: &mut Option<PipelinePacket>,
    ) -> Poll<Result<PushOutcome, G2gError>> {
        if let Some(PipelinePacket::DataFrame(frame)) = packet.take()
            && let Some(bytes) = frame.domain.as_system_slice()
        {
            let header = self
                .length
                .is_none()
                .then(|| with_length_box(bytes))
                .flatten();
            let bytes = match &header {
                Some((patched, fields)) => {
                    self.length = Some(LengthFields {
                        mehd: self.bytes + fields.mehd,
                        mvhd: self.bytes + fields.mvhd,
                        ..*fields
                    });
                    patched.as_slice()
                }
                None => bytes,
            };
            if let Err(error) = self.file.write_all(bytes) {
                self.error = Some(error.to_string());
                let code = error.raw_os_error().unwrap_or(0);
                return Poll::Ready(Err(G2gError::Hardware(HardwareError::Io(code))));
            }
            self.bytes += bytes.len() as u64;
        }
        Poll::Ready(Ok(PushOutcome::Accepted))
    }
}

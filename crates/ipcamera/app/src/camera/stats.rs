//! Live numbers for a stream: picture size, decode rate and when its last picture came.

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use ipcamera_media::FrameSlot;

const RATE_WINDOW: Duration = Duration::from_secs(1);

/// The numbers for one running session.
#[derive(Default)]
pub struct StreamStats {
    produced: u64,
    painted: u64,
    resolution: Option<(u32, u32)>,
    fps: f32,
    sampled_at: Option<Instant>,
    produced_then: u64,
    pictured_at: Option<Instant>,
}

impl StreamStats {
    /// Catches up with the frame slot and the count of frames drawn.
    pub fn sample(&mut self, frames: &FrameSlot, painted: u64, now: Instant) {
        let produced = frames.produced();
        if produced != self.produced {
            self.pictured_at = Some(now);
            if let Some(frame) = frames.latest()
                && frame.width > 0
                && frame.height > 0
            {
                self.resolution = Some((frame.width, frame.height));
            }
        }
        self.produced = produced;
        self.painted = painted;

        match self.sampled_at {
            Some(sampled_at) if now.duration_since(sampled_at) >= RATE_WINDOW => {
                let seconds = now.duration_since(sampled_at).as_secs_f32();
                self.fps = produced.saturating_sub(self.produced_then) as f32 / seconds;
                self.sampled_at = Some(now);
                self.produced_then = produced;
            }
            None => {
                self.sampled_at = Some(now);
                self.produced_then = produced;
            }
            Some(_) => {}
        }
    }

    #[cfg(test)]
    fn dropped(&self) -> u64 {
        self.produced.saturating_sub(self.painted)
    }

    /// The size and the frame rate alone, for a narrow tile.
    pub fn brief(&self) -> String {
        let Some((width, height)) = self.resolution else {
            return "waiting for video\u{2026}".to_string();
        };
        format!("{width}\u{00d7}{height} \u{00b7} {:.0} fps", self.fps)
    }

    /// Whether no picture has come yet, so its size isn't known.
    pub fn waiting(&self) -> bool {
        self.resolution.is_none()
    }

    /// When the last picture came, in milliseconds since 1970 on `clock`; 0 before the first.
    pub fn last_picture_at(&self, now: Instant, clock: SystemTime) -> f64 {
        self.pictured_at
            .and_then(|at| clock.checked_sub(now.saturating_duration_since(at)))
            .and_then(|at| at.duration_since(UNIX_EPOCH).ok())
            .map_or(0.0, |since| since.as_millis() as f64)
    }

    /// The numbers as a wide tile shows them.
    pub fn line(&self, use_tcp: bool) -> String {
        let Some((width, height)) = self.resolution else {
            return "waiting for video\u{2026}".to_string();
        };

        let transport = if use_tcp { "TCP" } else { "UDP" };
        format!(
            "{width}\u{00d7}{height} \u{00b7} {:.1} fps \u{00b7} {transport}",
            self.fps
        )
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use ipcamera_media::DecodedFrame;

    use super::*;

    fn frame(width: u32, height: u32) -> Arc<DecodedFrame> {
        Arc::new(DecodedFrame {
            width,
            height,
            bgra: vec![0; (width * height * 4) as usize],
        })
    }

    #[test]
    fn it_counts_the_frames_that_were_never_drawn() {
        let slot = FrameSlot::default();
        let mut stats = StreamStats::default();

        slot.store(frame(4, 4));
        slot.store(frame(4, 4));
        slot.store(frame(4, 4));
        stats.sample(&slot, 1, Instant::now());

        assert_eq!(stats.dropped(), 2);
        assert_eq!(stats.resolution, Some((4, 4)));
    }

    #[test]
    fn drawing_more_than_was_counted_never_goes_negative() {
        // The two counters are read a moment apart, so the drawn one can be ahead.
        let slot = FrameSlot::default();
        let mut stats = StreamStats::default();

        slot.store(frame(4, 4));
        stats.sample(&slot, 2, Instant::now());

        assert_eq!(stats.dropped(), 0);
    }

    #[test]
    fn the_rate_is_the_frames_in_the_last_second() {
        let slot = FrameSlot::default();
        let mut stats = StreamStats::default();
        let start = Instant::now();

        stats.sample(&slot, 0, start);
        for _ in 0..30 {
            slot.store(frame(4, 4));
        }
        stats.sample(&slot, 30, start + Duration::from_secs(1));

        assert_eq!(stats.fps, 30.0);
    }

    #[test]
    fn the_rate_holds_still_between_resamples() {
        let slot = FrameSlot::default();
        let mut stats = StreamStats::default();
        let start = Instant::now();

        stats.sample(&slot, 0, start);
        slot.store(frame(4, 4));
        stats.sample(&slot, 1, start + Duration::from_millis(250));

        assert_eq!(stats.fps, 0.0, "a quarter second is too soon to resample");
    }

    #[test]
    fn it_remembers_when_the_last_picture_came() {
        let slot = FrameSlot::default();
        let mut stats = StreamStats::default();
        let start = Instant::now();
        let clock = UNIX_EPOCH + Duration::from_secs(1_000);

        stats.sample(&slot, 0, start);
        assert_eq!(stats.last_picture_at(start, clock), 0.0, "none yet");

        slot.store(frame(4, 4));
        stats.sample(&slot, 1, start + Duration::from_secs(1));
        stats.sample(&slot, 1, start + Duration::from_secs(6));

        assert_eq!(
            stats.last_picture_at(start + Duration::from_secs(6), clock),
            995_000.0,
            "five seconds before the clock's now"
        );
    }

    #[test]
    fn the_line_waits_for_a_picture_then_shows_the_numbers() {
        let slot = FrameSlot::default();
        let mut stats = StreamStats::default();

        assert_eq!(stats.line(true), "waiting for video\u{2026}");
        assert_eq!(stats.brief(), "waiting for video\u{2026}");

        slot.store(frame(1920, 1080));
        slot.store(frame(1920, 1080));
        stats.sample(&slot, 1, Instant::now());

        assert_eq!(
            stats.line(false),
            "1920\u{00d7}1080 \u{00b7} 0.0 fps \u{00b7} UDP"
        );
        assert_eq!(stats.brief(), "1920\u{00d7}1080 \u{00b7} 0 fps");
    }
}

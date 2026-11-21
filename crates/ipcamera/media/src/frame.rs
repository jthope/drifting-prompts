//! Decoded pictures as plain bytes, and the slot that holds the newest one.

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};

use ipcamera_core::sync::LockExt;

/// One decoded picture: B, G, R, A per pixel, tightly packed with no row padding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecodedFrame {
    /// The picture's width, in pixels.
    pub width: u32,
    /// The picture's height, in pixels.
    pub height: u32,
    /// B, G, R, A for every pixel: `width * height * 4` bytes.
    pub bgra: Vec<u8>,
}

impl DecodedFrame {
    /// The top-left `width` × `height` of a BGRA picture `source_rows` rows tall, any row stride.
    pub fn from_bgra_rows(data: &[u8], source_rows: u32, width: u32, height: u32) -> Option<Self> {
        let (columns, rows, source_rows) = (width as usize, height as usize, source_rows as usize);
        if columns == 0 || rows == 0 || rows > source_rows {
            return None;
        }
        let row_bytes = columns * 4;
        let stride = data.len() / source_rows;
        if stride < row_bytes {
            return None;
        }

        let mut bgra = Vec::with_capacity(row_bytes * rows);
        for row in data.chunks_exact(stride).take(rows) {
            bgra.extend_from_slice(&row[..row_bytes]);
        }
        Some(Self {
            width,
            height,
            bgra,
        })
    }

    /// The same pixels as opaque R, G, B, A, since PNG has no BGRA.
    pub fn to_rgba(&self) -> Vec<u8> {
        let mut rgba = Vec::with_capacity(self.bgra.len());
        for pixel in self.bgra.as_chunks::<4>().0 {
            rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], 255]);
        }
        rgba
    }
}

/// The newest decoded frame: one slot, so a repaint that falls behind drops frames.
#[derive(Clone, Default)]
pub struct FrameSlot {
    current: Arc<Mutex<Option<Arc<DecodedFrame>>>>,
    produced: Arc<AtomicU64>,
}

impl FrameSlot {
    /// Replaces the frame, and counts it as offered.
    pub fn store(&self, frame: Arc<DecodedFrame>) {
        *self.current.locked() = Some(frame);
        self.produced.fetch_add(1, Ordering::Relaxed);
    }

    /// The frame to draw, if one has arrived since the last [`FrameSlot::clear`].
    pub fn latest(&self) -> Option<Arc<DecodedFrame>> {
        self.current.locked().clone()
    }

    /// How many frames have been offered since this slot was created.
    pub fn produced(&self) -> u64 {
        self.produced.load(Ordering::Relaxed)
    }

    /// Empties the slot, so a stopped stream's last picture doesn't linger.
    pub fn clear(&self) {
        *self.current.locked() = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_decoded_frame_is_exactly_width_by_height_pixels() {
        let frame = DecodedFrame {
            width: 3,
            height: 2,
            bgra: vec![0u8; 3 * 2 * 4],
        };

        assert_eq!(frame.bgra.len(), (frame.width * frame.height * 4) as usize);
    }

    #[test]
    fn rgba_swaps_blue_and_red_and_is_opaque() {
        let frame = DecodedFrame {
            width: 2,
            height: 1,
            bgra: vec![1, 2, 3, 0, 10, 20, 30, 128],
        };

        assert_eq!(frame.to_rgba(), [3, 2, 1, 255, 30, 20, 10, 255]);
    }

    #[test]
    fn padded_rows_are_packed_tightly() {
        let mut data = Vec::new();
        for row in 0..2u8 {
            data.extend_from_slice(&[row; 8]);
            data.extend_from_slice(&[9; 4]);
        }

        let frame = DecodedFrame::from_bgra_rows(&data, 2, 2, 2).expect("the picture fits");

        assert_eq!(frame.bgra, [[0u8; 8], [1u8; 8]].concat());
    }

    #[test]
    fn a_coded_picture_is_cropped_to_the_declared_size() {
        let data: Vec<u8> = (0..3u8)
            .flat_map(|row| (0..3u8).flat_map(move |column| [row * 10 + column; 4]))
            .collect();

        let frame = DecodedFrame::from_bgra_rows(&data, 3, 2, 2).expect("the crop fits");

        assert_eq!((frame.width, frame.height), (2, 2));
        assert_eq!(frame.bgra, [[0u8; 4], [1; 4], [10; 4], [11; 4]].concat());
    }

    #[test]
    fn a_picture_with_too_few_bytes_is_refused() {
        assert!(DecodedFrame::from_bgra_rows(&[0u8; 4 * 4 * 4 - 1], 4, 4, 4).is_none());
        assert!(DecodedFrame::from_bgra_rows(&[0u8; 16], 4, 0, 4).is_none());
        assert!(
            DecodedFrame::from_bgra_rows(&[0u8; 64], 4, 4, 5).is_none(),
            "more rows than the source has"
        );
    }

    #[test]
    fn the_slot_hands_back_the_newest_frame_and_counts_them() {
        let slot = FrameSlot::default();
        assert!(slot.latest().is_none(), "an empty slot has nothing to draw");

        let first = Arc::new(DecodedFrame {
            width: 4,
            height: 4,
            bgra: vec![1u8; 4 * 4 * 4],
        });
        let second = Arc::new(DecodedFrame {
            width: 8,
            height: 8,
            bgra: vec![2u8; 8 * 8 * 4],
        });
        slot.store(first);
        slot.store(second);

        let newest = slot.latest().expect("a frame arrived");
        assert_eq!((newest.width, newest.height), (8, 8));
        assert_eq!(slot.produced(), 2, "both frames were offered");

        slot.clear();
        assert!(slot.latest().is_none(), "clearing drops the frame");
        assert_eq!(
            slot.produced(),
            2,
            "clearing is not a new frame, so the counter stays"
        );
    }
}

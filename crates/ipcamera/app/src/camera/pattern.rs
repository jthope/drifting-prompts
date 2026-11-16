//! A moving 1080p test picture, for running without a camera, or a still picture in its place.
//!
//! It stands in for the pipeline as closely as it can: a thread allocates a full BGRA frame at a
//! camera's rate and stores it in a [`FrameSlot`], so everything downstream does the same work it
//! does for a real stream. The only part it skips is network and decode. Set `IPCAMERA_SOURCE=pattern`
//! to develop or test without a camera, and without touching one.
//!
//! Like a stream, it does nothing while no tile shows it ([`Pattern::set_shown`]): it draws no
//! frame and keeps none, as a stream decodes none, except the one picture asked for
//! ([`Pattern::want_picture`]), as a stream's decode gate lets one keyframe's through.
//!
//! With `IPCAMERA_PICTURES` naming a folder, a camera whose name a PNG there has (`Front door.png`)
//! shows that picture, still, in place of the moving one ([`picture_for`]), and its status says it
//! is live, as a camera's would. With `IPCAMERA_FOUND` naming a JSON file, the network search
//! lists the cameras in it rather than refusing to search ([`found_cameras`]). `cargo screenshots`
//! takes the READMEs' screenshots this way (`crates/xtask`), with pictures and cameras made up for
//! it.

use std::{
    ffi::OsStr,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use ipcamera_core::FoundCamera;
use ipcamera_media::{DecodedFrame, FrameSlot};

/// The folder of pictures that stand in for cameras, each named for its camera: `<name>.png`.
pub(super) const PICTURES_VARIABLE: &str = "IPCAMERA_PICTURES";

/// The JSON file of the cameras the network search finds in their place: a list of
/// `ipcamera_core::FoundCamera`, as the server's `/api/discover` answers.
pub(super) const FOUND_VARIABLE: &str = "IPCAMERA_FOUND";

const WIDTH: u32 = 1920;
const HEIGHT: u32 = 1080;
const FPS: u32 = 30;

/// The moving bar's width, and how far it travels each frame, in pixels.
const BAR_WIDTH: u32 = 96;
const BAR_STEP: u32 = 16;

/// A running test picture. Dropping it stops the thread.
pub struct Pattern {
    frames: FrameSlot,
    /// Set while no tile shows it.
    hidden: Arc<AtomicBool>,
    /// Set when one picture is wanted while it is hidden.
    wanted: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    /// Whether it shows a picture of its own rather than the moving one.
    still: bool,
}

impl Pattern {
    /// The moving picture, or `picture`, still, in its place.
    pub fn start(picture: Option<DecodedFrame>) -> Self {
        let still = picture.is_some();
        let frames = FrameSlot::default();
        let hidden = Arc::new(AtomicBool::new(false));
        let wanted = Arc::new(AtomicBool::new(false));
        let stop = Arc::new(AtomicBool::new(false));

        let worker = {
            let frames = frames.clone();
            let hidden = hidden.clone();
            let wanted = wanted.clone();
            let stop = stop.clone();
            thread::spawn(move || run(&frames, &hidden, &wanted, &stop, picture))
        };

        Self {
            frames,
            hidden,
            wanted,
            stop,
            worker: Some(worker),
            still,
        }
    }

    /// Whether it shows a picture of its own ([`picture_for`]) rather than the moving one.
    pub fn is_still(&self) -> bool {
        self.still
    }

    pub fn frames(&self) -> FrameSlot {
        self.frames.clone()
    }

    /// Draws again, or stops drawing and lets go of the picture in the slot.
    pub fn set_shown(&self, shown: bool) {
        self.hidden.store(!shown, Ordering::SeqCst);
        if !shown {
            self.frames.clear();
        }
    }

    /// Draws the next picture even while hidden, and keeps it.
    pub fn want_picture(&self) {
        self.wanted.store(true, Ordering::SeqCst);
    }
}

impl Drop for Pattern {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

/// The picture standing in for the camera named `name`, when `IPCAMERA_PICTURES` names a folder
/// that has one.
pub fn picture_for(name: &str) -> Option<DecodedFrame> {
    let folder = std::env::var_os(PICTURES_VARIABLE)?;
    picture_in(Path::new(&folder), name)
}

/// `<name>.png` in `folder`, as BGRA; `None` when there is none or it can't be read, and for a name
/// that isn't a plain file name, which would reach outside the folder.
fn picture_in(folder: &Path, name: &str) -> Option<DecodedFrame> {
    let file = format!("{name}.png");
    if name.is_empty() || Path::new(&file).file_name() != Some(OsStr::new(&file)) {
        return None;
    }
    let rgba = image::open(folder.join(&file)).ok()?.into_rgba8();
    let (width, height) = rgba.dimensions();
    let mut bgra = rgba.into_raw();
    let (pixels, _) = bgra.as_chunks_mut::<4>();
    for pixel in pixels {
        pixel.swap(0, 2);
    }
    Some(DecodedFrame {
        width,
        height,
        bgra,
    })
}

/// The cameras the network search finds when `IPCAMERA_FOUND` names a file: those it lists, or
/// why it can't be read. `None` when it names none.
pub fn found_cameras() -> Option<Result<Vec<FoundCamera>, String>> {
    let file = std::env::var_os(FOUND_VARIABLE)?;
    Some(found_in(Path::new(&file)))
}

/// The cameras listed in `file`.
fn found_in(file: &Path) -> Result<Vec<FoundCamera>, String> {
    let text = std::fs::read_to_string(file)
        .map_err(|error| format!("Couldn't read {}: {error}.", file.display()))?;
    serde_json::from_str(&text)
        .map_err(|error| format!("{} isn't a list of cameras: {error}.", file.display()))
}

fn run(
    frames: &FrameSlot,
    hidden: &AtomicBool,
    wanted: &AtomicBool,
    stop: &AtomicBool,
    picture: Option<DecodedFrame>,
) {
    let still = picture.is_some();
    let DecodedFrame {
        width,
        height,
        bgra: background,
    } = picture.unwrap_or_else(|| DecodedFrame {
        width: WIDTH,
        height: HEIGHT,
        bgra: gradient(),
    });
    let interval = Duration::from_secs(1) / FPS;
    let mut next = Instant::now();
    let mut step = 0u32;

    while !stop.load(Ordering::SeqCst) {
        let shown = !hidden.load(Ordering::SeqCst);
        if shown || wanted.swap(false, Ordering::SeqCst) {
            // A new buffer every frame, as `DecodedFrame::from_bgra_rows` makes for a real picture.
            let mut bgra = background.clone();
            if !still {
                draw_bar(&mut bgra, (step * BAR_STEP) % WIDTH);
            }
            frames.store(Arc::new(DecodedFrame {
                width,
                height,
                bgra,
            }));
            // Checked after the store: a hide that lands in between empties the slot itself. A
            // picture asked for while hidden stays.
            if shown && hidden.load(Ordering::SeqCst) {
                frames.clear();
            }
        }

        step = step.wrapping_add(1);
        next += interval;
        if let Some(wait) = next.checked_duration_since(Instant::now()) {
            thread::sleep(wait);
        }
    }
}

/// A blue-to-red sweep with a green fade downwards, so scaling and channel order are easy to judge.
fn gradient() -> Vec<u8> {
    let mut bgra = Vec::with_capacity((WIDTH * HEIGHT * 4) as usize);
    for y in 0..HEIGHT {
        let green = (y * 255 / HEIGHT) as u8;
        for x in 0..WIDTH {
            let red = (x * 255 / WIDTH) as u8;
            bgra.extend_from_slice(&[255 - red, green, red, 255]);
        }
    }
    bgra
}

/// A white bar at `left`, cut off at the right edge of the picture.
fn draw_bar(bgra: &mut [u8], left: u32) {
    let right = (left + BAR_WIDTH).min(WIDTH);
    let row = (WIDTH * 4) as usize;
    let span = (left * 4) as usize..(right * 4) as usize;
    for line in bgra.chunks_exact_mut(row) {
        line[span.clone()].fill(255);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_gradient_fills_a_whole_frame() {
        assert_eq!(gradient().len(), (WIDTH * HEIGHT * 4) as usize);
    }

    #[test]
    fn the_bar_stops_at_the_right_edge() {
        let mut bgra = vec![0u8; (WIDTH * HEIGHT * 4) as usize];
        draw_bar(&mut bgra, WIDTH - 1);

        let row = (WIDTH * 4) as usize;
        assert!(bgra[row - 4..row].iter().all(|byte| *byte == 255));
        assert!(bgra[row - 8..row - 4].iter().all(|byte| *byte == 0));
    }

    #[test]
    fn a_camera_s_picture_is_its_name_in_the_folder_as_bgra() {
        let folder = tempfile::tempdir().unwrap();
        image::RgbaImage::from_raw(2, 1, vec![10, 20, 30, 255, 40, 50, 60, 255])
            .unwrap()
            .save(folder.path().join("Front door.png"))
            .unwrap();

        let picture = picture_in(folder.path(), "Front door").unwrap();
        assert_eq!((picture.width, picture.height), (2, 1));
        assert_eq!(picture.bgra, [30, 20, 10, 255, 60, 50, 40, 255]);

        assert!(picture_in(folder.path(), "Driveway").is_none());
        assert!(picture_in(folder.path(), "").is_none());
        assert!(picture_in(folder.path(), "../Front door").is_none());
    }

    #[test]
    fn the_cameras_found_are_the_file_s_and_a_file_that_isn_t_says_so() {
        let folder = tempfile::tempdir().unwrap();
        let file = folder.path().join("found.json");
        std::fs::write(
            &file,
            r#"[{ "host": "192.168.1.21", "onvif_port": 80, "model": "Dome 4" }, { "host": "192.168.1.30", "rtsp_port": 554 }]"#,
        )
        .unwrap();

        let found = found_in(&file).unwrap();
        assert_eq!(found.len(), 2);
        assert_eq!(
            (
                found[0].host.as_str(),
                found[0].onvif_port,
                found[0].model.as_str()
            ),
            ("192.168.1.21", Some(80), "Dome 4")
        );
        assert_eq!(found[1].rtsp_port, Some(554));

        std::fs::write(&file, "{}").unwrap();
        assert!(
            found_in(&file)
                .unwrap_err()
                .contains("isn't a list of cameras")
        );
        assert!(found_in(&folder.path().join("none.json")).is_err());
    }
}

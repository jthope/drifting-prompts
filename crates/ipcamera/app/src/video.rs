//! The live picture: a native component that draws a camera's newest frame with Direct2D.
//!
//! nsh's `<Video>` plays through Media Foundation and its `Canvas` has no pixel access, so neither
//! can show what `ipcamera-media` decodes. A registered component can: nsh hands it the Direct2D
//! context it is already drawing the window with, which makes the picture ordinary view content.
//! Text, sheets and menus draw over it, and it clips and resizes like any `<View>`.
//!
//! The component only shows a camera's stream: its `camera` prop names the camera, and its
//! `stream` prop which of its streams (a `ipcamera_core::TileStream` key, or `""` for its own). The
//! session belongs to `camera`, so several tiles can show one stream, and removing a tile leaves it
//! running. Its `fit` prop is a view's [`Fit`]: how the picture fills the tile.
//!
//! The work for a frame happens once per stream, however many tiles show it. [`Shown`] holds the
//! stream's frame on the GPU, which every tile draws, and one thread that asks those tiles to draw
//! when a new frame arrives. Done per tile, a 4 × 4 grid of one camera copied the same 8 MB frame to
//! the GPU sixteen times, and the window drew two or three times for each frame.
//!
//! **Zoom.** The wheel over a picture zooms into it, and out again, around the point under the
//! pointer, which stays where it is ([`Zoom`]): each tile its own, drawn from the same bitmap as
//! the rest, so zooming costs no GPU memory. The component takes the wheel itself and keeps the
//! zoom, so a step of the wheel never waits on JS. The component tells TSX how far
//! in it is with its `zoom` event (`{ scale }`, 1 for the whole picture), and goes back to the
//! whole picture on the `resetZoom` command, or when it is given another camera or stream.
//!
//! **Switching streams.** A tile given another of its camera's streams keeps drawing the one it
//! showed, as it last was, until the new one's first picture, for [`KEEP_LEAVING`] at most: a new
//! stream shows nothing until its first keyframe, seconds on some cameras, and a blank tile looked
//! as if the switch had done nothing. The tile's own text says what it is waiting for.
//!
//! **Pan.** A zoomed picture follows the pointer as it is dragged. TSX binds the drag to two
//! animated values that it passes on as the `panX` and `panY` props, which the platform sets here
//! on every frame the drag moves, with no JavaScript in between. They add every drag up (TSX
//! moves each drag into the values' offset as it ends), so a value never jumps: the part shown is
//! the zoom as it was when the wheel last changed it, moved by how far the values have moved
//! since ([`Zoom::dragged`]).

use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use ipcamera_core::{Fit, TileStream, sync::LockExt};
use ipcamera_media::{DecodedFrame, FrameSlot};
use nsh_platform_windows::{
    ComponentContext, Components, NativeComponent, Wheel,
    windows::Win32::Graphics::{
        Direct2D::{
            Common::{D2D_RECT_F, D2D_SIZE_U, D2D1_ALPHA_MODE_IGNORE, D2D1_PIXEL_FORMAT},
            D2D1_BITMAP_OPTIONS_NONE, D2D1_BITMAP_PROPERTIES1, D2D1_INTERPOLATION_MODE_LINEAR,
            ID2D1Bitmap1, ID2D1Device, ID2D1DeviceContext,
        },
        Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM,
    },
};
use serde_json::Value as JsonValue;

use crate::camera::{self, Camera, Output, StreamKey};

/// The name `views/picture.tsx` declares with `nativeComponent`.
const COMPONENT: &str = "ipcamera.video";

/// How often [`watch`] looks for a new frame. Well under one frame at 60 fps, so it adds little
/// delay; a callback from `FrameSlot` could replace it, but that would mean changing `crates/ipcamera/media`.
const POLL_EVERY: Duration = Duration::from_millis(4);

/// Numbers the tiles, so the tiles of one stream can be told apart.
static NEXT_TILE: AtomicU64 = AtomicU64::new(0);

/// How long a tile switching streams keeps drawing the one it showed while the new one has no
/// picture yet: a keyframe interval or two, after which a stream that still sends nothing has
/// failed, and the old picture would only mislead.
const KEEP_LEAVING: Duration = Duration::from_secs(10);

/// The furthest a picture zooms in: an eighth of it across the tile.
const MOST_ZOOM: f32 = 8.0;

/// How much one notch of the wheel zooms in or out. A precision touchpad's fractions of a notch
/// zoom as far in proportion.
const ZOOM_PER_NOTCH: f32 = 1.25;

/// How far a tile is zoomed into its picture, and where.
///
/// `scale` is 1 for the whole picture, and up to [`MOST_ZOOM`]. The part shown is `1 / scale` of
/// the picture each way, and `left` and `top` say where, as fractions of the picture (each from 0
/// to `1 - 1 / scale`), so a zoom stays where it was as the picture's size or the tile's fit
/// changes.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Zoom {
    scale: f32,
    left: f32,
    top: f32,
}

impl Default for Zoom {
    fn default() -> Self {
        Self {
            scale: 1.0,
            left: 0.0,
            top: 0.0,
        }
    }
}

impl Zoom {
    fn zoomed(self) -> bool {
        self.scale > 1.0
    }

    /// The zoom with its picture dragged `by` points, in a tile that draws the part shown
    /// `across` points wide and high: the part shown moves the other way, as far as the picture
    /// goes. The whole picture doesn't move.
    fn dragged(self, by: (f32, f32), across: (f32, f32)) -> Self {
        if !self.zoomed() || across.0 <= 0.0 || across.1 <= 0.0 {
            return self;
        }
        let most = 1.0 - 1.0 / self.scale;
        Self {
            scale: self.scale,
            left: (self.left - by.0 / (across.0 * self.scale)).clamp(0.0, most),
            top: (self.top - by.1 / (across.1 * self.scale)).clamp(0.0, most),
        }
    }

    /// The zoom `notches` of the wheel further in (fewer: out) around `at`, a point of the part
    /// shown now as fractions of it (0 to 1 each way), which shows at the same place after.
    fn wheeled(self, notches: f32, at: (f32, f32)) -> Self {
        let scale = (self.scale * ZOOM_PER_NOTCH.powf(notches)).clamp(1.0, MOST_ZOOM);
        // The point under the pointer, as fractions of the whole picture.
        let point = (self.left + at.0 / self.scale, self.top + at.1 / self.scale);
        let most = 1.0 - 1.0 / scale;
        Self {
            scale,
            left: (point.0 - at.0 / scale).clamp(0.0, most),
            top: (point.1 - at.1 / scale).clamp(0.0, most),
        }
    }

    /// The part of `source`, the part of the picture the fit draws (in its pixels), that this
    /// zoom shows; `source` itself while it shows the whole picture.
    fn part_of(self, source: D2D_RECT_F) -> D2D_RECT_F {
        if !self.zoomed() {
            return source;
        }
        let (width, height) = (source.right - source.left, source.bottom - source.top);
        let left = source.left + self.left * width;
        let top = source.top + self.top * height;
        D2D_RECT_F {
            left,
            top,
            right: left + width / self.scale,
            bottom: top + height / self.scale,
        }
    }
}

/// A stream on screen: its camera's id, and which of the camera's streams.
type ShownKey = (String, StreamKey);

thread_local! {
    /// The streams on screen. nsh makes, draws and drops the tiles on the main thread, so this
    /// lives there and only the tiles use it.
    static SHOWN: RefCell<HashMap<ShownKey, Shown>> = RefCell::new(HashMap::new());
}

/// Registers the component on the views nsh makes.
pub fn register(components: &Components) {
    components.register(
        COMPONENT,
        Rc::new(|context: ComponentContext| -> Box<dyn NativeComponent> {
            Box::new(VideoView::new(context))
        }),
    );
}

/// The frame on the GPU, reused until the next one arrives.
struct Painted {
    /// The Direct2D device the bitmap belongs to: a lost device needs a new bitmap.
    device: ID2D1Device,
    bitmap: ID2D1Bitmap1,
    /// The picture in `bitmap`, so a repaint of the same frame copies nothing.
    source: Arc<DecodedFrame>,
}

/// Asks one tile's window to draw again.
type Redraw = Box<dyn Fn() + Send>;

/// The tiles showing one stream, each with the way to ask its window to draw again. The stream's
/// [`watch`] thread shares it.
#[derive(Clone, Default)]
struct Viewers(Arc<Mutex<Vec<(u64, Redraw)>>>);

impl Viewers {
    fn add(&self, tile: u64, redraw: Redraw) {
        self.list().push((tile, redraw));
    }

    /// Takes `tile` out, returning whether any tile is left.
    fn remove(&self, tile: u64) -> bool {
        let mut list = self.list();
        list.retain(|(shown, _)| *shown != tile);
        !list.is_empty()
    }

    /// Asks every tile to draw. They are asked together, so the window draws once for all of them.
    fn redraw(&self) {
        for (_, redraw) in self.list().iter() {
            redraw();
        }
    }

    /// The list, even if a thread panicked holding it: a stale tile is better than a stuck window.
    fn list(&self) -> MutexGuard<'_, Vec<(u64, Redraw)>> {
        self.0.locked()
    }
}

/// One stream on screen, shared by every tile showing it.
struct Shown {
    output: Arc<Output>,
    viewers: Viewers,
    /// Tells its [`watch`] to stop.
    stop: Arc<AtomicBool>,
    /// Its newest frame on the GPU, which every tile draws.
    painted: Option<Painted>,
}

impl Shown {
    /// Starts watching `output`, where a stream puts its pictures, for new frames.
    fn start(output: Arc<Output>) -> Self {
        let viewers = Viewers::default();
        let stop = Arc::new(AtomicBool::new(false));
        {
            let output = output.clone();
            let viewers = viewers.clone();
            let stop = stop.clone();
            thread::spawn(move || watch(|| output.frames(), &viewers, &stop));
        }
        Self {
            output,
            viewers,
            stop,
            painted: None,
        }
    }
}

impl Drop for Shown {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

/// Starts showing stream `key`, which puts its pictures in `output`, in `tile`, which `redraw`
/// asks to draw again.
fn add_tile(key: &ShownKey, output: Arc<Output>, tile: u64, redraw: Redraw) {
    SHOWN.with_borrow_mut(|shown| {
        shown
            .entry(key.clone())
            .or_insert_with(|| Shown::start(output))
            .viewers
            .add(tile, redraw);
    });
}

/// Stops showing stream `key` in `tile`. The last tile to go takes the stream's frame on the GPU
/// and its watcher with it.
fn remove_tile(key: &ShownKey, tile: u64) {
    // `try_with`, because nsh may drop tiles as the main thread ends, after `SHOWN` has gone. What
    // goes is dropped after the borrow ends.
    let gone = SHOWN.try_with(|shown| {
        let mut shown = shown.borrow_mut();
        let left = shown
            .get(key)
            .is_some_and(|stream| stream.viewers.remove(tile));
        if left { None } else { shown.remove(key) }
    });
    drop(gone);
}

/// Stream `key`'s newest frame on the GPU, and its size in pixels. Only the first tile to draw a
/// new frame copies it there; the others draw the same bitmap.
fn frame_on_gpu(key: &ShownKey, dc: &ID2D1DeviceContext) -> Option<(ID2D1Bitmap1, (f32, f32))> {
    SHOWN.with_borrow_mut(|shown| {
        let on_screen = shown.get_mut(key)?;
        let Some(frame) = on_screen.output.frames().latest() else {
            // The stream stopped, so the frame it last uploaded can go.
            on_screen.painted = None;
            return None;
        };
        if frame.width == 0 || frame.height == 0 {
            return None;
        }
        let bitmap = uploaded(&mut on_screen.painted, dc, &frame, &on_screen.output)?.clone();
        Some((bitmap, (frame.width as f32, frame.height as f32)))
    })
}

/// Stream `key`'s last frame on the GPU, whether or not the stream still sends, and its size in
/// pixels: what a tile switching away from it keeps drawing. Copied again only after a lost device.
fn last_on_gpu(key: &ShownKey, dc: &ID2D1DeviceContext) -> Option<(ID2D1Bitmap1, (f32, f32))> {
    SHOWN.with_borrow_mut(|shown| {
        let on_screen = shown.get_mut(key)?;
        let source = on_screen.painted.as_ref()?.source.clone();
        let bitmap = uploaded(&mut on_screen.painted, dc, &source, &on_screen.output)?.clone();
        Some((bitmap, (source.width as f32, source.height as f32)))
    })
}

/// One video tile.
struct VideoView {
    context: ComponentContext,
    /// This tile's number among the tiles of the stream it shows.
    tile: u64,
    /// The camera and the stream its props name.
    wanted: ShownKey,
    /// The stream it shows, with its camera, while there is such a camera.
    shown: Option<(ShownKey, Arc<Camera>)>,
    /// The stream of the same camera it showed before `shown`, and since when: drawn as it last
    /// was until `shown` has a picture, for [`KEEP_LEAVING`] at most.
    leaving: RefCell<Option<(ShownKey, Instant)>>,
    fit: Fit,
    zoom: Zoom,
    /// The drags' moves added up, in points, as the `panX` and `panY` props give them.
    pan: (f32, f32),
    /// The zoom when something other than a drag last changed it, and `pan` then: a drag moves the
    /// picture from there.
    dragged_from: (Zoom, (f32, f32)),
    /// What it last drew, for where the wheel's pointer is in the picture. `None` until it has
    /// drawn one.
    drawn: Cell<Option<Drawn>>,
}

/// The size of a tile and of the picture it last drew.
#[derive(Clone, Copy)]
struct Drawn {
    /// The tile's, in points.
    room: (f32, f32),
    /// The picture's, in pixels.
    picture: (f32, f32),
}

impl VideoView {
    fn new(context: ComponentContext) -> Self {
        Self {
            context,
            tile: NEXT_TILE.fetch_add(1, Ordering::Relaxed),
            wanted: (String::new(), None),
            shown: None,
            leaving: RefCell::new(None),
            fit: Fit::default(),
            zoom: Zoom::default(),
            pan: (0.0, 0.0),
            dragged_from: (Zoom::default(), (0.0, 0.0)),
            drawn: Cell::new(None),
        }
    }

    /// Zooms to `zoom` by the wheel, a reset or another picture, from where drags then move it.
    fn rezoom(&mut self, zoom: Zoom) {
        self.dragged_from = (zoom, self.pan);
        self.show_part(zoom);
    }

    /// Shows the part of the picture `zoom` says, drawing again, and telling TSX when how far in
    /// it is changed: a drag, which only moves it, sends nothing.
    fn show_part(&mut self, zoom: Zoom) {
        if zoom == self.zoom {
            return;
        }
        let rescaled = zoom.scale != self.zoom.scale;
        self.zoom = zoom;
        self.context.redraw();
        if rescaled {
            self.context
                .emit("zoom", serde_json::json!({ "scale": zoom.scale }));
        }
    }

    /// Takes the drags' moves as far as `x` and `y` (each `None` where it is unchanged), and moves
    /// the zoomed picture with them.
    fn pan_to(&mut self, x: Option<f32>, y: Option<f32>) {
        self.pan = (x.unwrap_or(self.pan.0), y.unwrap_or(self.pan.1));
        let Some(Drawn { room, picture }) = self.drawn.get() else {
            return;
        };
        let (rect, _) = place(self.fit, picture, room);
        let (from, pan_then) = self.dragged_from;
        let by = (self.pan.0 - pan_then.0, self.pan.1 - pan_then.1);
        let across = (rect.right - rect.left, rect.bottom - rect.top);
        self.show_part(from.dragged(by, across));
    }

    /// Shows the stream the props name, or nothing when there is no such camera. Another stream
    /// of the same camera takes over from the one it showed, which it keeps drawing until the new
    /// one has a picture (`leaving`).
    fn show(&mut self) {
        if self
            .shown
            .as_ref()
            .is_some_and(|(shown, _)| *shown == self.wanted)
        {
            return;
        }
        let before = self.shown.take().map(|(key, _)| key);
        let same_camera = before.as_ref().is_some_and(|key| key.0 == self.wanted.0);
        if !same_camera {
            self.let_go_of_leaving();
        }
        if let Some(key) = before {
            // The first stream it switched away from is the one with a picture to keep; one it
            // passed on the way had none yet.
            if same_camera && self.leaving.borrow().is_none() {
                *self.leaving.borrow_mut() = Some((key, Instant::now()));
                // Nothing else may draw once the old stream stops, so a draw comes when its time
                // is up, which lets it go.
                let context = self.context.clone();
                thread::spawn(move || {
                    thread::sleep(KEEP_LEAVING);
                    context.redraw();
                });
            } else {
                remove_tile(&key, self.tile);
            }
        }
        // Another picture starts whole.
        self.rezoom(Zoom::default());
        self.drawn.set(None);

        let Ok(camera) = camera::find(&self.wanted.0) else {
            self.let_go_of_leaving();
            return;
        };
        let back = self
            .leaving
            .borrow()
            .as_ref()
            .is_some_and(|(key, _)| *key == self.wanted);
        if back {
            // Back on the stream it is still drawing, whose tile it still is.
            self.leaving.borrow_mut().take();
        } else {
            let context = self.context.clone();
            add_tile(
                &self.wanted,
                camera.output(&self.wanted.1),
                self.tile,
                Box::new(move || context.redraw()),
            );
        }
        self.shown = Some((self.wanted.clone(), camera));
    }

    fn hide(&mut self) {
        if let Some((key, _)) = self.shown.take() {
            remove_tile(&key, self.tile);
        }
        self.let_go_of_leaving();
    }

    /// Stops drawing the stream it switched away from.
    fn let_go_of_leaving(&self) {
        let leaving = self.leaving.borrow_mut().take();
        if let Some((key, _)) = leaving {
            remove_tile(&key, self.tile);
        }
    }

    /// The picture of the stream it switched away from, as it last was, while that is to be
    /// kept: until [`KEEP_LEAVING`] has passed.
    fn leaving_frame(&self, dc: &ID2D1DeviceContext) -> Option<(ID2D1Bitmap1, (f32, f32))> {
        let expired = self
            .leaving
            .borrow()
            .as_ref()
            .is_some_and(|(_, since)| since.elapsed() >= KEEP_LEAVING);
        if expired {
            self.let_go_of_leaving();
            return None;
        }
        let leaving = self.leaving.borrow();
        let (key, _) = leaving.as_ref()?;
        last_on_gpu(key, dc)
    }
}

impl NativeComponent for VideoView {
    fn draw(&self, dc: &ID2D1DeviceContext, size: (f32, f32)) {
        let Some((key, _)) = &self.shown else {
            return;
        };
        let current = frame_on_gpu(key, dc);
        if current.is_some() {
            // The new stream has its first picture: the old one's can go.
            self.let_go_of_leaving();
        }
        let Some((bitmap, picture)) = current.or_else(|| self.leaving_frame(dc)) else {
            return;
        };
        self.drawn.set(Some(Drawn {
            room: size,
            picture,
        }));

        let (rect, fitted) = place(self.fit, picture, size);
        // The zoom shows part of what the fit draws: the whole picture, or Fill's crop of it.
        let source = if self.zoom.zoomed() {
            let whole = D2D_RECT_F {
                left: 0.0,
                top: 0.0,
                right: picture.0,
                bottom: picture.1,
            };
            Some(self.zoom.part_of(fitted.unwrap_or(whole)))
        } else {
            fitted
        };
        // SAFETY: nsh calls `draw` between BeginDraw and EndDraw, and the bitmap belongs to this
        // context's device, which `uploaded` just checked.
        unsafe {
            dc.DrawBitmap(
                &bitmap,
                Some(&rect),
                1.0,
                D2D1_INTERPOLATION_MODE_LINEAR,
                source.as_ref().map(std::ptr::from_ref),
                None,
            );
        }
    }

    fn set_prop(&mut self, name: &str, value: Option<&JsonValue>) {
        let text = value.and_then(JsonValue::as_str).unwrap_or_default();
        match name {
            "camera" => {
                self.wanted.0 = text.to_string();
                self.show();
            }
            "stream" => {
                self.wanted.1 = TileStream::played(text);
                self.show();
            }
            "fit" => {
                self.fit = Fit::from_key(text).unwrap_or_default();
                self.context.redraw();
            }
            // Numbers, set on every frame a drag moves them.
            "panX" => self.pan_to(value.and_then(JsonValue::as_f64).map(|x| x as f32), None),
            "panY" => self.pan_to(None, value.and_then(JsonValue::as_f64).map(|y| y as f32)),
            _ => {}
        }
    }

    fn command(&mut self, name: &str, _args: &JsonValue) {
        if name == "resetZoom" {
            self.rezoom(Zoom::default());
        }
    }

    /// Zooms in or out around the pointer. Taken whenever the tile has drawn a picture, so the
    /// wheel never scrolls what is around it.
    fn wheel(&mut self, event: &Wheel) -> bool {
        let Some(Drawn { room, picture }) = self.drawn.get() else {
            return false;
        };
        let (rect, _) = place(self.fit, picture, room);
        // Where the pointer is in the picture as drawn; over the bars beside it, the nearest edge.
        let along = |at: f32, from: f32, to: f32| ((at - from) / (to - from)).clamp(0.0, 1.0);
        let at = (
            along(event.x, rect.left, rect.right),
            along(event.y, rect.top, rect.bottom),
        );
        // A notch away from the user zooms in, as a map does.
        self.rezoom(self.zoom.wheeled(-event.notches_y, at));
        true
    }
}

impl Drop for VideoView {
    /// nsh drops its components as the window closes, and this is the last the app hears of it:
    /// nsh's close event goes to JS, which a closing window never waits for. A camera in the
    /// middle of a PTZ move must not be left moving, so
    /// the stop goes out here, and this waits for it, but only briefly (`Camera::release_ptz`): a
    /// camera that stopped answering never holds the window. Dropping a tile for any other reason
    /// costs only a reconnect on the next move.
    fn drop(&mut self) {
        let camera = self.shown.as_ref().map(|(_, camera)| camera.clone());
        self.hide();
        if let Some(camera) = camera {
            camera.release_ptz();
        }
    }
}

/// The bitmap holding `frame`, copying it to the GPU only when it is a new picture.
///
/// One bitmap per stream on screen is reused for as long as it runs, so a live feed allocates no GPU
/// memory per frame.
fn uploaded<'a>(
    painted: &'a mut Option<Painted>,
    dc: &ID2D1DeviceContext,
    frame: &Arc<DecodedFrame>,
    output: &Output,
) -> Option<&'a ID2D1Bitmap1> {
    // SAFETY: a getter on the context nsh is drawing with.
    let device = unsafe { dc.GetDevice() }.ok()?;
    let pitch = frame.width * 4;

    match painted {
        Some(current) if Arc::ptr_eq(&current.source, frame) && current.device == device => {
            return painted.as_ref().map(|current| &current.bitmap);
        }
        Some(current)
            if current.device == device
                && current.source.width == frame.width
                && current.source.height == frame.height =>
        {
            // SAFETY: the buffer is `pitch * height` bytes, which `DecodedFrame` guarantees, and
            // the bitmap is that same size. Direct2D copies it before this returns.
            unsafe {
                current
                    .bitmap
                    .CopyFromMemory(None, frame.bgra.as_ptr().cast(), pitch)
            }
            .ok()?;
            current.source = frame.clone();
        }
        _ => {
            let properties = D2D1_BITMAP_PROPERTIES1 {
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_IGNORE,
                },
                dpiX: 96.0,
                dpiY: 96.0,
                bitmapOptions: D2D1_BITMAP_OPTIONS_NONE,
                colorContext: std::mem::ManuallyDrop::new(None),
            };
            let size = D2D_SIZE_U {
                width: frame.width,
                height: frame.height,
            };
            // SAFETY: as above, and `properties` outlives the call.
            let bitmap = unsafe {
                dc.CreateBitmap(size, Some(frame.bgra.as_ptr().cast()), pitch, &properties)
            }
            .ok()?;
            *painted = Some(Painted {
                device,
                bitmap,
                source: frame.clone(),
            });
        }
    }

    output.count_painted();
    painted.as_ref().map(|current| &current.bitmap)
}

/// Where a `picture` goes in a tile of size `room` for `fit`: the rectangle it is drawn in, and the
/// part of the picture drawn there (all of it when `None`).
fn place(fit: Fit, picture: (f32, f32), room: (f32, f32)) -> (D2D_RECT_F, Option<D2D_RECT_F>) {
    let whole = D2D_RECT_F {
        left: 0.0,
        top: 0.0,
        right: room.0,
        bottom: room.1,
    };
    match fit {
        Fit::Original => (contain(picture, room), None),
        Fit::Stretch => (whole, None),
        Fit::Fill => (whole, Some(cover(picture, room))),
        // The picture is reshaped to the ratio, then fitted like `Original`.
        forced => (contain(forced.aspect().unwrap_or(picture), room), None),
    }
}

/// The middle part of `picture` that has `room`'s shape: what `Fill` shows, scaled up to the tile.
fn cover(picture: (f32, f32), room: (f32, f32)) -> D2D_RECT_F {
    let scale = (room.0 / picture.0).max(room.1 / picture.1);
    let (width, height) = (room.0 / scale, room.1 / scale);
    let (left, top) = ((picture.0 - width) / 2.0, (picture.1 - height) / 2.0);
    D2D_RECT_F {
        left,
        top,
        right: left + width,
        bottom: top + height,
    }
}

/// Scales `picture` to fit `room` and centers it, leaving the view's background around it.
fn contain(picture: (f32, f32), room: (f32, f32)) -> D2D_RECT_F {
    let scale = (room.0 / picture.0).min(room.1 / picture.1);
    let (width, height) = (picture.0 * scale, picture.1 * scale);
    let (left, top) = ((room.0 - width) / 2.0, (room.1 - height) / 2.0);
    D2D_RECT_F {
        left,
        top,
        right: left + width,
        bottom: top + height,
    }
}

/// Asks the tiles showing a stream to draw whenever it has a new frame, or stops having one.
///
/// `frames` gives the stream's slot, read again each time because a new session brings a new one.
/// A frame count alone would miss a stream stopping, and a new session resets the count, so this
/// compares both with what it saw last.
fn watch(frames: impl Fn() -> FrameSlot, viewers: &Viewers, stop: &AtomicBool) {
    let mut seen = (0, false);

    while !stop.load(Ordering::SeqCst) {
        let frames = frames();
        let now = (frames.produced(), frames.latest().is_some());
        if now != seen {
            seen = now;
            viewers.redraw();
        }
        thread::sleep(POLL_EVERY);
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::atomic::AtomicUsize, time::Instant};

    use super::*;

    #[test]
    fn a_wide_picture_is_letterboxed_in_a_square() {
        let rect = contain((200.0, 100.0), (100.0, 100.0));
        assert_eq!(
            (rect.left, rect.top, rect.right, rect.bottom),
            (0.0, 25.0, 100.0, 75.0)
        );
    }

    fn edges(rect: D2D_RECT_F) -> (f32, f32, f32, f32) {
        (rect.left, rect.top, rect.right, rect.bottom)
    }

    #[test]
    fn fill_crops_the_middle_of_the_picture_to_the_tile_shape() {
        let (rect, source) = place(Fit::Fill, (200.0, 100.0), (100.0, 100.0));
        assert_eq!(edges(rect), (0.0, 0.0, 100.0, 100.0));
        assert_eq!(edges(source.unwrap()), (50.0, 0.0, 150.0, 100.0));
    }

    #[test]
    fn stretch_fills_the_tile_with_the_whole_picture() {
        let (rect, source) = place(Fit::Stretch, (200.0, 100.0), (100.0, 100.0));
        assert_eq!(edges(rect), (0.0, 0.0, 100.0, 100.0));
        assert!(source.is_none());
    }

    #[test]
    fn a_forced_ratio_reshapes_the_picture_then_fits_it() {
        let (rect, source) = place(Fit::Square, (1920.0, 1080.0), (200.0, 100.0));
        assert_eq!(edges(rect), (50.0, 0.0, 150.0, 100.0));
        assert!(source.is_none());
    }

    #[test]
    fn a_tall_picture_is_pillarboxed_in_a_square() {
        let rect = contain((100.0, 200.0), (100.0, 100.0));
        assert_eq!(
            (rect.left, rect.top, rect.right, rect.bottom),
            (25.0, 0.0, 75.0, 100.0)
        );
    }

    #[test]
    fn a_camera_stays_on_screen_until_its_last_tile_goes() {
        let viewers = Viewers::default();
        viewers.add(1, Box::new(|| {}));
        viewers.add(2, Box::new(|| {}));

        assert!(viewers.remove(1), "tile 2 still shows the camera");
        assert!(
            viewers.remove(7),
            "a tile that never showed it changes nothing"
        );
        assert!(!viewers.remove(2), "that was the last tile");
    }

    fn counting(count: &Arc<AtomicUsize>) -> Redraw {
        let count = count.clone();
        Box::new(move || {
            count.fetch_add(1, Ordering::SeqCst);
        })
    }

    fn wait_until(done: impl Fn() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !done() {
            assert!(
                Instant::now() < deadline,
                "the tiles were not asked to draw"
            );
            thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn each_new_frame_asks_every_tile_of_the_camera_to_draw_once() {
        let slot = FrameSlot::default();
        let viewers = Viewers::default();
        let counts: Vec<_> = (0..3).map(|_| Arc::new(AtomicUsize::new(0))).collect();
        for (tile, count) in counts.iter().enumerate() {
            viewers.add(tile as u64, counting(count));
        }
        let all_at = |n| counts.iter().all(|count| count.load(Ordering::SeqCst) == n);

        let stop = Arc::new(AtomicBool::new(false));
        let watcher = {
            let (slot, viewers, stop) = (slot.clone(), viewers.clone(), stop.clone());
            thread::spawn(move || watch(|| slot.clone(), &viewers, &stop))
        };

        slot.store(Arc::new(DecodedFrame {
            width: 2,
            height: 2,
            bgra: vec![0; 16],
        }));
        wait_until(|| all_at(1));
        thread::sleep(POLL_EVERY * 5);
        assert!(all_at(1), "the same frame asks for nothing more");

        slot.clear();
        wait_until(|| all_at(2));

        stop.store(true, Ordering::SeqCst);
        watcher.join().expect("the watcher should stop");
    }

    #[test]
    fn the_wheel_zooms_in_around_the_point_under_the_pointer() {
        // The middle of the picture's right half, under the pointer.
        let at = (0.75, 0.5);
        let zoom = Zoom::default().wheeled(1.0, at);
        assert_eq!(zoom.scale, ZOOM_PER_NOTCH);
        let under = |zoom: Zoom| (zoom.left + at.0 / zoom.scale, zoom.top + at.1 / zoom.scale);
        assert_eq!(under(zoom), (0.75, 0.5), "the same point stays under it");
        let deeper = zoom.wheeled(3.0, at);
        let (x, y) = under(deeper);
        assert!(
            (x - 0.75).abs() < 1e-5 && (y - 0.5).abs() < 1e-5,
            "{x}, {y}"
        );
    }

    #[test]
    fn a_zoom_stays_between_the_whole_picture_and_an_eighth_of_it() {
        let most = Zoom::default().wheeled(100.0, (1.0, 1.0));
        assert_eq!(most.scale, MOST_ZOOM);
        assert_eq!(
            (most.left, most.top),
            (1.0 - 1.0 / MOST_ZOOM, 1.0 - 1.0 / MOST_ZOOM),
            "the part shown never leaves the picture"
        );
        assert_eq!(most.wheeled(-100.0, (0.2, 0.9)), Zoom::default());
        assert!(!Zoom::default().wheeled(-1.0, (0.5, 0.5)).zoomed());
    }

    #[test]
    fn a_dragged_picture_follows_the_pointer_as_far_as_it_goes() {
        let zoom = Zoom {
            scale: 4.0,
            left: 0.5,
            top: 0.5,
        };
        // A tile 400 points wide shows a quarter of the picture: 100 points are a sixteenth of it.
        let moved = zoom.dragged((100.0, -100.0), (400.0, 300.0));
        assert_eq!(moved.scale, 4.0);
        assert!((moved.left - (0.5 - 1.0 / 16.0)).abs() < 1e-6, "{moved:?}");
        assert!(
            (moved.top - (0.5 + 100.0 / 1200.0)).abs() < 1e-6,
            "{moved:?}"
        );

        let far = zoom.dragged((-10_000.0, 10_000.0), (400.0, 300.0));
        assert_eq!(
            (far.left, far.top),
            (0.75, 0.0),
            "no further than the picture's edges"
        );
        assert_eq!(
            Zoom::default().dragged((50.0, 50.0), (400.0, 300.0)),
            Zoom::default(),
            "the whole picture doesn't move"
        );
    }

    #[test]
    fn a_zoom_shows_part_of_what_the_fit_draws() {
        let whole = D2D_RECT_F {
            left: 0.0,
            top: 0.0,
            right: 200.0,
            bottom: 100.0,
        };
        assert_eq!(edges(Zoom::default().part_of(whole)), edges(whole));
        let zoom = Zoom {
            scale: 2.0,
            left: 0.5,
            top: 0.25,
        };
        assert_eq!(edges(zoom.part_of(whole)), (100.0, 25.0, 200.0, 75.0));
        // Fill's crop of a wide picture in a square tile, zoomed the same.
        let (_, crop) = place(Fit::Fill, (200.0, 100.0), (100.0, 100.0));
        assert_eq!(
            edges(zoom.part_of(crop.unwrap())),
            (100.0, 25.0, 150.0, 75.0)
        );
    }
}

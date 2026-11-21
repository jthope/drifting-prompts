//! Tests of a camera's session, against local cameras that never touch a real one.

use super::*;
use crate::pipeline::{AudioCodec, Offer};
use g2g_core::VideoCodec;
use std::{
    env,
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    thread::sleep,
    time::Instant,
};

struct FakeCamera {
    port: u16,
}

impl FakeCamera {
    fn start(serve: fn(TcpStream)) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a local port should be free");
        let port = listener.local_addr().expect("it has an address").port();
        thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                thread::spawn(move || serve(stream));
            }
        });
        Self { port }
    }

    fn setting(&self) -> CameraConfig {
        CameraConfig {
            host: "127.0.0.1".into(),
            rtsp_port: self.port,
            user: "admin".into(),
            password: "wrong".into(),
            auto_reconnect: true,
            ..CameraConfig::default()
        }
    }
}

fn refuse_every_login(stream: TcpStream) {
    let mut writer = stream.try_clone().expect("the stream should clone");
    let mut lines = BufReader::new(stream).lines();
    let mut sequence = String::from("1");

    while let Some(Ok(line)) = lines.next() {
        if let Some(value) = line.strip_prefix("CSeq:") {
            sequence = value.trim().to_string();
        }
        if line.is_empty() {
            let reply = format!(
                "RTSP/1.0 401 Unauthorized\r\nCSeq: {sequence}\r\n\
                 WWW-Authenticate: Basic realm=\"camera\"\r\n\r\n"
            );
            if writer.write_all(reply.as_bytes()).is_err() {
                return;
            }
        }
    }
}

fn hang_up(stream: TcpStream) {
    drop(stream);
}

fn offer_only_mjpeg(stream: TcpStream) {
    const SDP: &str = "v=0\r\no=- 0 0 IN IP4 127.0.0.1\r\ns=Fake\r\nc=IN IP4 0.0.0.0\r\n\
                       t=0 0\r\nm=video 0 RTP/AVP 26\r\na=control:trackID=0\r\n";
    let mut writer = stream.try_clone().expect("the stream should clone");
    let mut lines = BufReader::new(stream).lines();
    let (mut method, mut sequence) = (String::new(), String::from("1"));

    while let Some(Ok(line)) = lines.next() {
        if method.is_empty() {
            method = line.split(' ').next().unwrap_or_default().to_string();
        }
        if let Some(value) = line.strip_prefix("CSeq:") {
            sequence = value.trim().to_string();
        }
        if !line.is_empty() {
            continue;
        }
        let reply = if method == "DESCRIBE" {
            format!(
                "RTSP/1.0 200 OK\r\nCSeq: {sequence}\r\nContent-Type: application/sdp\r\n\
                 Content-Length: {}\r\n\r\n{SDP}",
                SDP.len()
            )
        } else {
            format!("RTSP/1.0 200 OK\r\nCSeq: {sequence}\r\nPublic: OPTIONS, DESCRIBE\r\n\r\n")
        };
        if writer.write_all(reply.as_bytes()).is_err() {
            return;
        }
        method.clear();
    }
}

fn wait_for(session: &MediaSession, wanted: fn(&MediaStatus) -> bool) -> MediaStatus {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let status = session.status();
        if wanted(&status) {
            return status;
        }
        assert!(Instant::now() < deadline, "still {status:?} after 10 s");
        sleep(Duration::from_millis(10));
    }
}

#[cfg(windows)]
const PRETEND_SIZE: (u32, u32) = (640, 360);

#[cfg(windows)]
fn pretend_camera() -> CameraConfig {
    let (width, height) = PRETEND_SIZE;
    crate::pretend::start(crate::pretend::Pattern {
        width,
        height,
        fps: 25,
        frames: 500,
    })
    .expect("the pretend camera should start")
}

#[cfg(windows)]
fn first_picture(session: &MediaSession) -> Arc<crate::DecodedFrame> {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(frame) = session.frames().latest() {
            return frame;
        }
        if let MediaStatus::Failed(message) = session.status() {
            panic!("the session failed: {message}");
        }
        assert!(
            Instant::now() < deadline,
            "no picture within 15 s; the session is {:?}",
            session.status()
        );
        sleep(Duration::from_millis(50));
    }
}

#[test]
fn a_crash_becomes_an_error_that_says_why() {
    let error = contain::<()>(|| panic!("corrupt packet")).unwrap_err();
    assert_eq!(
        error.to_string(),
        "the video pipeline crashed: corrupt packet"
    );

    // A message with arguments comes as a `String`, not a `&str`.
    let error = contain::<()>(|| panic!("bad slice {}", 7)).unwrap_err();
    assert_eq!(error.to_string(), "the video pipeline crashed: bad slice 7");

    assert_eq!(contain(|| 7).unwrap(), 7, "no crash, no error");
}

#[test]
fn a_crash_fails_the_session_when_it_may_not_reconnect() {
    let shared = Shared::new(Output::Display);
    let camera = CameraConfig {
        auto_reconnect: false,
        ..CameraConfig::default()
    };

    supervise(camera, &shared, |_, _, _| panic!("corrupt packet"));

    assert_eq!(
        shared.status(),
        MediaStatus::Failed("the video pipeline crashed: corrupt packet".to_string()),
        "never stuck at whatever it said before the crash"
    );
}

#[test]
fn a_crash_is_retried_like_a_broken_stream() {
    let shared = Shared::new(Output::Display);
    shared.record.start(
        Arc::new(|_| PathBuf::from("unused.mp4")),
        RecordRules::default(),
    );
    let mut tries = 0;
    let mut retried_with = None;

    supervise(CameraConfig::default(), &shared, |runtime, _, shared| {
        tries += 1;
        // Whatever runtime it gets, before the crash or after, still runs.
        runtime.block_on(async {});
        if tries == 1 {
            panic!("corrupt packet");
        }
        retried_with = Some((shared.status(), shared.record.status()));
        shared.stop.store(true, Ordering::SeqCst);
        Ended::Stopped
    });

    assert_eq!(tries, 2);
    assert_eq!(
        retried_with,
        Some((
            MediaStatus::Reconnecting { attempt: 1 },
            RecordStatus::Waiting
        )),
        "it reconnects, and a recording asked for carries on into the new stream"
    );
}

#[cfg(windows)]
#[test]
fn decodes_a_local_h264_stream_into_bgra_pictures() {
    let (width, height) = PRETEND_SIZE;
    let camera = pretend_camera();
    let mut session = MediaSession::start(&camera).expect("the session should start");
    let picture = first_picture(&session);

    assert_eq!((picture.width, picture.height), (width, height));
    assert_eq!(picture.bgra.len(), (width * height * 4) as usize);
    let (darkest, brightest) = picture
        .bgra
        .as_chunks::<4>()
        .0
        .iter()
        .map(|[blue, green, red, _]| u32::from(*blue) + u32::from(*green) + u32::from(*red))
        .fold((u32::MAX, 0), |(low, high), sum| {
            (low.min(sum), high.max(sum))
        });
    assert!(
        brightest - darkest > 100,
        "the test pattern came out flat ({darkest}..{brightest})"
    );
    // Live comes up to 50 ms after the picture.
    wait_for(&session, |status| *status == MediaStatus::Live);

    session.stop();
    assert_eq!(session.status(), MediaStatus::Stopped);
}

#[cfg(windows)]
#[test]
fn a_hidden_live_view_goes_live_without_decoding() {
    let camera = pretend_camera();
    let mut session = MediaSession::start(&camera).expect("the session should start");
    session.set_shown(false);
    wait_for(&session, |status| *status == MediaStatus::Live);

    let produced = session.frames().produced();
    sleep(Duration::from_millis(500));
    assert_eq!(
        session.frames().produced(),
        produced,
        "nothing is decoded while hidden"
    );
    assert!(session.frames().latest().is_none());

    session.set_shown(true);
    let picture = first_picture(&session);
    assert_eq!((picture.width, picture.height), PRETEND_SIZE);

    session.stop();
    assert_eq!(session.status(), MediaStatus::Stopped);
}

#[cfg(windows)]
#[test]
fn relays_a_local_h264_stream_to_a_viewer() {
    use crate::relay::Relayed;

    let camera = pretend_camera();
    let mut session = MediaSession::relay(&camera).expect("the session should start");
    let mut viewer = session.feed().watch();
    wait_for(&session, |status| *status == MediaStatus::Live);

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut next = || {
        runtime
            .block_on(async { tokio::time::timeout(Duration::from_secs(5), viewer.next()).await })
            .expect("the feed should keep coming")
            .expect("the feed should not end")
    };
    let Relayed::Config(config) = next() else {
        panic!("the set-up should come first");
    };
    assert!(config.codec.starts_with("avc1."), "{}", config.codec);
    assert_eq!(config.size, Some(PRETEND_SIZE));
    let Relayed::Picture(first) = next() else {
        panic!("a picture should follow");
    };
    assert!(first.keyframe, "the first picture decodes on its own");
    let types: Vec<u8> = crate::annexb::nal_units(&first.bytes)
        .iter()
        .filter_map(|nal| crate::annexb::nal_type(VideoCodec::H264, nal))
        .collect();
    assert!(
        types.contains(&7) && types.contains(&8),
        "the SPS and PPS go with it: {types:?}"
    );
    let Relayed::Picture(second) = next() else {
        panic!("the stream goes on");
    };
    assert!(second.timestamp_us > first.timestamp_us);
    assert!(session.frames().latest().is_none(), "nothing was decoded");

    session.stop();
    assert_eq!(session.status(), MediaStatus::Stopped);
}

#[cfg(windows)]
#[test]
fn a_relay_decodes_a_picture_when_asked() {
    let camera = pretend_camera();
    let mut session = MediaSession::relay(&camera).expect("the session should start");
    wait_for(&session, |status| *status == MediaStatus::Live);
    let feed = session.feed();

    let deadline = Instant::now() + Duration::from_secs(10);
    let picture = loop {
        match feed.picture() {
            Ok(picture) => break picture,
            Err(_) if Instant::now() < deadline => sleep(Duration::from_millis(50)),
            Err(why) => panic!("no picture after 10 s: {why:#}"),
        }
    };
    let (width, height) = PRETEND_SIZE;
    assert_eq!((picture.width, picture.height), (width, height));
    assert_eq!(picture.bgra.len(), (width * height * 4) as usize);
    assert!(
        picture.bgra.iter().any(|byte| *byte > 32),
        "the picture isn't black"
    );

    let keyframe = feed.keyframe_picture().expect("the keyframe decodes alone");
    assert_eq!((keyframe.width, keyframe.height), (width, height));
    assert!(feed.keyframes() >= 1);
    assert!(
        session.frames().latest().is_none(),
        "the session decoded none"
    );
    session.stop();
}

#[cfg(windows)]
#[test]
fn a_relay_records_into_an_mp4() {
    let folder = tempfile::TempDir::new().expect("a folder for the test");
    let camera = pretend_camera();
    let mut session = MediaSession::relay(&camera).expect("the session should start");
    wait_for(&session, |status| *status == MediaStatus::Live);

    let file = folder.path().join("relayed.mp4");
    record_a_fragment(&session, &file);
    assert_eq!(session.status(), MediaStatus::Live, "the relay carries on");

    let bytes = std::fs::read(&file).expect("the recording should be there");
    let kinds = mp4_boxes(&bytes).unwrap_or_else(|why| panic!("not a whole MP4: {why}"));
    assert_eq!(&kinds[..2], ["ftyp", "moov"], "{kinds:?}");
    assert!(kinds.iter().any(|kind| kind == "moof"), "{kinds:?}");
    session.stop();
}

#[cfg(windows)]
fn record_a_fragment(session: &MediaSession, file: &std::path::Path) {
    use crate::RecordStatus;

    let named = file.to_path_buf();
    session.start_recording(crate::RecordRules::default(), move |_| named.clone());
    let deadline = Instant::now() + Duration::from_secs(15);
    while !boxes_so_far(&std::fs::read(file).unwrap_or_default()).contains(&"mdat".to_string()) {
        assert!(
            Instant::now() < deadline,
            "no fragment within 15 s; the recording is {:?}",
            session.record_status()
        );
        sleep(Duration::from_millis(50));
    }
    assert_eq!(
        session.record_status(),
        RecordStatus::Writing(file.to_path_buf())
    );

    session.stop_recording();
    let deadline = Instant::now() + Duration::from_secs(2);
    while session.record_status() != RecordStatus::Off {
        assert!(Instant::now() < deadline, "the file was not finished");
        sleep(Duration::from_millis(20));
    }
}

#[cfg(windows)]
fn boxes_so_far(bytes: &[u8]) -> Vec<String> {
    let mut boxes = Vec::new();
    let mut at = 0;
    while let Some(header) = bytes.get(at..at + 8) {
        let size = u32::from_be_bytes([header[0], header[1], header[2], header[3]]) as usize;
        if size < 8 || at + size > bytes.len() {
            break;
        }
        boxes.push(String::from_utf8_lossy(&header[4..8]).into_owned());
        at += size;
    }
    boxes
}

#[cfg(windows)]
fn mp4_boxes(bytes: &[u8]) -> Result<Vec<String>, String> {
    let mut boxes = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        let header = bytes
            .get(at..at + 8)
            .ok_or_else(|| format!("a box header is cut off at {at}"))?;
        let size = u32::from_be_bytes([header[0], header[1], header[2], header[3]]) as usize;
        let kind = String::from_utf8_lossy(&header[4..8]).into_owned();
        if size < 8 || at + size > bytes.len() {
            return Err(format!(
                "the {kind} box at {at} runs past the end ({size} bytes)"
            ));
        }
        boxes.push(kind);
        at += size;
    }
    Ok(boxes)
}

#[cfg(windows)]
#[test]
fn records_a_local_h264_stream_into_an_mp4() {
    use crate::RecordStatus;

    let folder = tempfile::TempDir::new().expect("a folder for the test");
    let camera = pretend_camera();
    let mut session = MediaSession::start(&camera).expect("the session should start");
    first_picture(&session);
    assert_eq!(session.record_status(), RecordStatus::Off);

    let file = folder.path().join("clip.mp4");
    record_a_fragment(&session, &file);
    assert_eq!(
        session.status(),
        MediaStatus::Live,
        "the picture carries on"
    );

    let bytes = std::fs::read(&file).expect("the recording should be there");
    let kinds = mp4_boxes(&bytes).unwrap_or_else(|why| panic!("not a whole MP4: {why}"));
    assert_eq!(&kinds[..2], ["ftyp", "moov"], "{kinds:?}");
    let fragments = kinds.iter().filter(|kind| *kind == "moof").count();
    assert!(fragments >= 1, "{kinds:?}");
    assert_eq!(
        fragments,
        kinds.iter().filter(|kind| *kind == "mdat").count(),
        "every fragment has its data"
    );

    if let Ok(keep) = env::var("IPCAMERA_KEEP_RECORDING") {
        std::fs::copy(&file, keep).expect("the recording should copy");
    }
    session.stop();
}

#[test]
fn a_stream_it_cannot_play_stops_at_once_and_says_why() {
    let camera = FakeCamera::start(offer_only_mjpeg);
    let session = MediaSession::start(&camera.setting()).expect("the session should start");

    let status = wait_for(&session, |status| {
        matches!(
            status,
            MediaStatus::Failed(_) | MediaStatus::Reconnecting { .. }
        )
    });

    assert_eq!(
        status,
        MediaStatus::Failed("the camera sends no H.264 or H.265 video".to_string())
    );
    wait_for_the_end(&session);
    assert!(session.refused());
}

#[test]
fn a_rejected_login_stops_at_once_instead_of_retrying() {
    let camera = FakeCamera::start(refuse_every_login);
    let session = MediaSession::start(&camera.setting()).expect("the session should start");

    let status = wait_for(&session, |status| {
        matches!(
            status,
            MediaStatus::Failed(_) | MediaStatus::Reconnecting { .. }
        )
    });

    assert_eq!(
        status,
        MediaStatus::Failed("the camera rejected the username or password".to_string())
    );
    assert!(status.login_rejected());
    assert!(
        !MediaStatus::Failed("the camera sends no H.264 or H.265 video".into()).login_rejected()
    );

    wait_for_the_end(&session);
    assert!(session.refused());
}

fn wait_for_the_end(session: &MediaSession) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !session
        .worker
        .as_ref()
        .is_some_and(|worker| worker.is_finished())
    {
        assert!(
            Instant::now() < deadline,
            "the session still runs: {:?}",
            session.status()
        );
        sleep(Duration::from_millis(10));
    }
}

#[test]
fn reconnecting_gives_up_after_failures_in_a_row() {
    let mut attempt = 0;
    for expected in 1..=MAX_ATTEMPTS {
        attempt = next_attempt(attempt, false, true).expect("it should try again");
        assert_eq!(attempt, expected);
    }

    assert_eq!(
        next_attempt(attempt, false, true),
        None,
        "a camera that never answers is given up on"
    );
}

#[test]
fn a_stream_that_played_starts_the_count_again() {
    assert_eq!(
        next_attempt(MAX_ATTEMPTS, true, true),
        Some(1),
        "a drop after a picture is a new outage, however many came before"
    );
}

#[test]
fn without_auto_reconnect_nothing_tries_again() {
    assert_eq!(next_attempt(0, false, false), None);
    assert_eq!(next_attempt(0, true, false), None);
}

#[test]
fn a_dropped_connection_still_reconnects() {
    let camera = FakeCamera::start(hang_up);
    let session = MediaSession::start(&camera.setting()).expect("the session should start");

    let status = wait_for(&session, |status| {
        matches!(
            status,
            MediaStatus::Failed(_) | MediaStatus::Reconnecting { .. }
        )
    });

    assert_eq!(status, MediaStatus::Reconnecting { attempt: 1 });
}

#[test]
fn stopping_during_the_wait_before_a_retry_returns_at_once() {
    let camera = FakeCamera::start(hang_up);
    let mut session = MediaSession::start(&camera.setting()).expect("the session should start");
    wait_for(&session, |status| {
        matches!(status, MediaStatus::Reconnecting { .. })
    });

    let asked = Instant::now();
    session.stop();

    assert!(
        asked.elapsed() < Duration::from_secs(1),
        "stop waited out the 2 s backoff instead of ending it"
    );
    assert_eq!(session.status(), MediaStatus::Stopped);
}

#[test]
fn a_graph_builds_for_every_offer() {
    let camera = CameraConfig::default();
    let url = rtsp_url::build(&camera);
    let build = |video, audio| {
        pipeline::build(
            pipeline::Source::Rtsp(url.clone()),
            &camera,
            &Offer { video, audio },
            Arc::new(Visibility::default()),
            Arc::new(AtomicBool::new(false)),
            1.0,
            Recorder::new(RecordControl::default()),
        )
        .expect("the graph should build")
        .node_count()
    };

    assert_eq!(build(VideoCodec::H264, None), 8);
    assert_eq!(build(VideoCodec::H264, Some(AudioCodec::Aac)), 15);
    // G.711 needs no parser, and is not recorded.
    assert_eq!(build(VideoCodec::H264, Some(AudioCodec::Alaw)), 13);
    assert_eq!(build(VideoCodec::H264, Some(AudioCodec::Mulaw)), 13);
    #[cfg(windows)]
    assert_eq!(build(VideoCodec::H265, Some(AudioCodec::Aac)), 15);
}

/// Against a real camera: `IPCAMERA_HOST`, `IPCAMERA_USER` and `IPCAMERA_PASSWORD` name it.
#[test]
#[ignore = "needs a real camera on the network"]
fn pulls_frames_from_a_real_camera() {
    let mut camera = CameraConfig::default();
    camera.apply_env();
    println!("camera: {}", camera.stream_label());
    if let Ok(sub) = env::var("IPCAMERA_SUB")
        && let Ok(sub) = sub.parse()
    {
        camera.stream = ipcamera_core::Stream::from_index(sub).unwrap_or_default();
    }
    camera.auto_reconnect = false;

    let mut session = MediaSession::start(&camera).expect("the session should start");

    let deadline = Instant::now() + Duration::from_secs(20);
    let mut first_frame = None;
    while Instant::now() < deadline {
        if let Some(frame) = session.frames().latest()
            && frame.width > 0
            && frame.height > 0
        {
            first_frame = Some((frame.width, frame.height));
            break;
        }
        if let MediaStatus::Failed(message) = session.status() {
            panic!("the camera failed: {message}");
        }
        sleep(Duration::from_millis(100));
    }

    session.stop();

    let (width, height) =
        first_frame.expect("no frame arrived within 20 s — check the address, port and password");
    println!("first frame: {width}x{height}");
    assert!(width > 0 && height > 0);
}

/// The first camera saved in `IPCAMERA_SETTINGS_DIR`; never print its password.
#[test]
#[ignore = "needs a real camera and a saved settings folder"]
fn diagnose_a_saved_camera() {
    use ipcamera_core::Library;

    g2g_core::log::init_from_env();
    let dir = env::var("IPCAMERA_SETTINGS_DIR").expect("set IPCAMERA_SETTINGS_DIR");
    let library = Library::open(dir);
    let mut camera = library
        .cameras()
        .into_iter()
        .next()
        .expect("a saved camera");
    println!(
        "camera: {}, tcp {}, password set {}",
        camera.stream_label(),
        camera.use_tcp,
        !camera.password.is_empty()
    );
    camera.auto_reconnect = false;
    // An FLV stream says what it sends only once open.
    if let StreamPath::Rtsp(_) = camera.stream_path() {
        let offer = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a runtime")
            .block_on(pipeline::describe(&rtsp_url::build(&camera), &camera));
        println!("offer: {offer:?}");
    }

    let started = Instant::now();
    let mut session = MediaSession::start(&camera).expect("the session should start");
    let mut last = None;
    while started.elapsed() < Duration::from_secs(25) {
        let status = session.status();
        if last.as_ref() != Some(&status) {
            println!("{:>6} ms  {status:?}", started.elapsed().as_millis());
            last = Some(status.clone());
        }
        if let Some(frame) = session.frames().latest() {
            println!(
                "{:>6} ms  picture {}x{}",
                started.elapsed().as_millis(),
                frame.width,
                frame.height
            );
            break;
        }
        if matches!(status, MediaStatus::Failed(_)) {
            break;
        }
        sleep(Duration::from_millis(20));
    }

    let seconds: u64 = env::var("IPCAMERA_DIAGNOSE_SECONDS")
        .ok()
        .and_then(|text| text.parse().ok())
        .unwrap_or(0);
    let frames = session.frames();
    let mut produced = frames.produced();
    for _ in 0..seconds / 5 {
        sleep(Duration::from_secs(5));
        let now = frames.produced();
        let mean = frames.latest().map(|frame| {
            let pixels = frame.bgra.len() as u64 / 4;
            let mut sums = [0u64; 3];
            for [blue, green, red, _] in frame.bgra.as_chunks::<4>().0 {
                sums[0] += u64::from(*blue);
                sums[1] += u64::from(*green);
                sums[2] += u64::from(*red);
            }
            sums.map(|sum| sum / pixels.max(1))
        });
        println!(
            "{:>6} ms  {:?}, {:.1} fps, mean B/G/R {mean:?}",
            started.elapsed().as_millis(),
            session.status(),
            (now - produced) as f64 / 5.0
        );
        produced = now;
    }
    session.stop();
}

//! The ONVIF client and a watch over ONVIF, against a fake camera on loopback that knows one
//! account; and, when the owner asks, a real one.

use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use ipcamera_core::{
    Brand, CameraConfig, CameraWatch, Detection, Spotlight,
    onvif::{OnvifClient, OnvifError},
    ptz::{PtzSession, PtzVelocity},
};

const USER: &str = "admin";
const PASSWORD: &str = "s3cret";
const PATH: &str = "/onvif/device_service";

struct FakeCamera {
    port: u16,
    asked: Arc<Mutex<Vec<String>>>,
    bodies: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl FakeCamera {
    /// A camera sending motion on its second pull.
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let port = listener.local_addr().expect("the bound address").port();
        let asked = Arc::new(Mutex::new(Vec::new()));
        let bodies = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let pulls = Arc::new(AtomicUsize::new(0));

        let worker = {
            let (asked, bodies, stop) = (asked.clone(), bodies.clone(), stop.clone());
            thread::spawn(move || {
                for stream in listener.incoming() {
                    if stop.load(Ordering::SeqCst) {
                        break;
                    }
                    if let Ok(stream) = stream {
                        serve(stream, &asked, &bodies, &pulls);
                    }
                }
            })
        };

        Self {
            port,
            asked,
            bodies,
            stop,
            worker: Some(worker),
        }
    }

    fn asked(&self) -> Vec<String> {
        self.asked.lock().unwrap().clone()
    }

    fn setting(&self, user: &str) -> CameraConfig {
        CameraConfig {
            brand: Brand::Dahua,
            host: "127.0.0.1".into(),
            user: user.into(),
            password: PASSWORD.into(),
            onvif_port: self.port,
            onvif_device_path: PATH.into(),
            onvif_events: true,
            ..CameraConfig::default()
        }
    }
}

impl Drop for FakeCamera {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(("127.0.0.1", self.port));
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn between(xml: &str, name: &str) -> Option<String> {
    let open = xml
        .find(&format!(":{name}"))
        .or_else(|| xml.find(&format!("<{name}")))?;
    let start = xml[open..].find('>')? + open + 1;
    let end = xml[start..].find("</")? + start;
    Some(xml[start..end].to_string())
}

const ENVELOPE: &str = r#"<s:Envelope xmlns:s="http://www.w3.org/2003/05/soap-envelope" xmlns:tt="http://www.onvif.org/ver10/schema" xmlns:tds="http://www.onvif.org/ver10/device/wsdl" xmlns:tev="http://www.onvif.org/ver10/events/wsdl" xmlns:wsnt="http://docs.oasis-open.org/wsn/b-2" xmlns:wsa5="http://www.w3.org/2005/08/addressing" xmlns:wstop="http://docs.oasis-open.org/wsn/t-1" xmlns:tns1="http://www.onvif.org/ver10/topics" xmlns:ter="http://www.onvif.org/ver10/error" xmlns:trt="http://www.onvif.org/ver10/media/wsdl" xmlns:tptz="http://www.onvif.org/ver20/ptz/wsdl"><s:Body>"#;

fn motion(state: bool) -> String {
    format!(
        r#"<wsnt:NotificationMessage><wsnt:Topic>tns1:RuleEngine/CellMotionDetector/Motion</wsnt:Topic><wsnt:Message><tt:Message PropertyOperation="Changed"><tt:Source><tt:SimpleItem Name="VideoSourceConfigurationToken" Value="0"/></tt:Source><tt:Data><tt:SimpleItem Name="IsMotion" Value="{state}"/></tt:Data></tt:Message></wsnt:Message></wsnt:NotificationMessage>"#
    )
}

fn serve(
    stream: TcpStream,
    asked: &Mutex<Vec<String>>,
    bodies: &Mutex<Vec<String>>,
    pulls: &AtomicUsize,
) {
    let mut reader = BufReader::new(stream.try_clone().expect("a second handle"));
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).unwrap_or(0) == 0 {
        return;
    }
    let mut length = 0;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
            break;
        }
        if let Some((name, value)) = line.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            length = value.trim().parse().unwrap_or(0);
        }
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).expect("the body");
    let body = String::from_utf8(body).expect("UTF-8");

    let operation = [
        "GetSystemDateAndTime",
        "GetDeviceInformation",
        "GetCapabilities",
        "GetServices",
        "GetEventProperties",
        "CreatePullPointSubscription",
        "PullMessages",
        "Renew",
        "Unsubscribe",
        "GetProfiles",
        "GetVideoEncoderConfigurations",
        "GetStreamUri",
        "GetNodes",
        "ContinuousMove",
        "Stop",
        "SendAuxiliaryCommand",
    ]
    .into_iter()
    .find(|operation| {
        [">", "/>", " "]
            .iter()
            .any(|end| body.contains(&format!(":{operation}{end}")))
    })
    .unwrap_or("unknown")
    .to_string();
    asked.lock().unwrap().push(operation.clone());
    bodies.lock().unwrap().push(body.clone());

    let second_media = body.contains("<tr2:");
    let known = body.contains(&format!(":Username>{USER}</"));
    let answer = if operation != "GetSystemDateAndTime" && !known {
        r#"<s:Fault><s:Code><s:Value>s:Sender</s:Value><s:Subcode><s:Value>ter:NotAuthorized</s:Value></s:Subcode></s:Code><s:Reason><s:Text xml:lang="en">Sender not Authorized</s:Text></s:Reason></s:Fault>"#.to_string()
    } else {
        match operation.as_str() {
            "GetSystemDateAndTime" => "<tds:GetSystemDateAndTimeResponse><tds:SystemDateAndTime><tt:UTCDateTime><tt:Time><tt:Hour>12</tt:Hour><tt:Minute>0</tt:Minute><tt:Second>0</tt:Second></tt:Time><tt:Date><tt:Year>2026</tt:Year><tt:Month>11</tt:Month><tt:Day>10</tt:Day></tt:Date></tt:UTCDateTime></tds:SystemDateAndTime></tds:GetSystemDateAndTimeResponse>".to_string(),
            "GetDeviceInformation" => "<tds:GetDeviceInformationResponse><tds:Manufacturer>Dahua</tds:Manufacturer><tds:Model>IPC-HDW2431T</tds:Model><tds:FirmwareVersion>2.800</tds:FirmwareVersion><tds:SerialNumber>X</tds:SerialNumber><tds:HardwareId>1.00</tds:HardwareId></tds:GetDeviceInformationResponse>".to_string(),
            // A host the camera doesn't answer on: only the path is used.
            "GetCapabilities" => "<tds:GetCapabilitiesResponse><tds:Capabilities><tt:Events><tt:XAddr>http://0.0.0.0:1/onvif/event_service</tt:XAddr><tt:WSPullPointSupport>true</tt:WSPullPointSupport></tt:Events><tt:Media><tt:XAddr>http://0.0.0.0:1/onvif/media_service</tt:XAddr></tt:Media><tt:PTZ><tt:XAddr>http://0.0.0.0:1/onvif/ptz_service</tt:XAddr></tt:PTZ></tds:Capabilities></tds:GetCapabilitiesResponse>".to_string(),
            // The second media service names the sub stream's H.265, which the first can't.
            "GetServices" => r#"<tds:GetServicesResponse><tds:Service><tds:Namespace>http://www.onvif.org/ver10/media/wsdl</tds:Namespace><tds:XAddr>http://0.0.0.0:1/onvif/media_service</tds:XAddr></tds:Service><tds:Service><tds:Namespace>http://www.onvif.org/ver20/media/wsdl</tds:Namespace><tds:XAddr>http://0.0.0.0:1/onvif/media2</tds:XAddr></tds:Service></tds:GetServicesResponse>"#.to_string(),
            "GetProfiles" if second_media => r#"<tr2:GetProfilesResponse xmlns:tr2="http://www.onvif.org/ver20/media/wsdl"><tr2:Profiles token="Profile_2"><tr2:Name>sub</tr2:Name><tr2:Configurations><tr2:VideoEncoder token="e2"><tt:Name>enc</tt:Name></tr2:VideoEncoder></tr2:Configurations></tr2:Profiles><tr2:Profiles token="Profile_1"><tr2:Name>main</tr2:Name></tr2:Profiles></tr2:GetProfilesResponse>"#.to_string(),
            "GetProfiles" => r#"<trt:GetProfilesResponse><trt:Profiles token="Profile_2"><tt:Name>sub</tt:Name><tt:VideoEncoderConfiguration token="e2"><tt:Name>enc</tt:Name></tt:VideoEncoderConfiguration></trt:Profiles><trt:Profiles token="Profile_1"><tt:Name>main</tt:Name><tt:PTZConfiguration token="ptz0"/></trt:Profiles><trt:Profiles token="Profile_3"><tt:Name>none</tt:Name></trt:Profiles></trt:GetProfilesResponse>"#.to_string(),
            "GetVideoEncoderConfigurations" if second_media => r#"<tr2:GetVideoEncoderConfigurationsResponse xmlns:tr2="http://www.onvif.org/ver20/media/wsdl"><tr2:Configurations token="e2"><tt:Name>enc</tt:Name><tt:Encoding>H265</tt:Encoding><tt:Resolution><tt:Width>640</tt:Width><tt:Height>360</tt:Height></tt:Resolution></tr2:Configurations></tr2:GetVideoEncoderConfigurationsResponse>"#.to_string(),
            "GetVideoEncoderConfigurations" => r#"<trt:GetVideoEncoderConfigurationsResponse><trt:Configurations token="e2"><tt:Name>enc</tt:Name><tt:Encoding>H264</tt:Encoding><tt:Resolution><tt:Width>640</tt:Width><tt:Height>360</tt:Height></tt:Resolution></trt:Configurations></trt:GetVideoEncoderConfigurationsResponse>"#.to_string(),
            // No address for the third profile.
            "GetStreamUri" => match between(&body, "ProfileToken").as_deref() {
                Some(token) if token != "Profile_3" => format!("<trt:GetStreamUriResponse><trt:MediaUri><tt:Uri>rtsp://10.9.9.9:554/live/{token}?a=1&amp;b=2</tt:Uri></trt:MediaUri></trt:GetStreamUriResponse>"),
                _ => r#"<s:Fault><s:Code><s:Value>s:Sender</s:Value></s:Code><s:Reason><s:Text>no such stream</s:Text></s:Reason></s:Fault>"#.to_string(),
            },
            "GetNodes" => "<tptz:GetNodesResponse><tptz:PTZNode token=\"n\"><tt:SupportedPTZSpaces><tt:ContinuousPanTiltVelocitySpace><tt:URI>http://www.onvif.org/ver10/tptz/PanTiltSpaces/VelocityGenericSpace</tt:URI></tt:ContinuousPanTiltVelocitySpace></tt:SupportedPTZSpaces><tt:AuxiliaryCommands>tt:IRLamp|On</tt:AuxiliaryCommands><tt:AuxiliaryCommands>tt:Light|On</tt:AuxiliaryCommands><tt:AuxiliaryCommands>tt:Light|Off</tt:AuxiliaryCommands></tptz:PTZNode></tptz:GetNodesResponse>".to_string(),
            "ContinuousMove" => "<tptz:ContinuousMoveResponse/>".to_string(),
            "Stop" => "<tptz:StopResponse/>".to_string(),
            "SendAuxiliaryCommand" => "<tptz:SendAuxiliaryCommandResponse><tptz:AuxiliaryResponse>OK</tptz:AuxiliaryResponse></tptz:SendAuxiliaryCommandResponse>".to_string(),
            "GetEventProperties" => r#"<tev:GetEventPropertiesResponse><wstop:TopicSet><tns1:RuleEngine><CellMotionDetector><Motion wstop:topic="true"/></CellMotionDetector><MyRuleDetector><PeopleDetect wstop:topic="true"/></MyRuleDetector></tns1:RuleEngine></wstop:TopicSet></tev:GetEventPropertiesResponse>"#.to_string(),
            "CreatePullPointSubscription" => "<tev:CreatePullPointSubscriptionResponse><tev:SubscriptionReference><wsa5:Address>http://10.9.9.9/onvif/Subscription?Idx=0</wsa5:Address></tev:SubscriptionReference></tev:CreatePullPointSubscriptionResponse>".to_string(),
            "PullMessages" => {
                let pull = pulls.fetch_add(1, Ordering::SeqCst);
                let messages = if pull == 0 { motion(false) } else { motion(true) };
                format!("<tev:PullMessagesResponse>{messages}</tev:PullMessagesResponse>")
            }
            "Renew" => "<wsnt:RenewResponse/>".to_string(),
            "Unsubscribe" => "<wsnt:UnsubscribeResponse/>".to_string(),
            _ => r#"<s:Fault><s:Code><s:Value>s:Sender</s:Value></s:Code><s:Reason><s:Text>unknown</s:Text></s:Reason></s:Fault>"#.to_string(),
        }
    };
    let text = format!("{ENVELOPE}{answer}</s:Body></s:Envelope>");
    let status = if answer.contains("Fault") {
        "400 Bad Request"
    } else {
        "200 OK"
    };
    let mut stream = stream;
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: application/soap+xml\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
        text.len()
    );
    let _ = stream.flush();
}

#[test]
fn onvif_is_told_apart_without_a_login() {
    let camera = FakeCamera::start();
    assert!(OnvifClient::answers("127.0.0.1", camera.port, PATH));
    assert_eq!(camera.asked(), ["GetSystemDateAndTime"]);

    let closed = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = closed.local_addr().unwrap().port();
    drop(closed);
    assert!(!OnvifClient::answers("127.0.0.1", port, PATH));
}

#[test]
fn detect_leaves_alone_an_onvif_that_refused_this_login_before() {
    let camera = FakeCamera::start();
    let found = ipcamera_core::detect(&CameraConfig {
        brand: Brand::Other,
        onvif_refused: true,
        ..camera.setting(USER)
    });

    let why = found.unwrap_err();
    assert!(why.contains("refused this login before"), "{why}");
    assert!(
        camera
            .asked()
            .iter()
            .all(|operation| operation == "GetSystemDateAndTime"),
        "asked whether it is there, which takes no login, and nothing that does: {:?}",
        camera.asked()
    );
}

#[test]
fn the_streams_offered_are_the_profiles_the_camera_gives_an_address_for() {
    let camera = FakeCamera::start();
    let (mut client, _) =
        OnvifClient::connect("127.0.0.1", camera.port, PATH, USER, PASSWORD).unwrap();
    let media = client.services().unwrap().media.unwrap();

    let streams = client.streams(&media).unwrap();

    let listed: Vec<(&str, &str, &str)> = streams
        .iter()
        .map(|stream| {
            (
                stream.name.as_str(),
                stream.path.as_str(),
                stream.detail.as_str(),
            )
        })
        .collect();
    assert_eq!(
        listed,
        [
            ("sub", "/live/Profile_2?a=1&b=2", "640\u{d7}360, H265"),
            ("main", "/live/Profile_1?a=1&b=2", ""),
        ],
        "its names, its addresses' paths, what it says of each picture (its second \
         media service's H265 over the first's H264); none for the profile \
         it gives no address for"
    );
}

#[test]
fn a_refused_login_is_one_attempt() {
    let camera = FakeCamera::start();

    let refused = OnvifClient::connect("127.0.0.1", camera.port, PATH, "someone", PASSWORD).err();

    assert_eq!(
        refused,
        Some(OnvifError::LoginRejected("Sender not Authorized".into()))
    );
    assert_eq!(
        camera.asked(),
        ["GetSystemDateAndTime", "GetDeviceInformation"],
        "the time, then the one login"
    );
}

#[test]
fn a_watch_over_onvif_subscribes_and_follows_motion_then_unsubscribes() {
    let camera = FakeCamera::start();

    let watch =
        CameraWatch::start(&camera.setting(USER)).expect("ONVIF events on, with a password");
    let deadline = Instant::now() + Duration::from_secs(5);
    while !watch.watched().activity.motion {
        assert!(Instant::now() < deadline, "{:?}", watch.watched());
        thread::sleep(Duration::from_millis(20));
    }
    let watched = watch.watched();
    assert_eq!(watched.device.as_ref().unwrap().model, "Dahua IPC-HDW2431T");
    assert_eq!(
        watched.features.as_ref().unwrap().detections,
        [Detection::Person]
    );
    assert_eq!(
        watched.summary(),
        "Dahua IPC-HDW2431T: Motion, Person, Spotlight, Pan and tilt."
    );

    drop(watch);
    let deadline = Instant::now() + Duration::from_secs(5);
    while !camera.asked().contains(&"Unsubscribe".to_string()) {
        assert!(Instant::now() < deadline, "{:?}", camera.asked());
        thread::sleep(Duration::from_millis(20));
    }
    let asked = camera.asked();
    assert_eq!(
        asked[..7],
        [
            "GetSystemDateAndTime",
            "GetDeviceInformation",
            "GetCapabilities",
            "GetEventProperties",
            "GetNodes",
            "GetProfiles",
            "CreatePullPointSubscription"
        ]
    );
}

#[test]
fn a_camera_of_another_brand_moves_over_onvif_and_its_light_switches() {
    let camera = FakeCamera::start();
    let setting = CameraConfig {
        brand: Brand::Other,
        ..camera.setting(USER)
    };

    let session = PtzSession::connect(&setting);
    session.move_by(PtzVelocity::new(0.5, 0.0, 0.0));
    // A stop replaces a move not sent yet, so the move goes out first.
    let deadline = Instant::now() + Duration::from_secs(5);
    while !camera.asked().contains(&"ContinuousMove".to_string()) {
        assert!(Instant::now() < deadline, "{:?}", camera.asked());
        thread::sleep(Duration::from_millis(20));
    }
    session.stop();
    drop(session);

    let asked = camera.asked();
    assert!(asked.contains(&"ContinuousMove".to_string()), "{asked:?}");
    assert!(asked.contains(&"Stop".to_string()), "{asked:?}");
    let bodies = camera.bodies.lock().unwrap().clone();
    let moved = bodies
        .iter()
        .find(|body| body.contains(":ContinuousMove>"))
        .unwrap();
    assert!(moved.contains(r#":PanTilt x="0.5" y="0""#), "{moved}");
    assert!(moved.contains(r#":Zoom x="0""#), "{moved}");
    assert!(
        moved.contains(":ProfileToken>Profile_1</"),
        "the profile with PTZ"
    );

    let watch = CameraWatch::start(&setting).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !watch
        .watched()
        .features
        .is_some_and(|features| features.spotlight)
    {
        assert!(Instant::now() < deadline, "{:?}", watch.watched());
        thread::sleep(Duration::from_millis(20));
    }
    let on = Spotlight {
        on: true,
        brightness: 100,
    };
    assert_eq!(
        watch.set_spotlight(on).recv_timeout(Duration::from_secs(5)),
        Ok(Ok(()))
    );
    assert_eq!(watch.watched().activity.spotlight, Some(on));
    let bodies = camera.bodies.lock().unwrap().clone();
    assert!(
        bodies
            .iter()
            .any(|body| body.contains(":AuxiliaryData>tt:Light|On</")),
        "the bulb's own command"
    );
}

#[test]
fn a_watch_that_is_refused_stops_for_good() {
    let camera = FakeCamera::start();

    let watch = CameraWatch::start(&camera.setting("someone")).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while watch.watched().problem.is_none() {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(20));
    }
    thread::sleep(Duration::from_millis(300));

    assert!(
        watch
            .watched()
            .problem
            .unwrap()
            .contains("refused the ONVIF login")
    );
    assert_eq!(
        camera.asked(),
        ["GetSystemDateAndTime", "GetDeviceInformation"],
        "no second login"
    );
}

/// Detects and watches the camera `IPCAMERA_ONVIF_HOST`, `_USER` and `_PASSWORD` name, changing
/// nothing.
#[test]
#[ignore = "needs the owner's camera with ONVIF on"]
fn the_real_camera_says_what_it_is_and_sends_its_events() {
    let variable = |name: &str| {
        std::env::var(format!("IPCAMERA_ONVIF_{name}"))
            .ok()
            .filter(|value| !value.is_empty())
    };
    let (Some(host), Some(user), Some(password)) =
        (variable("HOST"), variable("USER"), variable("PASSWORD"))
    else {
        // Never a blank or guessed login: without all three, nothing is sent.
        println!(
            "Set IPCAMERA_ONVIF_HOST, IPCAMERA_ONVIF_USER and IPCAMERA_ONVIF_PASSWORD to run this."
        );
        return;
    };
    let setting = CameraConfig {
        brand: Brand::Other,
        host,
        user,
        password,
        onvif_port: variable("PORT")
            .and_then(|port| port.parse().ok())
            .unwrap_or(80),
        onvif_device_path: variable("PATH").unwrap_or_else(|| PATH.to_string()),
        onvif_events: true,
        ..CameraConfig::default()
    };

    let found = ipcamera_core::detect(&setting);
    println!("detect: {found:?}");
    let setting = CameraConfig {
        onvif_port: found
            .ok()
            .and_then(|found| found.onvif_port)
            .unwrap_or(setting.onvif_port),
        ..setting
    };

    let watch = CameraWatch::start(&setting).unwrap();
    for _ in 0..10 {
        thread::sleep(Duration::from_secs(1));
        let watched = watch.watched();
        println!(
            "{}  motion: {}  sees: {:?}",
            watched.summary(),
            watched.activity.motion,
            watched.activity.detected
        );
    }
}

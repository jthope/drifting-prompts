//! Tests of the camera form: made from a camera's settings and back, its checks, and what Detect
//! fills in.

use super::*;

#[test]
fn a_form_comes_back_as_the_setting_it_was_made_from() {
    let camera = CameraConfig {
        // The form never shows the id; the camera store keeps it.
        id: Default::default(),
        name: "Driveway".into(),
        brand: Brand::Reolink,
        host: "10.0.0.9".into(),
        user: "viewer".into(),
        password: "s3cret".into(),
        rtsp_port: 8554,
        channel: 2,
        stream: Stream::Sub,
        rtsp_path_override: "/live".into(),
        use_tcp: false,
        auto_reconnect: false,
        onvif_port: 8080,
        dvrip_port: 37778,
        onvif_device_path: "/onvif".into(),
        api_port: 8443,
        api_https: false,
        http_api: true,
        onvif_events: true,
        streams: vec![CameraStream {
            name: "main".into(),
            path: "/live".into(),
            detail: "1920×1080, H264".into(),
        }],
        onvif_refused: true,
        log_events: false,
        event_video: Some(EventVideo {
            after_seconds: 60,
            before_seconds: 0,
            longest_minutes: 5,
            motion_continues: false,
            stream: TileStream::Brand(Stream::Sub),
            kinds: vec![Detection::Animal],
        }),
    };

    let back = to_config(&from_config(&camera));

    assert_eq!(from_config(&back), from_config(&camera));
    assert_eq!(back.streams, camera.streams, "the listed streams are kept");
    assert!(back.onvif_refused, "and that its ONVIF refused the login");
    assert!(!back.log_events, "and that its events are off");
    assert_eq!(
        back.event_video, camera.event_video,
        "and its own event videos"
    );
}

#[test]
fn a_camera_follows_the_apps_event_videos_until_the_form_gives_it_its_own() {
    let form = from_config(&CameraConfig::default());
    assert!(!form.own_event_video);
    assert_eq!(
        form.event_video,
        event_video_settings(&EventVideo::default())
    );
    assert_eq!(to_config(&form).event_video, None);

    // Its own, with numbers outside the ranges and a kind this build doesn't know.
    let own = CameraForm {
        own_event_video: true,
        event_video: EventVideoSettings {
            after_seconds: 5,
            before_seconds: 99,
            longest_minutes: 0,
            motion_continues: true,
            stream: String::new(),
            kinds: vec!["vehicle".into(), "dragon".into()],
        },
        ..form
    };
    assert_eq!(
        to_config(&own).event_video,
        Some(EventVideo {
            after_seconds: 10,
            before_seconds: 30,
            longest_minutes: 0,
            motion_continues: true,
            stream: TileStream::Own,
            kinds: vec![Detection::Vehicle],
        })
    );
}

#[test]
fn a_camera_logs_its_events_until_the_form_turns_them_off() {
    let form = from_config(&CameraConfig::default());
    assert!(form.log_events);
    assert!(to_config(&form).log_events);

    let quiet = CameraForm {
        log_events: false,
        ..form
    };
    let camera = to_config(&quiet);
    assert!(!camera.log_events);
    assert!(
        !from_config(&camera).log_events,
        "and the form shows it off again"
    );
}

#[test]
fn blanks_and_bad_numbers_take_the_defaults() {
    let defaults = CameraConfig::default();
    let mut form = from_config(&defaults);
    form.rtsp_port = String::new();
    form.channel = "two".into();

    let camera = to_config(&form);

    assert_eq!(camera.rtsp_port, defaults.rtsp_port);
    assert_eq!(camera.channel, defaults.channel);
}

fn fields_of(form: &CameraForm) -> Vec<String> {
    problems(form)
        .into_iter()
        .map(|problem| problem.field)
        .collect()
}

/// A new camera's form with a name and an address: nothing missing, for a test to spoil.
fn complete() -> CameraForm {
    CameraForm {
        name: "Driveway".into(),
        host: "10.0.0.9".into(),
        ..blank()
    }
}

#[test]
fn a_complete_form_has_no_problems() {
    assert!(problems(&complete()).is_empty());
}

#[test]
fn blank_numbers_are_fine_because_they_take_the_defaults() {
    let mut form = complete();
    form.rtsp_port = String::new();
    form.channel = "  ".into();

    assert!(problems(&form).is_empty());
}

#[test]
fn the_address_and_username_are_required() {
    let mut form = complete();
    form.host = "   ".into();
    form.user = String::new();

    assert_eq!(fields_of(&form), ["host", "user"]);
}

#[test]
fn a_new_camera_needs_a_name_and_an_address_and_has_the_usual_ports() {
    let form = blank();

    assert_eq!(fields_of(&form), ["name", "host"]);
    assert_eq!(
        form,
        CameraForm {
            name: String::new(),
            brand: "dahua".into(),
            host: String::new(),
            user: "admin".into(),
            password: String::new(),
            rtsp_port: "554".into(),
            channel: "1".into(),
            sub_stream: 0,
            rtsp_path: String::new(),
            use_tcp: true,
            auto_reconnect: true,
            onvif_port: "80".into(),
            dvrip_port: "37777".into(),
            onvif_path: "/onvif/device_service".into(),
            api_port: "80".into(),
            api_https: false,
            http_api: false,
            onvif_events: false,
            password_note: String::new(),
            streams: Vec::new(),
            onvif_refused: false,
            log_events: true,
            own_event_video: false,
            event_video: event_video_settings(&EventVideo::default()),
        }
    );
}

#[test]
fn a_new_brand_brings_its_ports_and_keeps_what_was_typed() {
    let mut form = complete();
    form.dvrip_port = "37778".into();

    let reolink = with_brand(&form, "reolink");

    assert_eq!(reolink.brand, "reolink");
    assert_eq!(reolink.onvif_port, "8000", "Reolink's own");
    assert_eq!(
        (reolink.api_port.as_str(), reolink.api_https),
        ("443", true)
    );
    assert_eq!(reolink.dvrip_port, "37778", "typed, so kept");
    assert_eq!(reolink.host, form.host);
    assert_eq!(to_config(&reolink).brand, Brand::Reolink);

    let back = with_brand(&reolink, "dahua");
    assert_eq!(back.onvif_port, "80");
    assert_eq!((back.api_port.as_str(), back.api_https), ("80", false));
    assert_eq!(
        with_brand(&form, "hikvision"),
        form,
        "an unknown brand changes nothing"
    );
}

#[test]
fn detect_gives_the_form_the_streams_the_camera_listed_and_keeps_them_when_it_lists_none() {
    let stream = |name: &str, path: &str| CameraStream {
        name: name.into(),
        path: path.into(),
        detail: String::new(),
    };
    let listed = [stream("small", "/live/2"), stream("main", "/live/1")];
    let found = |form: &CameraForm, streams| {
        detected(
            form,
            Found {
                brand: Brand::Other,
                api: None,
                http_api: false,
                device: None,
                features: None,
                asked_with_password: true,
                onvif_events: false,
                onvif_port: None,
                onvif_problem: None,
                streams,
                onvif_refused: false,
            },
        )
        .form
    };

    let with_streams = found(&complete(), &listed);
    assert_eq!(with_streams.streams.len(), 2);
    assert_eq!(
        with_streams.rtsp_path, "/live/2",
        "none is at the path the app would build, so the first the camera lists"
    );

    // The one at the path that has played so far, whatever its place in the list.
    let built = rtsp_url::path_for(Brand::Other, 1, Stream::Main);
    let known = [stream("small", "/live/2"), stream("main", &built)];
    assert_eq!(found(&complete(), &known).rtsp_path, built);

    // A path typed by hand stays.
    let typed = CameraForm {
        rtsp_path: "/own".into(),
        ..complete()
    };
    assert_eq!(found(&typed, &listed).rtsp_path, "/own");

    // Asked again without a login, the camera lists none: those it listed before stay.
    assert_eq!(found(&with_streams, &[]).streams, with_streams.streams);
}

#[test]
fn detect_fills_in_the_brand_and_the_api_and_says_what_it_found() {
    let form = complete();
    let device = DeviceInfo {
        model: "RLC-811A".into(),
        firmware: "v3.1.0".into(),
        hardware: String::new(),
    };
    let features = CameraFeatures {
        motion: true,
        detections: vec![Detection::Person],
        spotlight: true,
        ..CameraFeatures::default()
    };

    let found = detected(
        &form,
        Found {
            brand: Brand::Reolink,
            api: Some((80, false)),
            http_api: false,
            device: Some(&device),
            features: Some(&features),
            asked_with_password: true,
            onvif_events: false,
            onvif_port: None,
            onvif_problem: None,
            streams: &[],
            onvif_refused: false,
        },
    );

    assert_eq!(found.form.brand, "reolink");
    assert_eq!(found.form.api_port, "80");
    assert!(!found.form.api_https);
    assert_eq!(found.summary, "Reolink RLC-811A, firmware v3.1.0.");
    assert_eq!(found.features, ["Motion", "Person", "Spotlight"]);

    let nothing = |brand| Found {
        brand,
        api: None,
        http_api: false,
        device: None,
        features: None,
        asked_with_password: false,
        onvif_events: false,
        onvif_port: None,
        onvif_problem: None,
        streams: &[],
        onvif_refused: false,
    };
    let without = detected(
        &form,
        Found {
            api: Some((443, true)),
            ..nothing(Brand::Reolink)
        },
    );
    assert!(
        without.summary.contains("Enter its password"),
        "{}",
        without.summary
    );
    let dahua = detected(&form, nothing(Brand::Dahua));
    assert_eq!(dahua.form.brand, "dahua");
    assert!(dahua.features.is_empty());
    assert!(!dahua.form.onvif_events);
    assert!(!dahua.form.http_api, "PTZ stays on DVRIP");
    assert!(
        dahua.summary.contains("DVRIP port answers"),
        "{}",
        dahua.summary
    );

    // An Amcrest light bulb camera: no DVRIP, Dahua's HTTP API on port 80.
    let model = DeviceInfo {
        model: "AL-L411W".into(),
        firmware: "2.800".into(),
        hardware: String::new(),
    };
    let bulb = detected(
        &form,
        Found {
            api: Some((80, false)),
            http_api: true,
            device: Some(&model),
            features: Some(&CameraFeatures {
                spotlight: true,
                pan_tilt: true,
                ..CameraFeatures::default()
            }),
            asked_with_password: true,
            ..nothing(Brand::Dahua)
        },
    );
    assert!(bulb.form.http_api, "PTZ and the light over the API");
    assert_eq!(
        (bulb.form.api_port.as_str(), bulb.form.api_https),
        ("80", false)
    );
    assert_eq!(bulb.summary, "Dahua / Amcrest AL-L411W, firmware 2.800.");
    assert_eq!(bulb.features, ["Spotlight", "Pan and tilt"]);
    let unasked = detected(
        &form,
        Found {
            api: Some((80, false)),
            http_api: true,
            ..nothing(Brand::Dahua)
        },
    );
    assert!(
        unasked.summary.contains("HTTP API answers"),
        "{}",
        unasked.summary
    );

    let onvif = DeviceInfo {
        model: "Dahua IPC-HDW2431T".into(),
        firmware: "2.800".into(),
        hardware: String::new(),
    };
    let watched = detected(
        &form,
        Found {
            device: Some(&onvif),
            features: Some(&CameraFeatures {
                motion: true,
                ..CameraFeatures::default()
            }),
            asked_with_password: true,
            onvif_events: true,
            ..nothing(Brand::Dahua)
        },
    );
    assert!(
        watched.form.onvif_events,
        "motion over ONVIF turns watching on"
    );
    assert_eq!(
        watched.summary, "Dahua IPC-HDW2431T, firmware 2.800.",
        "its maker once"
    );

    let bulb = detected(
        &form,
        Found {
            onvif_port: Some(8899),
            ..nothing(Brand::Other)
        },
    );
    assert_eq!(bulb.form.brand, "other");
    assert_eq!(bulb.form.onvif_port, "8899", "where ONVIF answered");
    assert!(bulb.summary.contains("on port 8899"), "{}", bulb.summary);

    let quiet = detected(
        &form,
        Found {
            asked_with_password: true,
            onvif_problem: Some("it refused"),
            streams: &[],
            onvif_refused: false,
            ..nothing(Brand::Dahua)
        },
    );
    assert_eq!(
        quiet.summary,
        "A Dahua camera: its DVRIP port answers, and PTZ goes over it. Its ONVIF service, \
         which is optional, didn't help: it refused"
    );
}

#[test]
fn every_brand_says_the_path_a_blank_override_stands_for() {
    let paths: Vec<_> = brands().into_iter().map(|brand| brand.rtsp_path).collect();
    assert_eq!(
        paths,
        [
            "/cam/realmonitor?channel=1&subtype=0",
            "/Preview_01_main",
            "/cam/realmonitor?channel=1&subtype=0"
        ]
    );
}

#[test]
fn a_port_must_be_a_real_port() {
    for bad in ["abc", "0", "70000", "-1", "5.5"] {
        let mut form = complete();
        form.dvrip_port = bad.into();

        assert_eq!(fields_of(&form), ["dvripPort"], "{bad} should be refused");
    }
}

#[test]
fn a_channel_starts_at_one() {
    let mut form = complete();
    form.channel = "0".into();

    assert_eq!(fields_of(&form), ["channel"]);
}

#[test]
fn every_bad_field_is_reported_at_once() {
    let mut form = complete();
    form.rtsp_port = "x".into();
    form.onvif_port = "x".into();
    form.api_port = "x".into();
    form.channel = "x".into();

    assert_eq!(
        fields_of(&form),
        ["rtspPort", "onvifPort", "apiPort", "channel"]
    );
}

#[test]
fn text_is_trimmed_but_the_password_is_kept_exactly() {
    let mut form = from_config(&CameraConfig::default());
    form.host = "  10.0.0.9 ".into();
    form.password = " spaced ".into();

    let camera = to_config(&form);

    assert_eq!(camera.host, "10.0.0.9");
    assert_eq!(camera.password, " spaced ");
}

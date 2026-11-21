//! Detect: which camera is at an address. Each protocol is asked without a login, then logged into
//! once with a password; nothing retries a login, since the camera counts each refusal against
//! the account.

use std::{
    net::{TcpStream, ToSocketAddrs},
    thread,
    time::Duration,
};

use serde_json::{Value, json};

use crate::{
    Brand, CameraConfig, CameraFeatures, CameraStream, DeviceInfo,
    camera_watch::{OnvifCamera, learn_onvif},
    dahua_http::{DahuaHttp, DahuaHttpError},
    onvif::{OnvifClient, OnvifError},
    ptz::{DvripClient, DvripError},
    reolink::ReolinkClient,
};

const PORT_PROBE: Duration = Duration::from_secs(3);

const ONVIF_PORTS: [u16; 6] = [80, 8000, 8899, 2020, 5000, 8080];

const OTHER_PORTS: [u16; 10] = [81, 88, 8001, 8008, 8081, 8088, 8443, 8888, 9000, 10080];

const REFUSED_BEFORE: &str = "It refused this login before, so Detect didn't ask it again. Type \
                              the password again to have it asked.";

fn onvif_streams(camera: &CameraConfig) -> Result<Vec<CameraStream>, OnvifError> {
    let (mut client, _) = OnvifClient::connect(
        &camera.host,
        camera.onvif_port,
        &camera.onvif_device_path,
        &camera.user,
        &camera.password,
    )?;
    let Some(media) = client.services()?.media else {
        return Ok(Vec::new());
    };
    match client.streams(&media) {
        Err(error @ OnvifError::LoginRejected(_)) => Err(error),
        listed => Ok(listed.unwrap_or_default()),
    }
}

/// What [`detect`] found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Detected {
    /// Who made the camera.
    pub brand: Brand,
    /// Where its HTTP API answered: the port, and whether it is HTTPS.
    pub api: Option<(u16, bool)>,
    /// For a Dahua: whether its PTZ and light go over that API.
    pub http_api: bool,
    /// What the camera says it is, when asked with a login.
    pub device: Option<DeviceInfo>,
    /// What it can do, when asked with a login.
    pub features: Option<CameraFeatures>,
    /// Whether ONVIF reports motion or has a light, for a camera that isn't a Reolink.
    pub onvif_events: bool,
    /// The port ONVIF answered on.
    pub onvif_port: Option<u16>,
    /// Why ONVIF told nothing of a camera found another way.
    pub onvif_problem: Option<String>,
    /// The streams the camera listed, when asked with a login.
    pub streams: Vec<CameraStream>,
    /// Whether ONVIF refused a login that works another way, now or before.
    pub onvif_refused: bool,
}

/// Tells which camera is at `camera.host`, blocking for a few seconds. Logs in at most once per
/// protocol, and never to an ONVIF that refused this login before (`camera.onvif_refused`).
pub fn detect(camera: &CameraConfig) -> Result<Detected, String> {
    let host = camera.host.trim();
    if host.is_empty() {
        return Err("Enter the camera's address first.".to_string());
    }

    let reolink_ports = unique([
        (camera.api_port, camera.api_https),
        (443, true),
        (80, false),
    ]);
    let dahua_ports = unique([
        (camera.api_port, camera.api_https),
        (80, false),
        (443, true),
    ]);
    let onvif_ports = unique([camera.onvif_port].into_iter().chain(ONVIF_PORTS));
    let heard = listen(camera, host, &reolink_ports, &dahua_ports, &onvif_ports);

    if let Some((port, https)) = heard.reolink {
        let mut found = Detected {
            brand: Brand::Reolink,
            api: Some((port, https)),
            http_api: false,
            device: None,
            features: None,
            onvif_events: false,
            onvif_port: None,
            onvif_problem: None,
            streams: Vec::new(),
            onvif_refused: false,
        };
        if !camera.password.is_empty() {
            let mut client =
                ReolinkClient::connect(host, port, https, &camera.user, &camera.password)
                    .map_err(|error| error.to_string())?;
            found.device = Some(client.device_info().map_err(|error| error.to_string())?);
            found.features = Some(
                client
                    .features(camera.channel)
                    .map_err(|error| error.to_string())?,
            );
            // ONVIF's streams first, since it names them; the API's add what ONVIF didn't list.
            let own = client.streams(camera.channel).unwrap_or_default();
            if let Some(onvif_port) = heard.onvif {
                let at_port = CameraConfig {
                    onvif_port,
                    ..camera.clone()
                };
                if camera.onvif_refused {
                    found.onvif_refused = true;
                    found.onvif_problem = Some(REFUSED_BEFORE.to_string());
                } else {
                    match onvif_streams(&at_port) {
                        Ok(named) => found.streams = named,
                        Err(error @ OnvifError::LoginRejected(_)) => {
                            found.onvif_refused = true;
                            found.onvif_problem = Some(error.to_string());
                        }
                        Err(_) => {}
                    }
                }
            }
            CameraStream::merge(&mut found.streams, own);
            let over_flv = client
                .flv_streams(camera.channel, &found.streams)
                .unwrap_or_default();
            CameraStream::merge(&mut found.streams, over_flv);
        }
        return Ok(found);
    }

    let Heard {
        dvrip,
        dahua: http,
        onvif: onvif_port,
        open,
        ..
    } = heard;
    if !dvrip && http.is_none() && onvif_port.is_none() {
        return Err(nothing_answered(camera, host, &open));
    }

    let mut found = Detected {
        brand: if dvrip || http.is_some() {
            Brand::Dahua
        } else {
            Brand::Other
        },
        api: http,
        // Without a login, only a closed DVRIP port says the API is the way in.
        http_api: !dvrip && http.is_some(),
        device: None,
        features: None,
        onvif_events: false,
        onvif_port,
        onvif_problem: None,
        streams: Vec::new(),
        onvif_refused: false,
    };
    if camera.password.is_empty() {
        return Ok(found);
    }

    if dvrip {
        let (device, features) = dahua_details(camera)?;
        found.device = Some(device);
        found.features = Some(features);
    }
    if let Some((port, https)) = http {
        match http_details(camera, port, https) {
            Ok((device, features)) => {
                found.http_api = !dvrip || features.spotlight;
                found.device.get_or_insert(device);
                found.features = Some(match found.features.take() {
                    Some(over_dvrip) => CameraFeatures {
                        spotlight: features.spotlight,
                        ..over_dvrip
                    },
                    None => features,
                });
            }
            Err(error @ DahuaHttpError::LoginRejected(_)) => return Err(error.to_string()),
            // Not Dahua's API after all: DVRIP or ONVIF says the rest.
            Err(error) => {
                found.api = None;
                found.http_api = false;
                if !dvrip {
                    if onvif_port.is_none() {
                        return Err(error.to_string());
                    }
                    found.brand = Brand::Other;
                }
            }
        }
    }
    if let Some(port) = onvif_port {
        let at_port = CameraConfig {
            onvif_port: port,
            ..camera.clone()
        };
        let learned = if camera.onvif_refused {
            Err(None)
        } else {
            learn_onvif(&at_port).map_err(Some)
        };
        match learned {
            Ok(OnvifCamera {
                mut client,
                device,
                features,
                media,
                ..
            }) => {
                if let Some(media) = &media {
                    found.streams = client.streams(media).unwrap_or_default();
                }
                // A light Dahua's API switches stays there; motion still comes over ONVIF.
                let light_over_onvif = features.spotlight && !found.http_api;
                found.onvif_events = features.motion || light_over_onvif;
                found.device.get_or_insert(device);
                found.features = Some(match found.features.take() {
                    // A Dahua moves over DVRIP or its API, whatever its ONVIF says of PTZ.
                    Some(dahua) => CameraFeatures {
                        pan_tilt: dahua.pan_tilt,
                        zoom: dahua.zoom,
                        spotlight: dahua.spotlight || light_over_onvif,
                        ..features
                    },
                    None => features,
                });
            }
            // Not asked: it refused this login before.
            Err(None) if found.brand == Brand::Dahua => {
                found.onvif_refused = true;
                found.onvif_problem = Some(REFUSED_BEFORE.to_string());
            }
            Err(None) => return Err(format!("ONVIF: {REFUSED_BEFORE}")),
            // DVRIP or Dahua's API already found the camera: ONVIF's failure is only a note.
            Err(Some(error)) if found.brand == Brand::Dahua => {
                found.onvif_refused = matches!(error, OnvifError::LoginRejected(_));
                found.onvif_problem = Some(error.to_string());
            }
            Err(Some(error)) => return Err(error.to_string()),
        }
    }
    Ok(found)
}

/// Where each protocol answered first, in the order its ports were listed, and the open ports.
struct Heard {
    reolink: Option<(u16, bool)>,
    dvrip: bool,
    dahua: Option<(u16, bool)>,
    onvif: Option<u16>,
    open: Vec<u16>,
}

/// Asks every port at once, so a port that drops what it won't answer, or a closed local port the
/// system takes seconds to refuse, costs one wait rather than one each.
fn listen(
    camera: &CameraConfig,
    host: &str,
    reolink: &[(u16, bool)],
    dahua: &[(u16, bool)],
    onvif: &[u16],
) -> Heard {
    fn first<T: Copy>(asked: Vec<(T, thread::ScopedJoinHandle<'_, bool>)>) -> Option<T> {
        let answered: Vec<(T, bool)> = asked
            .into_iter()
            .map(|(port, answer)| (port, answer.join().unwrap_or(false)))
            .collect();
        answered
            .into_iter()
            .find(|(_, answered)| *answered)
            .map(|(port, _)| port)
    }

    let listed = unique(
        reolink
            .iter()
            .chain(dahua)
            .map(|(port, _)| *port)
            .chain(onvif.iter().copied()),
    );
    let candidates = unique(
        listed
            .iter()
            .copied()
            .chain([camera.rtsp_port, camera.dvrip_port])
            .chain(OTHER_PORTS),
    );
    let open: Vec<u16> = thread::scope(|scope| {
        let asked: Vec<_> = candidates
            .iter()
            .map(|&port| (port, scope.spawn(move || port_open(host, port))))
            .collect();
        asked
            .into_iter()
            .filter_map(|(port, answer)| answer.join().unwrap_or(false).then_some(port))
            .collect()
    });

    // Neither RTSP nor DVRIP speaks HTTP.
    let others: Vec<u16> = open
        .iter()
        .copied()
        .filter(|port| {
            !listed.contains(port) && *port != camera.rtsp_port && *port != camera.dvrip_port
        })
        .collect();
    let https = |port: u16| port == 443 || port == 8443;
    let reolink: Vec<(u16, bool)> = reolink
        .iter()
        .copied()
        .filter(|(port, _)| open.contains(port))
        .collect();
    let dahua: Vec<(u16, bool)> = dahua
        .iter()
        .copied()
        .filter(|(port, _)| open.contains(port))
        .chain(others.iter().map(|&port| (port, https(port))))
        .collect();
    let onvif: Vec<u16> = onvif
        .iter()
        .copied()
        .filter(|port| open.contains(port))
        .chain(others.iter().copied())
        .collect();

    let path = camera.onvif_device_path.as_str();
    thread::scope(|scope| {
        let reolink = reolink
            .iter()
            .map(|&(port, https)| {
                let answer = scope.spawn(move || ReolinkClient::answers(host, port, https));
                ((port, https), answer)
            })
            .collect();
        let dahua = dahua
            .iter()
            .map(|&(port, https)| {
                let answer = scope.spawn(move || DahuaHttp::answers(host, port, https));
                ((port, https), answer)
            })
            .collect();
        let onvif = onvif
            .iter()
            .map(|&port| {
                let answer = scope.spawn(move || OnvifClient::answers(host, port, path));
                (port, answer)
            })
            .collect();
        Heard {
            reolink: first(reolink),
            dvrip: open.contains(&camera.dvrip_port),
            dahua: first(dahua),
            onvif: first(onvif),
            open: open.clone(),
        }
    })
}

fn nothing_answered(camera: &CameraConfig, host: &str, open: &[u16]) -> String {
    let asked = "Reolink's or Dahua's HTTP API, Dahua's DVRIP or ONVIF";
    let others: Vec<u16> = open
        .iter()
        .copied()
        .filter(|port| *port != camera.rtsp_port)
        .collect();
    if open.is_empty() {
        format!(
            "Nothing answered at {host}, not even its video. Check the address and that the \
             camera is on."
        )
    } else if others.is_empty() {
        format!(
            "Only the camera's video (RTSP, port {}) answers at {host}: no port a camera keeps \
             its API on is open, so the app can't reach its PTZ or light. They may go only \
             through its maker's app, or need local access turned on there.",
            camera.rtsp_port
        )
    } else {
        format!(
            "Nothing at {host} speaks {asked}, though port{} {} {} open. Pick the brand \
             yourself, or turn ONVIF on in the camera's own app if it has it.",
            if others.len() == 1 { "" } else { "s" },
            join(&others),
            if others.len() == 1 { "is" } else { "are" },
        )
    }
}

fn port_open(host: &str, port: u16) -> bool {
    (host, port)
        .to_socket_addrs()
        .ok()
        .and_then(|mut addresses| addresses.next())
        .is_some_and(|address| TcpStream::connect_timeout(&address, PORT_PROBE).is_ok())
}

fn unique<T: PartialEq>(items: impl IntoIterator<Item = T>) -> Vec<T> {
    let mut kept = Vec::new();
    for item in items {
        if !kept.contains(&item) {
            kept.push(item);
        }
    }
    kept
}

fn join(ports: &[u16]) -> String {
    ports
        .iter()
        .map(u16::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

fn http_details(
    camera: &CameraConfig,
    port: u16,
    https: bool,
) -> Result<(DeviceInfo, CameraFeatures), DahuaHttpError> {
    let (mut client, device) = DahuaHttp::connect(
        camera.host.trim(),
        port,
        https,
        &camera.user,
        &camera.password,
    )?;
    let caps = client.ptz_caps(camera.channel)?;
    let light = client.light(camera.channel)?;
    Ok((
        device,
        CameraFeatures {
            pan_tilt: caps.is_some_and(|caps| caps.pan_tilt),
            zoom: caps.is_some_and(|caps| caps.zoom),
            spotlight: light.is_some(),
            ..CameraFeatures::default()
        },
    ))
}

fn dahua_details(camera: &CameraConfig) -> Result<(DeviceInfo, CameraFeatures), String> {
    let mut client = DvripClient::connect(
        camera.host.trim(),
        camera.dvrip_port,
        &camera.user,
        &camera.password,
    )
    .map_err(|error| match error {
        DvripError::LoginRejected(reason) => {
            format!("The camera refused the login ({reason}). Check the username and password.")
        }
        other => format!("The camera's DVRIP port didn't answer the login ({other})."),
    })?;
    let text = |reply: Option<Value>, find: fn(&Value) -> &Value| {
        reply
            .as_ref()
            .and_then(|reply| find(reply).as_str())
            .unwrap_or_default()
            .trim()
            .to_string()
    };
    let device = DeviceInfo {
        model: text(
            client.call("magicBox.getDeviceType", None, None).ok(),
            |reply| &reply["params"]["type"],
        ),
        firmware: text(
            client.call("magicBox.getSoftwareVersion", None, None).ok(),
            |reply| &reply["params"]["version"]["Version"],
        ),
        hardware: String::new(),
    };

    // The API counts channels from 0; the camera's settings, from 1.
    let object = client
        .call(
            "ptz.factory.instance",
            Some(json!({ "channel": camera.channel.max(1) - 1 })),
            None,
        )
        .ok()
        .and_then(|reply| reply["result"].as_i64())
        .filter(|object| *object != 0);
    let caps = object
        .and_then(|object| {
            client
                .call("ptz.getCurrentProtocolCaps", None, Some(object))
                .ok()
        })
        .map(|reply| reply["params"]["caps"].clone())
        .unwrap_or_default();
    // "Tile" is how the firmware spells tilt.
    let has = |name: &str| caps[name].as_bool() == Some(true);
    Ok((
        device,
        CameraFeatures {
            pan_tilt: has("Pan") || has("Tile"),
            zoom: has("Zoom"),
            ..CameraFeatures::default()
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_needs_an_address() {
        assert!(detect(&CameraConfig::default()).is_err());
    }

    #[test]
    fn finding_nothing_says_what_was_open() {
        let camera = CameraConfig::default();
        let host = "192.168.1.63";
        assert!(nothing_answered(&camera, host, &[]).contains("not even its video"));
        let video_only = nothing_answered(&camera, host, &[554]);
        assert!(
            video_only.starts_with("Only the camera's video (RTSP, port 554)"),
            "{video_only}"
        );
        let other = nothing_answered(&camera, host, &[554, 8888]);
        assert!(other.contains("though port 8888 is open"), "{other}");
    }
}

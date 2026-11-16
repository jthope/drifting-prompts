//! What stops the camera settings form being saved, and what stops changes being given to several
//! cameras at once. Plain checks of the form's text, shared by the desktop's cameras and the web
//! build, so both refuse the same forms and the same changes. The sentences are `ipcamera_core`'s,
//! which the library checks the saved settings with too, so the server says the same.

use ipcamera_core::{Brand, InvalidCamera, StreamPath};

use crate::bridge::{CameraChanges, CameraForm, FormProblem};

/// What stops the form being saved, field by field.
///
/// A blank number is fine, because saving gives it the default. Anything else that is not a usable
/// number is reported here rather than quietly replaced by that default, which would save something
/// other than what was typed.
pub fn problems(form: &CameraForm) -> Vec<FormProblem> {
    let mut found = Vec::new();
    let mut report = |field: &str, problem: InvalidCamera| {
        found.push(FormProblem {
            field: field.to_string(),
            message: problem.message().to_string(),
        });
    };

    if form.name.trim().is_empty() {
        report("name", InvalidCamera::NoName);
    }
    if form.host.trim().is_empty() {
        report("host", InvalidCamera::NoAddress);
    }
    if form.user.trim().is_empty() {
        report("user", InvalidCamera::NoUser);
    }

    let ports = [
        ("rtspPort", &form.rtsp_port),
        ("onvifPort", &form.onvif_port),
        ("dvripPort", &form.dvrip_port),
        ("apiPort", &form.api_port),
    ];
    for (field, text) in ports {
        if !blank_or(text, |port: u16| port > 0) {
            report(field, InvalidCamera::BadPort);
        }
    }

    if !blank_or(&form.channel, |channel: u32| channel >= 1) {
        report("channel", InvalidCamera::BadChannel);
    }

    if matches!(StreamPath::of(&form.rtsp_path), StreamPath::ReolinkFlv(_))
        && Brand::from_key(&form.brand) != Some(Brand::Reolink)
    {
        report("rtspPath", InvalidCamera::FlvNotReolink);
    }

    found
}

/// What stops `changes` being given to several cameras, field by field, with the same messages as
/// [`problems`]. Only what the changes set is checked: what they leave out stays as each camera
/// has it, which was checked when it was saved.
pub fn change_problems(changes: &CameraChanges) -> Vec<FormProblem> {
    // A form with nothing wrong (blank numbers take the defaults), so every problem is a change's.
    // A Reolink's, which an flv: path suits: each camera's own brand is checked as it is saved.
    let mut form = CameraForm {
        name: "Camera".into(),
        brand: "reolink".into(),
        host: "camera".into(),
        user: "admin".into(),
        password: String::new(),
        rtsp_port: String::new(),
        channel: String::new(),
        sub_stream: 0,
        rtsp_path: String::new(),
        use_tcp: true,
        auto_reconnect: true,
        onvif_port: String::new(),
        dvrip_port: String::new(),
        onvif_path: String::new(),
        api_port: String::new(),
        api_https: false,
        http_api: false,
        onvif_events: false,
        password_note: String::new(),
        streams: Vec::new(),
        onvif_refused: false,
        log_events: true,
        own_event_video: false,
        event_video: crate::form::event_video_settings(&ipcamera_core::EventVideo::default()),
    };
    changes.apply_to(&mut form);
    problems(&form)
}

impl CameraChanges {
    /// Writes what the changes set into `form`, leaving the rest as it is.
    pub fn apply_to(&self, form: &mut CameraForm) {
        fn set<T: Clone>(field: &mut T, change: &Option<T>) {
            if let Some(value) = change {
                *field = value.clone();
            }
        }
        set(&mut form.host, &self.host);
        set(&mut form.user, &self.user);
        set(&mut form.password, &self.password);
        set(&mut form.rtsp_port, &self.rtsp_port);
        set(&mut form.sub_stream, &self.sub_stream);
        set(&mut form.rtsp_path, &self.rtsp_path);
        set(&mut form.use_tcp, &self.use_tcp);
        set(&mut form.auto_reconnect, &self.auto_reconnect);
        set(&mut form.onvif_port, &self.onvif_port);
        set(&mut form.dvrip_port, &self.dvrip_port);
        set(&mut form.onvif_path, &self.onvif_path);
    }
}

/// Whether `text` is blank, or a number that `valid` accepts.
fn blank_or<T: std::str::FromStr>(text: &str, valid: impl Fn(T) -> bool) -> bool {
    let text = text.trim();
    text.is_empty() || text.parse().is_ok_and(valid)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields_of(changes: &CameraChanges) -> Vec<String> {
        change_problems(changes)
            .into_iter()
            .map(|problem| problem.field)
            .collect()
    }

    fn driveway() -> CameraForm {
        CameraForm {
            name: "Driveway".into(),
            brand: "dahua".into(),
            host: "10.0.0.9".into(),
            user: "viewer".into(),
            password: "s3cret".into(),
            rtsp_port: "554".into(),
            channel: "2".into(),
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
            event_video: crate::form::event_video_settings(&ipcamera_core::EventVideo::default()),
        }
    }

    #[test]
    fn changes_set_what_they_name_and_leave_the_rest() {
        let mut form = driveway();
        CameraChanges {
            user: Some("admin".into()),
            sub_stream: Some(1),
            use_tcp: Some(false),
            ..CameraChanges::default()
        }
        .apply_to(&mut form);

        assert_eq!(
            form,
            CameraForm {
                user: "admin".into(),
                sub_stream: 1,
                use_tcp: false,
                ..driveway()
            }
        );
    }

    #[test]
    fn no_changes_leave_the_form_as_it_was() {
        let mut form = driveway();
        CameraChanges::default().apply_to(&mut form);

        assert_eq!(form, driveway());
        assert!(change_problems(&CameraChanges::default()).is_empty());
    }

    #[test]
    fn only_the_changes_are_checked_with_the_forms_messages() {
        let changes = CameraChanges {
            host: Some("  ".into()),
            dvrip_port: Some("70000".into()),
            // A blank port takes the default, as on the form.
            rtsp_port: Some(String::new()),
            ..CameraChanges::default()
        };

        assert_eq!(fields_of(&changes), ["host", "dvripPort"]);
        assert_eq!(
            change_problems(&changes)[1].message,
            "Use a port from 1 to 65535."
        );
    }

    #[test]
    fn only_a_reolink_takes_an_flv_stream() {
        let flv = CameraForm {
            rtsp_path: "flv:channel0_ext.bcs".into(),
            ..driveway()
        };
        let refused = problems(&flv);
        assert_eq!(refused.len(), 1);
        assert_eq!(refused[0].field, "rtspPath");
        assert_eq!(refused[0].message, InvalidCamera::FlvNotReolink.message());

        let reolink = CameraForm {
            brand: "reolink".into(),
            ..flv
        };
        assert!(problems(&reolink).is_empty());
        let several = CameraChanges {
            rtsp_path: Some("flv:channel0_ext.bcs".into()),
            ..CameraChanges::default()
        };
        assert!(
            fields_of(&several).is_empty(),
            "each camera's brand is checked as it is saved"
        );
    }
}

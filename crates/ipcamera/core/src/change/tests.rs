//! Tests of [`Change`], each applied to a library in memory.

use super::*;
use crate::{camera_config::InvalidCamera, library::Snapshot};

fn porch() -> CameraConfig {
    CameraConfig {
        name: "Porch".into(),
        host: "10.0.0.7".into(),
        user: "admin".into(),
        password: "s3cret".into(),
        ..CameraConfig::default()
    }
}

fn library() -> (Library, CameraId) {
    let library = Library::in_memory(vec![porch()]);
    let id = library.cameras()[0].id.clone();
    (library, id)
}

fn added(applied: Result<Applied, StoreError>) -> String {
    applied.expect("the change is made").added_id()
}

fn refusal(applied: Result<Applied, StoreError>) -> String {
    applied.expect_err("the change is refused").to_string()
}

#[test]
fn cameras_are_added_updated_and_deleted() {
    let (library, porch_id) = library();
    let yard = added(library.apply(&Change::AddCamera {
        camera: CameraConfig {
            name: "Yard".into(),
            ..porch()
        },
    }));
    assert_eq!(
        library.cameras()[1].id.as_str(),
        yard,
        "a new id, made here"
    );

    assert_eq!(
        refusal(library.apply(&Change::AddCamera {
            camera: CameraConfig {
                host: " ".into(),
                ..porch()
            },
        })),
        InvalidCamera::NoAddress.message()
    );

    library
        .apply(&Change::UpdateCameras {
            cameras: vec![CameraUpdate {
                id: porch_id.clone(),
                camera: CameraConfig {
                    user: "viewer".into(),
                    password: String::new(),
                    ..porch()
                },
                keep_password: true,
            }],
        })
        .unwrap();
    let saved = library.camera(&porch_id).unwrap();
    assert_eq!(saved.user, "viewer");
    assert_eq!(saved.password, "s3cret", "the saved password stays");

    library
        .apply(&Change::UpdateCameras {
            cameras: vec![CameraUpdate {
                id: porch_id.clone(),
                camera: CameraConfig {
                    password: "n3w".into(),
                    ..porch()
                },
                keep_password: false,
            }],
        })
        .unwrap();
    assert_eq!(library.camera(&porch_id).unwrap().password, "n3w");

    let before = library.revision();
    assert_eq!(
        refusal(library.apply(&Change::UpdateCameras {
            cameras: vec![
                CameraUpdate {
                    id: porch_id.clone(),
                    camera: porch(),
                    keep_password: true,
                },
                CameraUpdate {
                    id: CameraId::from(yard.as_str()),
                    camera: CameraConfig {
                        name: String::new(),
                        ..porch()
                    },
                    keep_password: true,
                },
            ],
        })),
        InvalidCamera::NoName.message()
    );
    assert_eq!(library.revision(), before, "a refused update saves nothing");

    library
        .apply(&Change::DeleteCamera {
            id: porch_id.clone(),
        })
        .unwrap();
    assert!(library.camera(&porch_id).is_none());
    assert_eq!(
        library.shown_tiles(&library.views()[0].id).unwrap()[0],
        None
    );
}

#[test]
fn a_camera_moves_among_the_cameras_and_its_groups_keep_their_order() {
    let (library, porch_id) = library();
    let yard = CameraId::from(added(library.apply(&Change::AddCamera {
        camera: CameraConfig {
            name: "Yard".into(),
            ..porch()
        },
    })));
    let outside = GroupId::from(added(library.apply(&Change::AddGroup {
        name: "Outside".into(),
        cameras: vec![porch_id.clone(), yard.clone()],
    })));
    let names = |library: &Library| {
        library
            .cameras()
            .into_iter()
            .map(|camera| camera.name)
            .collect::<Vec<_>>()
    };

    let before = library.revision();
    library
        .apply(&Change::MoveCamera {
            id: yard.clone(),
            index: 0,
        })
        .unwrap();
    assert_eq!(names(&library), ["Yard", "Porch"]);
    assert!(library.revision() > before, "the app hears about it");
    assert_eq!(
        library.tree()[0].cameras,
        [porch_id, yard.clone()],
        "the group keeps its own order"
    );
    assert_eq!(library.tree()[0].id, outside);

    library
        .apply(&Change::MoveCamera {
            id: yard.clone(),
            index: 7,
        })
        .unwrap();
    assert_eq!(
        names(&library),
        ["Porch", "Yard"],
        "past the end is the end"
    );
    assert_eq!(
        refusal(library.apply(&Change::MoveCamera {
            id: CameraId::from("gone"),
            index: 0,
        })),
        "no camera `gone`"
    );

    let change: Change = serde_json::from_str(&format!(
        r#"{{"change":"move_camera","id":"{yard}","index":0}}"#
    ))
    .unwrap();
    assert_eq!(change, Change::MoveCamera { id: yard, index: 0 });
    library.apply(&change).unwrap();
    assert_eq!(names(&library), ["Yard", "Porch"]);
}

#[test]
fn groups_are_added_changed_moved_and_removed() {
    let (library, porch_id) = library();
    let outside = GroupId::from(added(library.apply(&Change::AddGroup {
        name: "Outside".into(),
        cameras: vec![porch_id.clone()],
    })));
    let garage = GroupId::from(added(library.apply(&Change::AddGroup {
        name: "Garage".into(),
        cameras: Vec::new(),
    })));
    assert_eq!(
        refusal(library.apply(&Change::AddGroup {
            name: "Ghosts".into(),
            cameras: vec![CameraId::from("gone")],
        })),
        "no camera `gone`"
    );

    library
        .apply(&Change::RenameGroup {
            id: outside.clone(),
            name: "Yard".into(),
        })
        .unwrap();
    library
        .apply(&Change::MoveGroup {
            id: garage.clone(),
            index: 0,
        })
        .unwrap();
    library
        .apply(&Change::AddToGroup {
            group: garage.clone(),
            camera: porch_id.clone(),
        })
        .unwrap();
    library
        .apply(&Change::RemoveFromGroup {
            group: outside.clone(),
            camera: porch_id.clone(),
        })
        .unwrap();
    let tree = library.tree();
    assert_eq!(
        tree.iter()
            .map(|group| group.name.as_str())
            .collect::<Vec<_>>(),
        ["Garage", "Yard"]
    );
    assert_eq!(tree[0].cameras, std::slice::from_ref(&porch_id));
    assert!(tree[1].cameras.is_empty());

    library
        .apply(&Change::SaveGroup {
            id: outside.clone(),
            name: "Outside".into(),
            cameras: vec![porch_id],
        })
        .unwrap();
    assert_eq!(library.tree()[1].name, "Outside");

    library
        .apply(&Change::RemoveGroups {
            ids: vec![outside, garage],
        })
        .unwrap();
    assert!(library.tree().is_empty());
}

#[test]
fn views_are_added_divided_filled_and_removed() {
    let (library, porch_id) = library();
    let view = ViewId::from(added(library.apply(&Change::AddView {
        name: "Night".into(),
        layout: Layout::Four,
    })));
    library
        .apply(&Change::RenameView {
            id: view.clone(),
            name: "Night watch".into(),
        })
        .unwrap();
    library
        .apply(&Change::SetLayout {
            id: view.clone(),
            layout: Layout::Nine,
        })
        .unwrap();
    library
        .apply(&Change::SetFit {
            id: view.clone(),
            fit: Fit::ALL[1],
        })
        .unwrap();
    library
        .apply(&Change::SetTile {
            id: view.clone(),
            index: 8,
            camera: Some(porch_id.clone()),
            stream: TileStream::from_key("sub"),
        })
        .unwrap();
    library
        .apply(&Change::SetTileFit {
            id: view.clone(),
            index: 8,
            fit: Fit::Stretch,
        })
        .unwrap();
    let shown = library.shown_tiles(&view).unwrap();
    assert_eq!(shown.len(), 9);
    assert_eq!(
        shown[8],
        Some(ViewTile {
            camera: porch_id,
            stream: TileStream::from_key("sub"),
            fit: Some(Fit::Stretch),
        })
    );
    assert_eq!(library.views()[1].name, "Night watch");
    assert_eq!(library.views()[1].fit, Fit::ALL[1]);
    assert!(
        library
            .apply(&Change::SetTileFit {
                id: view.clone(),
                index: 7,
                fit: Fit::Fill,
            })
            .is_err()
    );

    library
        .apply(&Change::SetTile {
            id: view.clone(),
            index: 8,
            camera: None,
            stream: None,
        })
        .unwrap();
    assert_eq!(library.shown_tiles(&view).unwrap()[8], None);
    library.apply(&Change::RemoveView { id: view }).unwrap();
    assert_eq!(library.views().len(), 1);
}

#[test]
fn a_change_goes_over_the_wire_as_json() {
    let change = Change::SetTile {
        id: ViewId::from("v1"),
        index: 2,
        camera: None,
        stream: None,
    };
    let json = serde_json::to_string(&change).unwrap();
    assert_eq!(
        json,
        r#"{"change":"set_tile","id":"v1","index":2,"camera":null}"#
    );
    assert_eq!(serde_json::from_str::<Change>(&json).unwrap(), change);

    let sub = Change::SetTile {
        id: ViewId::from("v1"),
        index: 0,
        camera: Some(CameraId::from("c1")),
        stream: TileStream::from_key("sub"),
    };
    let json = serde_json::to_string(&sub).unwrap();
    assert_eq!(
        json,
        r#"{"change":"set_tile","id":"v1","index":0,"camera":"c1","stream":"sub"}"#
    );
    assert_eq!(serde_json::from_str::<Change>(&json).unwrap(), sub);

    let fit = Change::SetTileFit {
        id: ViewId::from("v1"),
        index: 1,
        fit: Fit::FourByThree,
    };
    let json = serde_json::to_string(&fit).unwrap();
    assert_eq!(
        json,
        r#"{"change":"set_tile_fit","id":"v1","index":1,"fit":"4:3"}"#
    );
    assert_eq!(serde_json::from_str::<Change>(&json).unwrap(), fit);

    let layout: Change =
        serde_json::from_str(r#"{"change":"add_view","name":"Yard","layout":"9"}"#).unwrap();
    assert_eq!(
        layout,
        Change::AddView {
            name: "Yard".into(),
            layout: Layout::Nine,
        }
    );

    let arranged = Change::ArrangeTiles {
        id: ViewId::from("v1"),
        tiles: vec![ArrangedTile {
            cell: crate::Cell::new(0, 0, 12, 12),
            was: Some(2),
        }],
    };
    let json = serde_json::to_string(&arranged).unwrap();
    assert_eq!(
        json,
        r#"{"change":"arrange_tiles","id":"v1","tiles":[{"cell":{"column":0,"row":0,"column_span":12,"row_span":12},"was":2}]}"#
    );
    assert_eq!(serde_json::from_str::<Change>(&json).unwrap(), arranged);
}

#[test]
fn a_snapshot_makes_a_copy_with_the_same_rules() {
    let (library, porch_id) = library();
    library
        .apply(&Change::AddGroup {
            name: "Outside".into(),
            cameras: vec![porch_id.clone()],
        })
        .unwrap();
    let snapshot = library.snapshot();
    assert_eq!(snapshot.revision, library.revision());

    let copy = Library::from_snapshot(snapshot.clone());
    assert_eq!(copy.cameras(), library.cameras(), "ids and all");
    assert_eq!(copy.tree(), library.tree());
    assert_eq!(copy.views(), library.views(), "no extra first view");
    assert_eq!(
        refusal(copy.apply(&Change::AddGroup {
            name: " ".into(),
            cameras: Vec::new(),
        })),
        refusal(library.apply(&Change::AddGroup {
            name: " ".into(),
            cameras: Vec::new(),
        })),
        "the copy refuses in the same words"
    );

    let json = serde_json::to_string(&snapshot).unwrap();
    assert_eq!(serde_json::from_str::<Snapshot>(&json).unwrap(), snapshot);
}

#[test]
fn a_client_copy_holds_no_password_and_says_who_has_one() {
    let (library, porch_id) = library();
    let yard = added(library.apply(&Change::AddCamera {
        camera: CameraConfig {
            name: "Yard".into(),
            password: String::new(),
            ..porch()
        },
    }));

    let copy = library.snapshot().for_client();
    assert_eq!(copy.with_password, [porch_id]);
    assert!(
        copy.snapshot
            .cameras
            .iter()
            .all(|camera| camera.password.is_empty())
    );
    assert_eq!(copy.snapshot.cameras[1].id.as_str(), yard);
    let json = serde_json::to_string(&copy).unwrap();
    assert!(!json.contains("s3cret"), "{json}");
}

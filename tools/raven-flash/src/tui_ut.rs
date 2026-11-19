// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use crate::device::{Device, Mount};
use crate::writer::WriteReport;

fn device(name: &str, mounted: bool) -> Candidate {
    Candidate {
        device: Device {
            name: name.to_string(),
            path: format!("/dev/{name}").into(),
            model: "Test Card".to_string(),
            serial: "SERIAL".to_string(),
            size_bytes: 64 * 1024 * 1024 * 1024,
            transport: "usb".to_string(),
            removable: true,
            mounted: if mounted {
                vec![Mount {
                    source: format!("/dev/{name}1"),
                    target: "/mnt/card".to_string(),
                    fstype: "vfat".to_string(),
                }]
            } else {
                Vec::new()
            },
            holders: Vec::new(),
            is_root: false,
        },
        reject: None,
    }
}

fn app() -> App {
    let mut refused = device("sdc", false);
    refused.reject = Some("image is on the target device".to_string());
    App::new(
        vec![
            ImageChoice {
                path: "build/disk.img".into(),
                size: 478_000_000,
            },
            ImageChoice {
                path: "build/other.img".into(),
                size: 100,
            },
        ],
        vec![device("sdb", true), refused],
    )
}

#[test]
fn image_screen_snapshot() {
    let text = render(&app());
    assert!(text.contains("raven-flash: select image"), "{text}");
    assert!(text.contains("build/disk.img"), "{text}");
    assert!(text.contains("Up/Down"), "{text}");
}

#[test]
fn target_screen_snapshot() {
    let mut app = app();
    app.screen = Screen::Target;
    let text = render(&app);
    assert!(text.contains("select removable target"), "{text}");
    assert!(text.contains("sdb"), "{text}");
    assert!(
        text.contains("refused: image is on the target device"),
        "{text}"
    );
    assert!(text.contains("mounted: /mnt/card"), "{text}");
}

#[test]
fn confirm_screen_snapshot() {
    let mut app = app();
    app.screen = Screen::Confirm;
    app.typed = "sdb".to_string();
    let text = render(&app);
    assert!(text.contains("RAW IMAGE WRITE"), "{text}");
    assert!(text.contains("target: sdb"), "{text}");
    assert!(
        text.contains("mounted partitions to unmount: /mnt/card"),
        "{text}"
    );
    assert!(text.contains("> sdb"), "{text}");
}

#[test]
fn progress_screen_snapshot() {
    let mut app = app();
    app.screen = Screen::Progress;
    app.progress = 42;
    app.status = "written 42/100 bytes".to_string();
    app.report = Some(WriteReport {
        image_sha256: "a".repeat(64),
        target_sha256: "a".repeat(64),
        bytes: 100,
        verified: true,
    });
    let text = render(&app);
    assert!(text.contains("writing: 42%"), "{text}");
    assert!(text.contains("written 42/100 bytes"), "{text}");
    assert!(text.contains("verified: true"), "{text}");
    assert!(text.contains("image sha256"), "{text}");
}

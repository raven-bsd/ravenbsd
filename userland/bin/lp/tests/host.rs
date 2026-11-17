// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the public `lp` client surface.

use lp::{
    DestinationKind, Document, NO_DESTINATION, Request, SubmitError, copy_order, destination_kind,
    input_names, parse, resolve_destination,
};

#[test]
fn builds_a_complete_request_from_the_command_line() {
    let options = parse([
        "-c",
        "-d",
        "/dev/lp0",
        "-n",
        "2",
        "-m",
        "-s",
        "-w",
        "-o",
        "duplex",
        "-t",
        "report",
        "first.txt",
        "-",
    ])
    .unwrap();
    let destination = resolve_destination(
        options.destination.as_deref(),
        Some("lpdest"),
        Some("printer"),
    )
    .unwrap();
    assert_eq!(destination, "/dev/lp0");
    assert_eq!(destination_kind(&destination), DestinationKind::LocalPath);
    let names = input_names(&options.files);
    assert_eq!(names, vec!["first.txt", "-"]);
    let documents = vec![
        Document {
            name: "first.txt".to_string(),
            data: b"first".to_vec(),
        },
        Document {
            name: "-".to_string(),
            data: b"second".to_vec(),
        },
    ];
    let request = Request {
        options,
        destination,
        documents,
    };
    let order: Vec<&[u8]> = copy_order(&request).collect();
    assert_eq!(
        order,
        vec![
            b"first".as_slice(),
            b"second".as_slice(),
            b"first".as_slice(),
            b"second".as_slice()
        ]
    );
}

#[test]
fn missing_destination_is_reported_before_any_input() {
    assert_eq!(resolve_destination(None, None, None), None);
    assert_eq!(NO_DESTINATION, "lp: no print destination configured");
}

#[test]
fn a_named_printer_has_no_service() {
    assert_eq!(destination_kind("lp0"), DestinationKind::Named);
    assert_eq!(
        SubmitError::NoService.message("lp0"),
        "lp: destination 'lp0' is not available"
    );
}

#[test]
fn a_local_device_error_carries_the_errno_description() {
    assert_eq!(
        SubmitError::Device("permission denied").message("/dev/lp0"),
        "lp: destination '/dev/lp0' is not available: permission denied"
    );
}

// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the `lp` client model.

use super::{
    DestinationKind, Document, NO_DESTINATION, Options, Request, SubmitError, USAGE, UsageError,
    copy_order, destination_kind, input_names, parse, printed_message, resolve_destination,
};

#[test]
fn defaults_match_the_posix_single_copy_request() {
    let options = Options::default();
    assert!(!options.copy);
    assert_eq!(options.destination, None);
    assert_eq!(options.copies, 1);
    assert!(!options.mail);
    assert!(!options.silent);
    assert_eq!(options.title, None);
    assert!(!options.write_terminal);
    assert!(options.printer_options.is_empty());
    assert!(options.files.is_empty());
}

#[test]
fn parses_all_separate_options() {
    let options = parse([
        "-c",
        "-d",
        "lp0",
        "-n",
        "3",
        "-m",
        "-s",
        "-w",
        "-o",
        "duplex",
        "-o",
        "media=a4",
        "-t",
        "report",
        "first.txt",
        "second.txt",
    ])
    .unwrap();
    assert!(options.copy);
    assert_eq!(options.destination.as_deref(), Some("lp0"));
    assert_eq!(options.copies, 3);
    assert!(options.mail);
    assert!(options.silent);
    assert!(options.write_terminal);
    assert_eq!(
        options.printer_options,
        vec!["duplex".to_string(), "media=a4".to_string()]
    );
    assert_eq!(options.title.as_deref(), Some("report"));
    assert_eq!(
        options.files,
        vec!["first.txt".to_string(), "second.txt".to_string()]
    );
}

#[test]
fn parses_attached_option_arguments() {
    let options = parse(["-dlp0", "-n3", "-oduplex", "-tReport", "file"]).unwrap();
    assert_eq!(options.destination.as_deref(), Some("lp0"));
    assert_eq!(options.copies, 3);
    assert_eq!(options.printer_options, vec!["duplex".to_string()]);
    assert_eq!(options.title.as_deref(), Some("Report"));
    assert_eq!(options.files, vec!["file".to_string()]);
}

#[test]
fn parses_grouped_boolean_options() {
    let options = parse(["-cmsw"]).unwrap();
    assert!(options.copy);
    assert!(options.mail);
    assert!(options.silent);
    assert!(options.write_terminal);
    assert_eq!(options.copies, 1);
}

#[test]
fn grouped_options_stop_at_an_argument_option() {
    let options = parse(["-cmn2", "file"]).unwrap();
    assert!(options.copy);
    assert!(options.mail);
    assert_eq!(options.copies, 2);
    assert_eq!(options.files, vec!["file".to_string()]);

    let separate = parse(["-cn", "2"]).unwrap();
    assert!(separate.copy);
    assert_eq!(separate.copies, 2);
}

#[test]
fn double_dash_ends_option_parsing() {
    let options = parse(["-c", "--", "-d", "file"]).unwrap();
    assert!(options.copy);
    assert_eq!(options.destination, None);
    assert_eq!(options.files, vec!["-d".to_string(), "file".to_string()]);
}

#[test]
fn lone_dash_is_a_standard_input_operand() {
    let options = parse(["-", "-c"]).unwrap();
    assert!(options.copy);
    assert_eq!(options.files, vec!["-".to_string()]);
}

#[test]
fn options_are_parsed_after_operands() {
    let options = parse(["file", "-c"]).unwrap();
    assert!(options.copy);
    assert_eq!(options.files, vec!["file".to_string()]);
}

#[test]
fn an_option_argument_can_begin_with_a_dash() {
    let options = parse(["-d", "-weird"]).unwrap();
    assert_eq!(options.destination.as_deref(), Some("-weird"));
}

#[test]
fn rejects_unknown_options() {
    assert_eq!(parse(["-z"]), Err(UsageError::UnknownOption('z')));
    assert_eq!(parse(["--help"]), Err(UsageError::UnknownOption('-')));
    assert_eq!(parse(["-cz"]), Err(UsageError::UnknownOption('z')));
}

#[test]
fn rejects_missing_option_arguments() {
    assert_eq!(parse(["-d"]), Err(UsageError::MissingOptionArgument('d')));
    assert_eq!(parse(["-n"]), Err(UsageError::MissingOptionArgument('n')));
    assert_eq!(parse(["-o"]), Err(UsageError::MissingOptionArgument('o')));
    assert_eq!(parse(["-t"]), Err(UsageError::MissingOptionArgument('t')));
    assert_eq!(parse(["-cd"]), Err(UsageError::MissingOptionArgument('d')));
}

#[test]
fn rejects_bad_copy_counts() {
    for value in ["0", "-1", "abc", "1x", "4294967296", "", "+2", " 2"] {
        assert_eq!(
            parse(["-n", value]),
            Err(UsageError::BadCopies),
            "value {value:?}"
        );
    }
}

#[test]
fn accepts_positive_copy_counts() {
    for (value, expected) in [("1", 1), ("01", 1), ("42", 42), ("4294967295", u32::MAX)] {
        assert_eq!(parse(["-n", value]).unwrap().copies, expected);
    }
}

#[test]
fn repeated_options_keep_the_last_value() {
    let options = parse([
        "-d", "first", "-d", "second", "-n", "2", "-n", "4", "-t", "a", "-t", "b",
    ])
    .unwrap();
    assert_eq!(options.destination.as_deref(), Some("second"));
    assert_eq!(options.copies, 4);
    assert_eq!(options.title.as_deref(), Some("b"));
}

#[test]
fn printer_options_keep_command_line_order() {
    let options = parse(["-o", "one", "-o", "two", "-o", "three"]).unwrap();
    assert_eq!(
        options.printer_options,
        vec!["one".to_string(), "two".to_string(), "three".to_string()]
    );
}

#[test]
fn rejects_an_empty_destination() {
    assert_eq!(parse(["-d", ""]), Err(UsageError::EmptyDestination));
}

#[test]
fn destination_precedence_is_option_then_lpdest_then_printer() {
    assert_eq!(
        resolve_destination(Some("cli"), Some("lpdest"), Some("printer")).as_deref(),
        Some("cli")
    );
    assert_eq!(
        resolve_destination(None, Some("lpdest"), Some("printer")).as_deref(),
        Some("lpdest")
    );
    assert_eq!(
        resolve_destination(None, None, Some("printer")).as_deref(),
        Some("printer")
    );
    assert_eq!(resolve_destination(None, None, None), None);
}

#[test]
fn empty_environment_values_are_unset() {
    assert_eq!(
        resolve_destination(None, Some(""), Some("printer")).as_deref(),
        Some("printer")
    );
    assert_eq!(
        resolve_destination(Some("cli"), Some(""), Some("")).as_deref(),
        Some("cli")
    );
    assert_eq!(resolve_destination(None, Some(""), Some("")), None);
}

#[test]
fn input_names_default_to_standard_input() {
    assert_eq!(input_names(&[]), vec!["-"]);
    let files = vec!["a".to_string(), "-".to_string()];
    assert_eq!(input_names(&files), vec!["a", "-"]);
}

#[test]
fn destination_kinds_separate_local_paths_from_printer_names() {
    assert_eq!(destination_kind("/dev/lp0"), DestinationKind::LocalPath);
    assert_eq!(destination_kind("tty:console"), DestinationKind::LocalPath);
    assert_eq!(
        destination_kind("file:/tmp/out"),
        DestinationKind::LocalPath
    );
    assert_eq!(destination_kind("lp0"), DestinationKind::Named);
    assert_eq!(destination_kind("./lp0"), DestinationKind::Named);
    assert_eq!(destination_kind("printer.class"), DestinationKind::Named);
}

#[test]
fn submit_diagnostics_are_exact() {
    assert_eq!(
        SubmitError::NoService.message("lp0"),
        "lp: destination 'lp0' is not available"
    );
    assert_eq!(
        SubmitError::Device("permission denied").message("/dev/lp0"),
        "lp: destination '/dev/lp0' is not available: permission denied"
    );
}

#[test]
fn copy_order_repeats_each_complete_copy() {
    let options = Options {
        copies: 2,
        ..Options::default()
    };
    let request = Request {
        options,
        destination: "/dev/lp0".to_string(),
        documents: vec![
            Document {
                name: "first".to_string(),
                data: b"one".to_vec(),
            },
            Document {
                name: "-".to_string(),
                data: b"two".to_vec(),
            },
        ],
    };
    let order: Vec<&[u8]> = copy_order(&request).collect();
    assert_eq!(
        order,
        vec![
            b"one".as_slice(),
            b"two".as_slice(),
            b"one".as_slice(),
            b"two".as_slice()
        ]
    );
}

#[test]
fn usage_and_diagnostic_text_is_stable() {
    assert_eq!(
        USAGE,
        "[-c] [-d dest] [-n copies] [-msw] [-o option]... [-t title] [file ...]"
    );
    assert_eq!(NO_DESTINATION, "lp: no print destination configured");
    assert_eq!(
        printed_message("/dev/lp0"),
        "lp: request printed on '/dev/lp0'"
    );
}

#[test]
fn usage_errors_have_stable_messages() {
    assert_eq!(
        UsageError::UnknownOption('z').message(),
        "lp: unknown option -- 'z'"
    );
    assert_eq!(
        UsageError::MissingOptionArgument('d').message(),
        "lp: option requires an argument -- 'd'"
    );
    assert_eq!(
        UsageError::BadCopies.message(),
        "lp: -n requires a positive decimal integer"
    );
    assert_eq!(
        UsageError::EmptyDestination.message(),
        "lp: -d requires a non-empty destination"
    );
}

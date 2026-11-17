// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn defaults_to_the_file_limit_query() {
    let request = parse_args(&[]).expect("request");
    assert_eq!(request.resource, Resource::File);
    assert_eq!(request.value, None);
    assert!(!request.hard && !request.soft);
}

#[test]
fn parses_hard_soft_and_resource() {
    let request = parse_args(&["-H", "-S", "-n", "1024"]).expect("request");
    assert!(request.hard && request.soft);
    assert_eq!(request.resource, Resource::OpenFiles);
    assert_eq!(request.value, Some(1024));
}

#[test]
fn parses_all_and_unlimited() {
    let request = parse_args(&["-a", "unlimited"]).expect("request");
    assert_eq!(request.resource, Resource::All);
    assert_eq!(request.value, Some(u64::MAX));
}

#[test]
fn rejects_unknown_option() {
    assert_eq!(parse_args(&["-z"]), Err(ParseError::UnknownOption));
}

#[test]
fn rejects_extra_operand() {
    assert_eq!(parse_args(&["1", "2"]), Err(ParseError::UnexpectedOperand));
}

#[test]
fn rejects_bad_value() {
    assert_eq!(parse_args(&["lots"]), Err(ParseError::BadValue));
}

#[test]
fn maps_resources_to_kernel_indices() {
    assert_eq!(kernel_index(Resource::Core), Some(4));
    assert_eq!(kernel_index(Resource::File), Some(1));
    assert_eq!(kernel_index(Resource::OpenFiles), Some(7));
    assert_eq!(kernel_index(Resource::Address), Some(9));
    assert_eq!(kernel_index(Resource::All), None);
}

#[test]
fn formats_and_scales_units() {
    assert_eq!(format_value(Resource::File, 1024), "2");
    assert_eq!(format_value(Resource::Stack, 2048), "2");
    assert_eq!(format_value(Resource::OpenFiles, 256), "256");
    assert_eq!(format_value(Resource::File, u64::MAX), "unlimited");
    assert_eq!(to_kernel(Resource::File, 2), 1024);
    assert_eq!(to_kernel(Resource::Cpu, 5), 5);
}

// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the allocation-free path helpers.

use super::*;

#[test]
fn public_path_limit_matches_posix_including_nul() {
    assert_eq!(PATH_MAX, 4096);
    assert_eq!(MAX_PATH, PATH_MAX - 1);
    assert_eq!(MAX_COMPONENT, 255);
}

const CASES: &[(&str, Result<&str, PathError>)] = &[
    ("/", Ok("/")),
    ("/a", Ok("/a")),
    ("/a/", Ok("/a")),
    ("/a//b", Ok("/a/b")),
    ("/a/./b", Ok("/a/b")),
    ("/./a", Ok("/a")),
    ("/a/b/..", Ok("/a")),
    ("/a/b/../..", Ok("/")),
    ("/a/b/../../c", Ok("/c")),
    ("/a/../b", Ok("/b")),
    ("/a/../../b", Err(PathError::EscapesRoot)),
    ("/..", Err(PathError::EscapesRoot)),
    ("/../", Err(PathError::EscapesRoot)),
    ("", Err(PathError::NotAbsolute)),
    ("a", Err(PathError::NotAbsolute)),
    ("a/b", Err(PathError::NotAbsolute)),
    ("/a/...", Ok("/a/...")),
    ("/a/.hidden", Ok("/a/.hidden")),
    ("/a/..hidden", Ok("/a/..hidden")),
    ("/a/b/./c", Ok("/a/b/c")),
    ("/a/b/c/../../d", Ok("/a/d")),
    ("/a/b/c/../../../d", Ok("/d")),
    ("/a/b/c/../../../../d", Err(PathError::EscapesRoot)),
    ("//a", Ok("/a")),
    ("///", Ok("/")),
    ("/a///b///", Ok("/a/b")),
    ("/a/./././b", Ok("/a/b")),
    ("/a/b/./../c", Ok("/a/c")),
    ("/a/b/.././c", Ok("/a/c")),
    ("/.", Ok("/")),
    ("/./", Ok("/")),
    ("/.//", Ok("/")),
    ("/a/.", Ok("/a")),
    ("/a/..", Ok("/")),
    ("/a/b/../..", Ok("/")),
    ("/x/../y/../z", Ok("/z")),
    ("/x/y/../z/..", Ok("/x")),
    ("/x/y/../z/../w", Ok("/x/w")),
    ("/A/B", Ok("/A/B")),
    ("/a b/c", Ok("/a b/c")),
    ("/é/ß", Ok("/é/ß")),
    ("/a/-b", Ok("/a/-b")),
    ("/a/_b", Ok("/a/_b")),
    ("/a/b-c_d.e", Ok("/a/b-c_d.e")),
    ("/a/b/..", Ok("/a")),
    ("/a/b/../c", Ok("/a/c")),
    ("/a/b/c/d/e/f", Ok("/a/b/c/d/e/f")),
    ("/a/b/c/d/e/..", Ok("/a/b/c/d")),
    ("/a/b/c/d/e/../..", Ok("/a/b/c")),
    ("/a/b/c/d/e/../../..", Ok("/a/b")),
    ("/a/b/c/d/e/../../../..", Ok("/a")),
    ("/a/b/c/d/e/../../../../..", Ok("/")),
    ("/a/b/c/d/e/../../../../../..", Err(PathError::EscapesRoot)),
    (
        "/a/b/c/d/e/../../../../../../..",
        Err(PathError::EscapesRoot),
    ),
    ("/a/b/c/d/e/f/../g", Ok("/a/b/c/d/e/g")),
    ("/a/b/c/d/e/f/../../g", Ok("/a/b/c/d/g")),
    ("/a/b/c/d/e/f/../../../g", Ok("/a/b/c/g")),
    ("/a/b/c/d/e/f/../../../../g", Ok("/a/b/g")),
    ("/a/b/c/d/e/f/../../../../../g", Ok("/a/g")),
    ("/a/b/c/d/e/f/../../../../../../g", Ok("/g")),
];

#[test]
fn normalize_table_has_sixty_cases() {
    assert_eq!(CASES.len(), 60);
    let mut buffer = [0u8; MAX_PATH];
    for (input, expected) in CASES {
        let actual = normalize_absolute(input, &mut buffer);
        assert_eq!(actual, *expected, "input {input:?}");
    }
}

#[test]
fn normalize_rejects_nul() {
    let mut buffer = [0u8; MAX_PATH];
    assert_eq!(
        normalize_absolute("/a\0b", &mut buffer),
        Err(PathError::Invalid)
    );
}

fn max_length_path() -> String {
    let mut path = String::from("/");
    for index in 0..16 {
        if index != 0 {
            path.push('/');
        }
        let width = if index == 15 { 254 } else { 255 };
        path.push_str(&"a".repeat(width));
    }
    path
}

#[test]
fn normalize_accepts_exact_max_path() {
    let input = max_length_path();
    assert_eq!(input.len(), MAX_PATH);
    let mut buffer = [0u8; MAX_PATH];
    assert_eq!(normalize_absolute(&input, &mut buffer).unwrap(), input);
}

#[test]
fn normalize_rejects_long_path() {
    let input = format!("{}/{}", max_length_path(), "a".repeat(255));
    assert!(input.len() > MAX_PATH);
    let mut buffer = [0u8; MAX_PATH + 16];
    assert_eq!(
        normalize_absolute(&input, &mut buffer),
        Err(PathError::TooLong)
    );
}

#[test]
fn normalize_rejects_long_component() {
    let input = format!("/{}", "a".repeat(MAX_COMPONENT + 1));
    let mut buffer = [0u8; MAX_PATH];
    assert_eq!(
        normalize_absolute(&input, &mut buffer),
        Err(PathError::ComponentTooLong)
    );
}

#[test]
fn normalize_rejects_small_output_buffer() {
    let mut buffer = [0u8; 3];
    assert_eq!(
        normalize_absolute("/abcd", &mut buffer),
        Err(PathError::TooLong)
    );
}

#[test]
fn split_extracts_first_component() {
    assert_eq!(split_first_component("a/b"), Ok(("a", "b")));
    assert_eq!(split_first_component("a"), Ok(("a", "")));
    assert_eq!(split_first_component("/a/b"), Ok(("a", "b")));
    assert_eq!(split_first_component("//a///b"), Ok(("a", "b")));
    assert_eq!(split_first_component("a/"), Ok(("a", "")));
    assert_eq!(split_first_component("a//b"), Ok(("a", "b")));
    assert_eq!(split_first_component("é/ß"), Ok(("é", "ß")));
}

#[test]
fn split_rejects_empty_and_dot_components() {
    assert_eq!(split_first_component(""), Err(PathError::Empty));
    assert_eq!(split_first_component("/"), Err(PathError::Empty));
    assert_eq!(split_first_component("."), Err(PathError::DotComponent));
    assert_eq!(split_first_component(".."), Err(PathError::DotComponent));
    assert_eq!(split_first_component("/./a"), Err(PathError::DotComponent));
    assert_eq!(split_first_component("/../a"), Err(PathError::DotComponent));
}

#[test]
fn split_bounds_components_and_total_length() {
    assert_eq!(split_first_component("/a\0b"), Err(PathError::Invalid));
    let long_component = "a".repeat(MAX_COMPONENT + 1);
    assert_eq!(
        split_first_component(&long_component),
        Err(PathError::ComponentTooLong)
    );
    let long_path = format!("/{}", "a".repeat(MAX_PATH));
    assert_eq!(split_first_component(&long_path), Err(PathError::TooLong));
}

#[test]
fn normalize_request_preserves_directory_intent() {
    let mut buffer = [0u8; MAX_PATH];
    type Case<'a> = (&'a str, Result<(&'a str, bool), PathError>);
    let cases: &[Case<'_>] = &[
        ("/", Ok(("/", true))),
        ("//", Ok(("/", true))),
        ("/a", Ok(("/a", false))),
        ("/a/", Ok(("/a", true))),
        ("/a//", Ok(("/a", true))),
        ("/a/.", Ok(("/a", true))),
        ("/a/..", Ok(("/", true))),
        ("/a/./b", Ok(("/a/b", false))),
        ("/a/b/..", Ok(("/a", true))),
        ("/a/../b", Ok(("/b", false))),
        ("", Err(PathError::NotAbsolute)),
        (".", Err(PathError::NotAbsolute)),
    ];
    for (input, expected) in cases {
        let actual =
            normalize_request(input, &mut buffer).map(|result| (result.path, result.requires_dir));
        assert_eq!(&actual, expected, "input {input:?}");
    }
}

#[test]
fn path_errors_map_to_errno_and_proto_errors() {
    assert_eq!(PathError::TooLong.errno(), Errno::ENAMETOOLONG);
    assert_eq!(PathError::Invalid.errno(), Errno::EINVAL);
    assert_eq!(
        ProtoError::from(PathError::TooLong),
        ProtoError::LengthOutOfRange
    );
    assert_eq!(ProtoError::from(PathError::Empty), ProtoError::Malformed);
}

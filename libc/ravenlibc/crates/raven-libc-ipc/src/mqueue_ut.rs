// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
use super::*;

#[test]
fn valid_names_are_accepted() {
    assert!(validate_name(b"/work").is_ok());
    assert!(validate_name(b"/a-b_c.1").is_ok());
    let longest = [b'a'; NAME_MAX];
    let mut name = [b'/'; NAME_MAX + 1];
    name[1..].copy_from_slice(&longest);
    assert!(validate_name(&name).is_ok());
}

#[test]
fn malformed_names_are_rejected() {
    assert_eq!(validate_name(b""), Err(EINVAL));
    assert_eq!(validate_name(b"work"), Err(EINVAL));
    assert_eq!(validate_name(b"/"), Err(EINVAL));
    assert_eq!(validate_name(b"/."), Err(EINVAL));
    assert_eq!(validate_name(b"/.."), Err(EINVAL));
    assert_eq!(validate_name(b"/a/b"), Err(EINVAL));
    let too_long = [b'a'; NAME_MAX + 1];
    let mut name = [b'/'; NAME_MAX + 2];
    name[1..].copy_from_slice(&too_long);
    assert_eq!(validate_name(&name), Err(ENAMETOOLONG));
}

#[test]
fn flags_and_attributes_are_bounded() {
    assert!(validate_flags(0).is_ok());
    assert!(validate_flags(O_NONBLOCK).is_ok());
    assert_eq!(validate_flags(1), Err(EINVAL));

    assert!(validate_attr(0, 0, 0).is_ok());
    assert!(validate_attr(10, 8192, 3).is_ok());
    assert_eq!(validate_attr(-1, 8192, 0), Err(EINVAL));
    assert_eq!(validate_attr(10, -1, 0), Err(EINVAL));
    assert_eq!(validate_attr(10, 8192, -1), Err(EINVAL));
}

#[test]
fn no_queue_service_is_advertised() {
    assert_eq!(unsupported(), crate::consts::ENOSYS);
    assert_eq!(queue_descriptor_error(), crate::consts::EBADF);
}

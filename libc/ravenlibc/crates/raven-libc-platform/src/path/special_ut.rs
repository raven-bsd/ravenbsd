// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Special-name tests.

use crate::path::special::{SpecialName, special_fd, target_fd};

#[test]
fn dev_fd_numbers() {
    assert_eq!(special_fd(b"/dev/fd/0"), Some(SpecialName::Fd(0)));
    assert_eq!(special_fd(b"/dev/fd/99"), Some(SpecialName::Fd(99)));
}

#[test]
fn proc_self_fd_numbers() {
    assert_eq!(special_fd(b"/proc/self/fd/1"), Some(SpecialName::Fd(1)));
}

#[test]
fn std_names() {
    assert_eq!(target_fd(SpecialName::Stdin), 0);
    assert_eq!(target_fd(SpecialName::Stdout), 1);
    assert_eq!(target_fd(SpecialName::Stderr), 2);
}

#[test]
fn other_dev_paths_are_not_special() {
    assert_eq!(special_fd(b"/dev/null"), None);
    assert_eq!(special_fd(b"/proc/self/status"), None);
    assert_eq!(special_fd(b"/dev/fd/"), None);
}

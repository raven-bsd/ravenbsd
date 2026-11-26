// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the native/Linux error conversion.

use super::*;
use raven_abi::errno::Errno;
use raven_linux_abi::errno;

#[test]
fn the_donor_representative_rows() {
    // FreeBSD sys/compat/linux/linux_errno.inc at 13334440.
    for (native, linux) in [
        (Errno::EPERM.0, errno::EPERM),
        (Errno::EDEADLK.0, errno::EDEADLK),
        (Errno::ENOMEM.0, errno::ENOMEM),
        (Errno::EAGAIN.0, errno::EAGAIN),
        (Errno::ENOSYS.0, errno::ENOSYS),
        (Errno::EADDRINUSE.0, errno::EADDRINUSE),
        (Errno::EOVERFLOW.0, errno::EOVERFLOW),
        (Errno::EILSEQ.0, errno::EILSEQ),
        (Errno::ETIMEDOUT.0, errno::ETIMEDOUT),
    ] {
        assert_eq!(linux_from_native(native), Some(linux), "native {native}");
    }
}

#[test]
fn the_reverse_rows_agree_with_the_donor_choices() {
    assert_eq!(native_from_linux(errno::EAGAIN), Some(Errno::EAGAIN.0));
    assert_eq!(native_from_linux(errno::EDEADLK), Some(Errno::EDEADLK.0));
    assert_eq!(native_from_linux(errno::ENOSYS), Some(Errno::ENOSYS.0));
    assert_eq!(
        native_from_linux(errno::EADDRINUSE),
        Some(Errno::EADDRINUSE.0)
    );
    assert_eq!(
        native_from_linux(errno::EOVERFLOW),
        Some(Errno::EOVERFLOW.0)
    );
    assert_eq!(native_from_linux(errno::EILSEQ), Some(Errno::EILSEQ.0));
    // The donor maps Linux ENOEXEC to native ENOENT and the unused 41/58 to EINVAL.
    assert_eq!(native_from_linux(errno::ENOEXEC), Some(Errno::ENOENT.0));
    assert_eq!(native_from_linux(41), Some(Errno::EINVAL.0));
    assert_eq!(native_from_linux(58), Some(Errno::EINVAL.0));
}

#[test]
fn every_native_error_has_a_linux_destination() {
    for native in 1..=Errno::ELAST {
        let linux = linux_from_native(native).unwrap_or_else(|| panic!("native {native}"));
        assert!(
            (1..=LINUX_LAST).contains(&linux),
            "native {native} mapped to {linux}"
        );
    }
}

#[test]
fn every_linux_error_has_a_native_destination() {
    for linux in 1..=LINUX_LAST {
        native_from_linux(linux).unwrap_or_else(|| panic!("linux {linux}"));
    }
}

#[test]
fn unknown_numbers_are_rejected() {
    assert_eq!(linux_from_native(0), None);
    assert_eq!(linux_from_native(Errno::ELAST + 1), None);
    assert_eq!(native_from_linux(0), None);
    assert_eq!(native_from_linux(LINUX_LAST + 1), None);
}

#[test]
fn the_one_to_one_rows_round_trip() {
    // EAGAIN/EDEADLK, the socket/network block, and the file block are one-to-one in the donor
    // tables. Native errors the donor approximates (EPROCLIM and the RPC class) do not reverse.
    for native in [
        Errno::EPERM,
        Errno::ENOENT,
        Errno::EBADF,
        Errno::EAGAIN,
        Errno::EDEADLK,
        Errno::ENOSYS,
        Errno::EADDRINUSE,
        Errno::EOVERFLOW,
        Errno::EILSEQ,
        Errno::ENOTCONN,
        Errno::ETIMEDOUT,
    ] {
        let linux = linux_from_native(native.0).expect("defined");
        assert_eq!(native_from_linux(linux), Some(native.0), "{native:?}");
    }
}

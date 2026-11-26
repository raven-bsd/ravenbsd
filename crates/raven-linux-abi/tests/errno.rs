// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Errno values are consistent with the native `raven-abi` contract and the Linux table.

use raven_linux_abi::errno;

mod support;

macro_rules! check_pair {
    ($name:ident, $native:expr, $linux:expr) => {{
        assert_eq!(
            raven_abi::errno::Errno::$name.0,
            $native,
            "native {}",
            stringify!($name)
        );
        assert_eq!(errno::$name, $linux, "linux {}", stringify!($name));
    }};
}

#[test]
fn native_and_linux_errno_domains_are_separate() {
    // DR-69: raven-abi is the FreeBSD native domain; raven-linux-abi is the Linux UAPI table.
    // Every name the old parity test checked is kept, now with both exact domain values.
    check_pair!(EPERM, 1, 1);
    check_pair!(ENOENT, 2, 2);
    check_pair!(ESRCH, 3, 3);
    check_pair!(EINTR, 4, 4);
    check_pair!(EIO, 5, 5);
    check_pair!(ENXIO, 6, 6);
    check_pair!(E2BIG, 7, 7);
    check_pair!(ENOEXEC, 8, 8);
    check_pair!(EBADF, 9, 9);
    check_pair!(ECHILD, 10, 10);
    check_pair!(EAGAIN, 35, 11);
    check_pair!(ENOMEM, 12, 12);
    check_pair!(EACCES, 13, 13);
    check_pair!(EFAULT, 14, 14);
    check_pair!(EBUSY, 16, 16);
    check_pair!(EEXIST, 17, 17);
    check_pair!(EXDEV, 18, 18);
    check_pair!(ENODEV, 19, 19);
    check_pair!(ENOTDIR, 20, 20);
    check_pair!(EISDIR, 21, 21);
    check_pair!(EINVAL, 22, 22);
    check_pair!(ENFILE, 23, 23);
    check_pair!(EMFILE, 24, 24);
    check_pair!(ENOTTY, 25, 25);
    check_pair!(EFBIG, 27, 27);
    check_pair!(ENOSPC, 28, 28);
    check_pair!(ESPIPE, 29, 29);
    check_pair!(EROFS, 30, 30);
    check_pair!(EMLINK, 31, 31);
    check_pair!(EPIPE, 32, 32);
    check_pair!(EDOM, 33, 33);
    check_pair!(ERANGE, 34, 34);
    check_pair!(ENAMETOOLONG, 63, 36);
    check_pair!(ENOSYS, 78, 38);
    check_pair!(ENOTEMPTY, 66, 39);
    check_pair!(ELOOP, 62, 40);
    check_pair!(EPROTO, 92, 71);
    check_pair!(EOVERFLOW, 84, 75);
    check_pair!(ENOTSOCK, 38, 88);
    check_pair!(EOPNOTSUPP, 45, 95);
    check_pair!(EAFNOSUPPORT, 47, 97);
    check_pair!(EADDRINUSE, 48, 98);
    check_pair!(EADDRNOTAVAIL, 49, 99);
    check_pair!(ENETUNREACH, 51, 101);
    check_pair!(ECONNABORTED, 53, 103);
    check_pair!(ECONNRESET, 54, 104);
    check_pair!(ENOBUFS, 55, 105);
    check_pair!(EISCONN, 56, 106);
    check_pair!(ENOTCONN, 57, 107);
    check_pair!(ETIMEDOUT, 60, 110);
    check_pair!(ECONNREFUSED, 61, 111);
    check_pair!(EHOSTUNREACH, 65, 113);
    check_pair!(EALREADY, 37, 114);
    check_pair!(EINPROGRESS, 36, 115);
    assert_eq!(
        raven_abi::errno::Errno::EWOULDBLOCK.0,
        raven_abi::errno::Errno::EAGAIN.0
    );
    assert_eq!(errno::EWOULDBLOCK, errno::EAGAIN);
}

#[test]
fn table_is_sorted_by_name() {
    assert!(errno::ERRNOS.windows(2).all(|pair| pair[0].0 < pair[1].0));
}

#[test]
fn table_has_no_duplicate_names() {
    let mut names: Vec<&str> = errno::ERRNOS.iter().map(|(name, _)| *name).collect();
    names.sort_unstable();
    let before = names.len();
    names.dedup();
    assert_eq!(before, names.len());
}

#[test]
fn every_value_has_a_matching_name() {
    for &(_, value) in errno::ERRNOS {
        let name = errno::name(value).unwrap_or_else(|| panic!("errno {value} has no name"));
        let found = errno::ERRNOS
            .iter()
            .find(|(candidate, _)| *candidate == name)
            .expect("lookup returned an unknown name");
        assert_eq!(found.1, value, "lookup for {value} returned {name}");
    }
}

#[test]
fn every_value_matches_linux_raw_sys() {
    let source = support::u32_consts("src/x86_64/errno.rs", "E");
    assert_eq!(errno::ERRNOS.len(), source.len());
    for &(name, value) in errno::ERRNOS {
        assert_eq!(
            source.get(name).copied(),
            Some(u32::from(value)),
            "errno {name}"
        );
    }
}

#[test]
fn spot_values_and_unknown_lookup() {
    assert_eq!(errno::EPERM, 1);
    assert_eq!(errno::ENOENT, 2);
    assert_eq!(errno::EAGAIN, 11);
    assert_eq!(errno::ENOSYS, 38);
    assert_eq!(errno::ETIMEDOUT, 110);
    assert_eq!(errno::EHWPOISON, 133);
    assert_eq!(errno::EWOULDBLOCK, errno::EAGAIN);
    assert!(matches!(errno::name(11), Some("EAGAIN" | "EWOULDBLOCK")));
    assert_eq!(errno::name(1), Some("EPERM"));
    assert_eq!(errno::name(133), Some("EHWPOISON"));
    assert_eq!(errno::name(0), None);
    assert_eq!(errno::name(4000), None);
}

// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Errno values are consistent with the native `raven-abi` contract and the Linux table.

use raven_linux_abi::errno;

macro_rules! check_raven_abi {
    ($($name:ident),+ $(,)?) => {
        $(
            assert_eq!(
                raven_abi::errno::Errno::$name.0,
                errno::$name,
                "{} differs between raven-abi and raven-linux-abi",
                stringify!($name)
            );
        )+
    };
}

#[test]
fn raven_abi_errno_values_match() {
    check_raven_abi!(
        EPERM,
        ENOENT,
        ESRCH,
        EINTR,
        EIO,
        ENXIO,
        E2BIG,
        ENOEXEC,
        EBADF,
        ECHILD,
        EAGAIN,
        ENOMEM,
        EACCES,
        EFAULT,
        EBUSY,
        EEXIST,
        EXDEV,
        ENODEV,
        ENOTDIR,
        EISDIR,
        EINVAL,
        ENFILE,
        EMFILE,
        ENOTTY,
        EFBIG,
        ENOSPC,
        ESPIPE,
        EROFS,
        EMLINK,
        EPIPE,
        EDOM,
        ERANGE,
        ENAMETOOLONG,
        ENOSYS,
        ENOTEMPTY,
        ELOOP,
        EPROTO,
        EOVERFLOW,
        ENOTSOCK,
        EOPNOTSUPP,
        EAFNOSUPPORT,
        EADDRINUSE,
        EADDRNOTAVAIL,
        ENETUNREACH,
        ECONNABORTED,
        ECONNRESET,
        ENOBUFS,
        EISCONN,
        ENOTCONN,
        ETIMEDOUT,
        ECONNREFUSED,
        EHOSTUNREACH,
        EALREADY,
        EINPROGRESS,
    );
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

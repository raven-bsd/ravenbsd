// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Errno values are consistent with the native `raven-abi` contract and the Linux table.

use raven_linux_abi::errno;

mod support;

#[test]
fn native_and_linux_errno_domains_are_separate() {
    // DR-69: raven-abi is the FreeBSD native domain; raven-linux-abi is the Linux UAPI table.
    for (name, native, linux) in [
        ("EPERM", 1, 1),
        ("ENOENT", 2, 2),
        ("EAGAIN", 35, 11),
        ("EDEADLK", 11, 35),
        ("ENOSYS", 78, 38),
        ("EADDRINUSE", 48, 98),
        ("EOVERFLOW", 84, 75),
        ("EILSEQ", 86, 84),
    ] {
        let ours = match name {
            "EPERM" => raven_abi::errno::Errno::EPERM.0,
            "ENOENT" => raven_abi::errno::Errno::ENOENT.0,
            "EAGAIN" => raven_abi::errno::Errno::EAGAIN.0,
            "EDEADLK" => raven_abi::errno::Errno::EDEADLK.0,
            "ENOSYS" => raven_abi::errno::Errno::ENOSYS.0,
            "EADDRINUSE" => raven_abi::errno::Errno::EADDRINUSE.0,
            "EOVERFLOW" => raven_abi::errno::Errno::EOVERFLOW.0,
            "EILSEQ" => raven_abi::errno::Errno::EILSEQ.0,
            _ => unreachable!(),
        };
        assert_eq!(ours, native, "native {name}");
        let theirs = match name {
            "EPERM" => errno::EPERM,
            "ENOENT" => errno::ENOENT,
            "EAGAIN" => errno::EAGAIN,
            "EDEADLK" => errno::EDEADLK,
            "ENOSYS" => errno::ENOSYS,
            "EADDRINUSE" => errno::EADDRINUSE,
            "EOVERFLOW" => errno::EOVERFLOW,
            "EILSEQ" => errno::EILSEQ,
            _ => unreachable!(),
        };
        assert_eq!(theirs, linux, "linux {name}");
        if native != linux {
            assert_ne!(ours, theirs, "{name} domains aliased");
        }
    }
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

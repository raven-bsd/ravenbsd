// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Errno values are consistent with the native `raven-abi` contract and the Linux table.

use raven_linux_abi::errno;

mod support;

#[test]
fn native_values_are_unchanged_and_linux_names_resolve() {
    // The Linux UAPI table resolves every Linux name, including the names that have no native
    // equivalent and that S722 removed from the FreeBSD-valued native domain (DR-69).
    assert_eq!(errno::EAGAIN, 11);
    assert_eq!(errno::EDEADLK, 35);
    assert_eq!(errno::ENOSYS, 38);
    assert_eq!(errno::ECHRNG, 44);
    assert_eq!(errno::EBADE, 52);
    assert_eq!(errno::EADV, 68);
    assert_eq!(errno::ENOMEDIUM, 123);
    assert_eq!(errno::EWOULDBLOCK, errno::EAGAIN);

    // The native contract keeps the FreeBSD values and does not alias the Linux numbers.
    assert_eq!(raven_abi::errno::Errno::EAGAIN.0, 35);
    assert_eq!(raven_abi::errno::Errno::EDEADLK.0, 11);
    assert_eq!(raven_abi::errno::Errno::ENOSYS.0, 78);
    assert_eq!(raven_abi::errno::Errno::EADDRINUSE.0, 48);
    assert_eq!(raven_abi::errno::Errno::EOVERFLOW.0, 84);
    assert_eq!(raven_abi::errno::Errno::EILSEQ.0, 86);

    // Numeric collisions across domains are not aliases: native 68 is EUSERS, not Linux EADV.
    assert_eq!(raven_abi::errno::Errno::EUSERS.0, 68);
    assert_eq!(raven_abi::errno::Errno(68).description(), "too many users");
    assert_eq!(errno::EUSERS, 87);
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

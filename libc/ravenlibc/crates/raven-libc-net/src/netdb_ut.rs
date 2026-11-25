// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests.

use super::*;
use raven_libc_abi::socket::{AF_INET, AF_INET6, AF_UNSPEC};

#[test]
fn public_family_translates_to_the_resolver_domain() {
    assert_eq!(
        resolver_family(AF_UNSPEC),
        Some(raven_libc_resolv::AF_UNSPEC)
    );
    assert_eq!(resolver_family(AF_INET), Some(raven_libc_resolv::AF_INET));
    assert_eq!(resolver_family(AF_INET6), Some(raven_libc_resolv::AF_INET6));
}

#[test]
fn resolver_family_translates_back_to_the_public_profile() {
    assert_eq!(public_family(raven_libc_resolv::AF_UNSPEC), AF_UNSPEC);
    assert_eq!(public_family(raven_libc_resolv::AF_INET), AF_INET);
    assert_eq!(public_family(raven_libc_resolv::AF_INET6), AF_INET6);
}

#[test]
fn the_resolver_ipv6_value_is_not_a_public_family() {
    // The resolver's IPv6 value (10) is not a public socket family in this profile, so it
    // must not be accepted as one by the public-to-resolver direction.
    assert_eq!(resolver_family(raven_libc_resolv::AF_INET6), None);
}

// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Our own IPv4 address type, independent of any backend.

use core::fmt;

/// An IPv4 address.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Ipv4Addr([u8; 4]);

impl Ipv4Addr {
    /// The unspecified address.
    pub const UNSPECIFIED: Self = Self([0; 4]);

    /// Construct from four octets.
    #[must_use]
    pub const fn new(a: u8, b: u8, c: u8, d: u8) -> Self {
        Self([a, b, c, d])
    }

    /// Construct from an octet array.
    #[must_use]
    pub const fn from_octets(octets: [u8; 4]) -> Self {
        Self(octets)
    }

    /// Return the four octets.
    #[must_use]
    pub const fn octets(self) -> [u8; 4] {
        self.0
    }
}

impl From<[u8; 4]> for Ipv4Addr {
    fn from(octets: [u8; 4]) -> Self {
        Self(octets)
    }
}

impl fmt::Display for Ipv4Addr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [a, b, c, d] = self.0;
        write!(f, "{a}.{b}.{c}.{d}")
    }
}

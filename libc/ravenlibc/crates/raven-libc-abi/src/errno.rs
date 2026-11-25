// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The `errno` newtype over the generated native error numbers.
//!
//! The generated table is the FreeBSD application ABI (DR-69): `EAGAIN` is 35 and `EDEADLK`
//! is 11. Linux numbers exist only inside the Linux personality (`raven-linux-abi`), which
//! converts at its boundaries.

pub use crate::generated::errno::*;

/// A positive error number in the native (FreeBSD-valued) domain.
///
/// `Errno` is not the kernel contract type (`raven_abi::errno::Errno`); it is the C library view.
/// The Linux-personality table is separate and must be translated, never cast.
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct Errno(pub u16);

impl Errno {
    /// Build the C-library error from one generated error number.
    ///
    /// The generated table exposes its names as `i32` constants in the platform's native domain;
    /// this is the one place they become the `Errno` type. A caller must pass a defined name,
    /// never a bare numerical literal.
    #[must_use]
    pub const fn from_generated(number: i32) -> Errno {
        Errno(number as u16)
    }

    /// Whether the value could be a Linux-profile number (1..=133, with 41 and 58 unused).
    ///
    /// This classifies the Linux value range only; it says nothing about the native domain and
    /// must not be used to interpret a native error.
    #[must_use]
    pub const fn is_linux(self) -> bool {
        self.0 >= 1 && self.0 <= 133 && self.0 != 41 && self.0 != 58
    }

    /// Whether the value is in the retired C36 BSD-extra range (1000..=1013).
    ///
    /// The native domain no longer uses that range (the donor numbers are 1..=97); the predicate
    /// remains so old registrations fail closed instead of being silently reinterpreted.
    #[must_use]
    pub const fn is_bsd_extra(self) -> bool {
        self.0 >= 1000 && self.0 <= 1013
    }

    /// The canonical name of the value, or `None` for an unknown number.
    #[must_use]
    pub fn name(self) -> Option<&'static str> {
        ALL.iter()
            .find(|(_, value)| *value == i128::from(self.0))
            .map(|(name, _)| *name)
    }
}

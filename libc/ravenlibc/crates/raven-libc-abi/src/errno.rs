// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The `errno` newtype over the generated Linux/BSD error numbers.

pub use crate::generated::errno::*;

/// A positive error number.
///
/// Values 1..=133 are Linux error numbers; 1000..=1013 are the BSD-only additions of doc 14
/// 3.A.4. `Errno` is not the kernel contract type (`raven_abi::errno::Errno`); it is the C
/// library view with the extra names of the first release.
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct Errno(pub u16);

impl Errno {
    /// Whether the value is a Linux `errno` (1..=133, with 41 and 58 unused).
    #[must_use]
    pub const fn is_linux(self) -> bool {
        self.0 >= 1 && self.0 <= 133 && self.0 != 41 && self.0 != 58
    }

    /// Whether the value is one of the BSD-only additions (1000..=1013).
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

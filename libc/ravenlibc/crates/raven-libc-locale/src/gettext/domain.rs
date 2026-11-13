// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! A gettext domain binding.

/// A domain name and its optional directory override.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Domain {
    /// The domain name.
    pub name: alloc::vec::Vec<u8>,
    /// The directory, or `None` for the default.
    pub dir: Option<alloc::vec::Vec<u8>>,
}

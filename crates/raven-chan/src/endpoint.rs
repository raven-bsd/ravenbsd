// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The two sides of one channel pair.

/// One endpoint of a [`Channel`](crate::Channel).
///
/// The names are positional, not directional: either endpoint may call, reply or receive. A
/// channel has exactly two endpoints and no endpoint identity beyond this pair, because the
/// handle and rights model belongs to the kernel integration, not to this crate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Endpoint {
    /// First endpoint of the pair.
    A,
    /// Second endpoint of the pair.
    B,
}

impl Endpoint {
    /// Both endpoints, in declaration order.
    pub const ALL: [Self; 2] = [Self::A, Self::B];

    /// The other endpoint of the same channel.
    pub const fn peer(self) -> Self {
        match self {
            Self::A => Self::B,
            Self::B => Self::A,
        }
    }

    /// A stable index for fixed-size endpoint tables.
    pub(crate) const fn index(self) -> usize {
        match self {
            Self::A => 0,
            Self::B => 1,
        }
    }
}

#[cfg(test)]
#[path = "endpoint_ut.rs"]
mod tests;

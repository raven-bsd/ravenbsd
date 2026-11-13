// SPDX-License-Identifier: ISC
// Copyright (c) 1996, David Mazieres <dm@uun.org>
// Copyright (c) 2008, Damien Miller <djm@openbsd.org>
// Copyright (c) 2013, Markus Friedl <markus@openbsd.org>
// Copyright (c) 2014, Theo de Raadt <deraadt@openbsd.org>
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from openbsd:lib/libc/crypt/arc4random.h @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
//
// RavenBSD: adapted from openbsd:lib/libc/crypt/arc4random.h @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
// (file revision tag: $OpenBSD: arc4random.h,v 1.4 2015/01/15 06:57:18 deraadt Exp $)
// The mmap/minherit allocator and fork-detect stub are replaced by the caller-owned state and
// the explicit ForkEvents hook; getentropy failure goes through RandPlatform.

//! Platform hooks for the random state machine.

/// Failure of the entropy source: never return weak data.
pub trait RandPlatform {
    /// Write `"arc4random: no entropy\n"` through the platform's fatal path and terminate.
    fn no_entropy(&self) -> !;
}

/// A platform that always fails (tests and the no-entropy path).
#[cfg(test)]
pub struct NoEntropy;

#[cfg(test)]
impl RandPlatform for NoEntropy {
    fn no_entropy(&self) -> ! {
        panic!("arc4random: no entropy")
    }
}

#[cfg(test)]
#[path = "platform_ut.rs"]
mod tests;

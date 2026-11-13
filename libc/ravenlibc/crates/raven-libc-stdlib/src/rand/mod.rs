// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The three random families: `rand`/`srand` (deterministic `random`), `random`/`srandom`/
//! `initstate`/`setstate` (five state types), `rand_r`, and the `drand48` family.

#[allow(clippy::module_inception)] // the family is named `rand` by the ticket
pub mod rand;
#[allow(clippy::module_inception)]
pub mod rand48;
pub mod rand_r;
pub mod random;

pub use rand::{DefaultRandom, RAND_MAX, rand, srand};
pub use rand_r::rand_r;
pub use rand48::{
    Rand48, drand48, erand48, jrand48, lcong48, lrand48, mrand48, nrand48, seed48, srand48,
    x_to_double,
};
pub use random::{MAX_TYPES, RandErr, RandomCtl, initstate, random, setstate, srandom};

#[cfg(test)]
#[path = "mod_ut.rs"]
mod tests;

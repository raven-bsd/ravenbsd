// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Arbitrary-precision integers over the per-call arena: `misc.c`, `dmisc.c`, `smisc.c`, `gmisc.c`
//! and `sum.c` of NetBSD gdtoa.
//!
//! The donor's `Bigint *` becomes a [`BigRef`] word index; every field goes through the six
//! accessors of [`crate::ctx`]. The names are the donor's (`multadd`, `quorem`, `s2b`, ...) so a
//! reviewer can diff function by function.

#![allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
// faithful numeric port; overflow and bounds excluded by the donor's argument; debug tests run with overflow-checks on

pub(crate) mod dmisc;
pub(crate) mod gmisc;
pub(crate) mod misc;
pub(crate) mod smisc;
pub(crate) mod sum;

pub(crate) use self::dmisc::quorem;
pub(crate) use self::gmisc::{rshift, trailz};
pub(crate) use self::misc::{
    b2d, cmp, d2b, diff, hi0bits, i2b, lo0bits, lshift, mult, multadd, pow5mult,
};
pub(crate) use self::smisc::{any_on, copybits, match_prefix, ratio, s2b};
pub(crate) use self::sum::sum;

use crate::ctx::{BigRef, DtoaCtx};

/// The donor's `Bcopy`: copy `sign`, `wds` and the significand words into a fresh block.
pub(crate) fn bcopy(ctx: &mut DtoaCtx, dst: BigRef, src: BigRef) {
    ctx.set_sign(dst, ctx.sign(src));
    let wds = ctx.wds(src);
    ctx.set_wds(dst, wds);
    let mut i = 0u32;
    while i < wds as u32 {
        ctx.set_x(dst, i, ctx.x(src, i));
        i = i.wrapping_add(1);
    }
}

#[cfg(test)]
#[path = "mod_ut.rs"]
mod tests;

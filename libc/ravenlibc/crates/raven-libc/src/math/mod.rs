// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `<math.h>` scalar exports: every body is one `raven-libc-m` call.
//!
//! The `f64`/`f32` families that C47a/C47b provide are bound; the `*l` long-double rows stay in
//! `expected/DEFERRED.txt` until C47c (no aliasing to `f64`). C23/legacy names the crate provides
//! are exported under their C spelling; complex `c*` entry points are not bound in this pass.
//!
//! The functions are split over small modules because the archive contract allows at most four C
//! text symbols per object member.

macro_rules! unary {
    ($name:ident, $call:path) => {
        /// Scalar math entry point.
        #[unsafe(no_mangle)]
        pub extern "C" fn $name(x: core::ffi::c_double) -> core::ffi::c_double {
            $call(x)
        }
    };
}

macro_rules! unaryf {
    ($name:ident, $call:path) => {
        /// Scalar math entry point.
        #[unsafe(no_mangle)]
        pub extern "C" fn $name(x: core::ffi::c_float) -> core::ffi::c_float {
            $call(x)
        }
    };
}

macro_rules! binary {
    ($name:ident, $call:path) => {
        /// Binary scalar math entry point.
        #[unsafe(no_mangle)]
        pub extern "C" fn $name(
            x: core::ffi::c_double,
            y: core::ffi::c_double,
        ) -> core::ffi::c_double {
            $call(x, y)
        }
    };
}

macro_rules! binaryf {
    ($name:ident, $call:path) => {
        /// Binary scalar math entry point.
        #[unsafe(no_mangle)]
        pub extern "C" fn $name(
            x: core::ffi::c_float,
            y: core::ffi::c_float,
        ) -> core::ffi::c_float {
            $call(x, y)
        }
    };
}

macro_rules! scalbn {
    ($name:ident, $call:path) => {
        /// `scalbn`-family entry point.
        #[unsafe(no_mangle)]
        pub extern "C" fn $name(
            x: core::ffi::c_double,
            n: core::ffi::c_int,
        ) -> core::ffi::c_double {
            $call(x, n)
        }
    };
}

macro_rules! scalbnf {
    ($name:ident, $call:path) => {
        /// `scalbnf`-family entry point.
        #[unsafe(no_mangle)]
        pub extern "C" fn $name(x: core::ffi::c_float, n: core::ffi::c_int) -> core::ffi::c_float {
            $call(x, n)
        }
    };
}

mod m01;
mod m02;
mod m03;
mod m04;
mod m05;
mod m06;
mod m07;
mod m08;
mod m09;
mod m10;
mod m11;
mod m12;
mod m13;
mod m14;
mod m15;
mod m16;
mod m17;
mod m18;
mod m19;
mod m20;
mod m21;
mod m22;
mod m23;
mod m24;
mod m25;
mod m26;
mod m27;
mod m28;
mod m29;
mod m30;
mod m31;
mod s01;
mod s02;
mod s03;
mod s04;
mod s05;
mod s06;
mod s07;

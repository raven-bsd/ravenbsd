// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `iswalnum_l`, `iswalpha_l`, `iswblank_l` and `iswcntrl_l`.

use core::ffi::{c_int, c_void};

macro_rules! classify_l {
    ($name:ident, $call:path) => {
        /// Locale-aware classification (the locale object is accepted and ignored).
        #[unsafe(no_mangle)]
        pub extern "C" fn $name(wc: u32, _locale: *mut c_void) -> c_int {
            c_int::from($call(wc))
        }
    };
}

classify_l!(iswalnum_l, raven_libc_ctype::iswalnum);
classify_l!(iswalpha_l, raven_libc_ctype::iswalpha);
classify_l!(iswblank_l, raven_libc_ctype::iswblank);
classify_l!(iswcntrl_l, raven_libc_ctype::iswcntrl);

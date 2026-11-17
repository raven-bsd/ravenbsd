// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `iswpunct_l`, `iswspace_l`, `iswupper_l` and `iswxdigit_l`.

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

classify_l!(iswpunct_l, raven_libc_ctype::iswpunct);
classify_l!(iswspace_l, raven_libc_ctype::iswspace);
classify_l!(iswupper_l, raven_libc_ctype::iswupper);
classify_l!(iswxdigit_l, raven_libc_ctype::iswxdigit);

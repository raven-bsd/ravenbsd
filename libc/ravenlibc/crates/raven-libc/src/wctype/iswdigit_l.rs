// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `iswdigit_l`, `iswgraph_l`, `iswlower_l` and `iswprint_l`.

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

classify_l!(iswdigit_l, raven_libc_ctype::iswdigit);
classify_l!(iswgraph_l, raven_libc_ctype::iswgraph);
classify_l!(iswlower_l, raven_libc_ctype::iswlower);
classify_l!(iswprint_l, raven_libc_ctype::iswprint);

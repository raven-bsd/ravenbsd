// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! Strong `__rl_fmt_wide`: one `wchar_t` to its UTF-8 bytes through `raven-libc-ctype`.
//!
//! The dummy skips the argument (the hook table); the strong member is pulled by the `wprintf`
//! family and by `-u __rl_fmt_wide_impl`.

use raven_libc_ctype::MbMode;
use raven_libc_ctype::utf8::wcrtomb;

/// The strong wide-character hook; returns the byte count or 0.
#[unsafe(no_mangle)]
pub fn __rl_fmt_wide_impl(wc: u32, out: &mut [u8; 4]) -> usize {
    wcrtomb(MbMode::Utf8, wc, out).unwrap_or(0)
}

/// Install the strong hook.
pub(crate) fn install() {
    crate::stdio::vprintf::__rl_fmt_wide::install(Some(__rl_fmt_wide_impl));
}

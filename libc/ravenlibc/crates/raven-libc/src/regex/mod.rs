// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `<regex.h>` exports over C45's `raven-libc-regex` engine (C60c).
//!
//! The shell owns `regex_t` (eight opaque words) and the C `regmatch_t` array; the engine owns
//! the parser and both matchers. A `Regex` value lives in a `Box` whose address is stored in
//! `regex_t.__opaque[0]`; word 1 is the validity mark `regfree` checks. The locale seam is
//! [`ShellLocale`], a shell selection of C45's UTF-8 implementation (H-3; C48/C60b replaces the
//! selection when the locale crate lands).

pub(crate) mod regcomp;
pub(crate) mod regerror;
pub(crate) mod regexec;
pub(crate) mod regfree;

use raven_libc_ctype::MbState;
use raven_libc_regex::{MbResult, RegMatch, Regex, RegexLocale, Utf8Locale, Wctype};

/// The shell's locale selection: the UTF-8 locale of H-3.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ShellLocale;

impl RegexLocale for ShellLocale {
    fn mb_cur_max(&self) -> usize {
        Utf8Locale.mb_cur_max()
    }

    fn mbrtowc(&self, s: &[u8], st: &mut MbState) -> MbResult {
        Utf8Locale.mbrtowc(s, st)
    }

    fn wctype(&self, name: &[u8]) -> Option<Wctype> {
        Utf8Locale.wctype(name)
    }

    fn iswctype(&self, c: u32, t: Wctype) -> bool {
        Utf8Locale.iswctype(c, t)
    }

    fn iswalpha(&self, c: u32) -> bool {
        Utf8Locale.iswalpha(c)
    }

    fn iswupper(&self, c: u32) -> bool {
        Utf8Locale.iswupper(c)
    }

    fn iswlower(&self, c: u32) -> bool {
        Utf8Locale.iswlower(c)
    }

    fn iswalnum(&self, c: u32) -> bool {
        Utf8Locale.iswalnum(c)
    }

    fn towlower(&self, c: u32) -> u32 {
        Utf8Locale.towlower(c)
    }

    fn towupper(&self, c: u32) -> u32 {
        Utf8Locale.towupper(c)
    }
}

/// Translate the public `<regex.h>` `regcomp` flags into C45's octal values.
///
/// The public header keeps the hex values (`REG_NOSPEC 0x0020`, `REG_PEND 0x0040`,
/// `REG_DUMP 0x0200`, `REG_GNU 0x0400`); NetBSD's engine uses the octal spellings
/// (`0o20`, `0o40`, `0o200`, `0o400`). The error codes agree.
pub(crate) fn cflags(flags: core::ffi::c_int) -> u32 {
    let mut out = 0u32;
    if flags & 0x0001 != 0 {
        out |= raven_libc_regex::consts::REG_EXTENDED;
    }
    if flags & 0x0002 != 0 {
        out |= raven_libc_regex::consts::REG_ICASE;
    }
    if flags & 0x0004 != 0 {
        out |= raven_libc_regex::consts::REG_NOSUB;
    }
    if flags & 0x0010 != 0 {
        out |= raven_libc_regex::consts::REG_NEWLINE;
    }
    if flags & 0x0020 != 0 {
        out |= raven_libc_regex::consts::REG_NOSPEC;
    }
    if flags & 0x0040 != 0 {
        out |= raven_libc_regex::consts::REG_PEND;
    }
    if flags & 0x0200 != 0 {
        out |= raven_libc_regex::consts::REG_DUMP;
    }
    if flags & 0x0400 != 0 {
        out |= raven_libc_regex::consts::REG_GNU;
    }
    out
}

/// Translate the public `<regex.h>` `regexec` flags into C45's octal values.
pub(crate) fn eflags(flags: core::ffi::c_int) -> u32 {
    let mut out = 0u32;
    if flags & 0x0001 != 0 {
        out |= raven_libc_regex::consts::REG_NOTBOL;
    }
    if flags & 0x0002 != 0 {
        out |= raven_libc_regex::consts::REG_NOTEOL;
    }
    if flags & 0x0004 != 0 {
        out |= raven_libc_regex::consts::REG_STARTEND;
    }
    if flags & 0x0400 != 0 {
        out |= raven_libc_regex::consts::REG_TRACE;
    }
    if flags & 0x1000 != 0 {
        out |= raven_libc_regex::consts::REG_LARGE;
    }
    if flags & 0x2000 != 0 {
        out |= raven_libc_regex::consts::REG_BACKR;
    }
    out
}

/// Translate `regerror`'s `REG_ITOA` bit from the public header value.
pub(crate) fn error_code(code: core::ffi::c_int) -> core::ffi::c_int {
    if code & 0x0400 != 0 {
        (code & !0x0400) | raven_libc_regex::consts::REG_ITOA
    } else {
        code
    }
}

/// The C `regex_t`: eight opaque `unsigned long` words.
pub(crate) type RegexT = [usize; 8];

/// The C `regmatch_t` (`regoff_t` is `i64`).
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct RegMatchC {
    /// Start offset.
    pub rm_so: i64,
    /// End offset.
    pub rm_eo: i64,
}

/// The validity mark in `regex_t.__opaque[1]`.
const REGEX_MARK: usize = 0x5245_4e31;

/// Store a compiled expression in a caller's `regex_t`.
///
/// # Safety
/// `slot` points to the eight writable words of a `regex_t`.
pub(crate) unsafe fn store(slot: *mut usize, re: Regex) {
    let raw = alloc::boxed::Box::into_raw(alloc::boxed::Box::new(re));
    // SAFETY: the caller's regex_t has eight words; words 0 and 1 are written.
    unsafe {
        *slot = raw as usize;
        *slot.add(1) = REGEX_MARK;
    }
}

/// The expression stored in a `regex_t`, or `None` when the mark is absent.
///
/// # Safety
/// `slot` points to eight readable words of a `regex_t`.
pub(crate) unsafe fn load<'a>(slot: *const usize) -> Option<&'a Regex> {
    if slot.is_null() {
        return None;
    }
    // SAFETY: the caller's regex_t has eight words.
    if unsafe { *slot.add(1) } != REGEX_MARK {
        return None;
    }
    // SAFETY: the mark means word 0 is a Box<Regex> pointer written by `store`.
    let raw = unsafe { *slot } as *const Regex;
    if raw.is_null() {
        return None;
    }
    // SAFETY: the Box is alive until `take`/`regfree`.
    Some(unsafe { &*raw })
}

/// Remove the expression from a `regex_t` and clear the mark.
///
/// # Safety
/// `slot` points to eight writable words of a `regex_t`.
pub(crate) unsafe fn take(slot: *mut usize) -> Option<alloc::boxed::Box<Regex>> {
    if slot.is_null() {
        return None;
    }
    // SAFETY: the caller's regex_t has eight words.
    if unsafe { *slot.add(1) } != REGEX_MARK {
        return None;
    }
    // SAFETY: the mark means word 0 is a Box<Regex> pointer written by `store`.
    let raw = unsafe { *slot } as *mut Regex;
    // SAFETY: as above; the mark is cleared before the caller can call again.
    unsafe {
        *slot = 0;
        *slot.add(1) = 0;
    }
    if raw.is_null() {
        None
    } else {
        // SAFETY: the pointer came from `Box::into_raw` in `store` and is taken exactly once.
        Some(unsafe { alloc::boxed::Box::from_raw(raw) })
    }
}

/// Clear a `regex_t` on a failed `regcomp`.
///
/// # Safety
/// `slot` points to eight writable words of a `regex_t`.
pub(crate) unsafe fn clear(slot: *mut usize) {
    if slot.is_null() {
        return;
    }
    // SAFETY: the caller's regex_t has eight words.
    unsafe {
        *slot = 0;
        *slot.add(1) = 0;
    }
}

/// Read the C match array into the engine's representation.
///
/// # Safety
/// `pmatch` is NULL or points to `nmatch` readable `regmatch_t` values.
pub(crate) unsafe fn copy_matches(
    pmatch: *const RegMatchC,
    nmatch: usize,
) -> alloc::vec::Vec<RegMatch> {
    let mut out = alloc::vec::Vec::new();
    if pmatch.is_null() || nmatch == 0 {
        return out;
    }
    out.reserve(nmatch);
    let mut index = 0usize;
    while index < nmatch {
        // SAFETY: the caller's array has `nmatch` entries.
        let item = unsafe { core::ptr::read_unaligned(pmatch.add(index)) };
        out.push(RegMatch {
            rm_so: item.rm_so,
            rm_eo: item.rm_eo,
        });
        index = index.wrapping_add(1);
    }
    out
}

/// Write the engine's matches back to the C array.
///
/// # Safety
/// `pmatch` is NULL or points to `matches.len()` writable `regmatch_t` values.
pub(crate) unsafe fn write_matches(pmatch: *mut RegMatchC, matches: &[RegMatch]) {
    if pmatch.is_null() {
        return;
    }
    let mut index = 0usize;
    while index < matches.len() {
        if let Some(item) = matches.get(index) {
            let value = RegMatchC {
                rm_so: item.rm_so,
                rm_eo: item.rm_eo,
            };
            // SAFETY: the caller's array has `matches.len()` entries.
            unsafe { core::ptr::write_unaligned(pmatch.add(index), value) };
        }
        index = index.wrapping_add(1);
    }
}

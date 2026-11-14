// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `<glob.h>`, `<fnmatch.h>` and `<wordexp.h>` exports over C46's `raven-libc-glob`
//! (C60c). The shell owns the C `glob_t`/`wordexp_t` arrays, the `DIR` handles of the path
//! seam and the C callbacks; C46 owns the matching, expansion and directory-walk logic.
//!
//! `ShellGlobDir` builds on C46's own `DirStream` through [`crate::platform::ShellDirSys`], so
//! there is one directory parser in the tree. `ShellRun` is the C64 hand-over point: until
//! `posix_spawn` lands, `wordexp` returns `WRDE_NOSYS` instead of pretending to expand.

pub(crate) mod fnmatch;
pub(crate) mod glob;
pub(crate) mod globfree;
pub(crate) mod wordexp;
pub(crate) mod wordfree;

use core::ffi::{c_char, c_int, c_void};

use raven_libc_abi::errno::Errno;
use raven_libc_abi::structs::Stat;
use raven_libc_fts::DirStream;
use raven_libc_glob::{
    DirHandle, DirSys, GlobEnv, GlobState, MbResult, PatternLocale, ShellRun, Wctype, WordExp,
};

/// The shell's pattern locale: the UTF-8 locale of H-3.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ShellPatternLocale;

impl PatternLocale for ShellPatternLocale {
    fn mb_cur_max(&self) -> usize {
        raven_libc_glob::Utf8Locale.mb_cur_max()
    }

    fn mbtowc(&self, s: &[u8]) -> MbResult {
        raven_libc_glob::Utf8Locale.mbtowc(s)
    }

    fn wctype(&self, name: &[u8]) -> Option<Wctype> {
        raven_libc_glob::Utf8Locale.wctype(name)
    }

    fn iswctype(&self, c: u32, t: Wctype) -> bool {
        raven_libc_glob::Utf8Locale.iswctype(c, t)
    }

    fn towupper(&self, c: u32) -> u32 {
        raven_libc_glob::Utf8Locale.towupper(c)
    }

    fn towlower(&self, c: u32) -> u32 {
        raven_libc_glob::Utf8Locale.towlower(c)
    }
}

/// The C `glob_t`.
#[repr(C)]
pub(crate) struct GlobT {
    /// `gl_pathc`.
    pub gl_pathc: usize,
    /// `gl_pathv`.
    pub gl_pathv: *mut *mut c_char,
    /// `gl_offs`.
    pub gl_offs: usize,
    /// `__dummy1`.
    pub dummy1: c_int,
    /// `__dummy2`.
    pub dummy2: [*mut c_void; 5],
}

/// The C `wordexp_t`.
#[repr(C)]
pub(crate) struct WordExpT {
    /// `we_wordc`.
    pub we_wordc: usize,
    /// `we_wordv`.
    pub we_wordv: *mut *mut c_char,
    /// `we_offs`.
    pub we_offs: usize,
}

/// The mark that says `glob_t.__dummy2[0]` is our [`ShellGlob`].
const GLOB_MARK: usize = 0x474c_4f42;

/// The persistent `glob` state behind a C `glob_t`.
pub(crate) struct ShellGlob {
    /// The engine state.
    pub state: GlobState,
    /// The published C array.
    pub pathv: *mut *mut c_char,
    /// The number of published path pointers (excluding `offs` and the final NULL).
    pub count: usize,
    /// The leading NULL slots of the published array.
    pub alloc_offs: usize,
}

impl ShellGlob {
    /// A fresh state.
    fn new() -> Self {
        Self {
            state: GlobState::default(),
            pathv: core::ptr::null_mut(),
            count: 0,
            alloc_offs: 0,
        }
    }

    /// Free the published C array and its strings.
    fn free_paths(&mut self) {
        if self.pathv.is_null() {
            return;
        }
        let mut index = 0usize;
        while index < self.count {
            let slot = self.alloc_offs.saturating_add(index);
            // SAFETY: the array has `alloc_offs + count + 1` entries.
            let pointer = unsafe { *self.pathv.add(slot) };
            if !pointer.is_null() {
                // SAFETY: each string was allocated by `copy_c_path`.
                unsafe { crate::malloc::free::free(pointer.cast::<c_void>()) };
            }
            index = index.wrapping_add(1);
        }
        // SAFETY: the array was allocated by `abi_alloc` in `publish`.
        unsafe { crate::malloc::free::free(self.pathv.cast::<c_void>()) };
        self.pathv = core::ptr::null_mut();
        self.count = 0;
        self.alloc_offs = 0;
    }

    /// Rebuild the C array from `state.pathv`.
    fn publish(&mut self, pglob: *mut GlobT) -> i32 {
        self.free_paths();
        let count = self.state.pathv.len();
        let offs = self.state.offs;
        let total = offs
            .saturating_add(count)
            .saturating_add(1)
            .saturating_mul(core::mem::size_of::<*mut c_char>());
        // SAFETY: the shell allocator returns `total` writable bytes or NULL.
        let array = unsafe { crate::malloc::abi_alloc(total) } as *mut *mut c_char;
        if array.is_null() {
            return raven_libc_glob::consts::GLOB_NOSPACE;
        }
        let mut slot = 0usize;
        while slot < offs.saturating_add(count).saturating_add(1) {
            // SAFETY: the array has `total / 8` slots.
            unsafe { *array.add(slot) = core::ptr::null_mut() };
            slot = slot.wrapping_add(1);
        }
        let mut index = 0usize;
        while index < count {
            let Some(path) = self.state.pathv.get(index) else {
                break;
            };
            // SAFETY: allocating a NUL-terminated copy of the match.
            let text = unsafe { crate::platform::copy_c_path(path) };
            if text.is_null() {
                self.pathv = array;
                self.count = index;
                self.alloc_offs = offs;
                self.free_paths();
                return raven_libc_glob::consts::GLOB_NOSPACE;
            }
            // SAFETY: the entry is inside the array.
            unsafe { *array.add(offs.saturating_add(index)) = text };
            index = index.wrapping_add(1);
        }
        self.pathv = array;
        self.count = count;
        self.alloc_offs = offs;
        // SAFETY: the caller's glob_t is writable.
        unsafe {
            (*pglob).gl_pathc = count;
            (*pglob).gl_pathv = array;
            (*pglob).gl_offs = offs;
            (*pglob).dummy2[0] = self as *mut Self as *mut c_void;
            (*pglob).dummy2[1] = GLOB_MARK as *mut c_void;
        }
        0
    }
}

/// The shell state of a `glob_t`, creating it on first use.
///
/// # Safety
/// `pglob` points to a writable `glob_t`.
pub(crate) unsafe fn state_of(pglob: *mut GlobT) -> Option<&'static mut ShellGlob> {
    if pglob.is_null() {
        return None;
    }
    // SAFETY: the caller's glob_t.
    let mark = unsafe { (*pglob).dummy2[1] } as usize;
    if mark == GLOB_MARK {
        // SAFETY: the mark means slot 0 is a Box pointer written below.
        let raw = unsafe { (*pglob).dummy2[0] } as *mut ShellGlob;
        if !raw.is_null() {
            return Some(unsafe { &mut *raw });
        }
    }
    let state = alloc::boxed::Box::into_raw(alloc::boxed::Box::new(ShellGlob::new()));
    // SAFETY: the caller's glob_t; the mark is published with the pointer.
    unsafe {
        (*pglob).dummy2[0] = state as *mut c_void;
        (*pglob).dummy2[1] = GLOB_MARK as *mut c_void;
        (*pglob).gl_pathc = 0;
        (*pglob).gl_pathv = core::ptr::null_mut();
    }
    Some(unsafe { &mut *state })
}

/// Take the shell state of a `glob_t` for `globfree`.
///
/// # Safety
/// `pglob` is NULL or points to a `glob_t`.
pub(crate) unsafe fn take_state(pglob: *mut GlobT) -> Option<alloc::boxed::Box<ShellGlob>> {
    if pglob.is_null() {
        return None;
    }
    // SAFETY: the caller's glob_t.
    if unsafe { (*pglob).dummy2[1] } as usize != GLOB_MARK {
        return None;
    }
    // SAFETY: the mark means slot 0 is a Box pointer.
    let raw = unsafe { (*pglob).dummy2[0] } as *mut ShellGlob;
    // SAFETY: the caller's glob_t; clear the mark before dropping.
    unsafe {
        (*pglob).dummy2[0] = core::ptr::null_mut();
        (*pglob).dummy2[1] = core::ptr::null_mut();
        (*pglob).gl_pathc = 0;
        (*pglob).gl_pathv = core::ptr::null_mut();
    }
    if raw.is_null() {
        None
    } else {
        // SAFETY: the pointer came from `Box::into_raw` in `state_of`.
        Some(unsafe { alloc::boxed::Box::from_raw(raw) })
    }
}

/// The path-based `glob` seam over C46's descriptor stream.
pub(crate) struct ShellGlobDir {
    streams: alloc::vec::Vec<Option<DirStream>>,
    sys: crate::platform::ShellDirSys,
}

impl ShellGlobDir {
    /// A fresh seam.
    pub(crate) fn new() -> Self {
        Self {
            streams: alloc::vec::Vec::new(),
            sys: crate::platform::ShellDirSys,
        }
    }
}

impl DirSys for ShellGlobDir {
    fn opendir(&mut self, path: &[u8]) -> Result<DirHandle, Errno> {
        let stream = DirStream::opendir(&mut self.sys, path)?;
        let index = self.streams.len();
        self.streams.push(Some(stream));
        Ok(DirHandle(index))
    }

    fn readdir(&mut self, d: &mut DirHandle) -> Result<Option<alloc::vec::Vec<u8>>, Errno> {
        let Some(stream) = self.streams.get_mut(d.0).and_then(|slot| slot.as_mut()) else {
            return Err(Errno(raven_libc_abi::errno::EBADF as u16));
        };
        let entry = stream.readdir(&mut self.sys)?;
        Ok(entry.map(|item| item.name))
    }

    fn closedir(&mut self, d: DirHandle) {
        let Some(slot) = self.streams.get_mut(d.0) else {
            return;
        };
        if let Some(stream) = slot.take() {
            let _ = stream.closedir(&mut self.sys);
        }
    }

    fn lstat(&mut self, path: &[u8]) -> Result<Stat, Errno> {
        crate::ffi::platform::global()
            .lstat(path)
            .map_err(crate::platform::map_error)
    }

    fn stat(&mut self, path: &[u8]) -> Result<Stat, Errno> {
        crate::ffi::platform::global()
            .stat(path)
            .map_err(crate::platform::map_error)
    }
}

/// The environment/user-database seam of `GLOB_TILDE`.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ShellGlobEnv;

impl GlobEnv for ShellGlobEnv {
    fn secure_getenv_home(&self) -> Option<alloc::vec::Vec<u8>> {
        crate::stdlib::env_support::secure(b"HOME").map(|value| value.to_vec())
    }

    fn getlogin(&self) -> Option<alloc::vec::Vec<u8>> {
        None
    }

    fn getpwnam_dir(&self, _name: &[u8]) -> Option<alloc::vec::Vec<u8>> {
        None
    }

    fn getpwuid_self_dir(&self) -> Option<alloc::vec::Vec<u8>> {
        None
    }
}

/// The C64 hand-over shell runner: `posix_spawn` does not exist yet, so the call fails.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ShellRunner {
    /// Set when the engine actually asked for a shell.
    pub called: bool,
}

impl ShellRun for ShellRunner {
    fn run(
        &mut self,
        _script: &[u8],
        _arg: &[u8],
        _show_stderr: bool,
    ) -> Result<alloc::vec::Vec<u8>, Errno> {
        self.called = true;
        Err(Errno(raven_libc_abi::errno::ENOSYS as u16))
    }
}

/// Read one C string array into NUL-free words.
///
/// # Safety
/// `array` is NULL or a NULL-terminated array of `count` readable C strings.
pub(crate) unsafe fn words_of(
    array: *mut *mut c_char,
    offs: usize,
    count: usize,
) -> alloc::vec::Vec<alloc::vec::Vec<u8>> {
    let mut words = alloc::vec::Vec::new();
    if array.is_null() {
        return words;
    }
    let mut index = 0usize;
    while index < count {
        // SAFETY: the array has `offs + count` readable slots.
        let pointer = unsafe { *array.add(offs.saturating_add(index)) };
        let text = if pointer.is_null() {
            alloc::vec::Vec::new()
        } else {
            // SAFETY: the slot is a NUL-terminated string.
            unsafe { crate::ffi::cstr::slice(pointer) }.to_vec()
        };
        words.push(text);
        index = index.wrapping_add(1);
    }
    words
}

/// Publish a `WordExp` result as the C `wordexp_t` arrays.
///
/// # Safety
/// `out` points to a writable `wordexp_t`.
pub(crate) unsafe fn publish_words(out: *mut WordExpT, engine: &WordExp) -> i32 {
    let count = engine.words.len();
    let offs = engine.offs;
    let total = offs
        .saturating_add(count)
        .saturating_add(1)
        .saturating_mul(core::mem::size_of::<*mut c_char>());
    // SAFETY: the shell allocator returns `total` writable bytes or NULL.
    let array = unsafe { crate::malloc::abi_alloc(total) } as *mut *mut c_char;
    if array.is_null() {
        return raven_libc_glob::consts::WRDE_NOSPACE;
    }
    let mut slot = 0usize;
    while slot < offs.saturating_add(count).saturating_add(1) {
        // SAFETY: the array has the computed number of slots.
        unsafe { *array.add(slot) = core::ptr::null_mut() };
        slot = slot.wrapping_add(1);
    }
    let mut index = 0usize;
    while index < count {
        let Some(word) = engine.words.get(index) else {
            break;
        };
        // SAFETY: allocating a NUL-terminated copy of the word.
        let text = unsafe { crate::platform::copy_c_path(word) };
        if text.is_null() {
            // SAFETY: free the partial array and its strings.
            unsafe { free_words(array, offs, index) };
            return raven_libc_glob::consts::WRDE_NOSPACE;
        }
        // SAFETY: the entry is inside the array.
        unsafe { *array.add(offs.saturating_add(index)) = text };
        index = index.wrapping_add(1);
    }
    // SAFETY: the caller's wordexp_t.
    unsafe {
        (*out).we_wordc = count;
        (*out).we_wordv = array;
        (*out).we_offs = offs;
    }
    0
}

/// Free `count` strings of a C array plus the array itself.
///
/// # Safety
/// `array` is a `NULL`-terminated or partial array with `offs + count + 1` slots.
pub(crate) unsafe fn free_words(array: *mut *mut c_char, offs: usize, count: usize) {
    if array.is_null() {
        return;
    }
    let mut index = 0usize;
    while index < count {
        // SAFETY: the array has `offs + count + 1` slots.
        let pointer = unsafe { *array.add(offs.saturating_add(index)) };
        if !pointer.is_null() {
            // SAFETY: each string was allocated by `copy_c_path`.
            unsafe { crate::malloc::free::free(pointer.cast::<c_void>()) };
        }
        index = index.wrapping_add(1);
    }
    // SAFETY: the array was allocated by `abi_alloc`.
    unsafe { crate::malloc::free::free(array.cast::<c_void>()) };
}

#[allow(dead_code)]
const _: () = assert!(core::mem::size_of::<GlobT>() == 72);
#[allow(dead_code)]
const _: () = assert!(core::mem::size_of::<WordExpT>() == 24);

/// Translate the public `<fnmatch.h>` flags into C46's FreeBSD values.
///
/// The public header keeps the musl values (`FNM_PATHNAME 0x1`, `FNM_NOESCAPE 0x2`); C46 uses
/// the FreeBSD header's swapped pair. Error codes agree (`FNM_NOMATCH == 1`).
pub(crate) fn fnmatch_flags(flags: c_int) -> u32 {
    let mut out = 0u32;
    if flags & 0x1 != 0 {
        out |= raven_libc_glob::consts::FNM_PATHNAME;
    }
    if flags & 0x2 != 0 {
        out |= raven_libc_glob::consts::FNM_NOESCAPE;
    }
    if flags & 0x4 != 0 {
        out |= raven_libc_glob::consts::FNM_PERIOD;
    }
    if flags & 0x8 != 0 {
        out |= raven_libc_glob::consts::FNM_LEADING_DIR;
    }
    if flags & 0x10 != 0 {
        out |= raven_libc_glob::consts::FNM_CASEFOLD;
    }
    out
}

/// Translate the public `<glob.h>` flags into C46's FreeBSD values.
pub(crate) fn glob_flags(flags: c_int) -> u32 {
    let mut out = 0u32;
    if flags & 0x01 != 0 {
        out |= raven_libc_glob::consts::GLOB_ERR;
    }
    if flags & 0x02 != 0 {
        out |= raven_libc_glob::consts::GLOB_MARK;
    }
    if flags & 0x04 != 0 {
        out |= raven_libc_glob::consts::GLOB_NOSORT;
    }
    if flags & 0x08 != 0 {
        out |= raven_libc_glob::consts::GLOB_DOOFFS;
    }
    if flags & 0x10 != 0 {
        out |= raven_libc_glob::consts::GLOB_NOCHECK;
    }
    if flags & 0x20 != 0 {
        out |= raven_libc_glob::consts::GLOB_APPEND;
    }
    if flags & 0x40 != 0 {
        out |= raven_libc_glob::consts::GLOB_NOESCAPE;
    }
    if flags & 0x1000 != 0 {
        out |= raven_libc_glob::consts::GLOB_TILDE;
    }
    if flags & 0x0080 != 0 {
        out |= raven_libc_glob::consts::GLOB_BRACE;
    }
    if flags & 0x0400 != 0 {
        out |= raven_libc_glob::consts::GLOB_QUOTE;
    }
    out
}

/// Translate a C46 glob result into the public `<glob.h>` code.
pub(crate) fn glob_code(code: i32) -> c_int {
    match code {
        0 => 0,
        raven_libc_glob::consts::GLOB_NOSPACE => 1,
        raven_libc_glob::consts::GLOB_ABORTED => 2,
        raven_libc_glob::consts::GLOB_NOMATCH => 3,
        _ => 4,
    }
}

/// Translate the public `<wordexp.h>` flags into C46's values.
pub(crate) fn wordexp_flags(flags: c_int) -> u32 {
    let mut out = 0u32;
    if flags & 1 != 0 {
        out |= raven_libc_glob::consts::WRDE_DOOFFS;
    }
    if flags & 2 != 0 {
        out |= raven_libc_glob::consts::WRDE_APPEND;
    }
    if flags & 4 != 0 {
        out |= raven_libc_glob::consts::WRDE_NOCMD;
    }
    if flags & 8 != 0 {
        out |= raven_libc_glob::consts::WRDE_REUSE;
    }
    if flags & 16 != 0 {
        out |= raven_libc_glob::consts::WRDE_SHOWERR;
    }
    if flags & 32 != 0 {
        out |= raven_libc_glob::consts::WRDE_UNDEF;
    }
    out
}

/// Translate a C46 wordexp result into the public `<wordexp.h>` code.
pub(crate) fn wordexp_code(code: i32) -> c_int {
    match code {
        0 => 0,
        raven_libc_glob::consts::WRDE_NOSPACE => 1,
        raven_libc_glob::consts::WRDE_BADCHAR => 2,
        raven_libc_glob::consts::WRDE_BADVAL => 3,
        raven_libc_glob::consts::WRDE_CMDSUB => 4,
        raven_libc_glob::consts::WRDE_SYNTAX => 5,
        _ => -1,
    }
}

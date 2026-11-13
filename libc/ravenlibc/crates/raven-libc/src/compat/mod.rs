// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The C-ABI exports of `raven-libc-compat` (ticket C65, `17` section 13 `LC-Wn-shell-<hdr>`).
//!
//! One module per exported symbol (or alias group); the file name is the symbol. Every body is
//! `unsafe` because it converts C pointers to slices, and every block carries a `// SAFETY:`
//! comment. The logic lives in `raven-libc-compat`; this shell only owns the C types, the
//! static buffers the C APIs return and the `errno` writes.
//!
//! The shell is class `U` of `17` section 1. `malloc`/`free` are declared here because the
//! allocator crate (`raven-libc-malloc`, C60+) provides the symbols at final link, not as Rust
//! items. The tty/process helpers declare the platform C entry points for the same reason;
//! C60 owns their final wiring.

#![allow(
    missing_docs,
    dead_code,
    unused_imports,
    unused_unsafe,
    unused_variables,
    unused_mut,
    // The C-ABI glue is pointer arithmetic over caller buffers; the safety argument of every
    // block is the C contract stated on its function. `libc-lint unsafe-scope` still confines
    // `unsafe` to this directory.
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::undocumented_unsafe_blocks,
    clippy::missing_safety_doc,
    clippy::deref_addrof,
    clippy::byte_char_slices,
    clippy::collapsible_if,
    clippy::manual_c_str_literals,
    clippy::needless_return,
    clippy::redundant_closure,
    clippy::unnecessary_cast,
    clippy::let_and_return
)]

use core::ffi::{c_char, c_int, c_void};

pub(crate) const EINVAL: c_int = 22;
pub(crate) const ENOMEM: c_int = 12;
pub(crate) const ERANGE: c_int = 34;
pub(crate) const ENOSPC: c_int = 28;
pub(crate) const ENOSYS: c_int = 38;
pub(crate) const ENOTTY: c_int = 25;
pub(crate) const ENOENT: c_int = 2;
pub(crate) const ENAMETOOLONG: c_int = 36;

unsafe extern "C" {
    /// The libc allocator (implemented by the malloc crate).
    pub(crate) fn malloc(size: usize) -> *mut c_void;
    /// The libc deallocator.
    pub(crate) fn free(pointer: *mut c_void);
    /// The errno cell accessor (C39).
    pub(crate) fn __errno_location() -> *mut c_int;
}

/// Set `errno`.
pub(crate) unsafe fn set_errno(value: c_int) {
    // SAFETY: `__errno_location` returns this thread's valid errno cell.
    unsafe {
        *__errno_location() = value;
    }
}

/// The length of a NUL-terminated C string, bounded by `max`.
pub(crate) unsafe fn c_len(pointer: *const c_char, max: usize) -> usize {
    let mut length = 0usize;
    while length < max {
        // SAFETY: the caller guarantees `pointer` points to at least `length + 1` readable bytes.
        if unsafe { *pointer.add(length) } == 0 {
            break;
        }
        length = length.wrapping_add(1);
    }
    length
}

/// A C string as a byte slice (bounded by `max`).
pub(crate) unsafe fn c_bytes<'a>(pointer: *const c_char, max: usize) -> &'a [u8] {
    if pointer.is_null() {
        return &[];
    }
    // SAFETY: the caller guarantees at least `max` readable bytes or a NUL.
    let length = unsafe { c_len(pointer, max) };
    // SAFETY: `length` bytes were just read as readable and non-NUL.
    unsafe { core::slice::from_raw_parts(pointer.cast::<u8>(), length) }
}

/// A caller's writable buffer.
pub(crate) unsafe fn out_bytes<'a>(pointer: *mut c_char, length: usize) -> &'a mut [u8] {
    // SAFETY: the caller promises `length` writable bytes at `pointer`.
    unsafe { core::slice::from_raw_parts_mut(pointer.cast::<u8>(), length) }
}

/// Write `data` and a NUL to a C buffer (the caller guarantees `data.len() + 1` bytes).
pub(crate) unsafe fn write_cstr(pointer: *mut c_char, data: &[u8]) {
    // SAFETY: the caller promises `data.len() + 1` writable bytes.
    unsafe {
        core::ptr::copy_nonoverlapping(data.as_ptr(), pointer.cast::<u8>(), data.len());
        *pointer.add(data.len()) = 0;
    }
}

/// Allocate a C copy of `data` plus a NUL, or null.
pub(crate) unsafe fn alloc_cstr(data: &[u8]) -> *mut c_char {
    // SAFETY: `malloc` is the libc allocator; the size is `data.len() + 1`.
    let pointer = unsafe { malloc(data.len().wrapping_add(1)) }.cast::<c_char>();
    if pointer.is_null() {
        return pointer;
    }
    // SAFETY: the allocation is `data.len() + 1` bytes.
    unsafe { write_cstr(pointer, data) };
    pointer
}

/// The maximum length of a C string the shell accepts from a caller (1 MiB, ticket 4.1).
pub(crate) const MAX_CSTR: usize = 1024 * 1024;

/// The maximum size of a caller buffer the shell copies a name into.
pub(crate) const MAX_NAME: usize = 4096;

pub mod addmntent;
pub mod backtrace;
pub mod backtrace_symbols;
pub mod backtrace_symbols_fd;
pub mod cgetcap;
pub mod cgetclose;
pub mod cgetent;
pub mod cgetfirst;
pub mod cgetmatch;
pub mod cgetnext;
pub mod cgetnum;
pub mod cgetset;
pub mod cgetstr;
pub mod cgetustr;
pub mod crypt;
pub mod crypt_r;
pub mod devname;
pub mod endmntent;
pub mod endttyent;
pub mod endusershell;
pub mod endutxent;
pub mod expand_number;
pub mod fmt_scaled;
pub mod forkpty;
pub mod fparseln;
pub mod getbsize;
pub mod getcap_state;
pub mod getmntent;
pub mod getmntent_r;
pub mod getmode;
pub mod getttyent;
pub mod getttynam;
pub mod getusershell;
pub mod getutxent;
pub mod getutxid;
pub mod getutxline;
pub mod hasmntopt;
pub mod humanize_number;
pub mod login_tty;
pub mod mntent_state;
pub mod nvis;
pub mod openpty;
pub mod pidfile;
pub mod pty_state;
pub mod pututxline;
pub mod readpassphrase;
pub mod scan_scaled;
pub mod setmntent;
pub mod setmode;
pub mod setttyent;
pub mod setusershell;
pub mod setutxent;
pub mod sl_add;
pub mod sl_find;
pub mod sl_free;
pub mod sl_init;
pub mod snvis;
pub mod strnunvis;
pub mod strnunvisx;
pub mod strnvis;
pub mod strnvisx;
pub mod strsnvis;
pub mod strsnvisx;
pub mod strsvis;
pub mod strsvisx;
pub mod strunvis;
pub mod strunvisx;
pub mod strvis;
pub mod strvisx;
pub mod svis;
pub mod ttyent_state;
pub mod unvis;
pub mod updwtmpx;
pub mod usershell_state;
pub mod uuid_compare;
pub mod uuid_create;
pub mod uuid_create_nil;
pub mod uuid_dec_be;
pub mod uuid_dec_le;
pub mod uuid_enc_be;
pub mod uuid_enc_le;
pub mod uuid_equal;
pub mod uuid_from_string;
pub mod uuid_hash;
pub mod uuid_is_nil;
pub mod uuid_to_string;
pub mod vis;

/// The C `FILE` type (opaque to this shell; the stdio crate owns its layout).
pub(crate) type File = c_void;

unsafe extern "C" {
    pub(crate) fn realloc(pointer: *mut c_void, size: usize) -> *mut c_void;
    pub(crate) fn getenv(name: *const c_char) -> *mut c_char;
    pub(crate) fn open(path: *const c_char, flags: c_int, ...) -> c_int;
    pub(crate) fn close(fd: c_int) -> c_int;
    pub(crate) fn read(fd: c_int, buffer: *mut c_void, count: usize) -> isize;
    pub(crate) fn write(fd: c_int, buffer: *const c_void, count: usize) -> isize;
    pub(crate) fn unlink(path: *const c_char) -> c_int;
    pub(crate) fn getpid() -> c_int;
    pub(crate) fn atexit(hook: extern "C" fn()) -> c_int;
    pub(crate) fn umask(mask: u32) -> u32;
    pub(crate) fn fopen(path: *const c_char, mode: *const c_char) -> *mut File;
    pub(crate) fn fclose(file: *mut File) -> c_int;
    pub(crate) fn fgets(buffer: *mut c_char, size: c_int, file: *mut File) -> *mut c_char;
    pub(crate) fn fgetln(file: *mut File, length: *mut usize) -> *mut c_char;
    pub(crate) fn fseek(file: *mut File, offset: i64, whence: c_int) -> c_int;
    pub(crate) fn fwrite(
        buffer: *const c_void,
        size: usize,
        count: usize,
        file: *mut File,
    ) -> usize;
    pub(crate) fn posix_openpt(flags: c_int) -> c_int;
    pub(crate) fn grantpt(fd: c_int) -> c_int;
    pub(crate) fn unlockpt(fd: c_int) -> c_int;
    pub(crate) fn ptsname_r(fd: c_int, buffer: *mut c_char, length: usize) -> c_int;
    pub(crate) fn tcgetattr(
        fd: c_int,
        termios: *mut raven_libc_abi::generated::structs::Termios,
    ) -> c_int;
    pub(crate) fn tcsetattr(
        fd: c_int,
        actions: c_int,
        termios: *const raven_libc_abi::generated::structs::Termios,
    ) -> c_int;
    pub(crate) fn ioctl(fd: c_int, request: core::ffi::c_ulong, ...) -> c_int;
    pub(crate) fn setsid() -> c_int;
    pub(crate) fn dup2(from: c_int, to: c_int) -> c_int;
    pub(crate) fn fork() -> c_int;
    pub(crate) fn signal(signum: c_int, handler: usize) -> usize;
    pub(crate) fn raise(signum: c_int) -> c_int;
}

/// `TCSAFLUSH`.
pub(crate) const TCSAFLUSH: c_int = 2;
/// `O_RDWR`.
pub(crate) const O_RDWR: c_int = 2;
/// `O_NOCTTY`.
pub(crate) const O_NOCTTY: c_int = 0o400;
/// `TIOCSCTTY`.
pub(crate) const TIOCSCTTY: core::ffi::c_ulong = 0x540e;
/// `TIOCSWINSZ`.
pub(crate) const TIOCSWINSZ: core::ffi::c_ulong = 0x5414;
/// `SEEK_END`.
pub(crate) const SEEK_END: c_int = 2;

// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The audited `unsafe` module (DR-39, class M).
//!
//! Exactly six things need raw pointers: the shareable cell that carries the FILE state, the
//! caller-supplied buffer region, the typed thunks of `fopencookie`/`funopen`, the
//! `Box::into_raw`/`Box::from_raw` pair for heap FILEs, the open-file list's raw link
//! pointers, and the `*buf`/`*size` slots of the memory streams. Every block carries a
//! `// SAFETY:` comment naming the invariant.

use alloc::boxed::Box;
use core::cell::UnsafeCell;
use core::ffi::{c_char, c_int, c_void};

use crate::file::File;
use crate::traits::Env;

/// A cell whose value is mutated under the FILE lock protocol rather than through `&mut`.
///
/// The FILE lock is recursive per thread and the single-thread fast path is a protocol, not a
/// Rust borrow, so the crate cannot express the state as `&mut` without threading a borrowing
/// token through every donor function. The cell concentrates that decision here: the crate
/// documents, per method, which lock contract makes the access exclusive.
pub struct FileCell<T> {
    value: UnsafeCell<T>,
}

// SAFETY: every access to the value goes through the FILE lock or through the exclusive
// construction phase of the stream (`StdStreams::new`, `fopen`). The lock protocol is:
// a thread that calls `get_mut` holds the stream's recursive lock, or is the only thread in
// the process (`Lock::threaded() == false`), or the stream is marked `F_BYCALLER` and the
// caller promised to serialise. `Sync` is what makes `&File` shareable across threads.
unsafe impl<T: Send> Sync for FileCell<T> {}

impl<T> FileCell<T> {
    /// A cell holding `value`; usable in a `static` initialiser.
    pub const fn new(value: T) -> Self {
        Self {
            value: UnsafeCell::new(value),
        }
    }

    /// A shared view.
    ///
    /// The caller holds the FILE lock unless the value is immutable after construction.
    #[allow(clippy::mut_from_ref)] // the exclusive view below is behind `UnsafeCell`
    pub fn get(&self) -> &T {
        // SAFETY: the caller guarantees the lock protocol documented on the type; the
        // reference does not escape the calling operation.
        unsafe { &*self.value.get() }
    }

    /// An exclusive view.
    ///
    /// The caller holds the FILE lock (or owns the stream exclusively).
    #[allow(clippy::mut_from_ref)] // the mutability is the point of the cell
    pub fn get_mut(&self) -> &mut T {
        // SAFETY: as `get`, and no other reference to the value is live in the caller; the
        // crate's operations re-borrow per statement and never hold two views at once.
        unsafe { &mut *self.value.get() }
    }
}

/// A caller-supplied or static byte region.
///
/// The region is never freed by the crate: it belongs to the caller (`setvbuf`, `fmemopen`,
/// `fopen_rb_ca`) or to the `StdStreams` value.
#[derive(Clone, Copy)]
pub struct RawBuf {
    ptr: *mut u8,
    len: usize,
}

// SAFETY: the region is only touched under the FILE lock (or during exclusive construction);
// the pointer itself carries no thread affinity.
unsafe impl Sync for RawBuf {}
// SAFETY: the pointer is a plain address; moving it between threads does not move the region,
// whose access is serialised by the FILE lock.
unsafe impl Send for RawBuf {}

impl RawBuf {
    /// Wrap a caller pointer and length.
    ///
    /// The C contract is that the region is valid for `len` bytes and outlives the stream;
    /// the caller promises it. A raw pair (not `&mut [u8]`) is the only form that models a
    /// region the stream keeps using after the call returns.
    pub fn from_parts(ptr: *mut u8, len: usize) -> Self {
        Self { ptr, len }
    }

    /// The region as a shared slice.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        if self.len == 0 {
            return &[];
        }
        // SAFETY: `ptr` is valid for `len` bytes for the lifetime of the stream (the
        // constructor contract); `len != 0`, so the pointer is non-null.
        unsafe { core::slice::from_raw_parts(self.ptr.cast_const(), self.len) }
    }

    /// The region as a mutable slice.
    #[allow(clippy::mut_from_ref)] // the region is shared state behind the FILE lock
    #[must_use]
    pub fn bytes_mut(&self) -> &mut [u8] {
        if self.len == 0 {
            return &mut [];
        }
        // SAFETY: as `bytes`, and the caller holds the FILE lock so no other view is live.
        unsafe { core::slice::from_raw_parts_mut(self.ptr, self.len) }
    }
}

/// A raw pointer with the `Send`/`Sync` promise of the original C object.
///
/// The pointee is a caller object whose access is serialised by the FILE lock (or by the
/// single-threaded construction phase); the wrapper exists so the backend structs stay
/// `Send` and the FILE cell stays `Sync` without scattering auto-trait impls.
#[repr(transparent)]
pub struct RawPtr<T> {
    pointer: *mut T,
}

impl<T> Clone for RawPtr<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for RawPtr<T> {}

// SAFETY: the wrapper adds no aliasing of its own; every pointee is used under the FILE lock
// or during exclusive construction.
unsafe impl<T> Send for RawPtr<T> {}
// SAFETY: as `Send`.
unsafe impl<T> Sync for RawPtr<T> {}

impl<T> RawPtr<T> {
    /// Wrap a pointer.
    pub const fn new(pointer: *mut T) -> Self {
        Self { pointer }
    }

    /// The pointer.
    #[must_use]
    pub const fn get(self) -> *mut T {
        self.pointer
    }
}

/// A fixed-size buffer stored inside [`crate::StdStreams`].
pub struct BufStore<const N: usize> {
    cell: FileCell<[u8; N]>,
}

impl<const N: usize> BufStore<N> {
    /// A zeroed buffer region, usable in a `static` initialiser.
    pub const fn new() -> Self {
        Self {
            cell: FileCell::new([0; N]),
        }
    }

    /// The region, borrowed for the life of the stream.
    #[must_use]
    pub fn raw(&self) -> RawBuf {
        RawBuf {
            // SAFETY: the array lives in the `StdStreams` value, which the caller keeps in
            // static storage; the FILE lock serialises the accesses.
            ptr: self.cell.get_mut().as_mut_ptr(),
            len: N,
        }
    }
}

impl<const N: usize> Default for BufStore<N> {
    fn default() -> Self {
        Self::new()
    }
}

/// Allocate a heap FILE.
pub fn into_raw<E: Env>(file: Box<File<E>>) -> *mut File<E> {
    Box::into_raw(file)
}

/// Borrow a heap FILE while it is alive.
#[must_use]
pub fn file_ref<'a, E: Env>(file: *const File<E>) -> &'a File<E> {
    // SAFETY: the pointer came from `into_raw`/`Box::into_raw` and the FILE is alive until
    // `free_file`; all FILEs live in static or leaked storage (trap S3).
    unsafe { &*file }
}

/// Free a heap FILE.
pub fn free_file<E: Env>(file: *mut File<E>) {
    // SAFETY: the pointer came from `into_raw` and is freed exactly once by `fclose`.
    drop(unsafe { Box::from_raw(file) });
}

/// Store a pointer into a caller slot (`open_memstream`'s `*buf`).
pub fn store_ptr(dst: *mut *mut u8, value: *mut u8) {
    if !dst.is_null() {
        // SAFETY: the caller passed a valid `char **` (the C signature is unsafe by
        // contract; the shell validates the non-NULL case in C).
        unsafe { *dst = value };
    }
}

/// Store a length into a caller slot (`open_memstream`'s `*size`).
pub fn store_usize(dst: *mut usize, value: usize) {
    if !dst.is_null() {
        // SAFETY: the caller passed a valid `size_t *`.
        unsafe { *dst = value };
    }
}

/// The GNU `cookie_io_functions_t`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct CookieIo {
    /// `ssize_t read(void *cookie, char *buf, size_t size)`.
    pub read: Option<unsafe extern "C" fn(*mut c_void, *mut c_char, usize) -> isize>,
    /// `ssize_t write(void *cookie, const char *buf, size_t size)`.
    pub write: Option<unsafe extern "C" fn(*mut c_void, *const c_char, usize) -> isize>,
    /// `int seek(void *cookie, off64_t *offset, int whence)`.
    pub seek: Option<unsafe extern "C" fn(*mut c_void, *mut i64, c_int) -> c_int>,
    /// `int close(void *cookie)`.
    pub close: Option<unsafe extern "C" fn(*mut c_void) -> c_int>,
}

impl CookieIo {
    /// Call the read callback.
    pub fn call_read(&self, cookie: *mut c_void, buf: &mut [u8]) -> Option<isize> {
        let callback = self.read?;
        // SAFETY: the callback comes from the caller's `cookie_io_functions_t` and the
        // buffer is valid for its length; the cookie is the caller's value.
        Some(unsafe { callback(cookie, buf.as_mut_ptr().cast::<c_char>(), buf.len()) })
    }

    /// Call the write callback.
    pub fn call_write(&self, cookie: *mut c_void, buf: &[u8]) -> Option<isize> {
        let callback = self.write?;
        // SAFETY: as `call_read`; the buffer is read-only.
        Some(unsafe { callback(cookie, buf.as_ptr().cast::<c_char>(), buf.len()) })
    }

    /// Call the seek callback.
    pub fn call_seek(&self, cookie: *mut c_void, off: i64, whence: i32) -> Option<(i32, i64)> {
        let callback = self.seek?;
        let mut offset = off;
        // SAFETY: the callback receives a pointer to a live `i64`, as its C signature says.
        let res = unsafe { callback(cookie, &mut offset, whence) };
        Some((res, offset))
    }

    /// Call the close callback.
    pub fn call_close(&self, cookie: *mut c_void) -> Option<c_int> {
        let callback = self.close?;
        // SAFETY: the cookie is the caller's value.
        Some(unsafe { callback(cookie) })
    }
}

/// The BSD `funopen` callbacks.
#[derive(Clone, Copy, Default)]
pub struct FunIo {
    /// `int readfn(void *cookie, char *buf, int n)`.
    pub read: Option<unsafe extern "C" fn(*mut c_void, *mut c_char, c_int) -> c_int>,
    /// `int writefn(void *cookie, const char *buf, int n)`.
    pub write: Option<unsafe extern "C" fn(*mut c_void, *const c_char, c_int) -> c_int>,
    /// `fpos_t seekfn(void *cookie, fpos_t off, int whence)`.
    pub seek: Option<unsafe extern "C" fn(*mut c_void, i64, c_int) -> i64>,
    /// `int closefn(void *cookie)`.
    pub close: Option<unsafe extern "C" fn(*mut c_void) -> c_int>,
}

impl FunIo {
    /// Call the read callback.
    pub fn call_read(&self, cookie: *mut c_void, buf: &mut [u8]) -> Option<c_int> {
        let callback = self.read?;
        let len = c_int::try_from(buf.len()).unwrap_or(c_int::MAX);
        // SAFETY: the callback and cookie come from `funopen`; the buffer is valid for `len`.
        Some(unsafe { callback(cookie, buf.as_mut_ptr().cast::<c_char>(), len) })
    }

    /// Call the write callback.
    pub fn call_write(&self, cookie: *mut c_void, buf: &[u8]) -> Option<c_int> {
        let callback = self.write?;
        let len = c_int::try_from(buf.len()).unwrap_or(c_int::MAX);
        // SAFETY: as `call_read`; the buffer is read-only.
        Some(unsafe { callback(cookie, buf.as_ptr().cast::<c_char>(), len) })
    }

    /// Call the seek callback.
    pub fn call_seek(&self, cookie: *mut c_void, off: i64, whence: i32) -> Option<i64> {
        let callback = self.seek?;
        // SAFETY: the cookie and the `fpos_t` value come from the caller.
        Some(unsafe { callback(cookie, off, whence) })
    }

    /// Call the close callback.
    pub fn call_close(&self, cookie: *mut c_void) -> Option<c_int> {
        let callback = self.close?;
        // SAFETY: the cookie is the caller's value.
        Some(unsafe { callback(cookie) })
    }
}

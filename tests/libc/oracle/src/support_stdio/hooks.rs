// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The `extern "C"` hook table the compiled musl oracle calls through `__rl_syscall`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use core::ffi::{c_char, c_int, c_long, c_void};
use std::ffi::CStr;
use std::sync::Once;

use raven_libc_abi::generated::errno::{EINVAL, ENOTTY};

use crate::support_stdio::store::{Kind, TraceOp, store};

/// The C layout of `struct rl_stdio_hooks` in `shim/stdio/musl/syscall.h`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Hooks {
    /// `read`.
    pub read: Option<extern "C" fn(c_int, *mut c_void, usize) -> c_long>,
    /// `write`.
    pub write: Option<extern "C" fn(c_int, *const c_void, usize) -> c_long>,
    /// `readv` with two iovecs.
    pub read2: Option<extern "C" fn(c_int, *mut c_void, usize, *mut c_void, usize) -> c_long>,
    /// `writev` with two iovecs.
    pub write2: Option<extern "C" fn(c_int, *const c_void, usize, *const c_void, usize) -> c_long>,
    /// `lseek`.
    pub seek: Option<extern "C" fn(c_int, c_long, c_int) -> c_long>,
    /// `close`.
    pub close: Option<extern "C" fn(c_int) -> c_long>,
    /// `ioctl` (only the tty probe).
    pub ioctl: Option<extern "C" fn(c_int, usize, *mut c_void) -> c_long>,
    /// `fcntl`.
    pub fcntl: Option<extern "C" fn(c_int, c_int, c_long) -> c_long>,
    /// `open`.
    pub open: Option<extern "C" fn(*const c_char, c_int, c_int) -> c_long>,
    /// `unlink`.
    pub unlink: Option<extern "C" fn(*const c_char) -> c_long>,
    /// `rmdir`.
    pub rmdir: Option<extern "C" fn(*const c_char) -> c_long>,
    /// `rename`.
    pub rename: Option<extern "C" fn(*const c_char, *const c_char) -> c_long>,
    /// Optional trace callback (the store records by itself).
    pub trace: Option<extern "C" fn(c_int, c_int, c_long, c_long)>,
}

impl Hooks {
    /// The identity-free default table.
    #[must_use]
    pub fn new() -> Self {
        Self {
            read: Some(hook_read),
            write: Some(hook_write),
            read2: Some(hook_read2),
            write2: Some(hook_write2),
            seek: Some(hook_seek),
            close: Some(hook_close),
            ioctl: Some(hook_ioctl),
            fcntl: Some(hook_fcntl),
            open: Some(hook_open),
            unlink: Some(hook_unlink),
            rmdir: Some(hook_rmdir),
            rename: Some(hook_rename),
            trace: None,
        }
    }
}

impl Default for Hooks {
    fn default() -> Self {
        Self::new()
    }
}

extern "C" fn hook_read(fd: c_int, buf: *mut c_void, len: usize) -> c_long {
    if buf.is_null() || len == 0 {
        return 0;
    }
    let slice = unsafe { core::slice::from_raw_parts_mut(buf.cast::<u8>(), len) };
    match store().lock().unwrap().read(fd, slice) {
        Ok(count) => count as c_long,
        Err(error) => c_long::from(error),
    }
}

extern "C" fn hook_write(fd: c_int, buf: *const c_void, len: usize) -> c_long {
    if buf.is_null() || len == 0 {
        return 0;
    }
    let slice = unsafe { core::slice::from_raw_parts(buf.cast::<u8>(), len) };
    let mut store = store().lock().unwrap();
    let result = store.write(fd, &[slice]);
    if let Ok(count) = result {
        store.trace.push(TraceOp {
            op: 1,
            fd,
            len: i64::try_from(len).unwrap_or(i64::MAX),
            result: count as i64,
        });
    }
    match result {
        Ok(count) => count as c_long,
        Err(error) => c_long::from(error),
    }
}

extern "C" fn hook_read2(
    fd: c_int,
    a: *mut c_void,
    alen: usize,
    b: *mut c_void,
    blen: usize,
) -> c_long {
    let mut total = 0usize;
    if !a.is_null() && alen != 0 {
        let slice = unsafe { core::slice::from_raw_parts_mut(a.cast::<u8>(), alen) };
        match store().lock().unwrap().read(fd, slice) {
            Ok(count) => {
                total += count;
                if count < alen {
                    return total as c_long;
                }
            }
            Err(error) => return c_long::from(error),
        }
    }
    if !b.is_null() && blen != 0 {
        let slice = unsafe { core::slice::from_raw_parts_mut(b.cast::<u8>(), blen) };
        match store().lock().unwrap().read(fd, slice) {
            Ok(count) => total += count,
            Err(error) => return c_long::from(error),
        }
    }
    total as c_long
}

extern "C" fn hook_write2(
    fd: c_int,
    a: *const c_void,
    alen: usize,
    b: *const c_void,
    blen: usize,
) -> c_long {
    let first: &[u8] = if a.is_null() || alen == 0 {
        &[]
    } else {
        unsafe { core::slice::from_raw_parts(a.cast::<u8>(), alen) }
    };
    let second: &[u8] = if b.is_null() || blen == 0 {
        &[]
    } else {
        unsafe { core::slice::from_raw_parts(b.cast::<u8>(), blen) }
    };
    let mut store = store().lock().unwrap();
    let result = store.write(fd, &[first, second]);
    if let Ok(count) = result {
        store.trace.push(TraceOp {
            op: 1,
            fd,
            len: i64::try_from(alen.saturating_add(blen)).unwrap_or(i64::MAX),
            result: count as i64,
        });
    }
    match result {
        Ok(count) => count as c_long,
        Err(error) => c_long::from(error),
    }
}

extern "C" fn hook_seek(fd: c_int, off: c_long, whence: c_int) -> c_long {
    let mut store = store().lock().unwrap();
    let result = store.seek(fd, off, whence);
    if let Ok(position) = result {
        store.trace.push(TraceOp {
            op: 2,
            fd,
            len: off,
            result: position,
        });
    }
    match result {
        Ok(position) => position as c_long,
        Err(error) => c_long::from(error),
    }
}

extern "C" fn hook_close(fd: c_int) -> c_long {
    let mut store = store().lock().unwrap();
    let result = store.close(fd);
    if result.is_ok() {
        store.trace.push(TraceOp {
            op: 3,
            fd,
            len: 0,
            result: 0,
        });
    }
    match result {
        Ok(()) => 0,
        Err(error) => c_long::from(error),
    }
}

extern "C" fn hook_ioctl(fd: c_int, _request: usize, _arg: *mut c_void) -> c_long {
    let store = store().lock().unwrap();
    match store.files.get(&fd) {
        Some(entry) if entry.kind == Kind::Tty => 0,
        _ => c_long::from(-(ENOTTY as i32)),
    }
}

extern "C" fn hook_fcntl(fd: c_int, cmd: c_int, value: c_long) -> c_long {
    match store().lock().unwrap().fcntl(fd, cmd, value) {
        Ok(result) => result as c_long,
        Err(error) => c_long::from(error),
    }
}

extern "C" fn hook_open(path: *const c_char, flags: c_int, mode: c_int) -> c_long {
    if path.is_null() {
        return c_long::from(-(EINVAL as i32));
    }
    let bytes = unsafe { CStr::from_ptr(path) }.to_bytes();
    match store().lock().unwrap().open(bytes, flags, mode as u32) {
        Ok(fd) => c_long::from(fd),
        Err(error) => c_long::from(error),
    }
}

extern "C" fn hook_unlink(_path: *const c_char) -> c_long {
    c_long::from(-(EINVAL as i32))
}

extern "C" fn hook_rmdir(_path: *const c_char) -> c_long {
    c_long::from(-(EINVAL as i32))
}

extern "C" fn hook_rename(_old: *const c_char, _new: *const c_char) -> c_long {
    c_long::from(-(EINVAL as i32))
}

/// Install the table into the loaded oracle.
pub fn install() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| unsafe {
        crate::decls::stdio_musl::oracle_rl_stdio_hooks = Hooks::new();
    });
}

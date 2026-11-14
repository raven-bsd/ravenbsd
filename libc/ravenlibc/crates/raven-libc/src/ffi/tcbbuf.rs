// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The per-thread scratch areas of C64 4.3, as one process-wide object until C57 installs the
//! thread pointer. C60 already ships the `errno` and `strtok` cells the same way; this module
//! is their typed sibling and the single place a later TLS provider has to replace.
//!
//! Buffers: `inet_ntoa` 16 bytes, the `hostent`/`servent`/`protoent`/`netent`/`passwd`/`group`
//! areas 1024 bytes each (`GETPW_R_SIZE_MAX`), and the name/login strings 256 bytes. The
//! non-`_r` user-database functions keep their iteration cursor as one owned pointer in the
//! area (caller state, never a static list). Access is serialised by [`super::lock::SpinLock`]
//! and no lock is held across a file or server call.

use core::cell::UnsafeCell;
use core::ffi::c_char;
use core::mem::MaybeUninit;
use core::sync::atomic::{AtomicU8, Ordering};

use raven_libc_abi::errno::{ERANGE, Errno};
use raven_libc_abi::structs::{Group, Passwd};

use super::lock::SpinLock;

/// The `inet_ntoa` text buffer.
#[repr(C, align(8))]
pub(crate) struct InetNtoa {
    /// NUL-terminated dotted quad.
    pub bytes: [u8; 16],
}

/// The `gethostbyname`-style area (names, aliases, addresses).
#[repr(C, align(8))]
pub(crate) struct HostArea {
    /// The packed host record.
    pub bytes: [u8; 1024],
}

/// The `getservby*` area.
#[repr(C, align(8))]
pub(crate) struct ServArea {
    /// The packed service record.
    pub bytes: [u8; 1024],
}

/// The `getprotoby*` area.
#[repr(C, align(8))]
pub(crate) struct ProtoArea {
    /// The packed protocol record.
    pub bytes: [u8; 1024],
}

/// The `getnetby*` area.
#[repr(C, align(8))]
pub(crate) struct NetArea {
    /// The packed network record.
    pub bytes: [u8; 1024],
}

/// The `getpwnam`-style record and its string buffer.
#[repr(C, align(8))]
pub(crate) struct PwdArea {
    /// The C `struct passwd` the non-`_r` forms return.
    pub record: Passwd,
    /// The packed strings.
    pub buf: [u8; 1024],
    /// The `getpwent` cursor (owned by the cursor helper).
    pub cursor: *mut u8,
}

/// The `getgrnam`-style record and its string buffer.
#[repr(C, align(8))]
pub(crate) struct GrpArea {
    /// The C `struct group` the non-`_r` forms return.
    pub record: Group,
    /// The packed strings and member pointer slots.
    pub buf: [u8; 1024],
    /// The `getgrent` cursor (owned by the cursor helper).
    pub cursor: *mut u8,
}

/// A 256-byte text area for `getlogin`, `user_from_uid` and friends.
#[repr(C, align(8))]
pub(crate) struct TextArea {
    /// NUL-terminated text.
    pub bytes: [u8; 256],
}

/// Every area of the shell.
#[repr(C, align(8))]
pub(crate) struct Area {
    /// `inet_ntoa`.
    pub inet_ntoa: InetNtoa,
    /// Host database.
    pub host: HostArea,
    /// Service database.
    pub serv: ServArea,
    /// Protocol database.
    pub proto: ProtoArea,
    /// Network database.
    pub net: NetArea,
    /// User database.
    pub passwd: PwdArea,
    /// Group database.
    pub group: GrpArea,
    /// Login name.
    pub login: TextArea,
    /// Name cache text.
    pub name: TextArea,
    /// The `getusershell` cursor.
    pub shell: *mut u8,
}

impl Area {
    /// The all-zero area (every buffer empty, every cursor NULL).
    const fn new() -> Self {
        Self {
            inet_ntoa: InetNtoa { bytes: [0; 16] },
            host: HostArea { bytes: [0; 1024] },
            serv: ServArea { bytes: [0; 1024] },
            proto: ProtoArea { bytes: [0; 1024] },
            net: NetArea { bytes: [0; 1024] },
            passwd: PwdArea {
                record: Passwd::ZERO,
                buf: [0; 1024],
                cursor: core::ptr::null_mut(),
            },
            group: GrpArea {
                record: Group::ZERO,
                buf: [0; 1024],
                cursor: core::ptr::null_mut(),
            },
            login: TextArea { bytes: [0; 256] },
            name: TextArea { bytes: [0; 256] },
            shell: core::ptr::null_mut(),
        }
    }
}

const UNINIT: u8 = 0;
const READY: u8 = 2;

struct Cell(UnsafeCell<MaybeUninit<Area>>);

// SAFETY: every access goes through [`with_area`], which serialises readers with `LOCK`; the
// area is written once before `READY` is published with Release (the `ffi::platform` pattern).
unsafe impl Sync for Cell {}

static STATE: AtomicU8 = AtomicU8::new(UNINIT);
static AREA: Cell = Cell(UnsafeCell::new(MaybeUninit::uninit()));
static LOCK: SpinLock = SpinLock::new();

/// Run `f` over the one area.
pub(crate) fn with_area<R>(f: impl FnOnce(&mut Area) -> R) -> R {
    LOCK.acquire();
    if STATE
        .compare_exchange(UNINIT, 1, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
    {
        // SAFETY: this caller won the one-shot initialisation and owns the slot until READY.
        unsafe { (*AREA.0.get()).write(Area::new()) };
        STATE.store(READY, Ordering::Release);
    }
    while STATE.load(Ordering::Acquire) != READY {
        core::hint::spin_loop();
    }
    // SAFETY: READY was published with Release after the write and the storage is a static.
    let area = unsafe { (&mut *AREA.0.get()).assume_init_mut() };
    let result = f(area);
    LOCK.release();
    result
}

/// Copy `bytes` and a NUL into `buf`; `ERANGE` when they do not fit.
pub(crate) fn copy_text(buf: &mut [u8], bytes: &[u8]) -> Result<*mut c_char, Errno> {
    let needed = bytes.len().saturating_add(1);
    if needed > buf.len() {
        return Err(Errno(ERANGE as u16));
    }
    let slot = buf.get_mut(..bytes.len()).ok_or(Errno(ERANGE as u16))?;
    slot.copy_from_slice(bytes);
    if let Some(last) = buf.get_mut(bytes.len()) {
        *last = 0;
    }
    Ok(buf.as_mut_ptr().cast::<c_char>())
}

/// Fill a caller's `struct passwd` and buffer from a crate entry; never a partial record.
pub(crate) fn fill_passwd(
    entry: &raven_libc_pwd::Passwd,
    record: *mut Passwd,
    buf: &mut [u8],
) -> Result<(), Errno> {
    let packed = raven_libc_pwd::pack_passwd(entry, buf).map_err(|_| Errno(ERANGE as u16))?;
    if record.is_null() {
        return Err(Errno(ERANGE as u16));
    }
    let base = buf.as_mut_ptr();
    // SAFETY: `pack_passwd` wrote every offset inside `buf` and the caller supplied a writable
    // `struct passwd`; the pointers name the NUL-terminated strings it just wrote.
    unsafe {
        let target = &mut *record;
        target.pw_name = base.add(packed.name).cast();
        target.pw_passwd = base.add(packed.passwd).cast();
        target.pw_uid = entry.uid;
        target.pw_gid = entry.gid;
        target.pw_gecos = base.add(packed.gecos).cast();
        target.pw_dir = base.add(packed.dir).cast();
        target.pw_shell = base.add(packed.shell).cast();
    }
    Ok(())
}

/// Fill a caller's `struct group` and buffer from a crate entry.
pub(crate) fn fill_group(
    entry: &raven_libc_pwd::Group,
    record: *mut Group,
    buf: &mut [u8],
) -> Result<(), Errno> {
    let packed = raven_libc_pwd::pack_group(entry, buf).map_err(|_| Errno(ERANGE as u16))?;
    if record.is_null() {
        return Err(Errno(ERANGE as u16));
    }
    let slots = entry.mem.len().saturating_add(1);
    let table = slots.saturating_mul(core::mem::size_of::<*mut c_char>());
    let table_start = packed.len.checked_sub(table).ok_or(Errno(ERANGE as u16))?;
    let base = buf.as_mut_ptr();
    let table_ptr = base.wrapping_add(table_start).cast::<*mut c_char>();
    for (index, offset) in packed.mem.iter().enumerate() {
        // SAFETY: `pack_group` reserved `slots` pointer-width slots at `table_start`.
        unsafe {
            table_ptr
                .add(index)
                .write(base.add(*offset).cast::<c_char>())
        };
    }
    // SAFETY: the last reserved slot is the NULL terminator of `gr_mem`.
    unsafe { table_ptr.add(entry.mem.len()).write(core::ptr::null_mut()) };
    // SAFETY: the caller supplied a writable `struct group` and the table is inside `buf`.
    unsafe {
        let target = &mut *record;
        target.gr_name = base.add(packed.name).cast();
        target.gr_passwd = base.add(packed.passwd).cast();
        target.gr_gid = entry.gid;
        target.gr_mem = table_ptr.cast();
    }
    Ok(())
}

/// Write the decimal form of `value` and a NUL into `buf`.
pub(crate) fn write_u32(buf: &mut [u8], value: u32) -> Result<*mut c_char, Errno> {
    let mut digits = [0u8; 10];
    let mut count = 0usize;
    let mut rest = value;
    loop {
        let digit = rest.wrapping_rem(10);
        let slot = digits.get_mut(count).ok_or(Errno(ERANGE as u16))?;
        *slot = b'0'.wrapping_add(digit as u8);
        count = count.saturating_add(1);
        rest = rest.wrapping_div(10);
        if rest == 0 {
            break;
        }
    }
    if count.saturating_add(1) > buf.len() {
        return Err(Errno(ERANGE as u16));
    }
    for index in 0..count {
        let source = digits
            .get(count.saturating_sub(index).saturating_sub(1))
            .copied()
            .ok_or(Errno(ERANGE as u16))?;
        let slot = buf.get_mut(index).ok_or(Errno(ERANGE as u16))?;
        *slot = source;
    }
    if let Some(last) = buf.get_mut(count) {
        *last = 0;
    }
    Ok(buf.as_mut_ptr().cast::<c_char>())
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
#[path = "tcbbuf_ut.rs"]
mod tests;

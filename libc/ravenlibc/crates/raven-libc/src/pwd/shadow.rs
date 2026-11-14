// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! Shadow-file state for the `getsp*` family. C51 owns the parsers; this module owns the
//! process scratch record and cursor, like `pwd::cursor` does for passwd.

use alloc::vec::Vec;
use core::cell::UnsafeCell;
use core::ffi::{c_char, c_long, c_ulong};
use core::mem::MaybeUninit;

use raven_libc_abi::errno::ERANGE;
use raven_libc_nss::FileSource;
use raven_libc_pwd::Spwd as CrateSpwd;

use crate::ffi::lock::SpinLock;

/// The C `struct spwd` layout from `<shadow.h>`.
#[repr(C)]
pub(crate) struct Spwd {
    pub(crate) namp: *mut c_char,
    pub(crate) pwdp: *mut c_char,
    pub(crate) lstchg: c_long,
    pub(crate) min: c_long,
    pub(crate) max: c_long,
    pub(crate) warn: c_long,
    pub(crate) inact: c_long,
    pub(crate) expire: c_long,
    pub(crate) flag: c_ulong,
}

impl Spwd {
    pub(crate) const ZERO: Self = Self {
        namp: core::ptr::null_mut(),
        pwdp: core::ptr::null_mut(),
        lstchg: 0,
        min: 0,
        max: 0,
        warn: 0,
        inact: 0,
        expire: 0,
        flag: 0,
    };
}

/// The process shadow cursor and record.
pub(crate) struct State {
    pub(crate) entries: Vec<CrateSpwd>,
    pub(crate) pos: usize,
    pub(crate) record: Spwd,
    pub(crate) buf: [u8; 2048],
}

impl State {
    const fn new() -> Self {
        Self {
            entries: Vec::new(),
            pos: 0,
            record: Spwd::ZERO,
            buf: [0; 2048],
        }
    }
}

const UNINIT: u8 = 0;
const READY: u8 = 2;

struct Cell(UnsafeCell<MaybeUninit<State>>);

// SAFETY: access is serialised by `LOCK`, and the storage is a static initialised once.
unsafe impl Sync for Cell {}

static LOCK: SpinLock = SpinLock::new();
static STATE: Cell = Cell(UnsafeCell::new(MaybeUninit::uninit()));
static INIT: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(UNINIT);

fn with_state<R>(f: impl FnOnce(&mut State) -> R) -> R {
    LOCK.acquire();
    if INIT
        .compare_exchange(
            UNINIT,
            1,
            core::sync::atomic::Ordering::AcqRel,
            core::sync::atomic::Ordering::Acquire,
        )
        .is_ok()
    {
        // SAFETY: this caller won the one-shot initialisation.
        unsafe { (&mut *STATE.0.get()).write(State::new()) };
        INIT.store(READY, core::sync::atomic::Ordering::Release);
    }
    while INIT.load(core::sync::atomic::Ordering::Acquire) != READY {
        core::hint::spin_loop();
    }
    // SAFETY: READY was published after the write; `LOCK` excludes other users.
    let state = unsafe { (&mut *STATE.0.get()).assume_init_mut() };
    let result = f(state);
    LOCK.release();
    result
}

fn read(path: &str) -> Option<Vec<u8>> {
    let files = crate::pwd::source::ShellFiles;
    files.read_file(path, 1 << 20).ok()
}

fn entries() -> Vec<CrateSpwd> {
    if let Some(bytes) = read("/etc/shadow") {
        let mut out = Vec::new();
        for line in bytes.split(|byte| *byte == b'\n') {
            if let Some(entry) = raven_libc_pwd::parse_shadow_line(line) {
                out.push(entry);
            }
        }
        if !out.is_empty() {
            return out;
        }
    }
    let mut out = Vec::new();
    if let Some(bytes) = read("/etc/master.passwd") {
        for line in bytes.split(|byte| *byte == b'\n') {
            if let Some((_, shadow)) = raven_libc_pwd::parse_master_passwd_line(line) {
                out.push(shadow);
            }
        }
    }
    out
}

/// Reload the shadow entries if the cursor is empty.
pub(crate) fn ensure_loaded(state: &mut State) {
    if state.entries.is_empty() {
        state.entries = entries();
        state.pos = 0;
    }
}

/// Fill a caller record and its string buffer; never a partial record.
pub(crate) fn fill(entry: &CrateSpwd, record: *mut Spwd, buf: &mut [u8]) -> Result<(), u16> {
    let needed = entry
        .namp
        .len()
        .saturating_add(entry.pwdp.len())
        .saturating_add(2);
    if record.is_null() || buf.len() < needed {
        return Err(ERANGE as u16);
    }
    let mut cursor = 0usize;
    let name_start = cursor;
    if let Some(slot) = buf.get_mut(cursor..cursor.saturating_add(entry.namp.len())) {
        slot.copy_from_slice(entry.namp.as_bytes());
    }
    cursor = cursor.saturating_add(entry.namp.len());
    if let Some(slot) = buf.get_mut(cursor) {
        *slot = 0;
    }
    cursor = cursor.saturating_add(1);
    let pass_start = cursor;
    if let Some(slot) = buf.get_mut(cursor..cursor.saturating_add(entry.pwdp.len())) {
        slot.copy_from_slice(entry.pwdp.as_bytes());
    }
    cursor = cursor.saturating_add(entry.pwdp.len());
    if let Some(slot) = buf.get_mut(cursor) {
        *slot = 0;
    }
    // SAFETY: the caller supplied a writable record and the offsets name the copied strings.
    unsafe {
        let target = &mut *record;
        target.namp = buf.as_mut_ptr().add(name_start).cast();
        target.pwdp = buf.as_mut_ptr().add(pass_start).cast();
        target.lstchg = entry.lstchg;
        target.min = entry.min;
        target.max = entry.max;
        target.warn = entry.warn;
        target.inact = entry.inact;
        target.expire = entry.expire;
        target.flag = entry.flag;
    }
    Ok(())
}

/// Fill the process record from a lookup result.
pub(crate) fn fill_static(entry: &CrateSpwd) -> *mut Spwd {
    with_state(|state| {
        let result = fill(entry, &mut state.record, &mut state.buf);
        match result {
            Ok(()) => &raw mut state.record,
            Err(error) => {
                crate::ffi::errno::set(raven_libc_abi::errno::Errno(error));
                core::ptr::null_mut()
            }
        }
    })
}

/// The next entry of the process cursor.
pub(crate) fn next() -> Option<CrateSpwd> {
    with_state(|state| {
        ensure_loaded(state);
        let entry = state.entries.get(state.pos).cloned();
        if entry.is_some() {
            state.pos = state.pos.saturating_add(1);
        }
        entry
    })
}

/// Look up one shadow login name.
pub(crate) fn by_name(name: &[u8]) -> Option<CrateSpwd> {
    let name = core::str::from_utf8(name).ok()?;
    with_state(|state| {
        ensure_loaded(state);
        state
            .entries
            .iter()
            .find(|entry| entry.namp == name)
            .cloned()
    })
}

/// Rewind the process cursor.
pub(crate) fn rewind() {
    with_state(|state| {
        ensure_loaded(state);
        state.pos = 0;
    });
}

/// Drop the process cursor.
pub(crate) fn end() {
    with_state(|state| {
        state.entries.clear();
        state.pos = 0;
    });
}

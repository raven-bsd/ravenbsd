// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The process random-generator objects of the shell (C41 owns the algorithms).
//!
//! `rand`/`srand` keep one `DefaultRandom`; `random`/`srandom`/`initstate`/`setstate` keep one
//! `RandomCtl` plus the current state buffer address (the internal 32-word buffer until a caller
//! supplies one); the `*48` family keeps one `Rand48` and the three-word `seed48` result. All
//! cells are process-global by the C contract; the shell is single-threaded until C57.

use core::ffi::c_char;

use raven_libc_abi::errno::Errno;
use raven_libc_stdlib::rand::{DefaultRandom, Rand48, RandomCtl};

static mut RAND: DefaultRandom = DefaultRandom::new();
static mut CTL: RandomCtl = RandomCtl::new();
static mut STATE: [u32; 32] = [0; 32];
static mut STATE_PTR: *mut u32 = core::ptr::null_mut();
static mut STATE_WORDS: usize = 0;
static mut SEED48_PREV: [u16; 3] = [0; 3];
static mut RAND48: Rand48 = Rand48::new();

const DEGREES: [usize; 5] = [0, 7, 15, 31, 63];

/// The default generator.
pub(crate) fn default_generator() -> &'static mut DefaultRandom {
    // SAFETY: one process generator; single-threaded until C57.
    unsafe { &mut *(&raw mut RAND) }
}

/// The 48-bit generator.
pub(crate) fn generator48() -> &'static mut Rand48 {
    // SAFETY: one process generator; single-threaded until C57.
    unsafe { &mut *(&raw mut RAND48) }
}

/// The three-word `seed48` result buffer.
pub(crate) fn seed48_buffer() -> &'static mut [u16; 3] {
    // SAFETY: one process buffer; single-threaded until C57.
    unsafe { &mut *(&raw mut SEED48_PREV) }
}

/// The current `random` state slice.
///
/// # Safety
/// The pointer and length are written only by [`initstate`]/[`setstate`].
pub(crate) unsafe fn state_slice<'a>() -> &'a mut [u32] {
    // SAFETY: the static cells name a live buffer owned by the process.
    unsafe { core::slice::from_raw_parts_mut(STATE_PTR, STATE_WORDS) }
}

/// The control block.
pub(crate) fn control() -> &'static mut RandomCtl {
    // SAFETY: one process control block; single-threaded until C57.
    unsafe { &mut *(&raw mut CTL) }
}

/// Initialise the default state as `srandom(1)`.
pub(crate) fn ensure_random() {
    // SAFETY: the static cells are the process generator.
    unsafe {
        if STATE_PTR.is_null() {
            STATE_PTR = (&raw mut STATE).cast::<u32>();
            STATE_WORDS = 32;
            let state = core::slice::from_raw_parts_mut(STATE_PTR, STATE_WORDS);
            let ctl = &raw mut CTL;
            raven_libc_stdlib::rand::srandom(&mut *ctl, state, 1);
        }
    }
}

/// The previous state pointer for `initstate`/`setstate`.
pub(crate) fn previous_state() -> *mut c_char {
    ensure_random();
    // SAFETY: the state pointer is live.
    unsafe { STATE_PTR.cast::<c_char>() }
}

/// `initstate` over the caller's buffer.
///
/// # Safety
/// `buffer` is NULL or points to `bytes` writable bytes.
pub(crate) unsafe fn init(
    seed: u32,
    buffer: *mut c_char,
    bytes: usize,
) -> Result<*mut c_char, Errno> {
    let previous = previous_state();
    if buffer.is_null() || bytes < 8 {
        return Err(Errno(raven_libc_abi::errno::EINVAL as u16));
    }
    if (buffer as usize) % core::mem::align_of::<u32>() != 0 {
        return Err(Errno(raven_libc_abi::errno::EINVAL as u16));
    }
    let words = bytes / core::mem::size_of::<u32>();
    // SAFETY: the caller's state buffer has `bytes` writable bytes.
    let state = unsafe { core::slice::from_raw_parts_mut(buffer.cast::<u32>(), words) };
    let ctl = control();
    raven_libc_stdlib::rand::initstate(ctl, state, seed, bytes)
        .map_err(|_| Errno(raven_libc_abi::errno::EINVAL as u16))?;
    // SAFETY: the caller's buffer is now the process state.
    unsafe {
        STATE_PTR = buffer.cast::<u32>();
        STATE_WORDS = words;
    }
    Ok(previous)
}

/// `setstate` over the caller's buffer.
///
/// # Safety
/// `buffer` is NULL or points to a state buffer produced by `initstate`/`setstate`.
pub(crate) unsafe fn restore(buffer: *mut c_char) -> Result<*mut c_char, Errno> {
    let previous = previous_state();
    if buffer.is_null() {
        return Err(Errno(raven_libc_abi::errno::EINVAL as u16));
    }
    if (buffer as usize) % core::mem::align_of::<u32>() != 0 {
        return Err(Errno(raven_libc_abi::errno::EINVAL as u16));
    }
    // SAFETY: the first word of a state buffer is the type word.
    let word = unsafe { core::ptr::read_unaligned(buffer.cast::<u32>()) };
    let kind = (word % 5) as usize;
    let needed = if kind == 0 {
        2
    } else {
        DEGREES.get(kind).copied().unwrap_or(0).saturating_add(1)
    };
    if needed == 0 {
        return Err(Errno(raven_libc_abi::errno::EINVAL as u16));
    }
    // SAFETY: the caller's buffer has at least `needed` words.
    let state = unsafe { core::slice::from_raw_parts_mut(buffer.cast::<u32>(), needed) };
    let ctl = control();
    raven_libc_stdlib::rand::setstate(ctl, state)
        .map_err(|_| Errno(raven_libc_abi::errno::EINVAL as u16))?;
    // SAFETY: the caller's buffer is now the process state.
    unsafe {
        STATE_PTR = buffer.cast::<u32>();
        STATE_WORDS = needed;
    }
    Ok(previous)
}

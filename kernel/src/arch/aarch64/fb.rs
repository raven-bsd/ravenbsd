// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Raspberry Pi 4 HDMI display-console early glue.
//!
//! The boot path asks the VideoCore firmware for a linear 1024x768x32 display buffer over mailbox
//! channel 8, maps it through the existing HHDM, and mirrors every kernel log line to it. The
//! UART remains the primary/recovery console: a missing mailbox, a malformed property response or
//! an unmapped buffer logs one fallback line and leaves serial output untouched.
//!
//! The message buffer and the display buffer are normal cacheable SDRAM. The CPU owns the buffer
//! before the mailbox write and the firmware owns the display buffer after it is handed over, so
//! the glue cleans the property buffer before the doorbell, invalidates it after the response, and
//! cleans each changed display row range before the VideoCore scans it out. This is a text
//! console; there is no GPU command stream, DRM or mode-set beyond the firmware property call.

use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crate::arch::HHDM_BASE;

// The crate source is shared with `crates/raven-fb`, where host tests exercise the full screen
// that the early console does not use yet.
#[path = "../../../../crates/raven-fb/src/font.rs"]
#[allow(dead_code, clippy::arithmetic_side_effects, clippy::indexing_slicing)]
mod font;
#[path = "../../../../crates/raven-fb/src/mailbox.rs"]
#[allow(dead_code, clippy::arithmetic_side_effects, clippy::indexing_slicing)]
mod mailbox;
#[path = "../../../../crates/raven-fb/src/pixel.rs"]
#[allow(dead_code, clippy::arithmetic_side_effects, clippy::indexing_slicing)]
mod pixel;
#[path = "../../../../crates/raven-fb/src/text.rs"]
#[allow(dead_code, clippy::arithmetic_side_effects, clippy::indexing_slicing)]
mod text;

use mailbox::DisplayRequest;
use pixel::PixelFormat;
use text::{ScreenBuffer, TextConsole};

/// Pi 4 mailbox register window (peripheral base `0xFE00_0000` + `0xB880`).
const MAILBOX_PHYS: u64 = 0xFE00_B880;
/// Mailbox 0 read register.
const MAILBOX_READ: u64 = 0x00;
/// Mailbox status register (full/empty bits).
const MAILBOX_STATUS: u64 = 0x18;
/// Mailbox 1 write register.
const MAILBOX_WRITE: u64 = 0x20;
/// Status bit: the interface cannot accept a write.
const STATUS_FULL: u32 = 0x8000_0000;
/// Status bit: no response is pending.
const STATUS_EMPTY: u32 = 0x4000_0000;
/// Property-buffer channel.
const CHANNEL_PROPERTY: u32 = 8;
/// VideoCore uncached SDRAM alias base for the Pi 4.
const VCBUS_SDRAM_UNCACHED: u32 = 0xC000_0000;
/// First address outside the HHDM RAM window mapped by the Pi 4 entry stub.
const HHDM_RAM_END: u64 = 0x8000_0000;
/// AArch64 cache-line size used for maintenance (Cortex-A72 is 64 bytes).
const CACHE_LINE: usize = 64;
/// Bounded mailbox polls; a stuck firmware interface falls back to UART.
const MAILBOX_POLLS: u32 = 2_000_000;

/// 16-byte-aligned property buffer required by the firmware.
#[repr(align(16))]
struct AlignedBuffer([u8; mailbox::MAX_PROPERTY_BYTES]);

struct BufferCell(UnsafeCell<AlignedBuffer>);

// SAFETY: the buffer is used only under the kernel log lock or during single-threaded boot; the
// HDMI console also rejects reentrant writes with an atomic guard.
unsafe impl Sync for BufferCell {}

static BUFFER: BufferCell = BufferCell(UnsafeCell::new(AlignedBuffer(
    [0; mailbox::MAX_PROPERTY_BYTES],
)));

struct Console {
    base: *mut u8,
    width: u32,
    pitch: u32,
    height: u32,
    format: PixelFormat,
    text: TextConsole,
}

struct StateCell(UnsafeCell<Option<Console>>);

// SAFETY: `write_text`/`init` are serialized by the atomic guard below; boot calls `init` once.
unsafe impl Sync for StateCell {}

static STATE: StateCell = StateCell(UnsafeCell::new(None));
static ACTIVE: AtomicBool = AtomicBool::new(false);
static WRITING: AtomicBool = AtomicBool::new(false);
static KERNEL_OFFSET: AtomicU64 = AtomicU64::new(0);

/// Record the kernel's virtual-to-physical offset.
///
/// Called by the Pi 4 boot path before `kmain`, because the mailbox needs the physical address of
/// the property buffer and of the returned display buffer.
pub fn set_kernel_offset(offset: u64) {
    KERNEL_OFFSET.store(offset, Ordering::Release);
}

/// Query the firmware display buffer and start mirroring kernel log output to it.
///
/// A failure is logged once and leaves the UART-only path in place.
pub fn init() {
    if ACTIVE.load(Ordering::Acquire) {
        return;
    }
    let offset = KERNEL_OFFSET.load(Ordering::Acquire);
    if offset == 0 {
        crate::kprintln!("fb: no Pi 4 kernel offset; UART only");
        return;
    }
    let request = DisplayRequest::default();
    // SAFETY: boot calls `init` once, before any other display-buffer user; the buffer is owned
    // here.
    let buffer = unsafe { &mut (*BUFFER.0.get()).0 };
    let length = match mailbox::encode_request(buffer, &request) {
        Ok(length) => length,
        Err(error) => {
            crate::kprintln!("fb: encode failed: {error:?}");
            return;
        }
    };
    let Some(buffer_phys) = (buffer.as_ptr() as u64).checked_sub(offset) else {
        crate::kprintln!("fb: property buffer has no physical address");
        return;
    };
    if buffer_phys == 0 || buffer_phys >= HHDM_RAM_END {
        crate::kprintln!("fb: property buffer outside SDRAM");
        return;
    }
    clean_dcache(buffer.as_ptr() as usize, length);
    if !mailbox_call(buffer_phys as u32) {
        crate::kprintln!("fb: mailbox timeout; UART only");
        return;
    }
    invalidate_dcache(buffer.as_ptr() as usize, length);

    let Some(response) = buffer.get(..length) else {
        crate::kprintln!("fb: property length out of range");
        return;
    };
    let info = match mailbox::decode_response(response) {
        Ok(info) => info,
        Err(error) => {
            crate::kprintln!("fb: response rejected: {error:?}; UART only");
            return;
        }
    };
    let Some(format) = PixelFormat::from_depth(info.depth) else {
        crate::kprintln!("fb: depth {} unsupported; UART only", info.depth);
        return;
    };
    let physical = if info.address >= VCBUS_SDRAM_UNCACHED {
        info.address.saturating_sub(VCBUS_SDRAM_UNCACHED)
    } else {
        info.address
    };
    let Some(required) = u64::from(info.pitch).checked_mul(u64::from(info.height)) else {
        crate::kprintln!("fb: display size overflow; UART only");
        return;
    };
    let end = u64::from(physical).saturating_add(required);
    if u64::from(physical) < 0x1000 || end > HHDM_RAM_END {
        crate::kprintln!("fb: display buffer outside the mapped RAM window");
        return;
    }
    let base = (HHDM_BASE.wrapping_add(u64::from(physical))) as *mut u8;
    let Some(length) = usize::try_from(required).ok() else {
        crate::kprintln!("fb: display buffer too large");
        return;
    };
    // SAFETY: the display buffer is inside the HHDM RAM window and the firmware has handed ownership
    // of the allocation to the ARM core.
    let pixels = unsafe { core::slice::from_raw_parts_mut(base, length) };
    let mut screen = ScreenBuffer {
        pixels,
        width: info.width,
        height: info.height,
        pitch: info.pitch,
        format,
    };
    let mut console = match TextConsole::new(&screen) {
        Ok(console) => console,
        Err(error) => {
            crate::kprintln!("fb: console geometry rejected: {error:?}");
            return;
        }
    };
    match console.clear(&mut screen) {
        Ok(dirty) => clean_rows(base, info.pitch, dirty.first, dirty.last),
        Err(error) => {
            crate::kprintln!("fb: clear failed: {error:?}; UART only");
            return;
        }
    }
    // SAFETY: `init` is the only writer of this cell and runs before log mirroring starts.
    unsafe {
        STATE.0.get().write(Some(Console {
            base,
            width: info.width,
            pitch: info.pitch,
            height: info.height,
            format,
            text: console,
        }));
    }
    ACTIVE.store(true, Ordering::Release);
    crate::kprintln!(
        "fb: {}x{}x{} pitch={} base=0x{:x} bus=0x{:x}",
        info.width,
        info.height,
        info.depth,
        info.pitch,
        physical,
        info.address
    );
}

/// Mirror one formatted log string to the HDMI console, if it is live.
pub fn write_text(bytes: &[u8]) {
    if !ACTIVE.load(Ordering::Acquire)
        || WRITING
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
    {
        return;
    }
    // SAFETY: `ACTIVE` and `WRITING` make this the only accessor; the state is written once by
    // `init` before `ACTIVE` becomes true.
    let state = unsafe { &mut *STATE.0.get() };
    let Some(console) = state.as_mut() else {
        WRITING.store(false, Ordering::Release);
        return;
    };
    let Some(length) = u64::from(console.pitch)
        .checked_mul(u64::from(console.height))
        .and_then(|bytes| usize::try_from(bytes).ok())
    else {
        WRITING.store(false, Ordering::Release);
        return;
    };
    // SAFETY: the buffer was validated against the HHDM RAM window in `init`.
    let pixels = unsafe { core::slice::from_raw_parts_mut(console.base, length) };
    let mut screen = ScreenBuffer {
        pixels,
        width: console.width,
        height: console.height,
        pitch: console.pitch,
        format: console.format,
    };
    if let Ok(dirty) = console.text.write(&mut screen, bytes) {
        clean_rows(console.base, console.pitch, dirty.first, dirty.last);
    }
    WRITING.store(false, Ordering::Release);
}

fn clean_rows(base: *mut u8, pitch: u32, first: u32, last: u32) {
    let Ok(pitch) = usize::try_from(pitch) else {
        return;
    };
    let Some(start) = usize::try_from(first)
        .ok()
        .and_then(|row| row.checked_mul(pitch))
    else {
        return;
    };
    let Some(rows) = last
        .checked_sub(first)
        .and_then(|count| count.checked_add(1))
    else {
        return;
    };
    let Some(length) = usize::try_from(rows)
        .ok()
        .and_then(|count| count.checked_mul(pitch))
    else {
        return;
    };
    let address = (base as usize).wrapping_add(start);
    clean_dcache(address, length);
}

fn mailbox_call(address: u32) -> bool {
    let base = HHDM_BASE.wrapping_add(MAILBOX_PHYS);
    let mut space = false;
    for _ in 0..MAILBOX_POLLS {
        // SAFETY: the mailbox frame is mapped through the HHDM device window by the entry stub.
        if unsafe { read32(base.wrapping_add(MAILBOX_STATUS)) } & STATUS_FULL == 0 {
            space = true;
            break;
        }
        core::hint::spin_loop();
    }
    if !space {
        return false;
    }
    barrier();
    // SAFETY: the status register reported room and the buffer is clean and 16-byte aligned.
    unsafe {
        write32(
            base.wrapping_add(MAILBOX_WRITE),
            (address & !0xf) | CHANNEL_PROPERTY,
        );
    }
    for _ in 0..MAILBOX_POLLS {
        // SAFETY: the mailbox frame is mapped; reading STATUS/READ is a register access.
        if unsafe { read32(base.wrapping_add(MAILBOX_STATUS)) } & STATUS_EMPTY == 0 {
            // SAFETY: the status register reported a pending response; READ is the mailbox 0 data
            // register and pops one channel message.
            let response = unsafe { read32(base.wrapping_add(MAILBOX_READ)) };
            if response & 0xf == CHANNEL_PROPERTY {
                barrier();
                return (response & !0xf) == (address & !0xf);
            }
        }
        core::hint::spin_loop();
    }
    false
}

/// Clean the data cache for `length` bytes at `address` to the point of coherency.
fn clean_dcache(address: usize, length: usize) {
    let start = address & !(CACHE_LINE - 1);
    let end = address
        .saturating_add(length)
        .saturating_add(CACHE_LINE - 1)
        & !(CACHE_LINE - 1);
    let mut current = start;
    while current < end {
        // SAFETY: `dc cvac` cleans the line containing a mapped normal-memory address; the range
        // is inside the kernel image or the validated HHDM display buffer.
        unsafe {
            core::arch::asm!("dc cvac, {address}", address = in(reg) current, options(nostack));
        }
        current = current.saturating_add(CACHE_LINE);
    }
    barrier();
}

/// Invalidate the data cache for `length` bytes at `address` after firmware DMA.
fn invalidate_dcache(address: usize, length: usize) {
    let start = address & !(CACHE_LINE - 1);
    let end = address
        .saturating_add(length)
        .saturating_add(CACHE_LINE - 1)
        & !(CACHE_LINE - 1);
    let mut current = start;
    while current < end {
        // SAFETY: `dc ivac` invalidates the line containing a mapped normal-memory address; the
        // property buffer is not dirty here and the firmware has finished writing it.
        unsafe {
            core::arch::asm!("dc ivac, {address}", address = in(reg) current, options(nostack));
        }
        current = current.saturating_add(CACHE_LINE);
    }
    barrier();
}

fn barrier() {
    // SAFETY: a full system barrier orders the cache maintenance against the mailbox doorbell.
    unsafe {
        core::arch::asm!("dsb sy", options(nostack, preserves_flags));
    }
}

unsafe fn read32(address: u64) -> u32 {
    // SAFETY: the caller passes a mapped mailbox register address.
    unsafe { (address as *const u32).read_volatile() }
}

unsafe fn write32(address: u64, value: u32) {
    // SAFETY: the caller passes a mapped mailbox register address.
    unsafe { (address as *mut u32).write_volatile(value) }
}

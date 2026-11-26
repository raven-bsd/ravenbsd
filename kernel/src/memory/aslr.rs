// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Address-space layout randomization from the boot entropy pool.
//!
//! The windows are deliberately small and page aligned. The seed comes from the same
//! `time::entropy_fill` pool that backs `GETRANDOM`; when it is unavailable the layout is fixed
//! and the reason is logged once, never guessed. `aslr=off` selects the fixed layout for the
//! control case.
//!
//! Windows (x86-64 and aarch64 share the user layout constants):
//!
//! * stack top: `USER_END - STACK_GUARD - slide`, slide in `0..STACK_WINDOW` pages below the
//!   fixed top; the 256 KiB stack and its 4 KiB guard are page aligned;
//! * first anonymous mmap: `MMAP_BASE + slide`, slide in `0..MMAP_WINDOW`;
//! * PIE load bias: `PIE_BASE + slide`;
//! * interpreter load bias: `INTERP_BASE + slide`.

use core::sync::atomic::{AtomicBool, Ordering};

#[cfg(target_arch = "aarch64")]
use raven_abi::layout::aarch64::{USER_END, USER_MIN};
#[cfg(target_arch = "x86_64")]
use raven_abi::layout::x86_64::{USER_END, USER_MIN};

/// User stack bytes (the same value as `process::USER_STACK_SIZE`).
pub const STACK_SIZE: u64 = 256 * 1024;
/// Guard page immediately below the stack.
pub const STACK_GUARD: u64 = 4096;
/// Stack slide window. 2,048 pages = 8 MiB.
pub const STACK_WINDOW: u64 = 8 * 1024 * 1024;
/// Anonymous mmap base; 80 TiB, above the interpreter and below the capture window.
pub const MMAP_BASE: u64 = 0x0000_5000_0000_0000;
/// Anonymous mmap slide window: 1 GiB.
pub const MMAP_WINDOW: u64 = 0x4000_0000;
/// PIE main-image base; 16 TiB.
pub const PIE_BASE: u64 = 0x0000_1000_0000_0000;
/// PIE slide window: 1 GiB.
pub const PIE_WINDOW: u64 = 0x4000_0000;
/// Interpreter base; 32 TiB.
pub const INTERP_BASE: u64 = 0x0000_2000_0000_0000;
/// Interpreter slide window: 1 GiB.
pub const INTERP_WINDOW: u64 = 0x4000_0000;

static LOGGED_FALLBACK: AtomicBool = AtomicBool::new(false);

/// True unless the boot command line selected the fixed layout.
#[must_use]
pub fn enabled() -> bool {
    !crate::memory::paging::cmdline()
        .split_ascii_whitespace()
        .any(|token| token == "aslr=off")
}

/// One page-aligned slide in `0..window`, or zero for the fixed layout.
fn page_slide(window: u64) -> u64 {
    if !enabled() {
        return 0;
    }
    let pages = window / crate::memory::frame::PAGE_SIZE;
    if pages == 0 {
        return 0;
    }
    let mut bytes = [0u8; 8];
    match crate::time::entropy_fill(&mut bytes) {
        Ok(8) => u64::from_le_bytes(bytes)
            .checked_rem(pages)
            .unwrap_or(0)
            .saturating_mul(crate::memory::frame::PAGE_SIZE),
        _ => {
            if !LOGGED_FALLBACK.swap(true, Ordering::AcqRel) {
                crate::kprintln!("aslr: entropy unavailable; fixed layout");
            }
            0
        }
    }
}

/// Randomized stack VMA base for a fresh address space.
#[must_use]
pub fn stack_base() -> u64 {
    let fixed = USER_END
        .saturating_sub(STACK_SIZE)
        .saturating_sub(STACK_GUARD);
    fixed.saturating_sub(page_slide(STACK_WINDOW)).max(USER_MIN)
}

/// Randomized first anonymous mmap hint for a fresh address space.
#[must_use]
pub fn mmap_base() -> u64 {
    MMAP_BASE.saturating_add(page_slide(MMAP_WINDOW))
}

/// Randomized `ET_DYN` main-image load bias for one exec.
#[must_use]
pub fn pie_bias() -> u64 {
    PIE_BASE.saturating_add(page_slide(PIE_WINDOW))
}

/// Randomized interpreter load bias for one exec.
#[must_use]
pub fn interp_bias() -> u64 {
    INTERP_BASE.saturating_add(page_slide(INTERP_WINDOW))
}

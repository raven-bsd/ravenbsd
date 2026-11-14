// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `raven-libc-crt`: the start files of ravenlibc.
//!
//! The crate is a build vehicle, not a library a program links: `xtask libc-pack` compiles it
//! as a `staticlib` with `codegen-units = 4096` and extracts one archive member per start
//! object. Each module below is one `global_asm!` item, hence one codegen unit and one member
//! (`donor-map/17` 7.1, `donor-map/10` 3.12).
//!
//! * `crt1` - static, non-PIE `_start`;
//! * `scrt1` - dynamic PIE `_start` (the loader's register convention);
//! * `rcrt1` - static-PIE `_start` and the stage-1 self-relocation;
//! * `crti`, `crtn` - empty members so unmodified link lines work;
//! * `crtbegin`, `crtbegin_s`, `crtend` - fallbacks for the compiler runtime objects;
//! * `selfreloc`, `selfreloc_consts` - the one copy of the self-relocator, included by the
//!   loader through `#[path]` (`crt/src/selfreloc.rs`), never copied.
//!
//! `note` holds the `.note.raven.abi` text shared by the three entry objects.
//!
//! Class `U`: raw assembly and pointer writes are the point of the crate. Every `unsafe` block
//! carries a `// SAFETY:` comment. The crate is `no_std` on the target and `std`-enabled for the
//! host unit tests of the relocation logic.
#![cfg_attr(not(test), no_std)]
#![allow(clippy::arithmetic_side_effects)] // address arithmetic in the self-relocator, by design
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented
)]
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing,
        clippy::undocumented_unsafe_blocks
    )
)]

/// The `.note.raven.abi` text shared by `crt1`, `Scrt1` and `rcrt1`.
#[macro_use]
#[path = "note.rs"]
mod note;

pub mod selfreloc;
pub mod selfreloc_consts;

// One module per start object; the build rejects the other architectures by target selection.
#[cfg(target_os = "none")]
mod crt1;
#[cfg(target_os = "none")]
mod crtbegin;
#[cfg(target_os = "none")]
mod crtbegin_s;
#[cfg(target_os = "none")]
mod crtend;
#[cfg(target_os = "none")]
mod crti;
#[cfg(target_os = "none")]
mod crtn;
#[cfg(target_os = "none")]
mod rcrt1;
#[cfg(target_os = "none")]
mod scrt1;

/// A `staticlib` needs a panic handler even when no code can panic; the member is never
/// extracted into a start object (`libc_pack_crt` picks its members by marker symbol).
#[cfg(target_os = "none")]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    #[cfg(target_arch = "x86_64")]
    // SAFETY: `ud2` is the architectural trap and does not return.
    unsafe {
        core::arch::asm!("ud2", options(noreturn, nostack, nomem));
    }
    #[cfg(target_arch = "aarch64")]
    // SAFETY: `brk #0` raises the breakpoint exception and does not return.
    unsafe {
        core::arch::asm!("brk #0", options(noreturn, nostack, nomem));
    }
    #[cfg(target_arch = "riscv64")]
    // SAFETY: `unimp` is the defined illegal instruction and does not return.
    unsafe {
        core::arch::asm!("unimp", options(noreturn, nostack, nomem));
    }
}

// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! RavenBSD no_std userland runtime.
//!
//! This crate provides the first userland ABI surface: `_start`, `args`, raw syscall wrappers,
//! printing, the panic handler, an optional allocator and the `entry!` macro. It deliberately has
//! no dependencies except the `raven-abi` contract crate.

#![cfg_attr(not(test), no_std)]
#![allow(unsafe_code)] // asm, the C8 stack parser and the allocator; every block has SAFETY.
#![deny(unsafe_op_in_unsafe_fn)]

#[cfg(feature = "alloc")]
extern crate alloc as alloc_crate;

#[cfg(feature = "alloc")]
pub mod alloc;
pub mod args;
pub mod entry;
#[cfg(feature = "alloc")]
pub mod fs;
pub mod initfs;
pub mod print;
pub mod spawn;
pub mod sync;
pub mod sys;
#[cfg(feature = "alloc")]
pub mod thread;

#[cfg(all(feature = "panic-handler", not(test)))]
mod panic;

/// The runtime's argument view over the C8 process stack.
pub use args::Args;
/// The runtime's process entry point.
#[cfg(not(test))]
pub use entry::__raven_start;
pub use initfs::initfs_has_manifest;
pub use sys::{
    DMA_FLAG_NON_CACHEABLE, DmaInfo, IRQ_LEGACY, IRQ_MSI, IRQ_MSIX, IrqInfo, boot_blob, clock_get,
    debug_read, debug_write, dma_alloc, dma_info, exit, futex_wait, futex_wake, handle_close,
    handle_dup, handle_transfer, irq_ack, irq_create, irq_wait, map_initfs, map_mmio, map_mmio_mut,
    mmio_create, nanosleep, proc_terminate, uname, yield_now,
};

/// `entry!(main)` registers the program's `main(args: Args) -> i32` function.
#[macro_export]
macro_rules! entry {
    ($main:path) => {
        #[allow(missing_docs)] // generated symbol, not public API
        #[unsafe(no_mangle)]
        pub extern "C" fn __raven_main(args: $crate::Args) -> i32 {
            $main(args)
        }
    };
}

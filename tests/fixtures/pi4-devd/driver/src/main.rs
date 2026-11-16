// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Fake platform driver for the P8 devd QEMU test.
//!
//! It receives the `bar`/`bar_size`/`dma`/`irq` tokens and the transferred handles from devd,
//! verifies that each declared handle exists, and exits. It never touches device memory.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use raven_abi::types::Handle;
    use raven_rt::entry;

    entry!(main);

    const BAR_HANDLE: Handle = Handle::new(3, 0);
    const DMA_HANDLE: Handle = Handle::new(4, 0);
    const IRQ_HANDLE: Handle = Handle::new(5, 0);

    fn main(args: raven_rt::Args) -> i32 {
        let mut bar = 0usize;
        let mut size = 0usize;
        let mut dma = 0usize;
        let mut irq = 0u32;
        for argument in args.iter().skip(1) {
            if let Some(value) = argument.strip_prefix("bar=") {
                bar = value.parse().unwrap_or(0);
            } else if let Some(value) = argument.strip_prefix("bar_size=") {
                size = value.parse().unwrap_or(0);
            } else if let Some(value) = argument.strip_prefix("dma=") {
                dma = value.parse().unwrap_or(0);
            } else if let Some(value) = argument.strip_prefix("irq=") {
                irq = value.parse().unwrap_or(0);
            }
        }
        raven_rt::println!("fake-platform: bar={bar} size={size} dma={dma} irq={irq}");
        if raven_rt::handle_close(BAR_HANDLE).is_err() {
            raven_rt::println!("fake-platform: bar handle missing");
            return 1;
        }
        if dma != 0 {
            let Ok(info) = raven_rt::dma_info(DMA_HANDLE) else {
                raven_rt::println!("fake-platform: dma handle missing");
                return 1;
            };
            raven_rt::println!(
                "fake-platform: dma bus={:#x} len={}",
                info.bus_addr,
                info.len
            );
            if raven_rt::handle_close(DMA_HANDLE).is_err() {
                return 1;
            }
        }
        if irq != 0 && raven_rt::handle_close(IRQ_HANDLE).is_err() {
            raven_rt::println!("fake-platform: irq handle missing");
            return 1;
        }
        raven_rt::println!("fake-platform: handles ok");
        0
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}

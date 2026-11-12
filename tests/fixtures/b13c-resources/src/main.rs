// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! B13c fixture driver: the BAR, configuration page, DMA buffer and legacy IRQ through `devd`.
//!
//! `devd` launches this program with `resources=dma,irq`, so the child receives four handles in
//! the documented order: BAR (3), configuration page (4), DMA buffer (5) and IRQ (6). The fixture
//! maps all four, proves the DMA bus address by making QEMU's `edu` copy a pattern out of the
//! buffer, raises the device interrupt and waits for it, then exits (twice, to exercise restart).
//!
//! It keeps `#![forbid(unsafe_code)]`: every device access goes through `raven-driver`.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::format;
    use alloc::string::{String, ToString};

    use raven_abi::types::Handle;
    use raven_driver::{AccessError, ConfigRegion, DmaRegion, Irq, MmioRegion, PAGE};
    use raven_rt::entry;

    entry!(main);

    /// A fresh child has 0 (self), 1 (initfs) and 2 (namespace); `devd` then grants in order.
    const BAR_HANDLE: Handle = Handle::new(3, 0);
    const CONFIG_HANDLE: Handle = Handle::new(4, 0);
    const DMA_HANDLE: Handle = Handle::new(5, 0);
    const IRQ_HANDLE: Handle = Handle::new(6, 0);

    /// QEMU `edu`: identification and DMA engine.
    const EDU_ID: u32 = 0x0100_00ed;
    const EDU_DMA_SRC: usize = 0x80;
    const EDU_DMA_DST: usize = 0x88;
    const EDU_DMA_CNT: usize = 0x90;
    const EDU_DMA_CMD: usize = 0x98;
    const EDU_DMA_RUN: u32 = 1;
    /// Command bit 1 selects the device-to-RAM direction.
    const EDU_DMA_TO_PCI: u32 = 2;
    const EDU_BUFFER: usize = 0x40000;
    const EDU_RAISE: usize = 0x60;
    const EDU_ACK: usize = 0x64;

    const FRAME: [u8; 16] = [
        0x52, 0x61, 0x76, 0x65, 0x6e, 0x42, 0x53, 0x44, 0x13, 0xc0, 0xde, 0xad, 0xbe, 0xef, 0x42,
        0x13,
    ];
    const IRQ_TIMEOUT_NS: u64 = 1_000_000_000;

    fn main(args: raven_rt::Args) -> i32 {
        match run(args) {
            Ok(()) => 0,
            Err(message) => {
                raven_rt::eprintln!("b13c: {}", message);
                1
            }
        }
    }

    fn run(args: raven_rt::Args) -> Result<(), String> {
        let mut bar_index = 0usize;
        let mut bar_size = 0usize;
        let mut dma_size = 0usize;
        let mut irq_line = 0u32;
        for argument in args.iter().skip(1) {
            if let Some(value) = argument.strip_prefix("bar=") {
                bar_index = value
                    .parse()
                    .map_err(|_| format!("bad bar argument `{argument}`"))?;
            } else if let Some(value) = argument.strip_prefix("bar_size=") {
                bar_size = value
                    .parse()
                    .map_err(|_| format!("bad bar_size argument `{argument}`"))?;
            } else if let Some(value) = argument.strip_prefix("dma=") {
                dma_size = value
                    .parse()
                    .map_err(|_| format!("bad dma argument `{argument}`"))?;
            } else if let Some(value) = argument.strip_prefix("irq=") {
                irq_line = value
                    .parse()
                    .map_err(|_| format!("bad irq argument `{argument}`"))?;
            }
        }
        if bar_size == 0 || dma_size == 0 {
            return Err("devd did not pass bar_size and dma".to_string());
        }

        let window = raven_rt::map_mmio_mut(BAR_HANDLE, bar_size)
            .map_err(|error| format!("bar map: {}", error.description()))?;
        let mut bar = MmioRegion::new(window);
        let id = bar.read_u32(0).map_err(describe)?;
        if id != EDU_ID {
            return Err(format!("unexpected device id {id:#010x}"));
        }
        raven_rt::println!("b13c: bar={bar_index} size={}", bar.len());

        let config_window = raven_rt::map_mmio(CONFIG_HANDLE, PAGE)
            .map_err(|error| format!("config map: {}", error.description()))?;
        let config = ConfigRegion::new(config_window);
        let (vendor, device) = config.ids().map_err(describe)?;
        raven_rt::println!("b13c: config vendor={vendor:#06x} device={device:#06x}");

        let info = raven_rt::dma_info(DMA_HANDLE)
            .map_err(|error| format!("dma info: {}", error.description()))?;
        if usize::try_from(info.len).unwrap_or(0) != dma_size {
            return Err(format!(
                "dma buffer is {} bytes, expected {dma_size}",
                info.len
            ));
        }
        if info.bus_addr == 0 || info.bus_addr >= (1u64 << 32) {
            return Err(format!(
                "dma bus address out of range: {:#x}",
                info.bus_addr
            ));
        }
        let dma_window = raven_rt::map_mmio_mut(DMA_HANDLE, dma_size)
            .map_err(|error| format!("dma map: {}", error.description()))?;
        let mut dma = DmaRegion::new(dma_window, info.bus_addr);
        raven_rt::println!("b13c: dma len={} bus={:#x}", dma.len(), dma.bus_address());

        // Put a frame into the buffer and round-trip it through the device: RAM -> the edu DMA
        // buffer, then back to RAM at the same bus address. The return copy lands in a region the
        // CPU can read, so this proves the transferred buffer and the reported bus address, not
        // just the CPU mapping. (The device's internal DMA buffer is not mapped into its BAR.)
        dma.write_bytes(0, &FRAME).map_err(describe)?;
        dma.publish();
        bar.write_u32(EDU_DMA_SRC, dma.bus_address() as u32)
            .map_err(describe)?;
        bar.write_u32(EDU_DMA_DST, EDU_BUFFER as u32)
            .map_err(describe)?;
        bar.write_u32(EDU_DMA_CNT, FRAME.len() as u32)
            .map_err(describe)?;
        bar.write_u32(EDU_DMA_CMD, EDU_DMA_RUN).map_err(describe)?;
        if !dma_complete(&bar) {
            return Err("device DMA copy timed out".to_string());
        }
        let zeros = [0u8; 16];
        dma.write_bytes(0, &zeros).map_err(describe)?;
        dma.publish();
        bar.write_u32(EDU_DMA_SRC, EDU_BUFFER as u32)
            .map_err(describe)?;
        bar.write_u32(EDU_DMA_DST, dma.bus_address() as u32)
            .map_err(describe)?;
        bar.write_u32(EDU_DMA_CNT, FRAME.len() as u32)
            .map_err(describe)?;
        bar.write_u32(EDU_DMA_CMD, EDU_DMA_RUN | EDU_DMA_TO_PCI)
            .map_err(describe)?;
        if !dma_complete(&bar) {
            return Err("device DMA return copy timed out".to_string());
        }
        dma.acquire();
        let mut copied = [0u8; 16];
        dma.read_bytes(0, &mut copied).map_err(describe)?;
        if copied != FRAME {
            let word = u32::from_le_bytes([copied[0], copied[1], copied[2], copied[3]]);
            return Err(format!("device DMA round trip mismatch first={word:#010x}"));
        }
        raven_rt::println!("b13c: dma bus ok");

        // Now the interrupt path: raise the device line, wait for the transferred IRQ handle.
        let irq = Irq::new(IRQ_HANDLE);
        bar.write_u32(EDU_RAISE, 1).map_err(describe)?;
        let fired = irq
            .wait(IRQ_TIMEOUT_NS)
            .map_err(|error| format!("irq wait: {}", error.description()))?;
        bar.write_u32(EDU_ACK, 1).map_err(describe)?;
        irq.ack()
            .map_err(|error| format!("irq ack: {}", error.description()))?;
        if fired == 0 {
            return Err(format!("irq line {irq_line} timed out"));
        }
        raven_rt::println!("b13c: irq line={irq_line} fired");
        raven_rt::println!("b13c: done");
        Ok(())
    }

    /// Poll the edu DMA command register until the device clears the run bit.
    fn dma_complete(bar: &MmioRegion) -> bool {
        let mut spins = 0usize;
        loop {
            match bar.read_u32(EDU_DMA_CMD) {
                Ok(command) if command & EDU_DMA_RUN == 0 => return true,
                Ok(_) => {}
                Err(_) => return false,
            }
            spins = spins.saturating_add(1);
            if spins > 10_000_000 {
                return false;
            }
            raven_rt::yield_now();
        }
    }

    fn describe(error: AccessError) -> String {
        match error {
            AccessError::Bounds => "device access out of bounds".to_string(),
            AccessError::Alignment => "misaligned device access".to_string(),
        }
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}

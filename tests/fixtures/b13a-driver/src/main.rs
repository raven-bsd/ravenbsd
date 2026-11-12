// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! B13a fixture driver: proves the driver path end to end on QEMU's `edu` device.
//!
//! `devd` launches this program with two grants, in order: the requested BAR (`MmioRegion`,
//! handle 3) and one function's PCI configuration page (`MmioRegion`, handle 4). The fixture maps
//! the BAR writable, writes and reads back a register, prints the BAR size `devd` passed, reads the
//! configuration header and walks the capability list through `raven-driver`.
//!
//! It keeps `#![forbid(unsafe_code)]`: every device access goes through the safe API.

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
    use raven_driver::{AccessError, ConfigRegion, MmioRegion, PAGE};
    use raven_rt::entry;

    entry!(main);

    /// The first transferred handle of a fresh child (self 0, initfs 1, namespace 2).
    const BAR_HANDLE: Handle = Handle::new(3, 0);
    /// The second transferred handle.
    const CONFIG_HANDLE: Handle = Handle::new(4, 0);

    /// QEMU `edu` device: identification and scratch registers.
    const EDU_ID: u32 = 0x0100_00ed;
    const EDU_SCRATCH: usize = 0x08;
    const SCRATCH_VALUE: u32 = 0xfeed_face;

    fn main(args: raven_rt::Args) -> i32 {
        match run(args) {
            Ok(()) => 0,
            Err(message) => {
                raven_rt::eprintln!("b13a: {}", message);
                1
            }
        }
    }

    fn run(args: raven_rt::Args) -> Result<(), String> {
        let mut bar_index = 0usize;
        let mut bar_size = 0usize;
        for argument in args.iter().skip(1) {
            if let Some(value) = argument.strip_prefix("bar=") {
                bar_index = value
                    .parse()
                    .map_err(|_| format!("bad bar argument `{argument}`"))?;
            } else if let Some(value) = argument.strip_prefix("bar_size=") {
                bar_size = value
                    .parse()
                    .map_err(|_| format!("bad bar_size argument `{argument}`"))?;
            }
        }
        if bar_size == 0 {
            return Err("devd did not pass bar_size".to_string());
        }

        // A full-size BAR, mapped writable and accessed through `MmioRegion`.
        let window = raven_rt::map_mmio_mut(BAR_HANDLE, bar_size)
            .map_err(|error| format!("bar map: {}", error.description()))?;
        let mut bar = MmioRegion::new(window);
        if bar.len() != bar_size {
            return Err(format!("mapped {} bytes, expected {bar_size}", bar.len()));
        }
        raven_rt::println!("b13a: bar={bar_index} size={}", bar.len());

        let id = bar.read_u32(0).map_err(describe)?;
        if id != EDU_ID {
            return Err(format!("unexpected device id {id:#010x}"));
        }
        raven_rt::println!("b13a: id={id:#010x}");

        // The scratch register is plain read/write state in the device, so a write followed by a
        // read proves the MMIO window is genuinely writable and not a read-only alias.
        bar.write_u32(EDU_SCRATCH, SCRATCH_VALUE)
            .map_err(describe)?;
        let echo = bar.read_u32(EDU_SCRATCH).map_err(describe)?;
        if echo != SCRATCH_VALUE {
            return Err(format!("scratch read back {echo:#010x}"));
        }
        raven_rt::println!("b13a: mmio {echo:#010x}");

        // The function's configuration page, read-only, and its capability list.
        let config_window = raven_rt::map_mmio(CONFIG_HANDLE, PAGE)
            .map_err(|error| format!("config map: {}", error.description()))?;
        let config = ConfigRegion::new(config_window);
        let (vendor, device) = config.ids().map_err(describe)?;
        raven_rt::println!("b13a: config vendor={vendor:#06x} device={device:#06x}");
        let list = raven_driver::capabilities(&config).map_err(describe)?;
        match list.get(0) {
            Some(capability) => raven_rt::println!(
                "b13a: cap id={:#04x} offset={:#04x}",
                capability.id,
                capability.offset
            ),
            None => raven_rt::println!("b13a: caps none"),
        }
        raven_rt::println!("b13a: done");
        Ok(())
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

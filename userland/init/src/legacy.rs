// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The M1 uname banner, kept for profiles that ship no service manifests.
//!
//! `base`, `dev` and `tiny` have no `/etc/services.d/` files, so `svcd` behaves exactly like the
//! old `init`: print the banner, the `uname -a` line and exit 0. The `boot-uname` and `user-fmt`
//! qtests keep matching those lines.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;
use core::fmt::Write;
use core::sync::atomic::{AtomicU32, Ordering};

use raven_rt::print::write_str;
use raven_uname::{Options, format};

// A non-zero and a zero static force the `.data` and `.bss` segments required by C2's user ELF
// shape. The values are also read below so both stay live.
#[used]
static DATA_MARKER: AtomicU32 = AtomicU32::new(0x494e_4954);
#[used]
static BSS_MARKER: AtomicU32 = AtomicU32::new(0);

struct Writer;

struct Probe(usize);

impl fmt::Write for Probe {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        self.0 += text.len();
        Ok(())
    }
}

trait Value {
    fn value(&self) -> u32;
}

struct Number(u32);

impl Value for Number {
    fn value(&self) -> u32 {
        self.0
    }
}

fn format_probe() -> Result<(), fmt::Error> {
    let mut probe = Probe(0);
    probe.write_fmt(format_args!(
        "{} {:x} {:>8} {:?} {:?}",
        "str",
        255u32,
        255u32,
        [1u8, 2],
        Some(7u32)
    ))?;
    let object: &dyn Value = &Number(7);
    if object.value() != 7 {
        return Err(fmt::Error);
    }
    let mut closure = |value: u32| value + 1;
    let function: &mut dyn FnMut(u32) -> u32 = &mut closure;
    if function(7) != 8 || probe.0 == 0 {
        return Err(fmt::Error);
    }
    Ok(())
}

fn allocation_probe() -> Result<(), fmt::Error> {
    let text: String = alloc::format!("{} {:x}", "fmt", 255u32);
    if text.as_str() != "fmt ff" {
        return Err(fmt::Error);
    }
    let mut values = Vec::new();
    for value in 0..64u32 {
        values.push(value);
    }
    if values.len() != 64 || values[63] != 63 {
        return Err(fmt::Error);
    }
    Ok(())
}

impl fmt::Write for Writer {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        // Emit bytes directly. `print!("{text}")` would go through `Display for str`, whose
        // debug-build thunk is a GOT/PLT call that the user linker script discards.
        write_str(text).map(|_| ()).map_err(|_| fmt::Error)
    }
}

/// Print the M1 banner and return the process exit code.
pub fn run() -> i32 {
    let _ = (
        DATA_MARKER.load(Ordering::Relaxed),
        BSS_MARKER.load(Ordering::Relaxed),
    );
    let _ = write_str("init: starting\n");
    if format_probe().is_err() || allocation_probe().is_err() {
        return 1;
    }
    raven_rt::println!("init: fmt ok {} {:x} {}", 255u32, 255u32, "str");
    let options = Options {
        sysname: true,
        nodename: true,
        release: true,
        version: true,
        machine: true,
        processor: false,
        os: false,
    };
    let uname = match raven_rt::uname() {
        Ok(uname) => uname,
        Err(error) => {
            let _ = write_str("init: uname: ");
            let _ = write_str(error.description());
            let _ = write_str("\n");
            return 1;
        }
    };
    let mut writer = Writer;
    if format(&options, &uname, &mut writer).is_err() {
        return 1;
    }
    let _ = write_str("init: done\n");
    0
}

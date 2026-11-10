// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD PID 1 program for milestone M1.
//!
//! It prints the M1 banner through the same `raven-uname` formatting library used by the `uname`
//! utility: `init: starting`, the full `uname -a` line and `init: done`. The spawn-based service
//! manager arrives with K09/S24; a later ticket replaces this banner with
//! `spawn("/bin/uname", ["uname", "-a"])` and starts the service manifests.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
mod program {
    use core::fmt;
    use core::sync::atomic::{AtomicU32, Ordering};

    use raven_rt::entry;
    use raven_rt::print::write_str;
    use raven_uname::{Options, format};

    entry!(main);

    // A non-zero and a zero static force the `.data` and `.bss` segments required by C2's user
    // ELF shape. The values are also read below so both stay live.
    #[used]
    static DATA_MARKER: AtomicU32 = AtomicU32::new(0x494e_4954);
    #[used]
    static BSS_MARKER: AtomicU32 = AtomicU32::new(0);

    struct Writer;

    impl fmt::Write for Writer {
        fn write_str(&mut self, text: &str) -> fmt::Result {
            // Emit bytes directly. `print!("{text}")` would go through `Display for str`,
            // whose debug-build thunk is a GOT/PLT call that the user linker script discards.
            write_str(text).map(|_| ()).map_err(|_| fmt::Error)
        }
    }

    fn main(_args: raven_rt::Args) -> i32 {
        let _ = (
            DATA_MARKER.load(Ordering::Relaxed),
            BSS_MARKER.load(Ordering::Relaxed),
        );
        let _ = write_str("init: starting\n");
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
}

#[cfg(not(target_os = "none"))]
fn main() {}

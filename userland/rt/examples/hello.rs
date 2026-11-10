// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The K06 acceptance example: prints through `raven-rt` and exits 0.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
mod program {
    use core::sync::atomic::{AtomicU32, Ordering};

    use raven_rt::entry;

    // One non-zero and one zero static make this example exercise the `.data` and `.bss`
    // segments required by C2.
    #[used]
    static DATA_MARKER: AtomicU32 = AtomicU32::new(0x5241_5645);
    #[used]
    static BSS_MARKER: AtomicU32 = AtomicU32::new(0);

    entry!(main);

    /// Print the runtime banner and arguments, then exit successfully.
    fn main(args: raven_rt::Args) -> i32 {
        let _ = (
            DATA_MARKER.load(Ordering::Relaxed),
            BSS_MARKER.load(Ordering::Relaxed),
        );
        raven_rt::println!("hello from raven-rt");
        raven_rt::println!("argc={}", args.len());
        for (index, argument) in args.iter().enumerate() {
            raven_rt::println!("argv[{}]={}", index, argument);
        }
        0
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}

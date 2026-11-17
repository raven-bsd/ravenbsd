// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `times` utility.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
// The target reads the kernel's process/child counters through the raw PROC_TIMES syscall. The
// only unsafe block is the scalar syscall wrapper, with a SAFETY comment.
#![cfg_attr(not(target_os = "none"), forbid(unsafe_code))]

#[cfg(target_os = "none")]
mod program {
    use raven_abi::errno::Errno;
    use raven_rt::entry;
    use raven_tool_core::error::{EXIT_ERROR, EXIT_OK, EXIT_USAGE, usage};
    use times::{parse_args, render};

    /// `proc_times(out[4], count)`; see `kernel/src/syscall/process_ctl.rs`.
    const PROC_TIMES: usize = 0x4C;

    entry!(main);

    fn main(args: raven_rt::Args) -> i32 {
        if parse_args(args.iter().skip(1)).is_err() {
            raven_rt::eprintln!("{}", usage("times", ""));
            return EXIT_USAGE;
        }
        let mut values = [0u64; 4];
        // SAFETY: PROC_TIMES writes four u64 values to the pointer and takes the count as its
        // second scalar argument; the array is valid for exactly four elements.
        let result = unsafe {
            raven_rt::sys::syscall6(
                PROC_TIMES,
                values.as_mut_ptr() as usize,
                values.len(),
                0,
                0,
                0,
                0,
            )
        };
        if Errno::from_ret(result).is_err() {
            return EXIT_ERROR;
        }
        // The kernel reports nanoseconds; 1e9 ticks/second renders the POSIX hundredths form.
        raven_rt::print!(
            "{}",
            render(values[0], values[1], values[2], values[3], 1_000_000_000)
        );
        EXIT_OK
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}

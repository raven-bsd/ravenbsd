// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! P2e procd child: prints the argv/environment handed to it, then exits or sleeps.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![allow(unsafe_code)] // The SIGRECV wrapper below has a SAFETY comment.

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use raven_abi::errno::Errno;
    use raven_rt::entry;

    const PROC_SIGRECV: usize = 0x4B;

    entry!(main);

    fn main(args: raven_rt::Args) -> i32 {
        let mode = args.get(1).unwrap_or("missing");
        match mode {
            "exec" => {
                let test = args.env_get("TEST").unwrap_or("<unset>");
                raven_rt::println!("procd-child: exec TEST={test}");
                7
            }
            "sleep" => {
                raven_rt::println!("procd-child: sleeping");
                loop {
                    let signal = receive_signal();
                    if signal == 15 {
                        raven_rt::println!("procd-child: got signal 15");
                        return 143;
                    }
                    if signal < 0 {
                        let _ = raven_rt::nanosleep(10_000_000);
                    }
                }
            }
            _ => {
                raven_rt::println!("procd-child: unknown mode={mode}");
                2
            }
        }
    }

    fn receive_signal() -> i32 {
        let mut words = [0u64; 3];
        // SAFETY: SIGRECV writes three u64 words to the live array owned by the caller; a
        // blocking zero flags call waits for the next queued signal.
        let raw = unsafe { raven_rt::sys::syscall2(PROC_SIGRECV, words.as_mut_ptr() as usize, 0) };
        Errno::from_ret(raw).map_or(-1, |signo| signo as i32)
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}

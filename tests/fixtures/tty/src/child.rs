// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! P3 pty child: opens the slave and waits for the terminal interrupt.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![allow(unsafe_code)] // The SIGRECV wrapper below has a SAFETY comment.

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use raven_abi::errno::Errno;
    use raven_proto_fs::OpenFlags;
    use raven_rt::entry;
    use raven_scheme::Client;

    const PROC_SIGRECV: usize = 0x4B;

    entry!(main);

    fn main(_args: raven_rt::Args) -> i32 {
        let client = Client::new();
        let _ = client.open("pty:slave", u32::from(OpenFlags::READ_WRITE.to_wire()));
        raven_rt::println!("tty-child: ready");
        loop {
            let signal = receive_signal();
            if signal == 2 {
                raven_rt::println!("tty-child: sigint");
                return 130;
            }
            if signal < 0 {
                let _ = raven_rt::nanosleep(10_000_000);
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

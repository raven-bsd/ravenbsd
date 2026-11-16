// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! P2m controlling-terminal child: joins the parent's ttyd session and exercises background
//! read/write stops and foreground access.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![allow(unsafe_code)] // the SIGRECV wrapper has a SAFETY comment

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::format;

    use raven_abi::errno::Errno;
    use raven_proto_fs::OpenFlags;
    use raven_rt::entry;
    use raven_rt::println;
    use raven_rt::sys::{read, write};
    use raven_scheme::Client;

    const PROC_SIGRECV: usize = 0x4B;
    const SIGUSR1: u32 = 10;
    const SIGUSR2: u32 = 12;
    const SIGCONT: u32 = 18;

    entry!(main);

    fn main(args: raven_rt::Args) -> i32 {
        let Some(sid) = args.get(1) else {
            return 1;
        };
        let path = format!("ttyd:session/{sid}");
        println!("tty-ctl: open {path}");
        let client = Client::new();
        let tty = match client.open(&path, u32::from(OpenFlags::READ_WRITE.to_wire())) {
            Ok(handle) => handle,
            Err(error) => {
                println!("tty-ctl: open-fail {error:?}");
                return 2;
            }
        };
        println!("tty-ctl: ready");
        let mut byte = [0u8; 1];
        loop {
            match sigrecv() {
                SIGUSR1 => match read(tty, &mut byte) {
                    Err(Errno::EINTR) => println!("tty-ctl: ttin"),
                    Ok(_) | Err(Errno::EAGAIN) => println!("tty-ctl: fg-read"),
                    Err(_) => {
                        println!("tty-ctl: read-fail");
                        return 3;
                    }
                },
                SIGUSR2 => match write(tty, b"x") {
                    Err(Errno::EINTR) => println!("tty-ctl: ttou"),
                    Ok(_) => {
                        println!("tty-ctl: fg-write");
                        return 0;
                    }
                    Err(_) => {
                        println!("tty-ctl: write-fail");
                        return 4;
                    }
                },
                SIGCONT => continue,
                _ => continue,
            }
        }
    }

    fn sigrecv() -> u32 {
        let mut words = [0u64; 3];
        // SAFETY: PROC_SIGRECV writes three u64 words to the live array owned by the caller; a
        // blocking zero-flags call waits for the next queued signal.
        let raw = unsafe { raven_rt::sys::syscall2(PROC_SIGRECV, words.as_mut_ptr() as usize, 0) };
        Errno::from_ret(raw).map_or(0, |signo| signo as u32)
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}

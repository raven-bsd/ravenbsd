// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `kill` utility.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
// The runtime exposes no safe POSIX signal wrapper yet; this one raw PROC_SIGNAL call has a
// SAFETY comment and scalar arguments only.
#![allow(unsafe_code)]

#[cfg(target_os = "none")]
mod program {
    use raven_abi::errno::Errno;
    use raven_rt::entry;

    entry!(main);

    const PROC_SIGNAL: usize = 0x4A;
    const PROC_PID_INFO: usize = 0x3A;

    fn send(target: i32, signo: u32) -> Result<(), Errno> {
        if signo == 0 {
            return exists(target);
        }
        // SAFETY: PROC_SIGNAL takes two scalar values; the kernel validates the target pid or
        // process group, the signal number and the caller's permission before queuing it.
        let raw = unsafe {
            raven_rt::sys::syscall2(PROC_SIGNAL, target as isize as usize, signo as usize)
        };
        Errno::from_ret(raw).map(|_| ())
    }

    fn pid_info(pid: i32) -> Result<[u64; 8], Errno> {
        let mut words = [0u64; 8];
        // SAFETY: PROC_PID_INFO takes a scalar pid and a live eight-word array owned by the
        // caller; the kernel validates the pid and writes exactly eight u64 values.
        let raw = unsafe {
            raven_rt::sys::syscall3(
                PROC_PID_INFO,
                pid as isize as usize,
                words.as_mut_ptr() as usize,
                words.len(),
            )
        };
        Errno::from_ret(raw)?;
        Ok(words)
    }

    /// POSIX `kill -0`: check that a pid or process group exists without sending a signal.
    fn exists(target: i32) -> Result<(), Errno> {
        if target > 0 {
            pid_info(target).map(|_| ())?;
            return Ok(());
        }
        let group = if target == 0 {
            let info = pid_info(0)?;
            info[2]
        } else {
            u64::from(target.unsigned_abs())
        };
        for pid in 1..=4096 {
            if let Ok(info) = pid_info(pid) {
                if info[2] == group {
                    return Ok(());
                }
            }
        }
        Err(Errno::ESRCH)
    }

    fn list(query: Option<&str>) -> i32 {
        let Some(query) = query else {
            for (index, signal) in raven_kill::SIGNALS.iter().enumerate() {
                if index > 0 {
                    raven_rt::print!(" ");
                }
                raven_rt::print!("{}", signal.name);
            }
            raven_rt::println!();
            return 0;
        };
        if let Ok(number) = query.parse::<u32>() {
            if let Some(name) = raven_kill::by_number(number) {
                raven_rt::println!("{name}");
                return 0;
            }
        }
        if let Some(number) = raven_kill::by_name(query) {
            raven_rt::println!("{number}");
            return 0;
        }
        raven_rt::eprintln!("kill: unknown signal: {query}");
        1
    }

    fn main(args: raven_rt::Args) -> i32 {
        let action = match raven_kill::parse_args(args.iter().skip(1)) {
            Ok(action) => action,
            Err(_) => {
                raven_rt::eprintln!("usage: kill {}", raven_kill::USAGE);
                return 2;
            }
        };
        match action {
            raven_kill::Action::List { query } => list(query.as_deref()),
            raven_kill::Action::Send { signal, targets } => {
                let mut status = 0;
                for target in targets {
                    if let Err(error) = send(target, signal) {
                        raven_rt::eprintln!("kill: {target}: {}", error.description());
                        status = 1;
                    }
                }
                status
            }
        }
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}

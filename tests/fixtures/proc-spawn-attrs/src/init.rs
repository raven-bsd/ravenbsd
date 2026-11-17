// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Kernel process spawn attributes: fd map, signal mask and process group.
//!
//! No userspace process server is involved. The fixture uses the raven-rt/libc spawn path and
//! checks the attributes in the child through the RFDM and `AT_RAVEN_*` auxiliary vector.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![allow(unsafe_code)] // The raw process syscalls below carry SAFETY comments.

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use raven_abi::types::Handle;
    use raven_initfs::Initfs;
    use raven_rt::entry;
    use raven_rt::println;
    use raven_rt::spawn::{SpawnAttributes, SpawnOptions, spawn_with_attributes};

    const PROC_HANDLE_INFO: usize = 0x3E;
    const PROC_WAIT: usize = 0x31;
    const PROC_SETPGID: usize = 0x36;
    const PROC_GETPGID: usize = 0x49;
    const SIGUSR1: u32 = 10;
    const SIGUSR2: u32 = 12;
    const SIGTERM: u32 = 15;

    entry!(main);

    fn main(_args: raven_rt::Args) -> i32 {
        match run() {
            Ok(()) => {
                println!("proc-attrs: ok");
                0
            }
            Err(step) => {
                println!("proc-attrs: fail {step}");
                1
            }
        }
    }

    fn run() -> Result<(), &'static str> {
        let image = raven_rt::map_initfs().map_err(|_| "initfs map")?;
        let initfs = Initfs::parse(image).map_err(|_| "initfs parse")?;
        let (read, write) = raven_rt::pipe::pipe().map_err(|_| "pipe")?;
        raven_rt::sys::write(write, b"ok\n").map_err(|_| "pipe write")?;

        let mut sigmask = [0u8; 128];
        set_signal(&mut sigmask, SIGUSR1);
        let mut sigdefault = [0u8; 128];
        set_signal(&mut sigdefault, SIGTERM);
        let mut ignored = [0u8; 128];
        set_signal(&mut ignored, SIGUSR2);
        let fdmap = [(7, read.0)];
        let first = SpawnAttributes {
            fdmap: &fdmap,
            sigmask: &sigmask,
            sigdefault: &sigdefault,
            ignored: &ignored,
        };
        let child = spawn_with_attributes(
            &initfs,
            "/bin/proc-attrs-child",
            &SpawnOptions {
                args: &["proc-attrs-child", "attrs", "-1", "1"],
                env: &[],
            },
            &first,
        )
        .map_err(|_| "spawn1")?;
        let pid = handle_pid(child)?;
        setpgid(pid, 0)?;
        let code = wait(child)?;
        println!("proc-attrs: case1 exit={code}");
        if code != 0 {
            return Err("attrs1 exit");
        }

        let _ = raven_rt::sys::handle_close(read);
        let _ = raven_rt::sys::handle_close(write);
        Ok(())
    }

    fn handle_pid(child: Handle) -> Result<i32, &'static str> {
        // SAFETY: PROC_HANDLE_INFO reads the pid from a process handle owned by the caller.
        let pid = unsafe { raven_rt::sys::syscall1(PROC_HANDLE_INFO, child.0 as usize) };
        if pid < 0 {
            return Err("handle info");
        }
        i32::try_from(pid).map_err(|_| "pid range")
    }

    fn setpgid(pid: i32, pgid: i32) -> Result<(), &'static str> {
        // SAFETY: PROC_SETPGID takes the target pid and the new group.
        let result = unsafe { raven_rt::sys::syscall2(PROC_SETPGID, pid as usize, pgid as usize) };
        if result < 0 {
            return Err("setpgid");
        }
        Ok(())
    }

    fn wait(child: Handle) -> Result<i32, &'static str> {
        let mut code = 0i32;
        // SAFETY: PROC_WAIT writes one i32 exit code to a live stack slot.
        let result = unsafe {
            raven_rt::sys::syscall2(PROC_WAIT, child.0 as usize, &mut code as *mut i32 as usize)
        };
        if result < 0 {
            return Err("wait");
        }
        Ok(code)
    }

    fn set_signal(set: &mut [u8; 128], signal: u32) {
        let bit = usize::try_from(signal.saturating_sub(1)).unwrap_or(0);
        if let Some(byte) = set.get_mut(bit / 8) {
            *byte |= 1 << (bit % 8);
        }
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}

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
    use alloc::format;
    use alloc::string::String;

    use raven_abi::errno::Errno;
    use raven_abi::types::Handle;
    use raven_initfs::Initfs;
    use raven_rt::entry;
    use raven_rt::println;
    use raven_rt::spawn::{SpawnAttributes, SpawnOptions, spawn, spawn_with_attributes};

    const PROC_HANDLE_INFO: usize = 0x3E;
    const PROC_PID_INFO: usize = 0x3A;
    const PROC_SELF_INFO: usize = 0x35;
    const PROC_WAIT: usize = 0x31;
    const PROC_SETPGID: usize = 0x36;
    const INFO_WORDS: usize = 41;
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
        let cmd_ok = kernel_command_is(pid, "proc-attrs-child");
        println!("proc-attrs: cmd={}", u8::from(cmd_ok));
        if !cmd_ok {
            return Err("command");
        }
        setpgid(pid, 0)?;
        let code = wait(child)?;
        let _ = raven_rt::sys::handle_close(child);
        println!("proc-attrs: case1 exit={code}");
        if code != 0 {
            return Err("attrs1 exit");
        }

        let parent_pgid = self_pgid()?;
        let join_pgid = format!("{parent_pgid}");
        let join = spawn(
            &initfs,
            "/bin/proc-attrs-child",
            &SpawnOptions {
                args: &["proc-attrs-child", "join", join_pgid.as_str()],
                env: &[],
            },
        )
        .map_err(|_| "spawn-join")?;
        let join_pid = handle_pid(join)?;
        setpgid(join_pid, 0)?;
        setpgid(join_pid, parent_pgid)?;
        let code = wait(join)?;
        let _ = raven_rt::sys::handle_close(join);
        if code != 0 {
            return Err("join exit");
        }
        println!("proc-attrs: join=1");

        let session = spawn(
            &initfs,
            "/bin/proc-attrs-child",
            &SpawnOptions {
                args: &["proc-attrs-child", "session"],
                env: &[],
            },
        )
        .map_err(|_| "spawn-session")?;
        let session_pid = handle_pid(session)?;
        if !wait_session(session_pid) {
            return Err("setsid");
        }
        println!("proc-attrs: session=1");

        let idle = spawn(
            &initfs,
            "/bin/proc-attrs-child",
            &SpawnOptions {
                args: &["proc-attrs-child", "idle"],
                env: &[],
            },
        )
        .map_err(|_| "spawn-idle")?;
        let idle_pid = handle_pid(idle)?;
        let raw = setpgid_raw(idle_pid, session_pid);
        if raw != -(Errno::EPERM.0 as isize) {
            return Err("cross-session");
        }
        println!("proc-attrs: cross-session=1");
        let code = wait(idle)?;
        let _ = raven_rt::sys::handle_close(idle);
        if code != 0 {
            return Err("idle exit");
        }
        let code = wait(session)?;
        let _ = raven_rt::sys::handle_close(session);
        if code != 0 {
            return Err("session exit");
        }

        let exec_child = spawn(
            &initfs,
            "/bin/proc-attrs-child",
            &SpawnOptions {
                args: &["proc-attrs-child", "exec-name"],
                env: &[],
            },
        )
        .map_err(|_| "spawn-exec")?;
        let code = wait(exec_child)?;
        let _ = raven_rt::sys::handle_close(exec_child);
        if code != 0 {
            return Err("exec exit");
        }
        println!("proc-attrs: exec=1");

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
        let result = setpgid_raw(pid, pgid);
        if result < 0 {
            return Err("setpgid");
        }
        Ok(())
    }

    fn setpgid_raw(pid: i32, pgid: i32) -> isize {
        // SAFETY: PROC_SETPGID takes the target pid and the new group.
        unsafe { raven_rt::sys::syscall2(PROC_SETPGID, pid as usize, pgid as usize) }
    }

    fn self_pgid() -> Result<i32, &'static str> {
        let mut words = [0u64; 8];
        // SAFETY: PROC_SELF_INFO writes eight u64 identity words to the live caller array.
        let raw = unsafe {
            raven_rt::sys::syscall2(PROC_SELF_INFO, words.as_mut_ptr() as usize, words.len())
        };
        if raw < 0 {
            return Err("self info");
        }
        i32::try_from(words[2]).map_err(|_| "pgid range")
    }

    fn wait_session(pid: i32) -> bool {
        for _ in 0..100 {
            if let Some(ids) = ids_of(pid)
                && ids[2] == u64::try_from(pid).unwrap_or(0)
                && ids[3] == u64::try_from(pid).unwrap_or(0)
            {
                return true;
            }
            let _ = raven_rt::nanosleep(10_000_000);
        }
        false
    }

    fn ids_of(pid: i32) -> Option<[u64; 8]> {
        let mut words = [0u64; 8];
        // SAFETY: PROC_PID_INFO writes eight identity words to the live caller array.
        let raw = unsafe {
            raven_rt::sys::syscall3(
                PROC_PID_INFO,
                pid as isize as usize,
                words.as_mut_ptr() as usize,
                words.len(),
            )
        };
        if raw < 0 { None } else { Some(words) }
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

    fn kernel_command_is(pid: i32, expected: &str) -> bool {
        let mut words = [0u64; INFO_WORDS];
        // SAFETY: PROC_PID_INFO writes INFO_WORDS u64 words to the live caller-owned array.
        let raw = unsafe {
            raven_rt::sys::syscall3(
                PROC_PID_INFO,
                pid as isize as usize,
                words.as_mut_ptr() as usize,
                words.len(),
            )
        };
        if raw < 0 {
            return false;
        }
        command_from_words(&words).as_deref() == Some(expected)
    }

    fn command_from_words(words: &[u64; INFO_WORDS]) -> Option<String> {
        let mut bytes = alloc::vec::Vec::new();
        for word in words.iter().skip(8) {
            for byte in word.to_le_bytes() {
                if byte == 0 {
                    return core::str::from_utf8(&bytes).ok().map(String::from);
                }
                bytes.push(byte);
            }
        }
        core::str::from_utf8(&bytes).ok().map(String::from)
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}

// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! P2m session-leader hangup child.
//!
//! The process becomes a session leader, acquires a fresh ttyd session, forks a foreground child
//! in its own group and exits. The kernel must mark the controlling terminal hung and deliver
//! SIGHUP to the foreground group; the grandchild proves the signal and the hung read.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![allow(unsafe_code)] // raw fork/signal/pid wrappers, each with a SAFETY comment

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use raven_abi::errno::Errno;
    use raven_abi::types::Handle;
    use raven_proto_fs::OpenFlags;
    use raven_rt::entry;
    use raven_rt::println;
    use raven_scheme::Client;
    use raven_tty_fixture::{tcsetpgrp, tiocsctty};

    const PROC_FORK: usize = 0x39;
    const PROC_SETPGID: usize = 0x36;
    const PROC_SETSID: usize = 0x37;
    const PROC_HANDLE_INFO: usize = 0x3E;
    const PROC_SIGRECV: usize = 0x4B;
    const SIGHUP: u32 = 1;

    entry!(main);

    #[cfg(target_arch = "x86_64")]
    core::arch::global_asm!(
        r#"
        .section .text
        .global tty_hup_grandchild_entry
        .type tty_hup_grandchild_entry, @function
tty_hup_grandchild_entry:
        call tty_hup_grandchild_body
        mov edi, eax
        mov eax, 0x21
        syscall
        ud2
        .size tty_hup_grandchild_entry, . - tty_hup_grandchild_entry
    "#
    );

    #[cfg(target_arch = "aarch64")]
    core::arch::global_asm!(
        r#"
        .section .text
        .global tty_hup_grandchild_entry
        .type tty_hup_grandchild_entry, %function
tty_hup_grandchild_entry:
        bl tty_hup_grandchild_body
        mov x8, #0x21
        svc #0
1:
        b 1b
        .size tty_hup_grandchild_entry, . - tty_hup_grandchild_entry
    "#
    );

    unsafe extern "C" {
        fn tty_hup_grandchild_entry();
    }

    fn main(_args: raven_rt::Args) -> i32 {
        let client = Client::new();
        if setsid().is_err() {
            return 1;
        }
        let Ok(tty) = client.open("ttyd:session", u32::from(OpenFlags::READ_WRITE.to_wire()))
        else {
            return 2;
        };
        if tiocsctty(tty).is_err() {
            return 3;
        }
        if tcsetpgrp(tty, 0).is_err() {
            return 4;
        }
        // Give the grandchild its own stack. Reusing the leader's current stack would overlap
        // the still-active leader frames after the fork; a dedicated page keeps the two
        // processes' stacks independent even before the first COW fault.
        let Ok(stack) = raven_rt::sys::map_anonymous(0x4000) else {
            return 11;
        };
        let stack_top = stack.saturating_add(0x4000);
        let Ok(child) = fork() else {
            return 5;
        };
        let Ok(child_pid) = handle_info(child) else {
            return 6;
        };
        if setpgid(child_pid, child_pid).is_err() {
            return 7;
        }
        // Keep the leader in the foreground while creating the grandchild thread; hand the
        // terminal over only after the thread exists.
        let Ok(thread) = raven_rt::sys::thread_create(
            child,
            tty_hup_grandchild_entry as *const () as usize,
            stack_top,
            tty.0 as usize,
        ) else {
            return 9;
        };
        let _ = raven_rt::sys::handle_close(thread);
        if tcsetpgrp(tty, child_pid).is_err() {
            return 8;
        }
        println!("tty-hup: leader");
        0
    }

    /// Grandchild: wait for SIGHUP, then prove the terminal is hung.
    #[unsafe(no_mangle)]
    pub extern "C" fn tty_hup_grandchild_body(handle: usize) -> i32 {
        loop {
            match sigrecv() {
                SIGHUP => break,
                0 => return 10,
                _ => continue,
            }
        }
        println!("tty-hup: got");
        let mut byte = [0u8; 1];
        match raven_rt::sys::read(Handle(handle as u32), &mut byte) {
            Err(Errno::EIO) => println!("tty-hup: eio"),
            _ => println!("tty-hup: read-fail"),
        }
        0
    }

    fn fork() -> Result<Handle, Errno> {
        // SAFETY: PROC_FORK takes a self-process handle and zero flags; the kernel validates them.
        let raw = unsafe {
            raven_rt::sys::syscall6(PROC_FORK, Handle::SELF_PROC.0 as usize, 0, 0, 0, 0, 0)
        };
        Errno::from_ret(raw).map(|value| Handle(value as u32))
    }

    fn setpgid(pid: i32, pgid: i32) -> Result<(), Errno> {
        // SAFETY: both ids are scalar; the kernel checks the child relationship.
        let raw = unsafe {
            raven_rt::sys::syscall6(
                PROC_SETPGID,
                pid as isize as usize,
                pgid as isize as usize,
                0,
                0,
                0,
                0,
            )
        };
        Errno::from_ret(raw).map(|_| ())
    }

    fn setsid() -> Result<(), Errno> {
        // SAFETY: PROC_SETSID takes no arguments.
        let raw = unsafe { raven_rt::sys::syscall6(PROC_SETSID, 0, 0, 0, 0, 0, 0) };
        Errno::from_ret(raw).map(|_| ())
    }

    fn handle_info(handle: Handle) -> Result<i32, Errno> {
        // SAFETY: PROC_HANDLE_INFO takes a scalar handle and returns its pid.
        let raw =
            unsafe { raven_rt::sys::syscall6(PROC_HANDLE_INFO, handle.0 as usize, 0, 0, 0, 0, 0) };
        let pid = Errno::from_ret(raw)?;
        i32::try_from(pid).map_err(|_| Errno::EINVAL)
    }

    fn sigrecv() -> u32 {
        let mut words = [0u64; 3];
        // SAFETY: PROC_SIGRECV writes three u64 words to the live array; zero flags block.
        let raw = unsafe { raven_rt::sys::syscall2(PROC_SIGRECV, words.as_mut_ptr() as usize, 0) };
        Errno::from_ret(raw).map_or(0, |signo| signo as u32)
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}

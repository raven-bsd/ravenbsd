// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `popen`/`pclose` over the kernel raw `PIPE2` call, the existing `posix_spawn` driver and
//! `waitpid`.

use alloc::vec::Vec;
use core::ffi::{c_char, c_int, c_void};

use raven_abi::types::Handle;
use raven_libc_abi::errno::{EINVAL, Errno};
use raven_libc_abi::structs::PosixSpawnFileActionsT;
use raven_libc_platform::OfdKind;

use crate::ffi::platform;
use crate::stdio::streams::streams;

const PIPE2: usize = 0x6f;
const O_RDONLY: u32 = 0;
const O_WRONLY: u32 = 1;

fn fail(error: raven_libc_platform::Errno) -> c_int {
    crate::ffi::errno::set(Errno(error.0 as u16));
    -1
}

fn raw_pipe2() -> Result<(i32, i32), raven_libc_platform::Errno> {
    let mut handles = [0u32; 2];
    // SAFETY: PIPE2 writes two u32 handle ids into the caller's 8-byte buffer.
    let result = unsafe { raven_libc_sys::arch::syscall2(PIPE2, handles.as_mut_ptr() as usize, 0) };
    if result < 0 {
        return Err(raven_libc_platform::Errno(
            u16::try_from(-result).unwrap_or(5),
        ));
    }
    let platform = platform::global();
    let read = platform.install(Handle(handles[0]), OfdKind::Pipe, O_RDONLY, None, true)?;
    let write = match platform.install(Handle(handles[1]), OfdKind::Pipe, O_WRONLY, None, true) {
        Ok(fd) => fd,
        Err(error) => {
            let _ = platform.close(read);
            return Err(error);
        }
    };
    Ok((read, write))
}

fn direction(mode: &[u8]) -> Result<bool, Errno> {
    match mode.first() {
        Some(b'r') => Ok(true),
        Some(b'w') => Ok(false),
        _ => Err(Errno(EINVAL as u16)),
    }
}

/// `popen`: run `/bin/sh -c command` with a pipe to its stdin or stdout.
///
/// # Safety
/// `command` and `mode` are NUL-terminated strings.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn popen(command: *const c_char, mode: *const c_char) -> *mut c_void {
    if command.is_null() || mode.is_null() {
        crate::ffi::errno::set(Errno(EINVAL as u16));
        return core::ptr::null_mut();
    }
    // SAFETY: the caller's NUL-terminated mode.
    let mode_bytes = unsafe { crate::ffi::cstr::slice(mode) };
    let reading = match direction(mode_bytes) {
        Ok(value) => value,
        Err(error) => {
            crate::ffi::errno::set(error);
            return core::ptr::null_mut();
        }
    };
    let (read_fd, write_fd) = match raw_pipe2() {
        Ok(pair) => pair,
        Err(error) => {
            crate::ffi::errno::set(Errno(error.0 as u16));
            return core::ptr::null_mut();
        }
    };
    let (parent_end, child_end, target) = if reading {
        (read_fd, write_fd, 1)
    } else {
        (write_fd, read_fd, 0)
    };
    let mut actions = PosixSpawnFileActionsT::ZERO;
    let mut pid: c_int = 0;
    let mut argv = [
        b"/bin/sh\0".as_ptr().cast::<c_char>(),
        b"-c\0".as_ptr().cast::<c_char>(),
        command.cast_mut(),
        core::ptr::null_mut(),
    ];
    // SAFETY: the start path owns the environment array; spawn only reads it.
    let environment = unsafe { crate::stdlib::environ::environ };
    // SAFETY: all pointers follow the posix_spawn contract for the duration of the call.
    let result = unsafe {
        if crate::process::posix_spawn_file_actions_init::posix_spawn_file_actions_init(
            &mut actions,
        ) != 0
        {
            -1
        } else {
            let _ =
                crate::process::posix_spawn_file_actions_adddup2::posix_spawn_file_actions_adddup2(
                    &mut actions,
                    child_end,
                    target,
                );
            let _ = crate::process::posix_spawn_file_actions_addclose::posix_spawn_file_actions_addclose(
                &mut actions,
                child_end,
            );
            let _ = crate::process::posix_spawn_file_actions_addclose::posix_spawn_file_actions_addclose(
                &mut actions,
                parent_end,
            );
            for fd in raven_libc_stdio::close_sibling_pipes(streams().state()) {
                let _ = crate::process::posix_spawn_file_actions_addclose::posix_spawn_file_actions_addclose(
                    &mut actions,
                    fd,
                );
            }
            crate::process::posix_spawn::posix_spawn(
                &mut pid,
                b"/bin/sh\0".as_ptr().cast(),
                &actions,
                core::ptr::null(),
                argv.as_mut_ptr(),
                environment as *const *const c_char,
            )
        }
    };
    // SAFETY: the initialised action list is destroyed after the spawn.
    unsafe {
        let _ = crate::process::posix_spawn_file_actions_destroy::posix_spawn_file_actions_destroy(
            &mut actions,
        );
    }
    // Close the child's end in the parent; the parent end stays with the FILE.
    let platform = platform::global();
    let _ = platform.close(child_end);
    if result != 0 {
        let _ = platform.close(parent_end);
        crate::ffi::errno::set(Errno(u16::try_from(result).unwrap_or(5)));
        return core::ptr::null_mut();
    }
    match raven_libc_stdio::File::<crate::ffi::env::ShellEnv>::from_pipe(
        streams().state(),
        parent_end,
        pid,
        mode_bytes,
    ) {
        Ok(file) => file.cast(),
        Err(raven_libc_stdio::Fail::Errno(error)) => {
            let _ = platform.close(parent_end);
            crate::ffi::errno::set(error);
            core::ptr::null_mut()
        }
        Err(_) => {
            let _ = platform.close(parent_end);
            core::ptr::null_mut()
        }
    }
}

/// `pclose`: close the pipe stream and wait for the child.
///
/// # Safety
/// `stream` is a `popen` FILE.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pclose(stream: *mut c_void) -> c_int {
    if stream.is_null() {
        crate::ffi::errno::set(Errno(9));
        return -1;
    }
    // SAFETY: the caller passes a FILE from this libc.
    let file: &raven_libc_stdio::File<crate::ffi::env::ShellEnv> =
        raven_libc_stdio::file_from_ptr(stream.cast());
    let pid = match raven_libc_stdio::check_pipe(file) {
        Ok(pid) => pid,
        Err(raven_libc_stdio::Fail::Errno(error)) => {
            crate::ffi::errno::set(error);
            return -1;
        }
        Err(_) => return -1,
    };
    // SAFETY: the caller's FILE is closed once; the pid was copied first.
    let closed = unsafe { crate::stdio::file::fclose_impl(stream) };
    if closed != 0 {
        return -1;
    }
    let mut status: c_int = 0;
    // SAFETY: the child pid from the pipe record and a writable status slot.
    let waited = unsafe { crate::process::waitpid::waitpid(pid, &mut status, 0) };
    if waited < 0 { -1 } else { status }
}

#[allow(dead_code)]
fn _keep(_: Vec<u8>) {}

// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Container-like process start via clone with Linux namespaces.

use std::ffi::{CString, c_char, c_int, c_void};
use std::path::Path;
use std::time::Instant;

use anyhow::{Context, Result, bail};

use crate::affinity::pin_first_cpu;
use crate::result::BenchResult;
use crate::setup::ensure_ns_init;

const RUNS: usize = 200;
const WARMUP: usize = 10;
const STACK_WORDS: usize = 64 * 1024;

const FULL_FLAGS: c_int = libc::CLONE_NEWUSER
    | libc::CLONE_NEWPID
    | libc::CLONE_NEWNS
    | libc::CLONE_NEWNET
    | libc::SIGCHLD;
const NO_USER_FLAGS: c_int =
    libc::CLONE_NEWPID | libc::CLONE_NEWNS | libc::CLONE_NEWNET | libc::SIGCHLD;

pub(crate) fn run() -> Result<()> {
    let _affinity = pin_first_cpu()?;
    let init = ensure_ns_init()?;

    let (flags, labels, fallback) = match clone_start(FULL_FLAGS, &init) {
        Ok(_) => (FULL_FLAGS, "user,pid,mnt,net", None),
        Err(full_error) => match clone_start(NO_USER_FLAGS, &init) {
            Ok(_) => (
                NO_USER_FLAGS,
                "pid,mnt,net",
                Some(format!(
                    "user namespace unavailable: {full_error:#}; measured without CLONE_NEWUSER"
                )),
            ),
            Err(error) => {
                let note = format!(
                    "not measurable: clone with namespaces failed: {error:#}; without NEWUSER: {full_error:#}"
                );
                let result = BenchResult::from_samples("ns-start", "ns/op", &[], Some(note));
                result.print();
                result.write()?;
                return Ok(());
            }
        },
    };

    for _ in 0..WARMUP {
        clone_start(flags, &init)?;
    }
    let mut latencies = Vec::with_capacity(RUNS);
    let mut rss = Vec::with_capacity(RUNS);
    for _ in 0..RUNS {
        let (latency, peak) = clone_start(flags, &init)?;
        latencies.push(latency.as_nanos() as f64);
        rss.push(peak as f64);
    }

    let note = Some(match fallback {
        Some(message) => format!("namespaces={labels}; {message}"),
        None => format!("namespaces={labels}"),
    });
    let latency = BenchResult::from_samples("ns-start", "ns/op", &latencies, note.clone());
    latency.print();
    latency.write()?;
    let rss = BenchResult::from_samples("ns-start-rss", "bytes", &rss, note);
    rss.print();
    rss.write()?;
    Ok(())
}

fn clone_start(flags: c_int, init: &Path) -> Result<(std::time::Duration, u64)> {
    let mut pipe = [0; 2];
    // SAFETY: `pipe` is a writable two-int array and O_CLOEXEC is a valid flag.
    let status = unsafe { libc::pipe2(pipe.as_mut_ptr(), libc::O_CLOEXEC) };
    if status < 0 {
        bail!("pipe2 failed: {}", std::io::Error::last_os_error());
    }

    let path =
        CString::new(init.as_os_str().as_encoded_bytes()).context("init path contains NUL")?;
    let argv = [path.as_ptr(), std::ptr::null()];
    let args = Box::new(ChildArgs {
        path,
        argv,
        err_fd: pipe[1],
    });
    let arg = Box::into_raw(args) as *mut c_void;
    let mut stack = vec![0u128; STACK_WORDS];
    // SAFETY: the vector has STACK_WORDS elements; the one-past-end pointer is aligned and is
    // passed to clone as the initial child stack top.
    let stack_top = unsafe { stack.as_mut_ptr().add(stack.len()) } as *mut c_void;

    let start = Instant::now();
    // SAFETY: the child stack is a live 1 MiB allocation ending at `stack_top`, `arg` points to
    // a live ChildArgs, and the child function only execs or exits.
    let pid = unsafe { libc::clone(child_entry, stack_top, flags, arg) };
    if pid < 0 {
        let error = std::io::Error::last_os_error();
        // SAFETY: both descriptors were created by pipe2 above.
        unsafe {
            libc::close(pipe[0]);
            libc::close(pipe[1]);
        }
        // SAFETY: clone failed, so no child has a copy of the argument.
        drop(unsafe { Box::from_raw(arg as *mut ChildArgs) });
        return Err(anyhow::anyhow!("clone failed: {error}"));
    }

    // SAFETY: the parent closes its write end; the child owns the inherited descriptor.
    unsafe {
        libc::close(pipe[1]);
    }
    let mut errno_bytes = [0u8; 4];
    let mut read_count = 0;
    loop {
        // SAFETY: `errno_bytes` is a four-byte writable buffer and `pipe[0]` is open.
        let count = unsafe {
            libc::read(
                pipe[0],
                errno_bytes.as_mut_ptr() as *mut c_void,
                errno_bytes.len(),
            )
        };
        if count >= 0 {
            read_count = count as usize;
            break;
        }
        if std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted {
            break;
        }
    }
    // SAFETY: `pipe[0]` is the read end created above.
    unsafe {
        libc::close(pipe[0]);
    }

    let mut status = 0;
    // SAFETY: rusage is a plain C data structure for which all-zero is a valid value.
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    loop {
        // SAFETY: `pid` is our child, `status` and `usage` are writable.
        let waited = unsafe { libc::wait4(pid, &mut status, 0, &mut usage) };
        if waited == pid {
            break;
        }
        if waited < 0 && std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
            continue;
        }
        bail!("wait4 failed: {}", std::io::Error::last_os_error());
    }
    let elapsed = start.elapsed();
    // SAFETY: the child has been reaped, so its argument copy is no longer used.
    drop(unsafe { Box::from_raw(arg as *mut ChildArgs) });

    if read_count == 4 {
        let errno = i32::from_le_bytes(errno_bytes);
        bail!("init exec failed with errno {errno}");
    }
    if !libc::WIFEXITED(status) || libc::WEXITSTATUS(status) != 0 {
        bail!("init did not exit 0 (status {status})");
    }
    // ru_maxrss is in KiB on Linux.
    Ok((elapsed, usage.ru_maxrss as u64 * 1024))
}

struct ChildArgs {
    path: CString,
    argv: [*const c_char; 2],
    err_fd: c_int,
}

extern "C" fn child_entry(argument: *mut c_void) -> c_int {
    // SAFETY: the parent passes a live ChildArgs pointer and does not free it until wait4.
    let args = unsafe { &*(argument as *const ChildArgs) };
    // SAFETY: path is a NUL-terminated C string and argv is a NUL-terminated array; both live
    // in the parent-allocated ChildArgs until this exec returns or the child exits.
    unsafe {
        libc::execv(args.path.as_ptr(), args.argv.as_ptr());
    }
    // SAFETY: __errno_location returns this thread's writable errno slot.
    let errno = unsafe { *libc::__errno_location() };
    // SAFETY: err_fd is the inherited pipe write end; the four-byte errno buffer is local.
    unsafe {
        libc::write(
            args.err_fd,
            &errno as *const i32 as *const c_void,
            std::mem::size_of::<i32>(),
        );
        libc::_exit(127);
    }
}

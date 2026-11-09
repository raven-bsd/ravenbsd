// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! fork + execve + waitpid latency.

use std::ffi::CString;
use std::time::Instant;

use anyhow::{Context, Result, bail};

use crate::affinity::pin_first_cpu;
use crate::result::BenchResult;

const RUNS: usize = 1_000;
const WARMUP: usize = 20;

pub(crate) fn run() -> Result<()> {
    let _affinity = pin_first_cpu()?;
    std::fs::metadata("/bin/true").context("/bin/true is missing")?;
    let path = CString::new("/bin/true").expect("static path");
    let argv = [path.as_ptr(), std::ptr::null()];

    for _ in 0..WARMUP {
        fork_exec(&path, &argv)?;
    }
    let mut samples = Vec::with_capacity(RUNS);
    for _ in 0..RUNS {
        let start = Instant::now();
        fork_exec(&path, &argv)?;
        samples.push(start.elapsed().as_nanos() as f64);
    }

    let result = BenchResult::from_samples("fork-exec", "ns/op", &samples, None);
    result.print();
    result.write()?;
    Ok(())
}

fn fork_exec(path: &CString, argv: &[*const libc::c_char]) -> Result<()> {
    // SAFETY: fork has no arguments. The child immediately execs or exits without touching
    // shared allocator or lock state.
    let pid = unsafe { libc::fork() };
    if pid < 0 {
        bail!("fork failed: {}", std::io::Error::last_os_error());
    }
    if pid == 0 {
        // SAFETY: `path` and `argv` are valid, NUL-terminated C strings/arrays that outlive the
        // child; execve replaces the address space or returns an error.
        unsafe {
            libc::execv(path.as_ptr(), argv.as_ptr());
        }
        // SAFETY: _exit is async-signal-safe and does not return.
        unsafe {
            libc::_exit(127);
        }
    }

    let mut status = 0;
    loop {
        // SAFETY: `pid` is our child and `status` is a writable int.
        let waited = unsafe { libc::waitpid(pid, &mut status, 0) };
        if waited == pid {
            break;
        }
        if waited < 0 && std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
            continue;
        }
        bail!("waitpid failed: {}", std::io::Error::last_os_error());
    }
    if !libc::WIFEXITED(status) || libc::WEXITSTATUS(status) != 0 {
        bail!("child /bin/true did not exit 0 (status {status})");
    }
    Ok(())
}

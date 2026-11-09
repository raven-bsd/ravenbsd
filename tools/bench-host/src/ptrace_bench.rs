// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! PTRACE_SYSCALL forwarding proxy.

use std::time::{Duration, Instant};

use anyhow::{Result, bail};

use crate::affinity::pin_first_cpu;
use crate::result::{BenchResult, read_result};

const RUNS: usize = 30;
const SYSCALLS: usize = 100_000;

pub(crate) fn run() -> Result<()> {
    let _affinity = pin_first_cpu()?;
    let mut samples = Vec::with_capacity(RUNS);
    for _ in 0..RUNS {
        samples.push(trace_one()?.as_nanos() as f64 / SYSCALLS as f64);
    }

    let result = BenchResult::from_samples("ptrace-syscall", "ns/op", &samples, None);
    let note = match read_result("syscall-rtt-getpid")? {
        Some(native) if native.median > 0.0 => {
            format!("slowdown_vs_native={:.2}x", result.median / native.median)
        }
        _ => String::from("slowdown_vs_native=unknown (run syscall-rtt first)"),
    };
    let mut result = result;
    result.note = Some(note.clone());
    result.print();
    println!("{note}");
    result.write()?;
    Ok(())
}

fn trace_one() -> Result<Duration> {
    // SAFETY: fork has no arguments; the child only calls ptrace/raise/syscall/_exit.
    let pid = unsafe { libc::fork() };
    if pid < 0 {
        bail!("fork failed: {}", std::io::Error::last_os_error());
    }
    if pid == 0 {
        // SAFETY: ptrace(PTRACE_TRACEME) has no pointer arguments and is valid before exec.
        unsafe {
            libc::ptrace(
                libc::PTRACE_TRACEME,
                0,
                std::ptr::null_mut::<libc::c_void>(),
                std::ptr::null_mut::<libc::c_void>(),
            );
        }
        // SAFETY: raise delivers SIGSTOP to this process; the tracer is our parent.
        unsafe {
            libc::raise(libc::SIGSTOP);
        }
        for _ in 0..SYSCALLS {
            // SAFETY: SYS_getpid has no arguments and no memory side effects.
            unsafe {
                libc::syscall(libc::SYS_getpid);
            }
        }
        // SAFETY: _exit does not return and is safe after the trace loop.
        unsafe {
            libc::_exit(0);
        }
    }

    let mut child = TracedChild { pid, reaped: false };
    let mut status = 0;
    wait_for_change(pid, &mut status)?;
    if !libc::WIFSTOPPED(status) {
        bail!("child did not stop for the tracer (status {status})");
    }

    let start = Instant::now();
    loop {
        // SAFETY: `pid` is a stopped tracee; resume it to the next syscall stop.
        let resumed = unsafe {
            libc::ptrace(
                libc::PTRACE_SYSCALL,
                pid,
                std::ptr::null_mut::<libc::c_void>(),
                std::ptr::null_mut::<libc::c_void>(),
            )
        };
        if resumed < 0 {
            bail!(
                "ptrace(PTRACE_SYSCALL) failed: {}",
                std::io::Error::last_os_error()
            );
        }
        wait_for_change(pid, &mut status)?;
        if libc::WIFEXITED(status) {
            child.reaped = true;
            break;
        }
        if libc::WIFSIGNALED(status) {
            bail!("tracee terminated by signal {}", libc::WTERMSIG(status));
        }
        if !libc::WIFSTOPPED(status) {
            bail!("unexpected tracee status {status}");
        }
    }
    Ok(start.elapsed())
}

struct TracedChild {
    pid: libc::pid_t,
    reaped: bool,
}

impl Drop for TracedChild {
    fn drop(&mut self) {
        if self.reaped {
            return;
        }
        // SAFETY: the child is not reaped yet; kill and waitpid only reference its pid.
        unsafe {
            libc::kill(self.pid, libc::SIGKILL);
            libc::waitpid(self.pid, std::ptr::null_mut(), 0);
        }
    }
}

fn wait_for_change(pid: libc::pid_t, status: &mut libc::c_int) -> Result<()> {
    loop {
        // SAFETY: `pid` identifies the child and `status` is a writable int.
        let waited = unsafe { libc::waitpid(pid, status, 0) };
        if waited == pid {
            return Ok(());
        }
        if waited < 0 && std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
            continue;
        }
        return Err(anyhow::anyhow!(
            "waitpid failed: {}",
            std::io::Error::last_os_error()
        ));
    }
}

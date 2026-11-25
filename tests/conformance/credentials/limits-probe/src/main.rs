// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S190 limits probe: the kernel and libc `getrlimit`/`setrlimit`/`getrusage` rows.
//!
//! Packed as the initfs `/init` of the limits image (the `tests/fixtures/signal` pattern), so the
//! rows need no shell or service manager. The kernel rows go through the raw `PROC_*` calls and
//! are the reference; the libc rows enter through the `raven-libc` C exports so the marshalling
//! under test is the code the row exercises. The probe drops to the `raven` account first: an
//! unprivileged hard-limit raise must be `EPERM`.
//!
//! The account ids come from `/etc/passwd` inside the boot initfs. The probe is `/init`, so no
//! file server exists yet; the archive is read through the mapped initfs handle instead.
//!
//! Only `cred:` lines are read by `run.py --limits`. Exit 0 only when every row passed.

#![no_std]
#![no_main]
#![allow(unsafe_code)] // C ABI calls into raven-libc and raw syscalls; every block has SAFETY.

// Force the ravenlibc rlib into the link so its `#[no_mangle]` C exports resolve.
extern crate raven_libc;

use core::ffi::c_int;

use raven_abi::types::Handle;
use raven_libc_abi::resource::{RLIMIT_NOFILE, RLIM_INFINITY};
use raven_libc_abi::structs::{Rlimit, Rusage, Timeval};
use raven_libc_proc::syscall_backend::{PROC_GETRLIMIT, PROC_GETRUSAGE, PROC_SETRLIMIT};
use raven_rt::{entry, println};

// The libc C ABI under test. These are the same exports a C caller reaches; a Rust FFI call tests
// the export body and its marshalling, which is the S190 defect.
unsafe extern "C" {
    fn getrlimit(resource: c_int, limit: *mut Rlimit) -> c_int;
    fn setrlimit(resource: c_int, limit: *const Rlimit) -> c_int;
    fn getrusage(who: c_int, usage: *mut Rusage) -> c_int;
    fn geteuid() -> u32;
    fn setgid(gid: u32) -> c_int;
    fn setuid(uid: u32) -> c_int;
}

const EXPECTED_EPERM: i32 = 1;
const RUSAGE_SELF: c_int = 0;
const MAPPED_BYTES: usize = 4 * 1024 * 1024;
const PAGE: usize = 4096;
const SAMPLES: usize = 3;
const SPIN_ROUNDS: u64 = 8_000_000;
const NANOS_PER_SECOND: u64 = 1_000_000_000;
const NANOS_PER_MICROSECOND: u64 = 1_000;

struct Report {
    failures: u32,
}

impl Report {
    const fn new() -> Self {
        Self { failures: 0 }
    }

    fn fail(&mut self, row: &str, why: &str) {
        println!("cred: FAIL row={row} why={why}");
        self.failures = self.failures.saturating_add(1);
    }
}

/// Decode a raw syscall return into a positive errno (0 on success).
fn errno_of(raw: isize) -> i32 {
    match raven_libc_sys::errno::ret(raw) {
        Ok(_) => 0,
        Err(error) => i32::from(error.0),
    }
}

fn clear_errno() {
    raven_libc_sys::errno::set_errno(0);
}

fn libc_errno() -> i32 {
    raven_libc_sys::errno::errno()
}

fn raw_getrlimit(resource: usize, out: &mut [u64; 2]) -> i32 {
    // SAFETY: `out` is a live two-word array; the kernel validates the user pointer and writes
    // exactly the two u64 words `[cur, max]`.
    let raw = unsafe {
        raven_libc_sys::arch::syscall2(PROC_GETRLIMIT, resource, out.as_mut_ptr() as usize)
    };
    errno_of(raw)
}

fn raw_setrlimit(resource: usize, cur: u64, max: u64) -> i32 {
    let cur = usize::try_from(cur).unwrap_or(usize::MAX);
    let max = usize::try_from(max).unwrap_or(usize::MAX);
    // SAFETY: all three arguments are scalar words; the kernel applies the limit policy and the
    // privilege check itself.
    let raw = unsafe { raven_libc_sys::arch::syscall3(PROC_SETRLIMIT, resource, cur, max) };
    errno_of(raw)
}

fn raw_getrusage(who: usize, out: &mut [u64; 8]) -> i32 {
    let count = out.len();
    // SAFETY: `out` is a live eight-word array; the kernel validates the pointer and count and
    // writes exactly that many u64 values.
    let raw = unsafe {
        raven_libc_sys::arch::syscall3(PROC_GETRUSAGE, who, out.as_mut_ptr() as usize, count)
    };
    errno_of(raw)
}

fn mapped_bytes(report: &mut Report, row: &str) -> Option<usize> {
    match raven_rt::sys::map_anonymous(MAPPED_BYTES) {
        Ok(base) => {
            let mut offset = 0usize;
            while offset < MAPPED_BYTES {
                // SAFETY: `base..base + MAPPED_BYTES` is the live writable anonymous mapping
                // created above; `offset` stays page-aligned and inside that range.
                unsafe {
                    core::ptr::write_volatile((base + offset) as *mut u8, (offset >> 12) as u8);
                }
                offset = offset.saturating_add(PAGE);
            }
            Some(base)
        }
        Err(error) => {
            report.fail(row, "mmap");
            let _ = error;
            None
        }
    }
}

fn unmap(base: usize) {
    // SAFETY: `base` is the address returned by a successful anonymous mapping of MAPPED_BYTES;
    // the range is live and owned by this process.
    let _ = raven_rt::sys::unmap(Handle::SELF_PROC, base, MAPPED_BYTES);
}

fn spin() {
    let mut value = 0u64;
    let mut round = 0u64;
    while round < SPIN_ROUNDS {
        value = value.wrapping_add(round);
        round = round.wrapping_add(1);
    }
    core::hint::black_box(value);
}

fn timeval_ns(value: Timeval) -> u64 {
    let seconds = u64::try_from(value.tv_sec).unwrap_or(0);
    let micros = u64::try_from(value.tv_usec).unwrap_or(0);
    seconds
        .saturating_mul(NANOS_PER_SECOND)
        .saturating_add(micros.saturating_mul(NANOS_PER_MICROSECOND))
}

fn row_kernel_rlimit(report: &mut Report) {
    let resource = usize::try_from(RLIMIT_NOFILE).unwrap_or(usize::MAX);
    let mut original = [0u64; 2];
    let error = raw_getrlimit(resource, &mut original);
    if error != 0 {
        report.fail("kernel-rlimit-get", "syscall");
        return;
    }
    println!(
        "cred: kernel-rlimit-nofile cur={} max={}",
        original[0], original[1]
    );

    let error = raw_setrlimit(resource, original[0], original[1]);
    println!("cred: kernel-rlimit-roundtrip errno={error}");

    let error = raw_setrlimit(resource, original[1], original[1]);
    println!("cred: kernel-rlimit-soft-raise errno={error}");

    let raised = if original[1] == u64::MAX {
        original[1]
    } else {
        original[1] + 1
    };
    let error = raw_setrlimit(resource, original[0], raised);
    println!("cred: kernel-rlimit-hard-raise errno={error}");

    let mut after = [0u64; 2];
    let state = raw_getrlimit(resource, &mut after);
    println!(
        "cred: kernel-rlimit-hard-raise-state errno={state} cur={} max={}",
        after[0], after[1]
    );

    let error = raw_setrlimit(resource, original[0], original[1]);
    println!("cred: kernel-rlimit-restore errno={error}");
}

fn row_libc_rlimit(report: &mut Report) {
    let mut original = Rlimit::ZERO;
    clear_errno();
    // SAFETY: `original` is a live writable Rlimit; the libc checks the pointer and writes it.
    let result = unsafe { getrlimit(RLIMIT_NOFILE, &mut original) };
    println!(
        "cred: libc-getrlimit ret={result} errno={} cur={} max={}",
        libc_errno(),
        original.rlim_cur,
        original.rlim_max
    );
    if result != 0 {
        report.fail("libc-getrlimit", "getrlimit");
        return;
    }

    clear_errno();
    // SAFETY: `original` is a live readable Rlimit; the libc checks the pointer and reads it.
    let result = unsafe { setrlimit(RLIMIT_NOFILE, &original) };
    println!("cred: libc-setrlimit ret={result} errno={}", libc_errno());
    if result != 0 {
        report.fail("libc-setrlimit", "roundtrip");
    }

    let raised = Rlimit {
        rlim_cur: original.rlim_cur,
        rlim_max: if original.rlim_max == RLIM_INFINITY {
            original.rlim_max
        } else {
            original.rlim_max.saturating_add(1)
        },
    };
    clear_errno();
    // SAFETY: `raised` is a live readable Rlimit; the libc checks the pointer and reads it.
    let result = unsafe { setrlimit(RLIMIT_NOFILE, &raised) };
    let error = libc_errno();
    println!("cred: libc-hard-raise ret={result} errno={error}");
    if result != -1 || error != EXPECTED_EPERM {
        report.fail("libc-hard-raise", "expected EPERM");
    }
}

fn row_libc_rusage(report: &mut Report) {
    let Some(base) = mapped_bytes(report, "libc-rusage") else {
        return;
    };
    let mut sample = 0usize;
    while sample < SAMPLES {
        let mut usage = Rusage::ZERO;
        clear_errno();
        // SAFETY: `usage` is a live writable Rusage; the libc checks the pointer and writes it.
        let result = unsafe { getrusage(RUSAGE_SELF, &mut usage) };
        println!(
            "cred: libc-rusage{sample} ret={result} errno={} user={} sys={} maxrss={}",
            libc_errno(),
            timeval_ns(usage.ru_utime),
            timeval_ns(usage.ru_stime),
            usage.ru_maxrss
        );
        spin();
        sample = sample.saturating_add(1);
    }
    println!("cred: libc-rusage-mapped={MAPPED_BYTES}");
    unmap(base);
}

fn row_kernel_rusage(report: &mut Report) {
    let Some(base) = mapped_bytes(report, "kernel-rusage") else {
        return;
    };
    let mut sample = 0usize;
    while sample < SAMPLES {
        let mut words = [0u64; 8];
        let error = raw_getrusage(0, &mut words);
        println!(
            "cred: kernel-rusage{sample} errno={error} user={} sys={} maxrss={}",
            words[0], words[1], words[2]
        );
        if error != 0 {
            report.fail("kernel-rusage", "getrusage");
        }
        spin();
        sample = sample.saturating_add(1);
    }
    println!("cred: kernel-rusage-mapped={MAPPED_BYTES}");
    unmap(base);
}

/// The `raven` account ids from the boot initfs `/etc/passwd`.
fn lookup_raven() -> Option<(u32, u32)> {
    let image = raven_rt::map_initfs().ok()?;
    let archive = raven_initfs::Initfs::parse(image).ok()?;
    let entry = archive.get("/etc/passwd")?;
    let text = core::str::from_utf8(entry.data).ok()?;
    for line in text.lines() {
        let mut fields = line.split(':');
        if fields.next() != Some("raven") {
            continue;
        }
        let _password = fields.next()?;
        let uid = fields.next()?.parse::<u32>().ok()?;
        let gid = fields.next()?.parse::<u32>().ok()?;
        return Some((uid, gid));
    }
    None
}

/// Drop to the unprivileged `raven` account so the hard-raise row is the EPERM case.
fn drop_to_raven(report: &mut Report) -> bool {
    // SAFETY: `geteuid` takes no arguments and has no preconditions.
    if unsafe { geteuid() } != 0 {
        return true;
    }
    let Some((uid, gid)) = lookup_raven() else {
        report.fail("limits-service", "passwd-raven");
        return false;
    };
    // SAFETY: `gid` is the raven group id read from the initfs passwd file; `setgid` takes a
    // scalar id and the process is root at this point.
    if unsafe { setgid(gid) } != 0 {
        report.fail("limits-service", "setgid");
        return false;
    }
    // SAFETY: `uid` is the raven account id from the same file; the call drops the real,
    // effective and saved ids, so it must be the last identity change.
    if unsafe { setuid(uid) } != 0 {
        report.fail("limits-service", "setuid");
        return false;
    }
    true
}

entry!(main);

fn main(_args: raven_rt::Args) -> i32 {
    let mut report = Report::new();
    if !drop_to_raven(&mut report) {
        println!("cred: limits-service done failures={}", report.failures);
        return 1;
    }
    let before = report.failures;
    row_kernel_rlimit(&mut report);
    row_libc_rlimit(&mut report);
    row_libc_rusage(&mut report);
    row_kernel_rusage(&mut report);
    let failed_rows = report.failures.saturating_sub(before);
    println!("cred: limits-service done failures={failed_rows}");
    if failed_rows == 0 { 0 } else { 1 }
}

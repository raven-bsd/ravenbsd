// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! Minimal Rust std filesystem probe for C64f. It exercises `std::fs` over
//! ravenlibc: `mkdir`, write/`fsync`, metadata, `rename`, `fchmod` and
//! `isatty`. The body runs on the target under the `libc-test` profile; it is
//! not a host fallback.

use std::fs;
use std::io::{IsTerminal, Write};
use std::os::unix::fs::PermissionsExt;

/// Report a progress line to the kernel debug console.
///
/// The `libc-test` service is spawned without stdio handles, so Rust std's
/// `sanitize_standard_fds` points stdout and stderr at `/dev/null`. The qtest
/// reads the serial console, which is the `DEBUG_WRITE` sink. Emit there and
/// keep `println!` for a run that has a terminal attached.
fn report(line: &str) {
    debug_write(line.as_bytes());
    debug_write(b"\n");
    println!("{line}");
}

/// `DEBUG_WRITE(ptr, len)` (syscall 3): write bytes to the kernel diagnostic
/// console.
#[cfg(target_arch = "x86_64")]
fn debug_write(bytes: &[u8]) {
    // SAFETY: the kernel reads the slice for the duration of the call and does
    // not retain the pointer; the syscall has no other side effects.
    unsafe {
        core::arch::asm!(
            "syscall",
            inlateout("rax") 0x03usize => _,
            in("rdi") bytes.as_ptr(),
            in("rsi") bytes.len(),
            lateout("rcx") _,
            lateout("r11") _,
            clobber_abi("C"),
            options(nostack),
        );
    }
}

#[cfg(not(target_arch = "x86_64"))]
fn debug_write(_bytes: &[u8]) {}

fn main() {
    report("fs-probe: start");

    let root = "/tmp/c64e-fs";
    fs::create_dir(root).expect("create_dir");
    report("fs-probe: mkdir ok");

    let file = "/tmp/c64e-fs/a";
    let renamed = "/tmp/c64e-fs/b";
    {
        let mut handle = fs::File::create(file).expect("create");
        handle.write_all(b"hello").expect("write");
        handle.sync_all().expect("sync_all");
    }
    report("fs-probe: write+fsync ok");

    let metadata = fs::metadata(file).expect("metadata");
    assert_eq!(metadata.len(), 5);
    fs::rename(file, renamed).expect("rename");
    let metadata = fs::metadata(renamed).expect("metadata after rename");
    assert_eq!(metadata.len(), 5);
    report("fs-probe: metadata+rename ok");

    let handle = fs::File::open(renamed).expect("open for fchmod");
    handle
        .set_permissions(fs::Permissions::from_mode(0o600))
        .expect("fchmod");
    let mode = fs::metadata(renamed)
        .expect("metadata after fchmod")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o600);
    report("fs-probe: fchmod ok");

    report(&format!(
        "fs-probe: isatty={}",
        std::io::stdout().is_terminal()
    ));
    report("fs-probe: done");
}

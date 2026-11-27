// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S775 target probe: call the native `statfs(2)` with the uutils record type and check the
//! returned record. The pre-S775 120-byte record makes the native writer overrun the caller's
//! stack frame, so this probe is the target half of the record-size proof.

use std::ffi::CString;
use std::io::Write;
use std::mem::MaybeUninit;
use std::process::ExitCode;

use raven_statfs_abi::record::statfs as StatFs;

/// `sizeof(struct statfs)` in the native and FreeBSD records.
const NATIVE_SIZE: usize = 2344;

unsafe extern "C" {
    fn statfs(path: *const core::ffi::c_char, buf: *mut StatFs) -> core::ffi::c_int;
}

fn report(line: &str) {
    println!("{line}");
    let _ = std::io::stdout().flush();
}

fn main() -> ExitCode {
    report("statfs-abi: start");
    report(&format!("statfs-abi: size={}", core::mem::size_of::<StatFs>()));
    if core::mem::size_of::<StatFs>() != NATIVE_SIZE {
        report("statfs-abi: FAIL record-size");
        return ExitCode::FAILURE;
    }
    let path = CString::new("/").expect("path");
    let mut buffer = MaybeUninit::<StatFs>::zeroed();
    // SAFETY: the path is a NUL-terminated C string and the buffer is a writable struct statfs
    // of exactly the size the native libc writes.
    let rc = unsafe { statfs(path.as_ptr(), buffer.as_mut_ptr()) };
    if rc != 0 {
        report("statfs-abi: FAIL call");
        return ExitCode::FAILURE;
    }
    report("statfs-abi: done");
    ExitCode::SUCCESS
}

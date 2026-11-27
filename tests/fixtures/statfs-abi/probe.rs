// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S775 target probe: call the native `statfs(2)` with the uutils record type and check the
//! returned record. Linked against ravenlibc and packed as `/init` by `build.sh`.
//!
//! The pre-S775 120-byte record makes the native 2344-byte write overrun the caller's stack
//! frame, so this probe is the target half of the record-size proof. It is a `no_std` binary:
//! the record checks run before std (whose runtime is not needed here) can interfere.

#![no_std]
#![no_main]

use core::ffi::{c_char, c_int};

#[path = "../../../userland/uutils/libc-ravenbsd/src/unix/ravenbsd/statfs_record.rs"]
mod record;

/// The FreeBSD `STATFS_VERSION` the native writer fills in.
const STATFS_VERSION: u32 = 0x2014_0518;
/// `sizeof(struct statfs)` in the native and FreeBSD records.
const NATIVE_SIZE: usize = 2344;

unsafe extern "C" {
    fn statfs(path: *const c_char, buf: *mut record::statfs) -> c_int;
}

/// Write bytes to the kernel diagnostic console (`DEBUG_WRITE`, syscall 3).
fn out(bytes: &[u8]) {
    // SAFETY: the kernel reads the slice for the duration of the call and does not retain the
    // pointer; the syscall has no other side effects.
    unsafe {
        core::arch::asm!(
            "syscall",
            inlateout("rax") 3usize => _,
            in("rdi") bytes.as_ptr(),
            in("rsi") bytes.len(),
            lateout("rcx") _,
            lateout("r11") _,
            clobber_abi("C"),
            options(nostack),
        );
    }
}

fn fail(reason: &[u8]) -> c_int {
    out(b"statfs-abi: FAIL ");
    out(reason);
    out(b"\n");
    1
}

#[unsafe(no_mangle)]
pub extern "C" fn main(
    _argc: c_int,
    _argv: *const *const c_char,
    _envp: *const *const c_char,
) -> c_int {
    out(b"statfs-abi: start\n");
    if core::mem::size_of::<record::statfs>() != NATIVE_SIZE {
        return fail(b"record-size");
    }
    out(b"statfs-abi: size=2344\n");

    let path = b"/\0";
    let mut buffer = core::mem::MaybeUninit::<record::statfs>::uninit();
    // SAFETY: `path` is a NUL-terminated C string and the buffer is a writable struct statfs
    // of exactly the size the native libc writes.
    let rc = unsafe { statfs(path.as_ptr().cast(), buffer.as_mut_ptr()) };
    if rc != 0 {
        return fail(b"call");
    }
    // SAFETY: a successful call filled the whole record.
    let record = unsafe { buffer.assume_init() };

    if record.f_version != STATFS_VERSION {
        return fail(b"version");
    }
    out(b"statfs-abi: version=0x20140518\n");
    if record.f_bsize != 4096 {
        return fail(b"bsize");
    }
    out(b"statfs-abi: bsize=4096\n");
    if record.f_blocks == 0 {
        return fail(b"blocks");
    }
    out(b"statfs-abi: blocks ok\n");
    if record.f_files == 0 {
        return fail(b"files");
    }
    out(b"statfs-abi: files ok\n");
    if record.f_namemax != 255 {
        return fail(b"namemax");
    }
    out(b"statfs-abi: namemax=255\n");
    out(b"statfs-abi: done\n");
    0
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}

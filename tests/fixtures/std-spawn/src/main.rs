// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Minimal static std probe for the K548 spawn fault.
//!
//! It builds and links exactly like brush (libc + std, static, ravenbsd target) but contains no
//! brush code. Order:
//!   1. HashMap insert and rehash (the table pointer is printed);
//!   2. raw libc `posix_spawn` + raw `waitpid` (pid, return, errno, status, from_raw);
//!   3. std `Command::spawn`, the call that currently stores through a null result pointer.
//! The serial therefore names whether the libc spawn return path or std's result construction
//! owns the null.

use std::collections::HashMap;
use std::ffi::{c_char, c_int, c_void};
use std::os::unix::process::ExitStatusExt;
use std::process::Command;

unsafe extern "C" {
    fn posix_spawn(
        pid: *mut c_int,
        path: *const c_char,
        file_actions: *const c_void,
        attrp: *const c_void,
        argv: *const *const c_char,
        envp: *const *const c_char,
    ) -> c_int;
    fn waitpid(pid: c_int, status: *mut c_int, options: c_int) -> c_int;
}

fn main() {
    if std::env::args().nth(1).as_deref() == Some("child") {
        println!("stdprobe: child-ok");
        return;
    }

    println!("stdprobe: start");

    let mut map: HashMap<u64, u64> = HashMap::new();
    map.insert(1, 10);
    map.insert(2, 20);
    // The key reference points into the table allocation; a null value means the bucket table
    // was never initialized.
    let key_ptr = map.keys().next().map_or(std::ptr::null(), |key| key as *const u64);
    println!(
        "stdprobe: hashmap len={} cap={} key_ptr={key_ptr:p}",
        map.len(),
        map.capacity()
    );

    for index in 0..128u64 {
        map.insert(100 + index, index);
    }
    println!("stdprobe: rehash ok len={} cap={}", map.len(), map.capacity());

    // Step 2: the libc spawn return path, before any std Child/ExitStatus construction.
    let path = b"/bin/std-spawn\0";
    let arg0 = b"std-spawn\0";
    let arg1 = b"child\0";
    let env = b"STDPROBE=1\0";
    let argv: [*const c_char; 3] = [
        arg0.as_ptr().cast(),
        arg1.as_ptr().cast(),
        std::ptr::null(),
    ];
    let envp: [*const c_char; 2] = [env.as_ptr().cast(), std::ptr::null()];
    let mut raw_pid: c_int = -1;
    let rc = unsafe {
        posix_spawn(
            &mut raw_pid,
            path.as_ptr().cast(),
            std::ptr::null(),
            std::ptr::null(),
            argv.as_ptr(),
            envp.as_ptr(),
        )
    };
    println!("stdprobe: raw posix_spawn rc={rc} pid={raw_pid}");

    let mut raw_status: c_int = 0;
    let waited = unsafe { waitpid(raw_pid, &mut raw_status, 0) };
    let errno = std::io::Error::last_os_error().raw_os_error();
    println!(
        "stdprobe: raw waitpid rc={waited} errno={errno:?} status=0x{raw_status:08x} slot={:p}",
        std::ptr::addr_of!(raw_status)
    );
    let exit = std::process::ExitStatus::from_raw(raw_status);
    println!(
        "stdprobe: raw from_raw success={} code={:?}",
        exit.success(),
        exit.code()
    );

    // Step 3: the std path. The caller passes a valid result slot; this store is the null one.
    println!("stdprobe: std spawn next");
    let mut command = Command::new("/bin/std-spawn");
    command.arg("child").env_clear().env("STDPROBE", "1");
    match command.spawn() {
        Ok(child) => println!("stdprobe: std spawn pid={}", child.id()),
        Err(error) => println!("stdprobe: std spawn err {error:?}"),
    }

    println!("stdprobe: done");
}

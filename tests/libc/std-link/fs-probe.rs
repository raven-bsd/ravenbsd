// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! Minimal Rust std filesystem probe for C64e. It exercises the five symbols
//! that `std::fs` needs from ravenlibc: `mkdir`, `rename`, `fsync`, `isatty`
//! and (link-only) `fchmod`. The body runs on the target under the
//! `libc-test` profile; it is not a host fallback.

use std::fs;
use std::io::{IsTerminal, Write};

unsafe extern "C" {
    fn fchmod(fd: i32, mode: u32) -> i32;
}

fn main() {
    println!("fs-probe: start");

    // `std::fs::File::set_permissions` is the std path that references
    // `fchmod`; take its address so the symbol is linked without calling the
    // PG-21 platform stub.
    let _fchmod: unsafe extern "C" fn(i32, u32) -> i32 = fchmod;
    let _ = std::hint::black_box(_fchmod);

    let root = "/tmp/c64e-fs";
    fs::create_dir(root).expect("create_dir");
    println!("fs-probe: mkdir ok");

    let file = "/tmp/c64e-fs/a";
    let renamed = "/tmp/c64e-fs/b";
    {
        let mut handle = fs::File::create(file).expect("create");
        handle.write_all(b"hello").expect("write");
        handle.sync_all().expect("sync_all");
    }
    println!("fs-probe: write+fsync ok");

    let metadata = fs::metadata(file).expect("metadata");
    assert_eq!(metadata.len(), 5);
    fs::rename(file, renamed).expect("rename");
    let metadata = fs::metadata(renamed).expect("metadata after rename");
    assert_eq!(metadata.len(), 5);
    println!("fs-probe: metadata+rename ok");

    println!("fs-probe: isatty={}", std::io::stdout().is_terminal());
    println!("fs-probe: done");
}

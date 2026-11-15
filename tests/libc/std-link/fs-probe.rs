// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! Minimal Rust std filesystem probe for C64e. It exercises the five symbols
//! that `std::fs` needs from ravenlibc: `mkdir`, `rename`, `fsync`, `isatty`
//! and (link-only) `fchmod`. The body runs on the target under the
//! `libc-test` profile; it is not a host fallback.

use std::fs;
use std::io::{IsTerminal, Write};
use std::os::unix::fs::PermissionsExt;

fn main() {
    println!("fs-probe: start");

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
    println!("fs-probe: fchmod ok");

    println!("fs-probe: isatty={}", std::io::stdout().is_terminal());
    println!("fs-probe: done");
}

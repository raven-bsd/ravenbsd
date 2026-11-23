// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Minimal static std probe for the K548 spawn fault.
//!
//! It builds and links exactly like brush (libc + std, static, ravenbsd target) but contains no
//! brush code. The first insert records the HashMap table key address (a valid table is
//! non-null); the second block forces a rehash; then it spawns itself with the `child` argument
//! through `std::process::Command` and inherits stdio. If std/libc initialization is the defect,
//! this probe faults at the same place; if it passes, the defect is brush-specific.

use std::collections::HashMap;
use std::process::Command;

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

    // Diagnostic split: `nowait` spawns without status/wait; this separates the spawn/exec path
    // from the wait path. The default is the status path (the user-visible shell shape).
    if std::env::args().nth(1).as_deref() == Some("nowait") {
        match Command::new("/bin/std-spawn")
            .arg("child")
            .env_clear()
            .env("STDPROBE", "1")
            .spawn()
        {
            Ok(_) => println!("stdprobe: spawned"),
            Err(error) => println!("stdprobe: spawn err {error:?}"),
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
        println!("stdprobe: done");
        return;
    }

    let status = Command::new("/bin/std-spawn")
        .arg("child")
        .env_clear()
        .env("STDPROBE", "1")
        .status();
    match status {
        Ok(status) => println!("stdprobe: child status={:?}", status.code()),
        Err(error) => println!("stdprobe: child err {error:?}"),
    }

    println!("stdprobe: done");
}

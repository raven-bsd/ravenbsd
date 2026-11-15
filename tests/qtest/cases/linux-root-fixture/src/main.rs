// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! B32 Linux-root fixture.
//!
//! The initfs carries the Alpine tree staged by `addons/linux-root/build.sh`. The fixture verifies
//! the release file, BusyBox and the musl loader, then asks `linuxd` to run
//! `/compat/linux/bin/busybox ls`. BusyBox is a PIE with `PT_INTERP`; the current image parser
//! rejects its vaddr-0 segments, so the serial output records that boundary (B33 adds the PIE bias
//! and interpreter paths).

#![no_std]
#![no_main]

use raven_initfs::Initfs;
use raven_rt::entry;
use raven_rt::println;
use raven_rt::spawn::{RtSys, SpawnOptions, spawn_with};
use raven_rt::sys;

entry!(main);

/// The paths the recipe must have staged.
const RELEASE: &str = "/compat/linux/etc/alpine-release";
const BUSYBOX: &str = "/compat/linux/bin/busybox";
const LOADER: &str = "/compat/linux/lib/ld-musl-x86_64.so.1";

fn main(_args: raven_rt::Args) -> i32 {
    let image = match sys::map_initfs() {
        Ok(image) => image,
        Err(error) => {
            println!("linux-root: FAIL initfs map: {}", error.description());
            return 1;
        }
    };
    let initfs = match Initfs::parse(image) {
        Ok(initfs) => initfs,
        Err(_) => {
            println!("linux-root: FAIL initfs parse");
            return 1;
        }
    };
    let release = match initfs.get(RELEASE) {
        Some(entry) => entry.data,
        None => {
            println!("linux-root: FAIL missing {RELEASE}");
            return 1;
        }
    };
    let busybox = match initfs.get(BUSYBOX) {
        Some(entry) => entry.data,
        None => {
            println!("linux-root: FAIL missing {BUSYBOX}");
            return 1;
        }
    };
    let loader = match initfs.get(LOADER) {
        Some(entry) => entry.data,
        None => {
            println!("linux-root: FAIL missing {LOADER}");
            return 1;
        }
    };
    let release = core::str::from_utf8(release).unwrap_or("?").trim();
    println!("linux-root: release={release}");
    println!(
        "linux-root: busybox bytes={} loader bytes={}",
        busybox.len(),
        loader.len()
    );
    println!("linux-root: attempt path={BUSYBOX} argv1=ls");

    let options = SpawnOptions {
        args: &["/sbin/linuxd", BUSYBOX, "ls"],
        env: &[],
    };
    let child = match spawn_with(&initfs, "/sbin/linuxd", &options, &mut RtSys) {
        Ok(child) => child,
        Err(error) => {
            println!("linux-root: FAIL spawn linuxd: {error:?}");
            return 1;
        }
    };
    let mut rt = RtSys;
    let mut status = 0i32;
    if let Err(error) = rt.proc_wait(child, &mut status) {
        println!("linux-root: FAIL wait linuxd: {}", error.description());
        return 1;
    }
    println!("linux-root: busybox exit={status}");
    0
}

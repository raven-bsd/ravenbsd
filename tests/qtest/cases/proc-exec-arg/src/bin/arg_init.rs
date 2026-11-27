// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S168b fixture init: replace this image with `/arg-child` so the child's `_start` observes the
//! exec frame the kernel installs.

#![no_std]
#![no_main]

extern crate alloc;

use raven_rt::entry;
use raven_rt::exec::exec_image;
use raven_rt::println;

entry!(main);

fn main(_args: raven_rt::Args) -> i32 {
    println!("proc-exec-arg: init");
    let archive = match raven_rt::map_initfs() {
        Ok(archive) => archive,
        Err(error) => {
            println!("proc-exec-arg: init map error {error:?}");
            return 1;
        }
    };
    let initfs = match raven_initfs::Initfs::parse(archive) {
        Ok(initfs) => initfs,
        Err(_) => {
            println!("proc-exec-arg: init image error");
            return 1;
        }
    };
    let child = match initfs.get("/arg-child") {
        Some(child) => child,
        None => {
            println!("proc-exec-arg: init missing child");
            return 1;
        }
    };
    match exec_image(child.data, b"/arg-child", &[b"/arg-child"], &[], &[]) {
        Ok(()) => {
            println!("proc-exec-arg: init exec returned");
            1
        }
        Err(error) => {
            println!("proc-exec-arg: init exec error {error:?}");
            1
        }
    }
}

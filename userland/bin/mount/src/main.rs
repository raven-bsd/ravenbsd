// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `mount` utility.
//!
//! With no arguments it lists the static mount table from `/etc/mounts` as
//! `<scheme> on <path>` (K16 has no `LISTMOUNTS` control operation). `mount <scheme:> <path>`
//! adds a mount through the K60 `MOUNT` call on `file:`; `--selftest` lists, mounts `ramfs:` at
//! `/mnt`, unmounts it and prints the marker.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
mod program {
    use core::fmt;

    use raven_mount::{Action, format_mounts, parse_args};
    use raven_rt::entry;

    entry!(main);

    const TOOL: &str = "mount";

    struct Console;

    impl fmt::Write for Console {
        fn write_str(&mut self, text: &str) -> fmt::Result {
            raven_rt::print!("{text}");
            Ok(())
        }
    }

    fn list() -> i32 {
        // `raven_rt::fs` resolves bare paths to an absolute path but its K30 client needs an
        // explicit scheme here (the K38-era helper is not merged); use the file: scheme.
        let data = match raven_rt::fs::read_to_vec("file:/etc/mounts") {
            Ok(data) => data,
            Err(error) => {
                raven_rt::eprintln!("{TOOL}: /etc/mounts: {}", error.description());
                return 1;
            }
        };
        let text = match core::str::from_utf8(&data) {
            Ok(text) => text,
            Err(_) => {
                raven_rt::eprintln!("{TOOL}: /etc/mounts: invalid text");
                return 1;
            }
        };
        let mut console = Console;
        if format_mounts(text, &mut console).is_err() {
            return 1;
        }
        0
    }

    fn mount(scheme: &str, path: &str) -> i32 {
        match raven_rt::fs::mount(path, scheme) {
            Ok(()) => 0,
            Err(error) => {
                raven_rt::eprintln!("{TOOL}: {path}: {}", error.description());
                1
            }
        }
    }

    fn selftest() -> i32 {
        if list() != 0 {
            return 1;
        }
        let result: Result<(), raven_abi::errno::Errno> = (|| {
            raven_rt::fs::mount("/mnt", "ramfs:")?;
            raven_rt::fs::umount("/mnt")?;
            Ok(())
        })();
        match result {
            Ok(()) => {
                raven_rt::println!("{TOOL}: selftest ok");
                0
            }
            Err(error) => {
                let _ = raven_rt::fs::umount("/mnt");
                raven_rt::eprintln!("{TOOL}: {}", error.description());
                1
            }
        }
    }

    fn main(args: raven_rt::Args) -> i32 {
        match parse_args(args.iter().skip(1)) {
            Ok(Action::List) => list(),
            Ok(Action::Mount { scheme, path }) => mount(scheme, path),
            Ok(Action::Selftest) => selftest(),
            Err(error) => {
                raven_rt::eprintln!("{error}");
                2
            }
        }
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}

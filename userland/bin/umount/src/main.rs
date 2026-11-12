// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `umount` utility.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
mod program {
    use raven_rt::entry;
    use raven_umount::{Action, parse_args};

    entry!(main);

    const TOOL: &str = "umount";

    fn unmount(path: &str) -> i32 {
        match raven_rt::fs::umount(path) {
            Ok(()) => 0,
            Err(error) => {
                raven_rt::eprintln!("{TOOL}: {path}: {}", error.description());
                1
            }
        }
    }

    fn selftest() -> i32 {
        let result: Result<(), raven_abi::errno::Errno> = (|| {
            raven_rt::fs::mount("/mnt2", "ramfs:")?;
            raven_rt::fs::umount("/mnt2")?;
            Ok(())
        })();
        match result {
            Ok(()) => {
                raven_rt::println!("{TOOL}: selftest ok");
                0
            }
            Err(error) => {
                let _ = raven_rt::fs::umount("/mnt2");
                raven_rt::eprintln!("{TOOL}: {}", error.description());
                1
            }
        }
    }

    fn main(args: raven_rt::Args) -> i32 {
        match parse_args(args.iter().skip(1)) {
            Ok(Action::Path(path)) => unmount(path),
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

// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! P4a10 required-utility target gate.
//!
//! Starts the small service set needed by `newgrp`, then runs each of the seven required POSIX
//! utilities in a bounded non-interactive case. Every case prints a start line, the tool output,
//! and an `exit=<code> PASS|FAIL` verdict. The fixture exits non-zero when any required row
//! fails; all seven rows pass on x86_64 and aarch64.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
mod program {
    extern crate alloc;

    use alloc::format;
    use alloc::string::String;

    use raven_initfs::Initfs;
    use raven_proto_fs::O_RDONLY;
    use raven_rt::entry;
    use raven_rt::fs;
    use raven_rt::println;
    use raven_rt::spawn::{RtSys, SpawnOptions, spawn};

    entry!(main);

    fn main(_args: raven_rt::Args) -> i32 {
        match run() {
            Ok(false) => 0,
            Ok(true) => 1,
            Err(message) => {
                println!("required-utils: fail {message}");
                1
            }
        }
    }

    fn run() -> Result<bool, String> {
        let image = raven_rt::map_initfs().map_err(|_| String::from("initfs map"))?;
        let initfs = Initfs::parse(image).map_err(|_| String::from("initfs parse"))?;
        start_services(&initfs)?;
        println!("required-utils: services ok");

        let mut pass = 0usize;
        let mut fail = 0usize;
        case(
            &initfs,
            "time",
            "/bin/time",
            &["/bin/time", "-p", "/bin/true"],
            &[],
            0,
            &mut pass,
            &mut fail,
        )?;
        case(
            &initfs,
            "times",
            "/bin/times",
            &["/bin/times"],
            &[],
            0,
            &mut pass,
            &mut fail,
        )?;
        case(
            &initfs,
            "tput",
            "/bin/tput",
            &["/bin/tput", "cols"],
            &[],
            0,
            &mut pass,
            &mut fail,
        )?;
        case(
            &initfs,
            "ulimit-default",
            "/bin/ulimit",
            &["/bin/ulimit", "-n"],
            &[],
            0,
            &mut pass,
            &mut fail,
        )?;
        case(
            &initfs,
            "ulimit-set",
            "/bin/ulimit",
            &["/bin/ulimit", "-n", "128"],
            &[],
            0,
            &mut pass,
            &mut fail,
        )?;
        case(
            &initfs,
            "ulimit-get",
            "/bin/ulimit",
            &["/bin/ulimit", "-n"],
            &[],
            0,
            &mut pass,
            &mut fail,
        )?;
        case(
            &initfs,
            "lp",
            "/bin/lp",
            &["/bin/lp"],
            &[],
            1,
            &mut pass,
            &mut fail,
        )?;
        case(
            &initfs,
            "localedef",
            "/bin/localedef",
            &[
                "/bin/localedef",
                "-i",
                "/etc/locale",
                "-f",
                "/etc/charmap",
                "-o",
                "/tmp/catalog",
                "test",
            ],
            &[],
            0,
            &mut pass,
            &mut fail,
        )?;
        catalog_case(&mut pass, &mut fail)?;
        case(
            &initfs,
            "newgrp",
            "/bin/newgrp",
            &["/bin/newgrp", "users"],
            &["SHELL=/bin/sh"],
            0,
            &mut pass,
            &mut fail,
        )?;

        println!("required-utils: 10 cases, {pass} pass, {fail} fail");
        Ok(fail > 0)
    }

    fn catalog_case(pass: &mut usize, fail: &mut usize) -> Result<(), String> {
        let path = "file:/tmp/catalog";
        match fs::read_to_vec(path) {
            Ok(data) if data.starts_with(&[0xff, 0x88, 0xff, 0x89]) => {
                println!(
                    "required-utils: localedef catalog magic=ff88ff89 bytes={} PASS",
                    data.len()
                );
                *pass = pass.saturating_add(1);
            }
            Ok(data) => {
                println!(
                    "required-utils: localedef catalog magic=bad bytes={} FAIL",
                    data.len()
                );
                *fail = fail.saturating_add(1);
            }
            Err(error) => {
                println!(
                    "required-utils: localedef catalog read FAIL {}",
                    error.description()
                );
                *fail = fail.saturating_add(1);
            }
        }
        Ok(())
    }

    fn start_services(initfs: &Initfs<'_>) -> Result<(), String> {
        for (path, scheme) in [
            ("/sbin/initfsd", "initfs:"),
            ("/sbin/ramfsd", "ramfs:"),
            ("/sbin/vfsd", "file:"),
        ] {
            spawn(
                initfs,
                path,
                &SpawnOptions {
                    args: &[],
                    env: &[],
                },
            )
            .map_err(|_| format!("spawn {path}"))?;
            wait_scheme(scheme)?;
        }
        Ok(())
    }

    fn wait_scheme(scheme: &str) -> Result<(), String> {
        for _ in 0..3000 {
            if let Ok(file) = fs::open(scheme, O_RDONLY) {
                let _ = file.close();
                return Ok(());
            }
            let _ = raven_rt::nanosleep(10_000_000);
        }
        Err(format!("scheme {scheme} timeout"))
    }

    #[allow(clippy::too_many_arguments)] // one explicit case line per row keeps the fixture auditable
    fn case(
        initfs: &Initfs<'_>,
        name: &str,
        path: &str,
        args: &[&str],
        env: &[&str],
        expected: i32,
        pass: &mut usize,
        fail: &mut usize,
    ) -> Result<(), String> {
        println!("required-utils: {name} start");
        let child = spawn(initfs, path, &SpawnOptions { args, env })
            .map_err(|_| format!("spawn {name}"))?;
        let mut code = -1i32;
        RtSys
            .proc_wait(child, &mut code)
            .map_err(|_| format!("wait {name}"))?;
        let expected_ok = code == expected;
        let passed = expected_ok;
        println!(
            "required-utils: {name} exit={code} {}",
            if passed { "PASS" } else { "FAIL" }
        );
        if passed {
            *pass = pass.saturating_add(1);
        } else {
            *fail = fail.saturating_add(1);
        }
        Ok(())
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}

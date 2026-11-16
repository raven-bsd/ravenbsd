// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! U3e target orchestrator.
//!
//! Starts the filesystem and console services, copies the account files into a writable `/etc`,
//! runs the root and unprivileged `su`/`passwd` cases, and prints one durable line per assertion.
//! The qtest drives the `Password:` and `Current password:` prompts through the serial console.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use raven_abi::types::Handle;
    use raven_initfs::Initfs;
    use raven_rt::entry;
    use raven_user_secure_fixture::{
        HOSTILE_ENV, install_accounts, lock_exists, marker_exists, read_file, remove_marker,
        spawn_plain, spawn_setuid, start_services, wait,
    };

    entry!(main);

    fn main(_args: raven_rt::Args) -> i32 {
        match run() {
            Ok(()) => {
                raven_rt::println!("secure: ok");
                0
            }
            Err(step) => {
                raven_rt::println!("secure: fail {step}");
                1
            }
        }
    }

    fn run() -> Result<(), &'static str> {
        start_services()?;
        install_accounts()?;
        raven_rt::println!("secure: services ok");

        missing_shell()?;
        ids_and_groups()?;
        environment()?;
        wrong_password()?;
        locked_account()?;
        unknown_account()?;
        passwd_old_mismatch()?;
        Ok(())
    }

    fn su(args: &[&str], env: &[&str]) -> Result<Handle, &'static str> {
        spawn_setuid("/bin/su", args, env).map_err(|_| "su spawn")
    }

    fn runner(mode: &str, target: &str) -> Result<Handle, &'static str> {
        let image = raven_rt::map_initfs().map_err(|_| "initfs map")?;
        let initfs = Initfs::parse(image).map_err(|_| "initfs parse")?;
        let args = ["runner", mode, target];
        spawn_plain(&initfs, "/bin/runner", &args, &[]).map_err(|_| "runner spawn")
    }

    fn expect_ok(child: Handle, step: &'static str) -> Result<(), &'static str> {
        if wait(child).map_err(|_| step)? == 0 {
            Ok(())
        } else {
            Err(step)
        }
    }

    fn missing_shell() -> Result<(), &'static str> {
        raven_rt::println!("secure: case missing-shell");
        remove_marker();
        let child = su(&["su", "newshell"], &[])?;
        expect_ok(child, "missing-shell exit")?;
        if !marker_exists() {
            return Err("missing-shell did not start the fallback");
        }
        raven_rt::println!("secure: missing-shell=ok");
        Ok(())
    }

    fn ids_and_groups() -> Result<(), &'static str> {
        raven_rt::println!("secure: case ids-groups");
        remove_marker();
        let child = su(&["su", "-c", "/bin/idprobe", "alice"], &[])?;
        expect_ok(child, "ids-groups exit")?;
        if !marker_exists() {
            return Err("ids-groups did not start the shell");
        }
        raven_rt::println!("secure: ids-groups=ok");
        Ok(())
    }

    fn environment() -> Result<(), &'static str> {
        raven_rt::println!("secure: case environment");
        remove_marker();
        let child = su(&["su", "-c", "/bin/envprobe", "alice"], HOSTILE_ENV)?;
        expect_ok(child, "environment exit")?;
        if !marker_exists() {
            return Err("environment did not start the shell");
        }
        raven_rt::println!("secure: environment=ok");
        Ok(())
    }

    fn wrong_password() -> Result<(), &'static str> {
        raven_rt::println!("secure: case wrong-password");
        remove_marker();
        let child = runner("su", "alice")?;
        expect_ok(child, "wrong-password runner")?;
        if marker_exists() {
            return Err("wrong-password started a shell");
        }
        raven_rt::println!("secure: wrong-password=denied");
        Ok(())
    }

    fn locked_account() -> Result<(), &'static str> {
        raven_rt::println!("secure: case locked");
        remove_marker();
        let child = runner("su", "bob")?;
        expect_ok(child, "locked runner")?;
        if marker_exists() {
            return Err("locked account started a shell");
        }
        raven_rt::println!("secure: locked=denied");
        Ok(())
    }

    fn unknown_account() -> Result<(), &'static str> {
        raven_rt::println!("secure: case unknown");
        remove_marker();
        let child = runner("su", "ghost")?;
        expect_ok(child, "unknown runner")?;
        if marker_exists() {
            return Err("unknown account started a shell");
        }
        raven_rt::println!("secure: unknown=denied");
        Ok(())
    }

    fn passwd_old_mismatch() -> Result<(), &'static str> {
        raven_rt::println!("secure: case passwd-old");
        let before = read_file("file:/etc/shadow")?;
        let child = runner("passwd", "alice")?;
        expect_ok(child, "passwd-old runner")?;
        let after = read_file("file:/etc/shadow")?;
        if before != after {
            return Err("passwd-old changed the shadow");
        }
        if lock_exists() {
            return Err("passwd-old left the lock");
        }
        raven_rt::println!("secure: passwd-old=unchanged");
        Ok(())
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}

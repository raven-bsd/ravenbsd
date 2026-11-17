// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Drops to the fixture's unprivileged caller, runs a setuid `su`/`passwd`, and proves the
//! caller's own credentials and groups did not change when the tool returned.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::string::ToString;
    use alloc::vec::Vec;

    use raven_rt::entry;
    use raven_rt::proc;
    use raven_user_secure_fixture::{CALLER_GROUP, GID, UID, read_groups, spawn_setuid, wait};

    entry!(main);

    fn main(args: raven_rt::Args) -> i32 {
        match run(&args) {
            Ok(()) => 0,
            Err(step) => {
                raven_rt::println!("secure-runner: fail {step}");
                1
            }
        }
    }

    fn run(args: &raven_rt::Args) -> Result<(), &'static str> {
        let arguments: Vec<&str> = args.iter().skip(1).collect();
        let mode = arguments.first().copied().ok_or("mode")?;
        let target_args = arguments.get(1..).ok_or("target args")?;

        proc::setgroups(&[CALLER_GROUP]).map_err(|_| "setgroups")?;
        proc::setresgid(GID).map_err(|_| "setresgid")?;
        proc::setresuid(UID).map_err(|_| "setresuid")?;
        let caller = proc::credentials().map_err(|_| "caller credentials")?;
        let caller_groups = read_groups().map_err(|_| "caller groups")?;
        if caller.uid != UID
            || caller.euid != UID
            || caller.suid != UID
            || caller.gid != GID
            || caller.egid != GID
            || caller.sgid != GID
            || caller_groups.len() != 1
            || caller_groups.first() != Some(&CALLER_GROUP)
        {
            return Err("caller drop");
        }

        let (path, argv0) = match mode {
            "su" => ("/bin/su", "su"),
            "passwd" => ("/bin/passwd", "passwd"),
            _ => return Err("mode"),
        };
        let mut child_args = Vec::new();
        child_args.push(argv0);
        child_args.extend_from_slice(target_args);
        let child = spawn_setuid(path, &child_args, &[]).map_err(|_| "spawn")?;
        let code = wait(child).map_err(|_| "wait")?;
        raven_rt::println!("secure-runner: mode={mode} exit={code}");

        let after = proc::credentials().map_err(|_| "after credentials")?;
        let after_groups = read_groups().map_err(|_| "after groups")?;
        if after != caller || after_groups != caller_groups {
            raven_rt::println!("secure-runner: fail caller-changed");
            return Err("caller changed");
        }
        raven_rt::println!(
            "secure-runner: caller uid={} euid={} suid={} gid={} egid={} sgid={} groups={}",
            after.uid,
            after.euid,
            after.suid,
            after.gid,
            after.egid,
            after.sgid,
            after_groups
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(",")
        );
        Ok(())
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}

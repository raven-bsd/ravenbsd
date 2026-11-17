// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `newgrp` utility.
//!
//! Resolves the target group from the existing `/etc/group` database, verifies membership
//! without a password prompt, installs the real/effective gid through the libc credential path,
//! and starts `$SHELL` or the user's passwd shell.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::format;
    use alloc::string::{String, ToString};
    use alloc::vec::Vec;

    use newgrp::{Access, Options, find_group, membership, shell_plan};
    use raven_initfs::Initfs;
    use raven_rt::entry;
    use raven_rt::proc;
    use raven_rt::spawn::{RtSys, SpawnOptions};
    use raven_userdb::rt::RtIo;
    use raven_userdb::store::{Paths, load};

    entry!(main);

    fn main(args: raven_rt::Args) -> i32 {
        let options = match newgrp::parse_args(args.iter().skip(1)) {
            Ok(options) => options,
            Err(error) => {
                raven_rt::eprintln!("newgrp: {error:?}");
                raven_rt::eprintln!("usage: newgrp {}", newgrp::USAGE);
                return 2;
            }
        };
        match run(&args, &options) {
            Ok(code) => code,
            Err(message) => {
                raven_rt::eprintln!("newgrp: {message}");
                1
            }
        }
    }

    fn run(args: &raven_rt::Args, options: &Options<'_>) -> Result<i32, String> {
        let credentials = proc::credentials().map_err(|error| error.description().to_string())?;
        let mut io = RtIo;
        let database = load(&mut io, &Paths::system()).map_err(|error| error.to_string())?;
        let user = database
            .user_by_uid(credentials.euid)
            .ok_or_else(|| "no passwd record for the caller".to_string())?;
        let group = match options.group {
            Some(spec) => find_group(database.group(), spec)
                .ok_or_else(|| format!("{spec}: no such group"))?,
            None => database
                .group_by_gid(user.gid)
                .ok_or_else(|| format!("{}: no such group", user.gid))?,
        };
        let root = credentials.euid == 0;
        if membership(group, user, root) == Access::Denied {
            return Err(format!("{}: {}", group.name, newgrp::NO_PASSWORD));
        }
        proc::setgroups(&[group.gid])
            .map_err(|error| format!("setgroups: {}", error.description()))?;
        proc::setresgid(group.gid).map_err(|error| format!("setgid: {}", error.description()))?;
        let installed = proc::credentials().map_err(|error| error.description().to_string())?;
        if installed.gid != group.gid || installed.egid != group.gid {
            return Err("group switch did not take effect".to_string());
        }
        let plan = shell_plan(user, args.env_get("SHELL"), options.login);
        let image = raven_rt::map_initfs().map_err(|error| error.description().to_string())?;
        let initfs = Initfs::parse(image).map_err(|error| error.to_string())?;
        let arguments: Vec<&str> = plan.args.iter().map(String::as_str).collect();
        let environment: Vec<&str> = plan.env.iter().map(String::as_str).collect();
        let child = raven_rt::spawn::spawn(
            &initfs,
            &plan.shell,
            &SpawnOptions {
                args: &arguments,
                env: &environment,
            },
        )
        .map_err(|_| format!("{}: cannot start shell", plan.shell))?;
        let mut code = 1;
        RtSys
            .proc_wait(child, &mut code)
            .map_err(|error| error.description().to_string())?;
        Ok(code)
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}

// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `su` utility.
//!
//! The binary is installed setuid root. It reads the caller's real/effective ids, verifies the
//! target shadow hash when the caller is not root, installs the target's primary and supplementary
//! groups, drops all three uids/gids with `PROC_SETCRED_EX`, and starts the target shell through
//! the native spawn path. The shell runs as the target because the child inherits the parent's
//! credential tuple.

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

    use login::tty::Console;
    use raven_initfs::Initfs;
    use raven_rt::entry;
    use raven_rt::proc;
    use raven_rt::spawn::{RtSys, SpawnOptions};
    use raven_userdb::rt::RtIo;
    use raven_userdb::store::{Paths, load};

    entry!(main);

    fn main(args: raven_rt::Args) -> i32 {
        let options = match su::parse_args(args.iter().skip(1)) {
            Ok(options) => options,
            Err(_) => {
                raven_rt::eprintln!("usage: su {}", su::USAGE);
                return 2;
            }
        };
        match run(&args, &options) {
            Ok(code) => code,
            Err(message) => {
                raven_rt::eprintln!("su: {message}");
                1
            }
        }
    }

    fn run(args: &raven_rt::Args, options: &su::Options<'_>) -> Result<i32, String> {
        let credentials = proc::credentials().map_err(|error| error.description().to_string())?;
        if credentials.uid != 0 && credentials.euid != 0 {
            return Err("must be installed setuid root".to_string());
        }
        let mut io = RtIo;
        let database = load(&mut io, &Paths::system()).map_err(|error| error.to_string())?;
        let user = database
            .user(options.user)
            .ok_or_else(|| format!("{}: no such user", options.user))?;
        if su::needs_password(credentials.uid) {
            let shadow = database
                .shadow_by_name(&user.name)
                .ok_or_else(|| format!("{}: no shadow record", user.name))?;
            let password = prompt_password("Password: ")?;
            if !su::verify_target_password(shadow, &password) {
                return Err("incorrect password".to_string());
            }
        }
        let groups = database
            .groups_for_user(user)
            .map_err(|error| error.to_string())?;
        let supplementary = su::supplementary_groups(&groups, user.gid);
        proc::setgroups(&supplementary)
            .map_err(|error| format!("setgroups: {}", error.description()))?;
        proc::setresgid(user.gid).map_err(|error| format!("setgid: {}", error.description()))?;
        proc::setresuid(user.uid).map_err(|error| format!("setuid: {}", error.description()))?;
        let installed = proc::credentials().map_err(|error| error.description().to_string())?;
        if installed.euid != user.uid || installed.egid != user.gid {
            return Err("credential switch did not take effect".to_string());
        }
        let image = raven_rt::map_initfs().map_err(|error| error.description().to_string())?;
        let initfs = Initfs::parse(image).map_err(|error| error.to_string())?;
        let shell = su::shell_path(user);
        let arguments = su::shell_arguments(options);
        let mut environment: Vec<String> = su::environment(user, options.login);
        if let Some(term) = args.env_get("TERM") {
            environment.push(format!("TERM={term}"));
        }
        let environment: Vec<&str> = environment.iter().map(String::as_str).collect();
        let spawn_options = SpawnOptions {
            args: &arguments,
            env: &environment,
        };
        let child = raven_rt::spawn::spawn(&initfs, &shell, &spawn_options)
            .map_err(|_| format!("{shell}: cannot start shell"))?;
        let mut code = 1;
        RtSys
            .proc_wait(child, &mut code)
            .map_err(|error| error.description().to_string())?;
        Ok(code)
    }

    fn prompt_password(prompt: &str) -> Result<String, String> {
        let console = Console::open().map_err(|error| error.description().to_string())?;
        console
            .write(prompt.as_bytes())
            .map_err(|error| error.description().to_string())?;
        console.set_echo(false);
        let result = console.read_line();
        console.set_echo(true);
        let _ = console.write(b"\n");
        result.map_err(|error| error.description().to_string())
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}

// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `su` utility.
//!
//! U2 implements the root-to-root path and reports the U3 gap instead of pretending to switch to a
//! non-root account. `su` needs a setuid binary and the kernel `PROC_SETCRED_EX` path before the
//! other identities are real; neither exists in 0.2.0 yet.

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

    use raven_initfs::Initfs;
    use raven_rt::entry;
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
        let mut io = RtIo;
        let database = load(&mut io, &Paths::system()).map_err(|error| error.to_string())?;
        let user = database
            .user(options.user)
            .ok_or_else(|| format!("{}: no such user", options.user))?;
        if user.uid != 0 {
            return Err(format!(
                "switching to {} needs the U3 kernel credentials; 0.2.0 stays root-only",
                user.name
            ));
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
}

#[cfg(not(target_os = "none"))]
fn main() {}

// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `getty`: print the console banner, retry the `login:` prompt, then start the session shell.
//!
//! The kernel has no `exec`, so the session is a small process chain: `getty` owns the `initfs`
//! grant from its service manifest and uses it to spawn `/bin/login` and then the shell named by
//! login policy. `login` verifies the password and exits 0; a nonzero status re-prompts instead of
//! ending the session. No process is setuid and no password is kept in this binary.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::string::String;

    use login::tty::Console;
    use login::{Profile, parse_profile, parse_root_passwd};
    use raven_getty::{Session, run_session};
    use raven_initfs::Initfs;
    use raven_rt::entry;
    use raven_rt::fs;
    use raven_rt::spawn::{RtSys, SpawnOptions};

    entry!(main);

    struct ConsoleSession {
        console: Console,
        initfs: Initfs<'static>,
    }

    impl Session for ConsoleSession {
        fn write(&mut self, bytes: &[u8]) -> Result<(), ()> {
            self.console.write(bytes).map_err(|_| ())
        }

        fn read_line(&mut self) -> Result<String, ()> {
            self.console.read_line().map_err(|_| ())
        }

        fn run_login(&mut self, username: &str) -> Result<i32, ()> {
            let login_args = ["login", username];
            let login_env = [alloc::format!("USER={username}")];
            let login_env: alloc::vec::Vec<&str> = login_env.iter().map(String::as_str).collect();
            let options = SpawnOptions {
                args: &login_args,
                env: &login_env,
            };
            spawn_wait(&self.initfs, "/bin/login", &options)
        }

        fn run_shell(&mut self, shell: &str, args: &[&str], env: &[&str]) -> Result<i32, ()> {
            let options = SpawnOptions { args, env };
            spawn_wait(&self.initfs, shell, &options)
        }
    }

    fn main(_args: raven_rt::Args) -> i32 {
        let profile = read_profile();
        let shell = read_shell().unwrap_or_else(|| profile.shell.clone());
        let Ok(console) = Console::open() else {
            return 1;
        };
        let Some(image) = raven_rt::map_initfs().ok() else {
            return 1;
        };
        let Ok(initfs) = Initfs::parse(image) else {
            return 1;
        };
        let mut session = ConsoleSession { console, initfs };
        run_session(&mut session, &profile, &shell)
    }

    fn read_profile() -> Profile {
        fs::read_to_vec("file:/etc/profile")
            .ok()
            .and_then(|bytes| core::str::from_utf8(&bytes).ok().map(parse_profile))
            .unwrap_or_default()
    }

    fn read_shell() -> Option<String> {
        let passwd = fs::read_to_vec("file:/etc/passwd").ok()?;
        let text = core::str::from_utf8(&passwd).ok()?;
        parse_root_passwd(text).map(|entry| entry.shell)
    }

    fn spawn_wait(initfs: &Initfs<'_>, path: &str, options: &SpawnOptions<'_>) -> Result<i32, ()> {
        let process = raven_rt::spawn::spawn(initfs, path, options).map_err(|_| ())?;
        let mut code = 0i32;
        let waited = RtSys.proc_wait(process, &mut code);
        let _ = raven_rt::handle_close(process);
        waited.map(|()| code).map_err(|_| ())
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}

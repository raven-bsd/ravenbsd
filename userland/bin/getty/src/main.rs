// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `getty`: print the console banner and `login:` prompt, run `login`, then start the session
//! shell named by `/etc/passwd`.
//!
//! The kernel has no `exec`, so the session is a small process chain: `getty` owns the `initfs`
//! grant from its service manifest and uses it to spawn `/bin/login` and then `/bin/rsh` (or the
//! shell field from `/etc/passwd`). `login` verifies the password and exits 0; `getty` starts the
//! shell only on that status. No process is setuid and no password is kept in this binary.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::format;
    use alloc::string::String;
    use alloc::vec;
    use alloc::vec::Vec;

    use login::tty::Console;
    use login::{Profile, is_root_user, normalize_username, parse_profile, parse_root_passwd};
    use raven_initfs::Initfs;
    use raven_rt::entry;
    use raven_rt::fs;
    use raven_rt::spawn::{RtSys, SpawnOptions};

    entry!(main);

    /// Printed before the login prompt.
    const BANNER: &str = "RavenBSD 0.1 (x86_64) (tty:console)\n";

    fn main(_args: raven_rt::Args) -> i32 {
        let profile = read_profile();
        let Ok(console) = Console::open() else {
            return 1;
        };
        if console.write(BANNER.as_bytes()).is_err() || console.write(b"login: ").is_err() {
            return 1;
        }
        let username = match console.read_line() {
            Ok(line) => line,
            Err(_) => return 1,
        };
        drop(console);
        let Some(username) = normalize_username(&username) else {
            return 1;
        };
        if !is_root_user(username) {
            return 1;
        }
        let Some(image) = raven_rt::map_initfs().ok() else {
            return 1;
        };
        let Ok(initfs) = Initfs::parse(image) else {
            return 1;
        };
        let login_args = ["login", username];
        let login_env = ["USER=root"];
        let login_options = SpawnOptions {
            args: &login_args,
            env: &login_env,
        };
        let status = spawn_wait(&initfs, "/bin/login", &login_options);
        if status != 0 {
            return status;
        }
        let shell = read_shell().unwrap_or_else(|| profile.shell.clone());
        let env_strings = profile_env(&profile, &shell);
        let env: Vec<&str> = env_strings.iter().map(String::as_str).collect();
        // The console scheme has no POSIX tty, so brush cannot detect an interactive stdin;
        // the login shell is interactive by construction.
        let shell_args: &[&str] = if shell.rsplit('/').next() == Some("brush") {
            // argv[0] is the shell name; `-i` forces the interactive backend and `-l` loads
            // /etc/profile on the console.
            &["brush", "-i", "-l"]
        } else {
            &[]
        };
        let shell_options = SpawnOptions {
            args: shell_args,
            env: &env,
        };
        spawn_wait(&initfs, &shell, &shell_options)
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

    fn profile_env(profile: &Profile, shell: &str) -> Vec<String> {
        vec![
            format!("PATH={}", profile.path),
            format!("HOME={}", profile.home),
            format!("TERM={}", profile.term),
            format!("USER={}", profile.user),
            format!("SHELL={shell}"),
            String::from("PWD=/"),
        ]
    }

    fn spawn_wait(initfs: &Initfs<'_>, path: &str, options: &SpawnOptions<'_>) -> i32 {
        match raven_rt::spawn::spawn(initfs, path, options) {
            Ok(process) => {
                let mut code = 0i32;
                let waited = RtSys.proc_wait(process, &mut code);
                let _ = raven_rt::handle_close(process);
                if waited.is_ok() { code } else { 1 }
            }
            Err(_) => 1,
        }
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}

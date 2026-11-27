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

    use login::tty::{Console, PtyPair, forward_console_to_pty};
    use login::{
        Autologin, PasswdEntry, Profile, parse_autologin, parse_passwd, parse_profile,
        parse_root_passwd,
    };
    use raven_abi::types::Handle;
    use raven_getty::{Account, Session, SessionError, run_session};
    use raven_initfs::Initfs;
    use raven_rt::entry;
    use raven_rt::fs;
    use raven_rt::spawn::{
        RtSys, SpawnAttributes, SpawnError, SpawnOptions, spawn_image_with_fdmap,
        spawn_with_attributes,
    };

    /// Stack for the console-to-pty forwarder thread.
    const RELAY_STACK: usize = 16 * 1024;

    entry!(main);

    struct ConsoleSession {
        console: Console,
        initfs: Initfs<'static>,
        accounts: alloc::vec::Vec<PasswdEntry>,
    }

    impl Session for ConsoleSession {
        fn write(&mut self, bytes: &[u8]) -> Result<(), SessionError> {
            self.console.write(bytes).map_err(|_| SessionError)
        }

        fn read_line(&mut self) -> Result<String, SessionError> {
            self.console.read_line().map_err(|_| SessionError)
        }

        fn run_login(&mut self, username: &str) -> Result<i32, SessionError> {
            let login_args = ["login", username];
            let login_env = [alloc::format!("USER={username}")];
            let login_env: alloc::vec::Vec<&str> = login_env.iter().map(String::as_str).collect();
            let options = SpawnOptions {
                args: &login_args,
                env: &login_env,
            };
            spawn_wait(&self.initfs, "/bin/login", &options)
        }

        fn run_shell(
            &mut self,
            shell: &str,
            args: &[&str],
            env: &[&str],
        ) -> Result<i32, SessionError> {
            let options = SpawnOptions { args, env };
            // The console scheme reports an idle read as EAGAIN; give the shell a pty slave
            // whose read waits for the writer and forward console input into the master.
            if let Ok(pair) = PtyPair::open()
                && let Ok(_relay) = raven_rt::thread::spawn(
                    forward_console_to_pty,
                    pair.master().0 as usize,
                    RELAY_STACK,
                )
            {
                let process = spawn_shell(&self.initfs, shell, &options, pair.slave())
                    .map_err(|_| SessionError)?;
                let mut code = 0i32;
                let waited = RtSys.proc_wait(process, &mut code);
                let _ = raven_rt::handle_close(process);
                // `pair` and `_relay` stay alive until the shell has exited.
                return waited.map(|()| code).map_err(|_| SessionError);
            }
            // A profile without the pty service keeps the pre-S211 direct console session
            // instead of failing to start the shell; say so on the console.
            raven_rt::eprintln!("getty: pty unavailable; console stdin is the session");
            spawn_wait(&self.initfs, shell, &options)
        }

        fn account(&mut self, username: &str) -> Option<Account> {
            self.accounts
                .iter()
                .find(|entry| entry.name == username)
                .map(|entry| Account {
                    home: entry.home.clone(),
                    shell: entry.shell.clone(),
                })
        }
    }

    fn main(_args: raven_rt::Args) -> i32 {
        let profile = read_profile();
        let passwd = read_passwd();
        let mut accounts = parse_passwd(&passwd);
        for entry in &mut accounts {
            if entry.name == login::ROOT_USER {
                entry.shell = alloc::string::String::from(login::DEFAULT_SHELL);
            }
        }
        let shell = root_shell(&passwd).unwrap_or_else(|| profile.shell.clone());
        let autologin = read_autologin();
        let Ok(console) = Console::open() else {
            return 1;
        };
        let Some(image) = raven_rt::map_initfs().ok() else {
            return 1;
        };
        let Ok(initfs) = Initfs::parse(image) else {
            return 1;
        };
        let mut session = ConsoleSession {
            console,
            initfs,
            accounts,
        };
        run_session(&mut session, &profile, &shell, autologin.as_deref())
    }

    fn read_passwd() -> String {
        fs::read_to_vec("file:/etc/passwd")
            .ok()
            .and_then(|bytes| {
                core::str::from_utf8(&bytes)
                    .ok()
                    .map(alloc::string::String::from)
            })
            .unwrap_or_default()
    }

    fn root_shell(passwd: &str) -> Option<alloc::string::String> {
        parse_root_passwd(passwd).map(|entry| entry.shell)
    }

    fn read_autologin() -> Option<alloc::string::String> {
        let bytes = fs::read_to_vec("file:/etc/raven/autologin.toml").ok()?;
        let text = core::str::from_utf8(&bytes).ok()?;
        let Autologin { enabled, user } = parse_autologin(text);
        (enabled && !user.is_empty()).then_some(user)
    }

    fn read_profile() -> Profile {
        fs::read_to_vec("file:/etc/profile")
            .ok()
            .and_then(|bytes| core::str::from_utf8(&bytes).ok().map(parse_profile))
            .unwrap_or_default()
    }

    /// Spawn the session shell with `stdin` installed as child fd 0.
    ///
    /// The initfs carries the shell in a boot image; after a root pivot it lives on the
    /// mounted root, so the K176 file-server fallback reads the image and keeps the fd map.
    fn spawn_shell(
        initfs: &Initfs<'_>,
        path: &str,
        options: &SpawnOptions<'_>,
        stdin: Handle,
    ) -> Result<Handle, SpawnError> {
        let fdmap = [(0i32, stdin.0)];
        let zero = [0u8; 128];
        let attributes = SpawnAttributes {
            fdmap: &fdmap,
            sigmask: &zero,
            sigdefault: &zero,
            ignored: &zero,
        };
        match spawn_with_attributes(initfs, path, options, &attributes) {
            Ok(child) => Ok(child),
            Err(SpawnError::NotFound) => {
                let image = raven_rt::spawn::read_file_image(path)?;
                spawn_image_with_fdmap(&image, path, options, &fdmap, &mut RtSys)
            }
            Err(error) => Err(error),
        }
    }

    fn spawn_wait(
        initfs: &Initfs<'_>,
        path: &str,
        options: &SpawnOptions<'_>,
    ) -> Result<i32, SessionError> {
        let process = raven_rt::spawn::spawn(initfs, path, options).map_err(|_| SessionError)?;
        let mut code = 0i32;
        let waited = RtSys.proc_wait(process, &mut code);
        let _ = raven_rt::handle_close(process);
        waited.map(|()| code).map_err(|_| SessionError)
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}

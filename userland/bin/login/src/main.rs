// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `login`: read the password, verify the root shadow record and print the motd.
//!
//! The session shell is started by `getty` after this process exits 0, because the kernel has no
//! `exec` and only `getty` holds the initfs grant needed by `spawn`. `login` itself never runs as
//! a setuid binary and never writes the shadow file.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use login::tty::Console;
    use login::{LOGIN_OK, login_status, normalize_username};
    use raven_rt::entry;
    use raven_rt::fs;

    entry!(main);

    fn main(args: raven_rt::Args) -> i32 {
        let username = args.get(1).and_then(normalize_username).unwrap_or_default();
        let Ok(console) = Console::open() else {
            return login::LOGIN_RETRY;
        };
        if console.write(b"Password: ").is_err() {
            return login::LOGIN_RETRY;
        }
        console.set_echo(false);
        let password = match console.read_line() {
            Ok(line) => line,
            Err(_) => {
                console.set_echo(true);
                return login::LOGIN_RETRY;
            }
        };
        console.set_echo(true);
        let _ = console.write(b"\n");
        // getty starts the session shell after this process exits; discard any console bytes
        // buffered across the handoff so the first shell line is clean.
        console.flush();
        let shadow = fs::read_to_vec("file:/etc/shadow").unwrap_or_default();
        let shadow_text = core::str::from_utf8(&shadow).unwrap_or_default();
        // Unknown accounts take the same path as a wrong password: one generic message, no
        // shell, and the retry status getty loops on.
        if login_status(shadow_text, username, password.trim_end()) != LOGIN_OK {
            let _ = console.write(b"Login incorrect\n");
            return login::LOGIN_RETRY;
        }
        if let Ok(motd) = fs::read_to_vec("file:/etc/motd") {
            let _ = console.write(&motd);
        }
        LOGIN_OK
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}

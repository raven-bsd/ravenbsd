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
    use login::{is_root_user, normalize_username, verify_root_password};
    use raven_rt::entry;
    use raven_rt::fs;

    entry!(main);

    fn main(args: raven_rt::Args) -> i32 {
        let username = args.get(1).and_then(normalize_username).unwrap_or_default();
        if !is_root_user(username) {
            return 1;
        }
        let Ok(console) = Console::open() else {
            return 1;
        };
        if console.write(b"Password: ").is_err() {
            return 1;
        }
        console.set_echo(false);
        let password = match console.read_line() {
            Ok(line) => line,
            Err(_) => {
                console.set_echo(true);
                return 1;
            }
        };
        console.set_echo(true);
        let _ = console.write(b"\n");
        let shadow = fs::read_to_vec("file:/etc/shadow").unwrap_or_default();
        let shadow_text = core::str::from_utf8(&shadow).unwrap_or_default();
        if !verify_root_password(shadow_text, password.trim_end()) {
            let _ = console.write(b"Login incorrect\n");
            return 1;
        }
        if let Ok(motd) = fs::read_to_vec("file:/etc/motd") {
            let _ = console.write(&motd);
        }
        0
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}

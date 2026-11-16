// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `whoami` utility.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
mod program {
    use raven_rt::entry;
    use raven_rt::proc;
    use raven_userdb::rt::RtIo;
    use raven_userdb::store::{Paths, load_lookup};

    entry!(main);

    fn main(args: raven_rt::Args) -> i32 {
        if whoami::parse_args(args.iter().skip(1)).is_err() {
            raven_rt::eprintln!("usage: whoami");
            return 2;
        }
        let mut io = RtIo;
        let database = match load_lookup(&mut io, &Paths::system()) {
            Ok(database) => database,
            Err(error) => {
                raven_rt::eprintln!("whoami: {error}");
                return 1;
            }
        };
        let uid = match proc::credentials() {
            Ok(credentials) => credentials.euid,
            Err(error) => {
                raven_rt::eprintln!("whoami: {}", error.description());
                return 1;
            }
        };
        raven_rt::println!("{}", whoami::name_for_uid(&database, uid));
        if whoami::known_uid(&database, uid) {
            0
        } else {
            1
        }
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}

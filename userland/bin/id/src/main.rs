// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `id` utility.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
mod program {
    use raven_rt::entry;
    use raven_userdb::rt::RtIo;
    use raven_userdb::store::{Paths, load_lookup};

    entry!(main);

    fn main(args: raven_rt::Args) -> i32 {
        let options = match id::parse_args(args.iter().skip(1)) {
            Ok(options) => options,
            Err(_) => {
                raven_rt::eprintln!("usage: id {}", id::USAGE);
                return 2;
            }
        };
        let mut io = RtIo;
        let database = match load_lookup(&mut io, &Paths::system()) {
            Ok(database) => database,
            Err(error) => {
                raven_rt::eprintln!("id: {error}");
                return 1;
            }
        };
        let credential = match id::credential_for(&database, options.user) {
            Ok(credential) => credential,
            Err(raven_userdb::DbError::UnknownUser) => {
                raven_rt::eprintln!("id: {}: no such user", options.user.unwrap_or(""));
                return 1;
            }
            Err(error) => {
                raven_rt::eprintln!("id: {error}");
                return 1;
            }
        };
        raven_rt::println!("{}", id::format(&database, &credential, &options));
        0
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}

// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `id` utility.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::vec::Vec;

    use raven_rt::entry;
    use raven_rt::proc;
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
        let credential = match options.user {
            Some(_) => id::credential_for(&database, options.user),
            None => current_credential(),
        };
        let credential = match credential {
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

    fn current_credential() -> Result<raven_userdb::Credential, raven_userdb::DbError> {
        let credentials = proc::credentials()
            .map_err(|error| raven_userdb::DbError::Io(raven_abi::errno::Errno(error.0)))?;
        let mut groups = [0u32; proc::MAX_GROUPS];
        let count = proc::groups(&mut groups)
            .map_err(|error| raven_userdb::DbError::Io(raven_abi::errno::Errno(error.0)))?;
        let groups: Vec<u32> = groups
            .get(..count)
            .ok_or(raven_userdb::DbError::TooManyGroups)?
            .to_vec();
        Ok(id::process_credential(&id::ProcessIds {
            uid: credentials.uid,
            euid: credentials.euid,
            gid: credentials.gid,
            egid: credentials.egid,
            groups,
        }))
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}

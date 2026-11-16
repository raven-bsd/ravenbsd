// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `useradd` utility.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
mod program {
    use raven_rt::entry;
    use raven_userdb::AddUser;
    use raven_userdb::rt::{RtIo, create_home};
    use raven_userdb::store::{Paths, Transaction};

    entry!(main);

    fn main(args: raven_rt::Args) -> i32 {
        let options = match useradd::parse_args(args.iter().skip(1)) {
            Ok(options) => options,
            Err(_) => {
                raven_rt::eprintln!("usage: useradd {}", useradd::USAGE);
                return 2;
            }
        };
        let request = AddUser {
            name: options.name,
            uid: options.uid,
            group: options.group,
            groups: &options.groups,
            gecos: options.gecos.unwrap_or(""),
            home: options.home,
            shell: options.shell,
            system: false,
        };
        let mut transaction = match Transaction::begin(RtIo, Paths::system()) {
            Ok(transaction) => transaction,
            Err(error) => {
                raven_rt::eprintln!("useradd: {error}");
                return 1;
            }
        };
        let outcome = match transaction.database_mut().add_user(&request) {
            Ok(outcome) => outcome,
            Err(error) => {
                raven_rt::eprintln!("useradd: {error}");
                return 1;
            }
        };
        if let Err(error) = transaction.commit() {
            raven_rt::eprintln!("useradd: {error}");
            return 1;
        }
        if options.create_home
            && let Err(error) = create_home(&outcome.user.home)
        {
            raven_rt::eprintln!("useradd: {}: {error}", outcome.user.home);
            return 1;
        }
        0
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}

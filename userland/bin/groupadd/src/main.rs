// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `groupadd` utility.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
mod program {
    use raven_rt::entry;
    use raven_userdb::AddGroup;
    use raven_userdb::rt::RtIo;
    use raven_userdb::store::{Paths, Transaction};

    entry!(main);

    fn main(args: raven_rt::Args) -> i32 {
        let options = match groupadd::parse_args(args.iter().skip(1)) {
            Ok(options) => options,
            Err(_) => {
                raven_rt::eprintln!("usage: groupadd {}", groupadd::USAGE);
                return 2;
            }
        };
        let request = AddGroup {
            name: options.name,
            gid: options.gid,
            system: options.system,
        };
        let mut transaction = match Transaction::begin(RtIo, Paths::system()) {
            Ok(transaction) => transaction,
            Err(error) => {
                raven_rt::eprintln!("groupadd: {error}");
                return 1;
            }
        };
        if let Err(error) = transaction.database_mut().add_group(&request) {
            raven_rt::eprintln!("groupadd: {error}");
            return 1;
        }
        if let Err(error) = transaction.commit() {
            raven_rt::eprintln!("groupadd: {error}");
            return 1;
        }
        0
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}

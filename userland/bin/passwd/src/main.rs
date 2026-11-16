// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `passwd` utility.
//!
//! U2 ships the root path only. A non-root caller needs a setuid binary and the kernel credential
//! switch from U3; the tool does not fake either. The password never goes through argv, the
//! environment or a temporary file.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::string::ToString;

    use raven_abi::types::clock;
    use raven_rt::entry;
    use raven_userdb::rt::{RtIo, random_bytes, read_line};
    use raven_userdb::store::{Paths, Transaction};

    entry!(main);

    fn main(args: raven_rt::Args) -> i32 {
        let options = match passwd::parse_args(args.iter().skip(1)) {
            Ok(options) => options,
            Err(_) => {
                raven_rt::eprintln!("usage: passwd {}", passwd::USAGE);
                return 2;
            }
        };
        // The kernel is root-only in 0.2.0. U3 supplies the real effective uid and the setuid
        // execution path; until then every caller is root and the root-only path is correct.
        let caller_uid = 0u32;
        let mut transaction = match Transaction::begin(RtIo, Paths::system()) {
            Ok(transaction) => transaction,
            Err(error) => {
                raven_rt::eprintln!("passwd: {error}");
                return 1;
            }
        };
        let name = match options.user {
            Some(name) => name.to_string(),
            None => match transaction.database().user_by_uid(caller_uid) {
                Some(user) => user.name.clone(),
                None => {
                    raven_rt::eprintln!("passwd: no account for uid {caller_uid}");
                    return 1;
                }
            },
        };
        let target = match transaction.database().user(&name) {
            Some(user) => user.clone(),
            None => {
                raven_rt::eprintln!("passwd: {}: no such user", name);
                return 1;
            }
        };
        match options.action {
            passwd::Action::Lock => {
                if let Err(error) = transaction.database_mut().lock_password(&name) {
                    raven_rt::eprintln!("passwd: {name}: {error}");
                    return 1;
                }
            }
            passwd::Action::Unlock => {
                if let Err(error) = transaction.database_mut().unlock_password(&name) {
                    raven_rt::eprintln!("passwd: {name}: {error}");
                    return 1;
                }
            }
            passwd::Action::Change => {
                let password = match read_line("New password: ") {
                    Ok(password) => password,
                    Err(error) => {
                        raven_rt::eprintln!("passwd: {error}");
                        return 1;
                    }
                };
                let confirmation = match read_line("Retype new password: ") {
                    Ok(password) => password,
                    Err(error) => {
                        raven_rt::eprintln!("passwd: {error}");
                        return 1;
                    }
                };
                if password != confirmation {
                    raven_rt::eprintln!("passwd: passwords do not match");
                    return 1;
                }
                if let Err(error) = passwd::validate_password(&password, caller_uid, target.uid) {
                    raven_rt::eprintln!("passwd: {}", error.message());
                    return 1;
                }
                let salt = match random_bytes::<16>("rand:") {
                    Ok(salt) => salt,
                    Err(error) => {
                        raven_rt::eprintln!("passwd: rand: {error}");
                        return 1;
                    }
                };
                let hash = match raven_auth::hash_password(&password, &salt) {
                    Ok(hash) => hash,
                    Err(error) => {
                        raven_rt::eprintln!("passwd: {error}");
                        return 1;
                    }
                };
                let today = raven_rt::clock_get(clock::REALTIME)
                    .map(|time| passwd::days_since_epoch(time.tv_sec))
                    .unwrap_or(0);
                if let Err(error) = transaction.database_mut().set_password(&name, &hash, today) {
                    raven_rt::eprintln!("passwd: {name}: {error}");
                    return 1;
                }
            }
        }
        if let Err(error) = transaction.commit() {
            raven_rt::eprintln!("passwd: {error}");
            return 1;
        }
        0
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}

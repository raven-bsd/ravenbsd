// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `passwd` utility.
//!
//! The binary is installed setuid root. Root may change any account without an old password. A
//! non-root caller may change only its own account after proving the old hash; the effective root
//! id from the setuid install is used only for the database transaction. The password never goes
//! through argv, the environment or a temporary file, and the console echo bit is restored before
//! the tool exits.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::format;
    use alloc::string::{String, ToString};

    use login::tty::Console;
    use raven_abi::types::clock;
    use raven_rt::entry;
    use raven_rt::proc;
    use raven_userdb::rt::{RtIo, random_bytes};
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
        match run(&options) {
            Ok(()) => 0,
            Err(message) => {
                raven_rt::eprintln!("passwd: {message}");
                1
            }
        }
    }

    fn run(options: &passwd::Options<'_>) -> Result<(), String> {
        let credentials = proc::credentials().map_err(|error| error.description().to_string())?;
        if credentials.euid != 0 {
            return Err("must be installed setuid root".to_string());
        }
        let mut transaction =
            Transaction::begin(RtIo, Paths::system()).map_err(|error| error.to_string())?;
        let name = match options.user {
            Some(name) => name.to_string(),
            None => match transaction.database().user_by_uid(credentials.uid) {
                Some(user) => user.name.clone(),
                None => return Err(format!("no account for uid {}", credentials.uid)),
            },
        };
        let target = transaction
            .database()
            .user(&name)
            .cloned()
            .ok_or_else(|| format!("{name}: no such user"))?;
        if !passwd::target_allowed(credentials.uid, target.uid) {
            return Err(format!("{name}: permission denied"));
        }
        match options.action {
            passwd::Action::Lock => {
                if credentials.uid != 0 {
                    return Err("only root may lock a password".to_string());
                }
                transaction
                    .database_mut()
                    .lock_password(&name)
                    .map_err(|error| format!("{name}: {error}"))?;
            }
            passwd::Action::Unlock => {
                if credentials.uid != 0 {
                    return Err("only root may unlock a password".to_string());
                }
                transaction
                    .database_mut()
                    .unlock_password(&name)
                    .map_err(|error| format!("{name}: {error}"))?;
            }
            passwd::Action::Change => {
                let console = Console::open().map_err(|error| error.description().to_string())?;
                if passwd::requires_old_password(credentials.uid) {
                    let shadow = transaction
                        .database()
                        .shadow_by_name(&name)
                        .ok_or_else(|| format!("{name}: no shadow record"))?;
                    let old = prompt(&console, "Current password: ")?;
                    if !passwd::verify_old_password(&shadow.hash, &old) {
                        return Err("incorrect password".to_string());
                    }
                }
                let password = prompt(&console, "New password: ")?;
                let confirmation = prompt(&console, "Retype new password: ")?;
                if password != confirmation {
                    return Err("passwords do not match".to_string());
                }
                passwd::validate_password(&password, credentials.uid, target.uid)
                    .map_err(|error| error.message().to_string())?;
                let salt = random_bytes::<16>("rand:").map_err(|error| format!("rand: {error}"))?;
                let hash = raven_auth::hash_password(&password, &salt)
                    .map_err(|error| error.to_string())?;
                let today = raven_rt::clock_get(clock::REALTIME)
                    .map(|time| passwd::days_since_epoch(time.tv_sec))
                    .unwrap_or(0);
                transaction
                    .database_mut()
                    .set_password(&name, &hash, today)
                    .map_err(|error| format!("{name}: {error}"))?;
            }
        }
        transaction.commit().map_err(|error| error.to_string())
    }

    fn prompt(console: &Console, prompt: &str) -> Result<String, String> {
        console
            .write(prompt.as_bytes())
            .map_err(|error| error.description().to_string())?;
        console.set_echo(false);
        let result = console.read_line();
        console.set_echo(true);
        let _ = console.write(b"\n");
        result.map_err(|error| error.description().to_string())
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}

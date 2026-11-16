// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `adduser` utility.
//!
//! The interactive front-end collects the same fields as `useradd`, then uses the same
//! `raven-userdb` transaction and `raven-auth` hash. It does not have a second file format.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::format;
    use alloc::string::{String, ToString};

    use raven_abi::types::clock;
    use raven_rt::entry;
    use raven_userdb::AddUser;
    use raven_userdb::rt::{RtIo, create_home, random_bytes, read_line};
    use raven_userdb::store::{Paths, Transaction};

    entry!(main);

    fn main(args: raven_rt::Args) -> i32 {
        match run(args) {
            Ok(()) => 0,
            Err(message) => {
                raven_rt::eprintln!("adduser: {message}");
                1
            }
        }
    }

    fn run(args: raven_rt::Args) -> Result<(), String> {
        let provided = adduser::parse_args(args.iter().skip(1))
            .map_err(|_| format!("usage: adduser {}", adduser::USAGE))?;
        let name = match provided {
            Some(name) => name.to_string(),
            None => ask("Login name: ")?,
        };
        if name.is_empty() {
            return Err("empty login name".to_string());
        }
        let gecos = ask("Full name: ")?;
        let default_home = adduser::default_home(&name);
        let home = ask(&format!("Home directory [{default_home}]: "))?;
        let home = if home.is_empty() { default_home } else { home };
        let shell = ask(&format!("Shell [{}]: ", adduser::DEFAULT_SHELL))?;
        let shell = if shell.is_empty() {
            adduser::DEFAULT_SHELL.to_string()
        } else {
            shell
        };
        let group = ask(&format!("Primary group [{name}]: "))?;
        let group = if group.is_empty() { None } else { Some(group) };
        let password = ask("Password: ")?;
        let confirmation = ask("Retype password: ")?;
        if password != confirmation {
            return Err("passwords do not match".to_string());
        }
        let mut transaction =
            Transaction::begin(RtIo, Paths::system()).map_err(|error| error.to_string())?;
        let request = AddUser {
            name: &name,
            uid: None,
            group: group.as_deref(),
            groups: &[],
            gecos: &gecos,
            home: Some(&home),
            shell: Some(&shell),
            system: false,
        };
        let outcome = transaction
            .database_mut()
            .add_user(&request)
            .map_err(|error| error.to_string())?;
        passwd::validate_password(&password, 0, outcome.user.uid)
            .map_err(|error| error.message().to_string())?;
        let salt = random_bytes::<16>("rand:").map_err(|error| error.to_string())?;
        let hash =
            raven_auth::hash_password(&password, &salt).map_err(|error| error.to_string())?;
        let today = raven_rt::clock_get(clock::REALTIME)
            .map(|time| passwd::days_since_epoch(time.tv_sec))
            .unwrap_or(0);
        transaction
            .database_mut()
            .set_password(&name, &hash, today)
            .map_err(|error| error.to_string())?;
        transaction.commit().map_err(|error| error.to_string())?;
        create_home(&outcome.user.home).map_err(|error| error.to_string())?;
        raven_rt::println!(
            "adduser: created {} (uid {})",
            outcome.user.name,
            outcome.user.uid
        );
        Ok(())
    }

    fn ask(prompt: &str) -> Result<String, String> {
        read_line(prompt).map_err(|error| error.to_string())
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}

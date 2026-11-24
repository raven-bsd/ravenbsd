// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-testable getty session policy: one banner, a retried login prompt and the shell handoff.
//!
//! The target binary owns the console handle and the `spawn` path; this module owns the loop that
//! decides what to print and when to start the session shell. A failed `login` never ends the
//! session: getty prints `Login incorrect` (or a clear spawn error) and prompts again.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing,
        clippy::arithmetic_side_effects
    )
)]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use login::{Profile, normalize_username};

/// Printed once before the first prompt.
///
/// The boot orchestrator now prints the styled title, rules, timed table and `Welcome` line;
/// getty only owns the `login: ` prompt, so its own banner is empty.
pub const BANNER: &str = "";
/// The username prompt; printed again after every failed attempt.
pub const PROMPT: &[u8] = b"login: ";
/// A rejected user name that getty answers without spawning `login`.
pub const INCORRECT: &[u8] = b"Login incorrect\n";
/// `login` could not be started; the prompt is retried instead of ending the session.
pub const LOGIN_ERROR: &[u8] = b"login: unable to start login\n";
/// The session shell could not be started.
pub const SHELL_ERROR: &[u8] = b"login: unable to start the session shell\n";

/// A console or spawn operation failed; getty only needs success/failure, not a reason.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SessionError;

impl core::fmt::Display for SessionError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("session operation failed")
    }
}

/// One account's console session fields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Account {
    /// Home directory.
    pub home: String,
    /// Login shell.
    pub shell: String,
}

/// Console and spawn operations the loop needs.
pub trait Session {
    /// Write bytes to the console.
    fn write(&mut self, bytes: &[u8]) -> Result<(), SessionError>;
    /// Read one canonical line without its terminator.
    fn read_line(&mut self) -> Result<String, SessionError>;
    /// Run `/bin/login` for `username` and return its exit status.
    fn run_login(&mut self, username: &str) -> Result<i32, SessionError>;
    /// Run the session shell with `args` and the profile `env`.
    fn run_shell(&mut self, shell: &str, args: &[&str], env: &[&str]) -> Result<i32, SessionError>;
    /// The session fields for an authenticated account.
    fn account(&mut self, username: &str) -> Option<Account>;
}

/// The environment handed to the session shell.
///
/// `LOGNAME` is part of the login contract; the flat `/etc/profile` parser does not carry it, so
/// getty derives it from the account that authenticated.
#[must_use]
pub fn profile_env(profile: &Profile, user: &str, home: &str, shell: &str) -> Vec<String> {
    let mut env = Vec::with_capacity(8);
    env.push(format!("PATH={}", profile.path));
    env.push(format!("HOME={home}"));
    env.push(format!("TERM={}", profile.term));
    env.push(format!("USER={user}"));
    env.push(format!("LOGNAME={user}"));
    env.push(format!("SHELL={shell}"));
    env.push(String::from("PWD=/"));
    // K571/S188 diagnostic: name the frames of the teardown panic; remove before release.
    env.push(String::from("RUST_BACKTRACE=full"));
    env
}

/// The session shell argument vector.
///
/// Brush is forced interactive and treated as a login shell so it loads `/etc/profile`; other
/// shells keep their default argv.
#[must_use]
pub fn shell_args(shell: &str) -> &'static [&'static str] {
    if shell.rsplit('/').next() == Some("brush") {
        &["brush", "-i", "-l"]
    } else {
        &[]
    }
}

/// Run the banner, the retried login attempt and the session shell.
///
/// An enabled console autologin starts its account's shell immediately, without a password. A
/// bad user name, a rejected password or a failed `login` spawn loops back to the prompt. Only a
/// zero status from `login` reaches the shell; the shell's status is getty's exit status. The
/// passwd home/shell are used for a non-root account; `default_shell` covers a missing field.
pub fn run_session<S: Session + ?Sized>(
    session: &mut S,
    profile: &Profile,
    default_shell: &str,
    autologin: Option<&str>,
) -> i32 {
    if session.write(BANNER.as_bytes()).is_err() {
        return 1;
    }
    if let Some(user) = autologin
        && let Some(account) = session.account(user)
    {
        return run_user(session, profile, user, &account, default_shell);
    }
    let username = loop {
        if session.write(PROMPT).is_err() {
            return 1;
        }
        let line = match session.read_line() {
            Ok(line) => line,
            Err(_) => return 1,
        };
        let Some(username) = normalize_username(&line) else {
            if session.write(INCORRECT).is_err() {
                return 1;
            }
            continue;
        };
        match session.run_login(username) {
            Ok(0) => break String::from(username),
            Ok(_) => continue,
            Err(_) => {
                if session.write(LOGIN_ERROR).is_err() {
                    return 1;
                }
                continue;
            }
        }
    };
    let account = session.account(&username).unwrap_or_else(|| Account {
        home: profile.home.clone(),
        shell: String::from(default_shell),
    });
    run_user(session, profile, &username, &account, default_shell)
}

fn run_user<S: Session + ?Sized>(
    session: &mut S,
    profile: &Profile,
    user: &str,
    account: &Account,
    default_shell: &str,
) -> i32 {
    let shell = if account.shell.is_empty() {
        default_shell
    } else {
        account.shell.as_str()
    };
    let home = if account.home.is_empty() {
        profile.home.as_str()
    } else {
        account.home.as_str()
    };
    let env = profile_env(profile, user, home, shell);
    let env: Vec<&str> = env.iter().map(String::as_str).collect();
    match session.run_shell(shell, shell_args(shell), &env) {
        Ok(status) => status,
        Err(_) => {
            let _ = session.write(SHELL_ERROR);
            1
        }
    }
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;

// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! RavenBSD `login` policy: the root account, its shadow record and the session profile.
//!
//! The binary is a thin target wrapper; this library owns the parsers and the authentication
//! handoff so they can be tested on the host. Password hashing and verification are never
//! reimplemented here: [`verify_root_password`] calls REL1b's `raven_auth` crate.

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

use alloc::string::String;
use alloc::vec::Vec;

/// Default search path when `/etc/profile` has no `PATH`.
pub const DEFAULT_PATH: &str = "/bin:/sbin";
/// Default home when `/etc/profile` has no `HOME`.
pub const DEFAULT_HOME: &str = "/";
/// Default terminal type when `/etc/profile` has no `TERM`.
pub const DEFAULT_TERM: &str = "vt100";
/// Default login name; the release image has the root account only.
pub const ROOT_USER: &str = "root";
/// Authentication succeeded; getty starts the session shell.
pub const LOGIN_OK: i32 = 0;
/// Authentication failed; getty re-prompts instead of starting a shell.
pub const LOGIN_RETRY: i32 = 1;
/// Default session shell for the root login. Both getty release images stage brush.
pub const DEFAULT_SHELL: &str = "/bin/brush";
/// Longest accepted account/profile field.
pub const MAX_FIELD: usize = 256;
/// Longest accepted login name.
pub const MAX_USER: usize = 32;

/// One `/etc/passwd` entry needed by the session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PasswdEntry {
    /// Account name.
    pub name: String,
    /// Home directory.
    pub home: String,
    /// Login shell.
    pub shell: String,
}

/// The session environment parsed from `/etc/profile`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Profile {
    /// `PATH` value.
    pub path: String,
    /// `HOME` value.
    pub home: String,
    /// `TERM` value.
    pub term: String,
    /// `USER` value.
    pub user: String,
    /// `SHELL` value from the profile; the root passwd shell resolves to [`DEFAULT_SHELL`].
    pub shell: String,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            path: String::from(DEFAULT_PATH),
            home: String::from(DEFAULT_HOME),
            term: String::from(DEFAULT_TERM),
            user: String::from(ROOT_USER),
            shell: String::from(DEFAULT_SHELL),
        }
    }
}

/// Find the `root` entry in `/etc/passwd`.
///
/// The root session shell is login policy, not the mutable passwd field: both release images
/// stage brush at [`DEFAULT_SHELL`], and the passwd shell is kept as account metadata.
#[must_use]
pub fn parse_root_passwd(text: &str) -> Option<PasswdEntry> {
    text.lines().find_map(|line| {
        let mut entry = parse_passwd_line(line)?;
        if entry.name != ROOT_USER {
            return None;
        }
        entry.shell = String::from(DEFAULT_SHELL);
        Some(entry)
    })
}

/// Parse every valid `/etc/passwd` entry. Malformed lines are ignored, as they are for
/// [`parse_root_passwd`]; the account database publisher owns the strict validation.
#[must_use]
pub fn parse_passwd(text: &str) -> Vec<PasswdEntry> {
    text.lines().filter_map(parse_passwd_line).collect()
}

/// Find one account in `/etc/passwd`.
#[must_use]
pub fn find_passwd(text: &str, name: &str) -> Option<PasswdEntry> {
    parse_passwd(text)
        .into_iter()
        .find(|entry| entry.name == name)
}

/// Persisted console autologin.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Autologin {
    /// Whether the console starts a session without a password.
    pub enabled: bool,
    /// Account to start.
    pub user: String,
}

/// Parse `/etc/raven/autologin.toml`.
///
/// The reader accepts the small `[console] user/enabled` shape written by `raven config`; it
/// ignores comments, sections and unknown keys. A malformed or invalid account disables the
/// feature rather than blocking the console.
#[must_use]
pub fn parse_autologin(text: &str) -> Autologin {
    let mut autologin = Autologin::default();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('[') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "user" => {
                autologin.user = String::from(
                    value
                        .strip_prefix('"')
                        .and_then(|value| value.strip_suffix('"'))
                        .unwrap_or(value),
                );
            }
            "enabled" => autologin.enabled = value == "true",
            _ => {}
        }
    }
    if normalize_username(&autologin.user).is_none() {
        autologin.user.clear();
        autologin.enabled = false;
    }
    autologin
}

fn parse_passwd_line(line: &str) -> Option<PasswdEntry> {
    let mut fields = line.split(':');
    let name = fields.next()?;
    let _password = fields.next()?;
    let _uid = fields.next()?;
    let _gid = fields.next()?;
    let _gecos = fields.next()?;
    let home = fields.next()?;
    let shell = fields.next()?;
    if fields.next().is_some() {
        return None;
    }
    if name.is_empty()
        || home.is_empty()
        || shell.is_empty()
        || home.len() > MAX_FIELD
        || shell.len() > MAX_FIELD
    {
        return None;
    }
    Some(PasswdEntry {
        name: String::from(name),
        home: String::from(home),
        shell: String::from(shell),
    })
}

/// Parse `/etc/profile` as a flat `KEY=VALUE` file.
///
/// Only `PATH`, `HOME`, `TERM`, `USER` and `SHELL` are consumed; comments, blank lines, unknown
/// keys and values longer than [`MAX_FIELD`] are ignored. There is no shell expansion.
#[must_use]
pub fn parse_profile(text: &str) -> Profile {
    let mut profile = Profile::default();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();
        if value.len() > MAX_FIELD {
            continue;
        }
        match key {
            "PATH" => profile.path = String::from(value),
            "HOME" => profile.home = String::from(value),
            "TERM" => profile.term = String::from(value),
            "USER" => profile.user = String::from(value),
            "SHELL" => profile.shell = String::from(value),
            _ => {}
        }
    }
    profile
}

/// Find the nine-field `root` shadow line.
#[must_use]
pub fn root_shadow_line(text: &str) -> Option<&str> {
    text.lines().find(|line| {
        let mut fields = line.split(':');
        fields.next() == Some(ROOT_USER) && fields.count() == 8
    })
}

/// Verify a password against the shadow text using REL1b's constant-time verifier.
#[must_use]
pub fn verify_root_password(shadow_text: &str, password: &str) -> bool {
    root_shadow_line(shadow_text).is_some_and(|line| raven_auth::verify_root_shadow(line, password))
}

/// The exit status for one login attempt.
///
/// Any account with a verifiable `$6$` shadow record can authenticate. An unknown account, a
/// locked record or a wrong password all return [`LOGIN_RETRY`] so getty prints
/// `Login incorrect` and prompts again.
#[must_use]
pub fn login_status(shadow_text: &str, username: &str, password: &str) -> i32 {
    let line = shadow_text.lines().find(|line| {
        let mut fields = line.split(':');
        fields.next() == Some(username) && fields.count() == 8
    });
    let Some(line) = line else {
        return LOGIN_RETRY;
    };
    match raven_auth::ShadowEntry::parse(line) {
        Ok(entry) if entry.verify(username, password) => LOGIN_OK,
        _ => LOGIN_RETRY,
    }
}

/// Normalise a login name: trim ASCII whitespace, accept 1..=32 account characters.
#[must_use]
pub fn normalize_username(text: &str) -> Option<&str> {
    let name = text.trim_matches(|character: char| character.is_ascii_whitespace());
    if name.is_empty()
        || name.len() > MAX_USER
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
    {
        return None;
    }
    Some(name)
}

/// The release image has one account.
#[must_use]
pub fn is_root_user(name: &str) -> bool {
    name == ROOT_USER
}

#[cfg(target_os = "none")]
pub mod tty {
    //! The `tty:console` session helpers shared by `login` and `getty`.

    use alloc::string::String;
    use alloc::vec::Vec;

    use raven_abi::errno::Errno;
    use raven_abi::types::Handle;
    use raven_proto_fs::O_RDWR;
    use raven_proto_tty::{CALL_LEN, Mode, Request, Settings, encode_request};
    use raven_rt::fs;
    use raven_scheme::Client;

    /// Longest line accepted from the console.
    pub const MAX_LINE: usize = 256;

    /// One open `tty:console` handle.
    pub struct Console {
        handle: Handle,
    }

    impl Console {
        /// Open the serial console for reading and writing.
        pub fn open() -> Result<Self, Errno> {
            let flags = fs::s11_flags(O_RDWR)?;
            let handle = Client::new().open("tty:console", flags)?;
            Ok(Self { handle })
        }

        /// Write all bytes to the console.
        pub fn write(&self, bytes: &[u8]) -> Result<(), Errno> {
            let client = Client::new();
            let mut written = 0usize;
            while written < bytes.len() {
                let chunk = bytes.get(written..).ok_or(Errno::EIO)?;
                let count = client.write(self.handle, 0, chunk)?;
                if count == 0 {
                    return Err(Errno::EIO);
                }
                written = written.checked_add(count).ok_or(Errno::EIO)?;
            }
            Ok(())
        }

        /// Read one canonical line without its terminator.
        pub fn read_line(&self) -> Result<String, Errno> {
            let client = Client::new();
            let mut line: Vec<u8> = Vec::new();
            let mut buffer = [0u8; 64];
            loop {
                let count = match client.read(self.handle, 0, &mut buffer) {
                    Ok(count) => count,
                    // The console server must not block its dispatch loop on an idle read
                    // (that starves the console output path and queues tty requests); poll.
                    Err(Errno::EAGAIN) => {
                        let _ = raven_rt::nanosleep(10_000_000);
                        continue;
                    }
                    Err(error) => return Err(error),
                };
                if count == 0 {
                    return Err(Errno::EIO);
                }
                let chunk = buffer.get(..count).ok_or(Errno::EIO)?;
                for &byte in chunk {
                    if byte == b'\n' {
                        return Ok(String::from_utf8_lossy(&line).into_owned());
                    }
                    if line.len() < MAX_LINE {
                        line.push(byte);
                    }
                }
            }
        }

        /// Discard input buffered before the session shell starts.
        ///
        /// The console scheme may retain the tail of a read in its canonical buffer. The login
        /// process hands the console to getty, which spawns the session shell; without a flush
        /// the first shell line can inherit stale bytes (observed as NULs before the first
        /// command). FLUSH is best-effort: a server that does not implement it keeps the old
        /// behaviour.
        pub fn flush(&self) {
            let mut encoded = [0u8; CALL_LEN];
            encode_request(Request::Flush, &mut encoded);
            let mut response = [0u8; CALL_LEN];
            let _ = Client::new().call(self.handle, &encoded, &mut response);
        }

        /// Switch the console echo bit; failures keep the previous setting.
        pub fn set_echo(&self, echo: bool) {
            let request = Request::SetMode {
                settings: Settings {
                    mode: Mode::Canonical,
                    echo,
                },
            };
            let mut encoded = [0u8; CALL_LEN];
            encode_request(request, &mut encoded);
            let mut response = [0u8; CALL_LEN];
            let _ = Client::new().call(self.handle, &encoded, &mut response);
        }
    }

    impl Drop for Console {
        fn drop(&mut self) {
            let _ = Client::new().close(self.handle);
        }
    }

    /// A pty pair that gives the session shell a blocking stdin.
    ///
    /// The console scheme answers an idle read with `EAGAIN` so the single-threaded server can
    /// keep dispatching writes (K536/K570). A foreign shell such as brush treats that as a fatal
    /// input error, so getty starts it on the slave side of a pty: the pty read waits for the
    /// writer, as the BSD tty handoff does. `forward_console_to_pty` supplies the writer.
    pub struct PtyPair {
        master: Handle,
        slave: Handle,
    }

    impl PtyPair {
        /// Open the `pty:` pair and put the slave in raw mode without echo.
        pub fn open() -> Result<Self, Errno> {
            let flags = fs::s11_flags(O_RDWR)?;
            let client = Client::new();
            let master = client.open("pty:master", flags)?;
            let slave = match client.open("pty:slave", flags) {
                Ok(slave) => slave,
                Err(error) => {
                    let _ = client.close(master);
                    return Err(error);
                }
            };
            let request = Request::SetMode {
                settings: Settings {
                    mode: Mode::Raw,
                    echo: false,
                },
            };
            let mut encoded = [0u8; CALL_LEN];
            encode_request(request, &mut encoded);
            let mut response = [0u8; CALL_LEN];
            // A pair that rejects the mode still works canonically; the shell keeps its line
            // discipline either way.
            let _ = client.call(slave, &encoded, &mut response);
            Ok(Self { master, slave })
        }

        /// The master side the console forwarder writes to.
        pub const fn master(&self) -> Handle {
            self.master
        }

        /// The slave side handed to the session shell as fd 0.
        pub const fn slave(&self) -> Handle {
            self.slave
        }
    }

    impl Drop for PtyPair {
        fn drop(&mut self) {
            let client = Client::new();
            let _ = client.close(self.master);
            let _ = client.close(self.slave);
        }
    }

    /// Forward console input to a pty master until the console closes.
    ///
    /// Runs on a getty session thread. The console read is polled on `EAGAIN` (K536/K570) and
    /// every byte is copied unchanged; the shell reads the matching pty slave with the pty
    /// server's blocking-read contract.
    pub fn forward_console_to_pty(argument: usize) {
        let Ok(raw) = u32::try_from(argument) else {
            return;
        };
        let master = Handle(raw);
        let Ok(console) = Console::open() else {
            return;
        };
        let client = Client::new();
        let mut buffer = [0u8; 256];
        loop {
            match client.read(console.handle, 0, &mut buffer) {
                Ok(0) => return,
                Ok(count) => {
                    let mut offset = 0usize;
                    while offset < count {
                        let Some(chunk) = buffer.get(offset..count) else {
                            return;
                        };
                        match client.write(master, 0, chunk) {
                            Ok(0) => return,
                            Ok(written) => offset = offset.saturating_add(written),
                            Err(Errno::EAGAIN) => {
                                let _ = raven_rt::nanosleep(10_000_000);
                            }
                            Err(Errno::EINTR) => {}
                            Err(_) => return,
                        }
                    }
                }
                Err(Errno::EAGAIN) => {
                    let _ = raven_rt::nanosleep(10_000_000);
                }
                Err(Errno::EINTR) => {}
                Err(_) => return,
            }
        }
    }
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;

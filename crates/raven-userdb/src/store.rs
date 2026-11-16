// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Locked read/modify/write transactions for the three account files.
//!
//! The sequence follows `docs/user-management.md` section 4.5: take an exclusive
//! `/etc/.pwd.lock`, read and validate all files, write same-directory temporary files, rename
//! them into place in a fixed order, restore the old contents on a partial failure, and release the
//! lock. The filesystem is abstracted behind [`DbIo`] so the host tests can exercise every failure
//! path without touching `/etc`.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use raven_abi::errno::Errno;

use crate::db::Database;
use crate::error::DbError;

/// Number of lock attempts before reporting [`DbError::Busy`].
pub const LOCK_ATTEMPTS: usize = 50;

/// The paths of one account database.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Paths<'a> {
    /// `/etc/passwd`.
    pub passwd: &'a str,
    /// `/etc/group`.
    pub group: &'a str,
    /// `/etc/shadow`.
    pub shadow: &'a str,
    /// `/etc/.pwd.lock`.
    pub lock: &'a str,
}

impl Paths<'static> {
    /// The production database paths.
    #[must_use]
    pub const fn system() -> Self {
        Self {
            passwd: "/etc/passwd",
            group: "/etc/group",
            shadow: "/etc/shadow",
            lock: "/etc/.pwd.lock",
        }
    }
}

/// The filesystem seam used by [`Transaction`].
pub trait DbIo {
    /// Read a whole file.
    fn read(&mut self, path: &str) -> Result<Vec<u8>, DbError>;
    /// Create a file exclusively; [`DbError::Busy`] when it already exists.
    fn create_exclusive(&mut self, path: &str) -> Result<(), DbError>;
    /// Remove a file; a missing file is not an error.
    fn remove(&mut self, path: &str) -> Result<(), DbError>;
    /// Create or truncate a file, apply `mode` before writing, and write the whole buffer.
    fn write_new(&mut self, path: &str, data: &[u8], mode: u32) -> Result<(), DbError>;
    /// Change the permission bits.
    fn chmod(&mut self, path: &str, mode: u32) -> Result<(), DbError>;
    /// Flush pending data to stable storage.
    fn sync(&mut self) -> Result<(), DbError>;
    /// Rename `from` over `to`.
    fn rename(&mut self, from: &str, to: &str) -> Result<(), DbError>;
    /// Wait a short interval between lock attempts.
    fn pause(&mut self) {}
}

/// Read and parse all three files without locking, for read-only tools.
///
/// # Errors
/// An I/O, UTF-8, parse or validation error.
pub fn load(io: &mut impl DbIo, paths: &Paths<'_>) -> Result<Database, DbError> {
    let passwd = io.read(paths.passwd)?;
    let group = io.read(paths.group)?;
    let shadow = io.read(paths.shadow)?;
    Database::parse(text(&passwd)?, text(&group)?, text(&shadow)?)
}

/// Read and parse passwd and group without the shadow file, for `id`/`whoami`.
///
/// # Errors
/// An I/O, UTF-8, parse or validation error.
pub fn load_lookup(io: &mut impl DbIo, paths: &Paths<'_>) -> Result<Database, DbError> {
    let passwd = io.read(paths.passwd)?;
    let group = io.read(paths.group)?;
    Database::parse_lookup(text(&passwd)?, text(&group)?)
}

/// One locked account-database edit.
pub struct Transaction<'a, I: DbIo> {
    io: I,
    paths: Paths<'a>,
    database: Database,
    passwd_old: Vec<u8>,
    group_old: Vec<u8>,
    shadow_old: Vec<u8>,
    active: bool,
}

impl<'a, I: DbIo> Transaction<'a, I> {
    /// Acquire the lock, read the files and parse the database.
    ///
    /// # Errors
    /// [`DbError::Busy`] after [`LOCK_ATTEMPTS`] collisions, or any I/O, parse or validation error.
    pub fn begin(mut io: I, paths: Paths<'a>) -> Result<Self, DbError> {
        acquire(&mut io, paths.lock)?;
        let mut transaction = Self {
            io,
            paths,
            database: Database::default(),
            passwd_old: Vec::new(),
            group_old: Vec::new(),
            shadow_old: Vec::new(),
            active: true,
        };
        if let Err(error) = transaction.read() {
            transaction.release();
            return Err(error);
        }
        Ok(transaction)
    }

    /// The parsed database.
    #[must_use]
    pub fn database(&self) -> &Database {
        &self.database
    }

    /// The database to edit.
    pub fn database_mut(&mut self) -> &mut Database {
        &mut self.database
    }

    /// Write the new contents and release the lock.
    ///
    /// # Errors
    /// A validation or I/O error; on a partial rename failure the already replaced files are
    /// restored from the contents read in [`Transaction::begin`].
    pub fn commit(mut self) -> Result<(), DbError> {
        let result = self.write();
        self.release();
        result
    }

    /// Release the lock without writing.
    pub fn abort(mut self) {
        self.release();
    }

    fn read(&mut self) -> Result<(), DbError> {
        self.passwd_old = self.io.read(self.paths.passwd)?;
        self.group_old = self.io.read(self.paths.group)?;
        self.shadow_old = self.io.read(self.paths.shadow)?;
        self.database = Database::parse(
            text(&self.passwd_old)?,
            text(&self.group_old)?,
            text(&self.shadow_old)?,
        )?;
        Ok(())
    }

    fn write(&mut self) -> Result<(), DbError> {
        self.database.validate()?;
        let passwd = self.database.render_passwd().into_bytes();
        let group = self.database.render_group().into_bytes();
        let shadow = self.database.render_shadow().into_bytes();
        self.stage(self.paths.passwd, &passwd, 0o644)?;
        self.stage(self.paths.group, &group, 0o644)?;
        self.stage(self.paths.shadow, &shadow, 0o600)?;
        let passwd_tmp = temp_path(self.paths.passwd);
        let group_tmp = temp_path(self.paths.group);
        let shadow_tmp = temp_path(self.paths.shadow);
        if let Err(error) = self.io.rename(&passwd_tmp, self.paths.passwd) {
            self.cleanup();
            return Err(error);
        }
        if let Err(error) = self.io.rename(&group_tmp, self.paths.group) {
            let _ = restore(&mut self.io, self.paths.passwd, &self.passwd_old, 0o644);
            self.cleanup();
            return Err(commit_error(error));
        }
        if let Err(error) = self.io.rename(&shadow_tmp, self.paths.shadow) {
            let restored = restore(&mut self.io, self.paths.passwd, &self.passwd_old, 0o644)
                .and_then(|()| restore(&mut self.io, self.paths.group, &self.group_old, 0o644));
            self.cleanup();
            return Err(match restored {
                Ok(()) => commit_error(error),
                Err(_) => DbError::CommitFailed,
            });
        }
        Ok(())
    }

    fn stage(&mut self, path: &str, data: &[u8], mode: u32) -> Result<(), DbError> {
        let temp = temp_path(path);
        if let Err(error) = self
            .io
            .write_new(&temp, data, mode)
            .and_then(|()| self.io.sync())
        {
            self.cleanup();
            return Err(error);
        }
        Ok(())
    }

    fn cleanup(&mut self) {
        let _ = self.io.remove(&temp_path(self.paths.passwd));
        let _ = self.io.remove(&temp_path(self.paths.group));
        let _ = self.io.remove(&temp_path(self.paths.shadow));
    }

    fn release(&mut self) {
        if self.active {
            self.active = false;
            let _ = self.io.remove(self.paths.lock);
        }
    }
}

impl<I: DbIo> Drop for Transaction<'_, I> {
    fn drop(&mut self) {
        self.release();
    }
}

fn restore(io: &mut impl DbIo, path: &str, data: &[u8], mode: u32) -> Result<(), DbError> {
    let temp = temp_path(path);
    io.write_new(&temp, data, mode)?;
    io.sync()?;
    io.rename(&temp, path)
}

fn commit_error(error: DbError) -> DbError {
    match error {
        DbError::Io(_) => DbError::Io(Errno::EIO),
        other => other,
    }
}

fn acquire(io: &mut impl DbIo, path: &str) -> Result<(), DbError> {
    for _ in 0..LOCK_ATTEMPTS {
        match io.create_exclusive(path) {
            Ok(()) => return Ok(()),
            Err(DbError::Busy) => io.pause(),
            Err(error) => return Err(error),
        }
    }
    Err(DbError::Busy)
}

fn text(bytes: &[u8]) -> Result<&str, DbError> {
    core::str::from_utf8(bytes).map_err(|_| DbError::Io(Errno::EILSEQ))
}

fn temp_path(path: &str) -> String {
    format!("{path}.tmp")
}

#[cfg(test)]
#[path = "store_ut.rs"]
mod tests;

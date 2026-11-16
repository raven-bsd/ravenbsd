// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The parsed account database, its validation rules and the tool-facing edits.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::error::{DbError, DbFile};
use crate::group::GroupEntry;
use crate::passwd::PasswdEntry;
use crate::shadow::ShadowEntry;
use crate::validate::{records, valid_gecos, valid_hash, valid_name, valid_path};

/// First system-account id.
pub const MIN_SYSTEM_ID: u32 = 1;
/// Last system-account id.
pub const MAX_SYSTEM_ID: u32 = 999;
/// First regular-account id.
pub const MIN_REGULAR_ID: u32 = 1000;
/// Last regular-account id.
pub const MAX_REGULAR_ID: u32 = 59_999;
/// `nobody`'s reserved id.
pub const NOBODY_ID: u32 = 65_534;
/// Kernel and 0.2.0 supplementary-group limit.
pub const MAX_GROUPS: usize = 16;
/// Default shell for a newly created regular account.
pub const DEFAULT_SHELL: &str = "/bin/brush";

/// Request for [`Database::add_user`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AddUser<'a> {
    /// Login name.
    pub name: &'a str,
    /// Explicit uid, or `None` to allocate the next regular id.
    pub uid: Option<u32>,
    /// Existing primary group, or `None` to create/reuse the same-name group.
    pub group: Option<&'a str>,
    /// Supplementary group names.
    pub groups: &'a [&'a str],
    /// Gecos field (may be empty).
    pub gecos: &'a str,
    /// Explicit home, or `None` for `/home/<name>`.
    pub home: Option<&'a str>,
    /// Explicit shell, or `None` for [`DEFAULT_SHELL`].
    pub shell: Option<&'a str>,
    /// Allocate a system id instead of a regular id.
    pub system: bool,
}

/// Result of a successful [`Database::add_user`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AddUserOutcome {
    /// The new passwd record.
    pub user: PasswdEntry,
    /// True when a same-name primary group was created.
    pub created_group: bool,
}

/// Request for [`Database::add_group`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AddGroup<'a> {
    /// Group name.
    pub name: &'a str,
    /// Explicit gid, or `None` to allocate one.
    pub gid: Option<u32>,
    /// Allocate a system gid instead of a regular gid.
    pub system: bool,
}

/// A process identity used by `id` and `whoami`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Credential {
    /// Real user id.
    pub uid: u32,
    /// Effective user id.
    pub euid: u32,
    /// Real group id.
    pub gid: u32,
    /// Effective group id.
    pub egid: u32,
    /// Primary group followed by supplementary groups.
    pub groups: Vec<u32>,
}

impl Credential {
    /// The root identity used while the kernel is root-only (pre-U3).
    #[must_use]
    pub fn root() -> Self {
        Self {
            uid: 0,
            euid: 0,
            gid: 0,
            egid: 0,
            groups: alloc::vec![0],
        }
    }

    /// Build a credential from one passwd record and its group memberships.
    ///
    /// # Errors
    /// [`DbError::TooManyGroups`] when the account exceeds the kernel limit.
    pub fn for_user(database: &Database, user: &PasswdEntry) -> Result<Self, DbError> {
        let groups = database.groups_for_user(user)?;
        Ok(Self {
            uid: user.uid,
            euid: user.uid,
            gid: user.gid,
            egid: user.gid,
            groups,
        })
    }
}

/// The parsed `/etc/passwd`, `/etc/group` and `/etc/shadow` contents.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Database {
    passwd: Vec<PasswdEntry>,
    passwd_lines: Vec<usize>,
    group: Vec<GroupEntry>,
    group_lines: Vec<usize>,
    shadow: Vec<ShadowEntry>,
    shadow_lines: Vec<usize>,
}

impl Database {
    /// Parse and cross-validate all three account files.
    ///
    /// # Errors
    /// A parse or validation error from any file.
    pub fn parse(passwd: &str, group: &str, shadow: &str) -> Result<Self, DbError> {
        let mut database = Self::default();
        for (line, text) in records(passwd) {
            database.passwd.push(PasswdEntry::parse(text, line)?);
            database.passwd_lines.push(line);
        }
        for (line, text) in records(group) {
            database.group.push(GroupEntry::parse(text, line)?);
            database.group_lines.push(line);
        }
        for (line, text) in records(shadow) {
            database.shadow.push(ShadowEntry::parse(text, line)?);
            database.shadow_lines.push(line);
        }
        database.validate()?;
        Ok(database)
    }

    /// Parse only passwd and group, for lookups that must work without the shadow file.
    ///
    /// # Errors
    /// A parse or validation error from either file.
    pub fn parse_lookup(passwd: &str, group: &str) -> Result<Self, DbError> {
        let mut database = Self::default();
        for (line, text) in records(passwd) {
            database.passwd.push(PasswdEntry::parse(text, line)?);
            database.passwd_lines.push(line);
        }
        for (line, text) in records(group) {
            database.group.push(GroupEntry::parse(text, line)?);
            database.group_lines.push(line);
        }
        database.validate()?;
        Ok(database)
    }

    /// All passwd records.
    #[must_use]
    pub fn passwd(&self) -> &[PasswdEntry] {
        &self.passwd
    }

    /// All group records.
    #[must_use]
    pub fn group(&self) -> &[GroupEntry] {
        &self.group
    }

    /// All shadow records.
    #[must_use]
    pub fn shadow(&self) -> &[ShadowEntry] {
        &self.shadow
    }

    /// Find a user by login name.
    #[must_use]
    pub fn user(&self, name: &str) -> Option<&PasswdEntry> {
        self.passwd.iter().find(|user| user.name == name)
    }

    /// Find a user by uid.
    #[must_use]
    pub fn user_by_uid(&self, uid: u32) -> Option<&PasswdEntry> {
        self.passwd.iter().find(|user| user.uid == uid)
    }

    /// Find a group by name.
    #[must_use]
    pub fn group_by_name(&self, name: &str) -> Option<&GroupEntry> {
        self.group.iter().find(|group| group.name == name)
    }

    /// Find a group by gid.
    #[must_use]
    pub fn group_by_gid(&self, gid: u32) -> Option<&GroupEntry> {
        self.group.iter().find(|group| group.gid == gid)
    }

    /// Find a shadow record by account name.
    #[must_use]
    pub fn shadow_by_name(&self, name: &str) -> Option<&ShadowEntry> {
        self.shadow.iter().find(|entry| entry.name == name)
    }

    /// Render `/etc/passwd`.
    #[must_use]
    pub fn render_passwd(&self) -> String {
        render_records(self.passwd.iter().map(PasswdEntry::render))
    }

    /// Render `/etc/group`.
    #[must_use]
    pub fn render_group(&self) -> String {
        render_records(self.group.iter().map(GroupEntry::render))
    }

    /// Render `/etc/shadow`.
    #[must_use]
    pub fn render_shadow(&self) -> String {
        render_records(self.shadow.iter().map(ShadowEntry::render))
    }

    /// Validate uniqueness and cross-file references.
    ///
    /// # Errors
    /// [`DbError::Duplicate`] for repeated names or ids, and [`DbError::InvalidField`] for a
    /// missing primary group, an unknown member, or an orphan shadow record.
    pub fn validate(&self) -> Result<(), DbError> {
        for (index, user) in self.passwd.iter().enumerate() {
            if self.passwd[..index]
                .iter()
                .any(|seen| seen.name == user.name || seen.uid == user.uid)
            {
                return Err(DbError::Duplicate {
                    file: DbFile::Passwd,
                    line: line_of(&self.passwd_lines, index),
                });
            }
            if self.group_by_gid(user.gid).is_none() {
                return Err(DbError::InvalidField {
                    file: DbFile::Passwd,
                    line: line_of(&self.passwd_lines, index),
                    field: "gid",
                });
            }
        }
        for (index, group) in self.group.iter().enumerate() {
            if self.group[..index]
                .iter()
                .any(|seen| seen.name == group.name || seen.gid == group.gid)
            {
                return Err(DbError::Duplicate {
                    file: DbFile::Group,
                    line: line_of(&self.group_lines, index),
                });
            }
            if group
                .members
                .iter()
                .any(|member| self.user(member).is_none())
            {
                return Err(DbError::InvalidField {
                    file: DbFile::Group,
                    line: line_of(&self.group_lines, index),
                    field: "member",
                });
            }
        }
        for (index, entry) in self.shadow.iter().enumerate() {
            if self.shadow[..index]
                .iter()
                .any(|seen| seen.name == entry.name)
            {
                return Err(DbError::Duplicate {
                    file: DbFile::Shadow,
                    line: line_of(&self.shadow_lines, index),
                });
            }
            if self.user(&entry.name).is_none() {
                return Err(DbError::InvalidField {
                    file: DbFile::Shadow,
                    line: line_of(&self.shadow_lines, index),
                    field: "name",
                });
            }
        }
        Ok(())
    }

    /// The next regular uid after the highest allocated regular uid.
    ///
    /// # Errors
    /// [`DbError::NoFreeId`] when the regular range is exhausted.
    pub fn next_regular_uid(&self) -> Result<u32, DbError> {
        next_id(self.passwd.iter().map(|user| user.uid), false)
    }

    /// The next regular gid after the highest allocated regular gid.
    ///
    /// # Errors
    /// [`DbError::NoFreeId`] when the regular range is exhausted.
    pub fn next_regular_gid(&self) -> Result<u32, DbError> {
        next_id(self.group.iter().map(|group| group.gid), false)
    }

    /// The next system gid after the highest allocated system gid.
    ///
    /// # Errors
    /// [`DbError::NoFreeId`] when the system range is exhausted.
    pub fn next_system_gid(&self) -> Result<u32, DbError> {
        next_id(self.group.iter().map(|group| group.gid), true)
    }

    /// Create a user, its same-name primary group (unless a group was named), its locked shadow
    /// record and its supplementary memberships.
    ///
    /// # Errors
    /// [`DbError::InvalidName`], [`DbError::Duplicate`], [`DbError::UnknownGroup`],
    /// [`DbError::TooManyGroups`], [`DbError::NoFreeId`] or [`DbError::InvalidField`].
    pub fn add_user(&mut self, request: &AddUser<'_>) -> Result<AddUserOutcome, DbError> {
        if !valid_name(request.name) {
            return Err(DbError::InvalidName);
        }
        if !valid_gecos(request.gecos) {
            return Err(DbError::InvalidName);
        }
        if request.groups.len() > MAX_GROUPS {
            return Err(DbError::TooManyGroups);
        }
        if self.user(request.name).is_some() {
            return Err(DbError::Duplicate {
                file: DbFile::Passwd,
                line: 0,
            });
        }
        let mut memberships = Vec::new();
        for name in request.groups {
            let index = self
                .group
                .iter()
                .position(|group| group.name == *name)
                .ok_or(DbError::UnknownGroup)?;
            if !memberships.contains(&index) {
                memberships.push(index);
            }
        }
        let uid = match request.uid {
            Some(uid) => {
                if !valid_id(uid, request.system) || self.user_by_uid(uid).is_some() {
                    return Err(DbError::Duplicate {
                        file: DbFile::Passwd,
                        line: 0,
                    });
                }
                uid
            }
            None => {
                if request.system {
                    next_id(self.passwd.iter().map(|user| user.uid), true)?
                } else {
                    self.next_regular_uid()?
                }
            }
        };
        let (gid, created_group) = match request.group {
            Some(name) => {
                let group = self.group_by_name(name).ok_or(DbError::UnknownGroup)?;
                (group.gid, false)
            }
            None => {
                if let Some(existing) = self.group_by_name(request.name) {
                    if existing.gid != uid {
                        return Err(DbError::Duplicate {
                            file: DbFile::Group,
                            line: 0,
                        });
                    }
                    (uid, false)
                } else if self.group_by_gid(uid).is_some() {
                    return Err(DbError::Duplicate {
                        file: DbFile::Group,
                        line: 0,
                    });
                } else {
                    (uid, true)
                }
            }
        };
        let home = match request.home {
            Some(home) => home.to_string(),
            None => format!("/home/{}", request.name),
        };
        let shell = match request.shell {
            Some(shell) => shell.to_string(),
            None => DEFAULT_SHELL.to_string(),
        };
        if home.is_empty() || !valid_path(&home) || !valid_path(&shell) {
            return Err(DbError::InvalidName);
        }
        let user = PasswdEntry {
            name: request.name.to_string(),
            passwd: "x".to_string(),
            uid,
            gid,
            gecos: request.gecos.to_string(),
            home,
            shell,
        };
        if created_group {
            self.group.push(GroupEntry {
                name: request.name.to_string(),
                passwd: "x".to_string(),
                gid,
                members: Vec::new(),
            });
            self.group_lines.push(0);
        }
        for index in memberships {
            self.group
                .get_mut(index)
                .ok_or(DbError::UnknownGroup)?
                .add_member(request.name);
        }
        self.passwd.push(user.clone());
        self.passwd_lines.push(0);
        self.shadow.push(ShadowEntry::locked(request.name));
        self.shadow_lines.push(0);
        Ok(AddUserOutcome {
            user,
            created_group,
        })
    }

    /// Create an empty group.
    ///
    /// # Errors
    /// [`DbError::InvalidName`], [`DbError::Duplicate`] or [`DbError::NoFreeId`].
    pub fn add_group(&mut self, request: &AddGroup<'_>) -> Result<GroupEntry, DbError> {
        if !valid_name(request.name) {
            return Err(DbError::InvalidName);
        }
        if self.group_by_name(request.name).is_some() {
            return Err(DbError::Duplicate {
                file: DbFile::Group,
                line: 0,
            });
        }
        let gid = match request.gid {
            Some(gid) => {
                if !valid_id(gid, request.system) || self.group_by_gid(gid).is_some() {
                    return Err(DbError::Duplicate {
                        file: DbFile::Group,
                        line: 0,
                    });
                }
                gid
            }
            None => {
                if request.system {
                    self.next_system_gid()?
                } else {
                    self.next_regular_gid()?
                }
            }
        };
        let group = GroupEntry {
            name: request.name.to_string(),
            passwd: "x".to_string(),
            gid,
            members: Vec::new(),
        };
        self.group.push(group.clone());
        self.group_lines.push(0);
        Ok(group)
    }

    /// Replace an account's shadow hash and last-change day, creating the record if needed.
    ///
    /// # Errors
    /// [`DbError::UnknownUser`] when the account does not exist and [`DbError::InvalidHash`] for
    /// anything that is not a `$6$` record or lock marker.
    pub fn set_password(
        &mut self,
        name: &str,
        hash: &str,
        last_change: i64,
    ) -> Result<(), DbError> {
        if !valid_hash(hash) {
            return Err(DbError::InvalidHash);
        }
        if self.user(name).is_none() {
            return Err(DbError::UnknownUser);
        }
        if let Some(index) = self.shadow.iter().position(|entry| entry.name == name) {
            if let Some(entry) = self.shadow.get_mut(index) {
                entry.hash = hash.to_string();
                entry.last_change = last_change;
            }
            return Ok(());
        }
        let mut entry = ShadowEntry::locked(name);
        entry.hash = hash.to_string();
        entry.last_change = last_change;
        self.shadow.push(entry);
        self.shadow_lines.push(0);
        Ok(())
    }

    /// Lock an account's password.
    ///
    /// # Errors
    /// [`DbError::UnknownUser`] when there is no shadow record.
    pub fn lock_password(&mut self, name: &str) -> Result<(), DbError> {
        let index = self
            .shadow
            .iter()
            .position(|entry| entry.name == name)
            .ok_or(DbError::UnknownUser)?;
        self.shadow
            .get_mut(index)
            .ok_or(DbError::UnknownUser)?
            .lock();
        Ok(())
    }

    /// Unlock an account's password.
    ///
    /// # Errors
    /// [`DbError::UnknownUser`] when there is no shadow record and [`DbError::InvalidHash`] when
    /// the record has no restorable `$6$` hash.
    pub fn unlock_password(&mut self, name: &str) -> Result<(), DbError> {
        let index = self
            .shadow
            .iter()
            .position(|entry| entry.name == name)
            .ok_or(DbError::UnknownUser)?;
        self.shadow
            .get_mut(index)
            .ok_or(DbError::UnknownUser)?
            .unlock()
    }

    /// Add a user to a group if not already listed.
    ///
    /// # Errors
    /// [`DbError::UnknownGroup`] or [`DbError::UnknownUser`].
    pub fn add_member(&mut self, group: &str, name: &str) -> Result<(), DbError> {
        if self.user(name).is_none() {
            return Err(DbError::UnknownUser);
        }
        let index = self
            .group
            .iter()
            .position(|entry| entry.name == group)
            .ok_or(DbError::UnknownGroup)?;
        self.group
            .get_mut(index)
            .ok_or(DbError::UnknownGroup)?
            .add_member(name);
        Ok(())
    }

    /// The primary gid followed by every group that names `user` explicitly.
    ///
    /// The kernel's [`MAX_GROUPS`] limit applies to the supplementary list; the primary gid is
    /// carried separately in `gid`/`egid`.
    ///
    /// # Errors
    /// [`DbError::TooManyGroups`] when the account has more than [`MAX_GROUPS`] distinct
    /// supplementary groups.
    pub fn groups_for_user(&self, user: &PasswdEntry) -> Result<Vec<u32>, DbError> {
        let mut groups = alloc::vec![user.gid];
        let mut supplementary = 0usize;
        for group in &self.group {
            if group.contains(&user.name) && !groups.contains(&group.gid) {
                supplementary = supplementary.saturating_add(1);
                if supplementary > MAX_GROUPS {
                    return Err(DbError::TooManyGroups);
                }
                groups.push(group.gid);
            }
        }
        Ok(groups)
    }
}

/// True when `id` is inside the regular or system range selected by `system`.
#[must_use]
pub const fn valid_id(id: u32, system: bool) -> bool {
    if system {
        id >= MIN_SYSTEM_ID && id <= MAX_SYSTEM_ID
    } else {
        id >= MIN_REGULAR_ID && id <= MAX_REGULAR_ID
    }
}

fn line_of(lines: &[usize], index: usize) -> usize {
    lines.get(index).copied().unwrap_or(0)
}

fn next_id(ids: impl Iterator<Item = u32>, system: bool) -> Result<u32, DbError> {
    let (min, max) = if system {
        (MIN_SYSTEM_ID, MAX_SYSTEM_ID)
    } else {
        (MIN_REGULAR_ID, MAX_REGULAR_ID)
    };
    let mut highest = min.saturating_sub(1);
    for id in ids {
        if id >= min && id <= max && id > highest {
            highest = id;
        }
    }
    let next = highest.checked_add(1).ok_or(DbError::NoFreeId)?;
    if next > max {
        return Err(DbError::NoFreeId);
    }
    Ok(next)
}

fn render_records(records: impl Iterator<Item = String>) -> String {
    let mut output = String::new();
    for record in records {
        output.push_str(&record);
        output.push('\n');
    }
    output
}

#[cfg(test)]
#[path = "db_ut.rs"]
mod tests;

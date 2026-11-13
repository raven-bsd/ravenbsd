// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The `passwd`, `group` and `shadow` models, parsers, packing and caches.
//!
//! The user databases are plain files read on every call; there is no `pwd.db`, no nscd and no
//! global state. The shell supplies the file layer through the shared [`FileSource`] seam.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented
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
)] // tests use fixed vectors and fail by panicking

extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use raven_libc_nss::{ColonScanner, FileSource};

/// A positive C `errno`.
pub type Errno = i32;

/// Maximum user/group line length.
pub const MAX_LINE: usize = 4096;
/// Maximum supplementary groups.
pub const NGROUPS_MAX: usize = 1023;
/// Default `getpwuid_r`/`getpwnam_r` buffer recommendation.
pub const GETPW_R_SIZE_MAX: usize = 1024;
/// Default `getgrgid_r`/`getgrnam_r` buffer recommendation.
pub const GETGR_R_SIZE_MAX: usize = 1024;
/// Maximum positive cache entries.
pub const MAX_CACHE: usize = 8;

/// A passwd entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Passwd {
    /// Login name.
    pub name: String,
    /// Encrypted password field.
    pub passwd: String,
    /// User id.
    pub uid: u32,
    /// Primary group id.
    pub gid: u32,
    /// GECOS field.
    pub gecos: String,
    /// Home directory.
    pub dir: String,
    /// Login shell.
    pub shell: String,
}

/// A group entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group {
    /// Group name.
    pub name: String,
    /// Encrypted password field.
    pub passwd: String,
    /// Group id.
    pub gid: u32,
    /// Member names.
    pub mem: Vec<String>,
}

/// A shadow entry; `-1` means unset.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Spwd {
    /// Login name.
    pub namp: String,
    /// Encrypted password.
    pub pwdp: String,
    /// Last change, days since epoch.
    pub lstchg: i64,
    /// Minimum days.
    pub min: i64,
    /// Maximum days.
    pub max: i64,
    /// Warning days.
    pub warn: i64,
    /// Inactive days.
    pub inact: i64,
    /// Expiry, days since epoch.
    pub expire: i64,
    /// Reserved flag.
    pub flag: u64,
}

/// Why a packing operation failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PwError {
    /// The caller's buffer is too small.
    Range,
    /// The file layer failed.
    Io(Errno),
    /// The entry is invalid for the operation.
    Invalid,
}

/// The offsets of a packed passwd record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PackedPasswd {
    /// Name offset.
    pub name: usize,
    /// Password offset.
    pub passwd: usize,
    /// GECOS offset.
    pub gecos: usize,
    /// Directory offset.
    pub dir: usize,
    /// Shell offset.
    pub shell: usize,
    /// Total used bytes.
    pub len: usize,
}

/// The offsets of a packed group record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackedGroup {
    /// Name offset.
    pub name: usize,
    /// Password offset.
    pub passwd: usize,
    /// Member string offsets.
    pub mem: Vec<usize>,
    /// Total used bytes.
    pub len: usize,
}

/// A passwd source.
pub trait PasswdSource {
    /// Look up one name.
    fn by_name(&self, name: &str) -> Result<Option<Passwd>, Errno>;
    /// Look up one uid.
    fn by_uid(&self, uid: u32) -> Result<Option<Passwd>, Errno>;
    /// Enumerate every entry.
    fn entries(&self) -> Result<Vec<Passwd>, Errno>;
}

/// A group source.
pub trait GroupSource {
    /// Look up one name.
    fn by_name(&self, name: &str) -> Result<Option<Group>, Errno>;
    /// Look up one gid.
    fn by_gid(&self, gid: u32) -> Result<Option<Group>, Errno>;
    /// Enumerate every entry.
    fn entries(&self) -> Result<Vec<Group>, Errno>;
}

/// Plain-file user and group database, read on every call.
pub struct FilesSource<'a> {
    /// The file seam.
    pub fs: &'a dyn FileSource,
}

impl<'a> FilesSource<'a> {
    /// Create a source over `fs`.
    #[must_use]
    pub const fn new(fs: &'a dyn FileSource) -> Self {
        Self { fs }
    }

    fn read(&self, path: &str) -> Result<Vec<u8>, Errno> {
        self.fs.read_file(path, 1024 * 1024)
    }

    fn passwd_entries(&self) -> Result<Vec<Passwd>, Errno> {
        let data = self.read("/etc/passwd")?;
        Ok(scan_passwd(&data))
    }

    fn group_entries(&self) -> Result<Vec<Group>, Errno> {
        let data = self.read("/etc/group")?;
        Ok(scan_group(&data))
    }
}

impl PasswdSource for FilesSource<'_> {
    fn by_name(&self, name: &str) -> Result<Option<Passwd>, Errno> {
        Ok(self
            .passwd_entries()?
            .into_iter()
            .find(|entry| entry.name == name))
    }

    fn by_uid(&self, uid: u32) -> Result<Option<Passwd>, Errno> {
        Ok(self
            .passwd_entries()?
            .into_iter()
            .find(|entry| entry.uid == uid))
    }

    fn entries(&self) -> Result<Vec<Passwd>, Errno> {
        self.passwd_entries()
    }
}

impl GroupSource for FilesSource<'_> {
    fn by_name(&self, name: &str) -> Result<Option<Group>, Errno> {
        Ok(self
            .group_entries()?
            .into_iter()
            .find(|entry| entry.name == name))
    }

    fn by_gid(&self, gid: u32) -> Result<Option<Group>, Errno> {
        Ok(self
            .group_entries()?
            .into_iter()
            .find(|entry| entry.gid == gid))
    }

    fn entries(&self) -> Result<Vec<Group>, Errno> {
        self.group_entries()
    }
}

/// Parse one 7-field passwd line.
#[must_use]
pub fn parse_passwd_line(line: &[u8]) -> Option<Passwd> {
    let mut fields = [b"".as_slice(); 8];
    if ColonScanner::split_fields(line, &mut fields) != 7 {
        return None;
    }
    Some(Passwd {
        name: text(fields.first().copied()?)?,
        passwd: text(fields.get(1)?)?,
        uid: number(fields.get(2)?)?,
        gid: number(fields.get(3)?)?,
        gecos: text(fields.get(4)?)?,
        dir: text(fields.get(5)?)?,
        shell: text(fields.get(6)?)?,
    })
}

/// Parse one 4-field group line.
#[must_use]
pub fn parse_group_line(line: &[u8]) -> Option<Group> {
    let mut fields = [b"".as_slice(); 6];
    if ColonScanner::split_fields(line, &mut fields) != 4 {
        return None;
    }
    let members = fields.get(3)?;
    let mem = if members.is_empty() {
        Vec::new()
    } else {
        members
            .split(|byte| *byte == b',')
            .filter(|member| !member.is_empty())
            .map(text)
            .collect::<Option<Vec<_>>>()?
    };
    Some(Group {
        name: text(fields.first().copied()?)?,
        passwd: text(fields.get(1)?)?,
        gid: number(fields.get(2)?)?,
        mem,
    })
}

/// Parse one 9-field Linux shadow line.
#[must_use]
pub fn parse_shadow_line(line: &[u8]) -> Option<Spwd> {
    let mut fields = [b"".as_slice(); 10];
    if ColonScanner::split_fields(line, &mut fields) != 9 {
        return None;
    }
    let flag_field = fields.get(8).copied()?;
    let flag = match flag_field {
        b"" => 0,
        value => unsigned(value)?,
    };
    Some(Spwd {
        namp: text(fields.first().copied()?)?,
        pwdp: text(fields.get(1)?)?,
        lstchg: signed(fields.get(2)?),
        min: signed(fields.get(3)?),
        max: signed(fields.get(4)?),
        warn: signed(fields.get(5)?),
        inact: signed(fields.get(6)?),
        expire: signed(fields.get(7)?),
        flag,
    })
}

/// Parse one 10-field BSD `master.passwd` line.
#[must_use]
pub fn parse_master_passwd_line(line: &[u8]) -> Option<(Passwd, Spwd)> {
    let mut fields = [b"".as_slice(); 12];
    if ColonScanner::split_fields(line, &mut fields) != 10 {
        return None;
    }
    let change = signed(fields.get(5)?);
    let expire = signed(fields.get(6)?);
    let days = |value: i64| {
        if value < 0 { -1 } else { value / 86_400 }
    };
    let passwd = Passwd {
        name: text(fields.first().copied()?)?,
        passwd: text(fields.get(1)?)?,
        uid: number(fields.get(2)?)?,
        gid: number(fields.get(3)?)?,
        gecos: text(fields.get(7)?)?,
        dir: text(fields.get(8)?)?,
        shell: text(fields.get(9)?)?,
    };
    let shadow = Spwd {
        namp: passwd.name.clone(),
        pwdp: passwd.passwd.clone(),
        lstchg: days(change),
        min: -1,
        max: -1,
        warn: -1,
        inact: -1,
        expire: days(expire),
        flag: 0,
    };
    Some((passwd, shadow))
}

/// Pack a passwd record into `buf`.
pub fn pack_passwd(pw: &Passwd, buf: &mut [u8]) -> Result<PackedPasswd, PwError> {
    let strings = [
        pw.name.as_bytes(),
        pw.passwd.as_bytes(),
        pw.gecos.as_bytes(),
        pw.dir.as_bytes(),
        pw.shell.as_bytes(),
    ];
    let required = strings.iter().try_fold(0usize, |total, value| {
        total
            .checked_add(value.len())
            .and_then(|value| value.checked_add(1))
    });
    let Some(required) = required else {
        return Err(PwError::Range);
    };
    if required > buf.len() {
        return Err(PwError::Range);
    }
    let mut offsets = [0usize; 5];
    let mut pos = 0usize;
    for (index, value) in strings.iter().enumerate() {
        let Some(slot) = offsets.get_mut(index) else {
            return Err(PwError::Range);
        };
        *slot = pos;
        let end = pos.checked_add(value.len()).ok_or(PwError::Range)?;
        buf.get_mut(pos..end)
            .ok_or(PwError::Range)?
            .copy_from_slice(value);
        if let Some(slot) = buf.get_mut(end) {
            *slot = 0;
        }
        pos = end.checked_add(1).ok_or(PwError::Range)?;
    }
    Ok(PackedPasswd {
        name: offsets[0],
        passwd: offsets[1],
        gecos: offsets[2],
        dir: offsets[3],
        shell: offsets[4],
        len: pos,
    })
}

/// Pack a group record into `buf`.
pub fn pack_group(gr: &Group, buf: &mut [u8]) -> Result<PackedGroup, PwError> {
    let mut pos = 0usize;
    let (name, passwd, member_offsets) = {
        let mut write = |value: &[u8]| -> Result<usize, PwError> {
            let offset = pos;
            let end = pos.checked_add(value.len()).ok_or(PwError::Range)?;
            buf.get_mut(pos..end)
                .ok_or(PwError::Range)?
                .copy_from_slice(value);
            if let Some(slot) = buf.get_mut(end) {
                *slot = 0;
            }
            pos = end.checked_add(1).ok_or(PwError::Range)?;
            Ok(offset)
        };
        let name = write(gr.name.as_bytes())?;
        let passwd = write(gr.passwd.as_bytes())?;
        let mut member_offsets = Vec::new();
        for member in &gr.mem {
            member_offsets.push(write(member.as_bytes())?);
        }
        (name, passwd, member_offsets)
    };
    let align = core::mem::size_of::<usize>();
    if align == 0 {
        return Err(PwError::Range);
    }
    let remainder = pos.checked_rem(align).ok_or(PwError::Range)?;
    if remainder != 0 {
        let padding = align.checked_sub(remainder).ok_or(PwError::Range)?;
        let end = pos.checked_add(padding).ok_or(PwError::Range)?;
        let Some(slice) = buf.get_mut(pos..end) else {
            return Err(PwError::Range);
        };
        slice.fill(0);
        pos = end;
    }
    let slots = gr.mem.len().checked_add(1).ok_or(PwError::Range)?;
    for index in 0..slots {
        let offset = member_offsets.get(index).copied().unwrap_or(0);
        let end = pos.checked_add(align).ok_or(PwError::Range)?;
        let Some(slice) = buf.get_mut(pos..end) else {
            return Err(PwError::Range);
        };
        slice.copy_from_slice(&offset.to_ne_bytes());
        pos = end;
    }
    Ok(PackedGroup {
        name,
        passwd,
        mem: member_offsets,
        len: pos,
    })
}

/// Why a group-list operation failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GrpListError {
    /// The output buffer needs at least `needed` slots.
    TooSmall {
        /// Required slot count.
        needed: usize,
    },
    /// The group source failed.
    Io(Errno),
}

/// Fill `out` with the base gid followed by every group listing `user`.
pub fn getgrouplist(
    user: &str,
    gid: u32,
    src: &dyn GroupSource,
    out: &mut [u32],
) -> Result<usize, GrpListError> {
    let mut groups = Vec::new();
    groups.push(gid);
    for group in src.entries().map_err(GrpListError::Io)? {
        if group.mem.iter().any(|member| member == user) && !groups.contains(&group.gid) {
            groups.push(group.gid);
        }
    }
    if groups.len() > out.len() {
        return Err(GrpListError::TooSmall {
            needed: groups.len(),
        });
    }
    for (index, group) in groups.iter().enumerate() {
        if let Some(slot) = out.get_mut(index) {
            *slot = *group;
        }
    }
    Ok(groups.len())
}

/// Build the `initgroups` list with musl's 32/+50% growth rule capped at 1023.
pub fn initgroups_groups(
    user: &str,
    gid: u32,
    src: &dyn GroupSource,
) -> Result<Vec<u32>, GrpListError> {
    let mut size = 32usize;
    loop {
        let mut buffer = alloc::vec![0u32; size];
        match getgrouplist(user, gid, src, &mut buffer) {
            Ok(count) => {
                buffer.truncate(count);
                return Ok(buffer);
            }
            Err(GrpListError::TooSmall { needed }) if size < NGROUPS_MAX => {
                size = needed.max(size.saturating_add(size / 2)).min(NGROUPS_MAX);
            }
            Err(error) => return Err(error),
        }
    }
}

/// A small positive/negative cache over passwd and group sources.
pub struct PwCache<'a> {
    /// Passwd source.
    pub passwd: &'a dyn PasswdSource,
    /// Group source.
    pub group: &'a dyn GroupSource,
    positive_users: Vec<(u32, String)>,
    negative_users: Vec<u32>,
    positive_groups: Vec<(u32, String)>,
    negative_groups: Vec<u32>,
}

impl<'a> PwCache<'a> {
    /// Create an empty cache.
    #[must_use]
    pub const fn new(passwd: &'a dyn PasswdSource, group: &'a dyn GroupSource) -> Self {
        Self {
            passwd,
            group,
            positive_users: Vec::new(),
            negative_users: Vec::new(),
            positive_groups: Vec::new(),
            negative_groups: Vec::new(),
        }
    }

    /// `user_from_uid` with the caller's `noname` fallback.
    pub fn user_from_uid(&mut self, uid: u32, noname: &str) -> Result<String, Errno> {
        if let Some((_, name)) = self.positive_users.iter().find(|(key, _)| *key == uid) {
            return Ok(name.clone());
        }
        if self.negative_users.contains(&uid) {
            return Ok(noname.to_string());
        }
        match self.passwd.by_uid(uid)? {
            Some(entry) => {
                self.positive_users.push((uid, entry.name.clone()));
                if self.positive_users.len() > MAX_CACHE {
                    self.positive_users.remove(0);
                }
                Ok(entry.name)
            }
            None => {
                self.negative_users.push(uid);
                Ok(noname.to_string())
            }
        }
    }

    /// `group_from_gid` with the caller's `noname` fallback.
    pub fn group_from_gid(&mut self, gid: u32, noname: &str) -> Result<String, Errno> {
        if let Some((_, name)) = self.positive_groups.iter().find(|(key, _)| *key == gid) {
            return Ok(name.clone());
        }
        if self.negative_groups.contains(&gid) {
            return Ok(noname.to_string());
        }
        match self.group.by_gid(gid)? {
            Some(entry) => {
                self.positive_groups.push((gid, entry.name.clone()));
                if self.positive_groups.len() > MAX_CACHE {
                    self.positive_groups.remove(0);
                }
                Ok(entry.name)
            }
            None => {
                self.negative_groups.push(gid);
                Ok(noname.to_string())
            }
        }
    }

    /// `uid_from_user`.
    pub fn uid_from_user(&self, name: &str) -> Result<Option<u32>, Errno> {
        Ok(self.passwd.by_name(name)?.map(|entry| entry.uid))
    }

    /// `gid_from_group`.
    pub fn gid_from_group(&self, name: &str) -> Result<Option<u32>, Errno> {
        Ok(self.group.by_name(name)?.map(|entry| entry.gid))
    }
}

/// A cursor over `/etc/shells`.
#[derive(Clone, Debug, Default)]
pub struct ShellCursor {
    entries: Vec<String>,
    pos: usize,
    loaded: bool,
}

impl ShellCursor {
    /// An unloaded cursor.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
            pos: 0,
            loaded: false,
        }
    }
}

/// Return the next shell, loading `/etc/shells` on the first call.
pub fn next_shell(list: &mut ShellCursor, fs: &dyn FileSource) -> Option<String> {
    if !list.loaded {
        list.loaded = true;
        if let Ok(data) = fs.read_file("/etc/shells", 1024 * 1024) {
            let mut scanner = ColonScanner::new(&data, MAX_LINE);
            while let Some(line) = scanner.next_line() {
                let line = line
                    .split(|byte| *byte == b'#')
                    .next()
                    .unwrap_or(&[])
                    .trim_ascii();
                if !line.is_empty()
                    && let Ok(text) = core::str::from_utf8(line)
                {
                    list.entries.push(text.to_string());
                }
            }
        }
        if list.entries.is_empty() {
            list.entries.push("/bin/sh".to_string());
        }
    }
    let value = list.entries.get(list.pos).cloned();
    if value.is_some() {
        list.pos = list.pos.saturating_add(1);
    }
    value
}

/// A byte sink for `putpwent`/`putgrent`.
pub trait ByteSink {
    /// Write every byte or fail.
    fn write_all(&mut self, bytes: &[u8]) -> Result<(), Errno>;
}

/// Format one passwd entry.
pub fn putpwent(entry: &Passwd, sink: &mut dyn ByteSink) -> Result<(), Errno> {
    let line = format!(
        "{}:{}:{}:{}:{}:{}:{}\n",
        entry.name, entry.passwd, entry.uid, entry.gid, entry.gecos, entry.dir, entry.shell
    );
    sink.write_all(line.as_bytes())
}

/// Format one group entry.
pub fn putgrent(entry: &Group, sink: &mut dyn ByteSink) -> Result<(), Errno> {
    let line = format!(
        "{}:{}:{}:{}\n",
        entry.name,
        entry.passwd,
        entry.gid,
        entry.mem.join(",")
    );
    sink.write_all(line.as_bytes())
}

fn scan_passwd(data: &[u8]) -> Vec<Passwd> {
    let mut scanner = ColonScanner::new(data, MAX_LINE);
    let mut entries = Vec::new();
    while let Some(line) = scanner.next_line() {
        if let Some(entry) = parse_passwd_line(line) {
            entries.push(entry);
        }
    }
    entries
}

fn scan_group(data: &[u8]) -> Vec<Group> {
    let mut scanner = ColonScanner::new(data, MAX_LINE);
    let mut entries = Vec::new();
    while let Some(line) = scanner.next_line() {
        if let Some(entry) = parse_group_line(line) {
            entries.push(entry);
        }
    }
    entries
}

fn text(bytes: &[u8]) -> Option<String> {
    core::str::from_utf8(bytes).ok().map(ToString::to_string)
}

fn number(bytes: &[u8]) -> Option<u32> {
    if bytes.is_empty() || !bytes.iter().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let mut value = 0u32;
    for byte in bytes {
        value = value
            .checked_mul(10)?
            .checked_add(u32::from(byte.checked_sub(b'0')?))?;
    }
    Some(value)
}

fn signed(bytes: &[u8]) -> i64 {
    if bytes.is_empty() {
        return -1;
    }
    let (negative, digits) = match bytes.first() {
        Some(b'-') => (true, bytes.get(1..).unwrap_or(&[])),
        _ => (false, bytes),
    };
    let mut value = 0i64;
    for byte in digits {
        if !byte.is_ascii_digit() {
            return -1;
        }
        let digit = byte.saturating_sub(b'0');
        value = value.saturating_mul(10).saturating_add(i64::from(digit));
    }
    if negative {
        value.saturating_neg()
    } else {
        value
    }
}

fn unsigned(bytes: &[u8]) -> Option<u64> {
    if bytes.is_empty() || !bytes.iter().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let mut value = 0u64;
    for byte in bytes {
        value = value
            .checked_mul(10)?
            .checked_add(u64::from(byte.checked_sub(b'0')?))?;
    }
    Some(value)
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;

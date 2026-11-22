// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Files-backed user management for the `raven config` Users module and `raven users`.
//!
//! The three account files are parsed into records, edited in memory, validated as a set and
//! written through same-directory temporary files. A password is never stored plaintext: the
//! first-party [`raven_auth`] SHA-512-crypt path produces the `/etc/shadow` `$6$` record.
//! `root`/uid 0 is preserved, names and ids stay unique, and a save creates the home directory
//! with the account's uid/gid and mode 0755.
//!
//! The console autologin file is `/etc/raven/autologin.toml`; `login`/`getty` own the no_std
//! reader, this module owns the writer.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt, chown};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// `/etc/passwd`, relative to the configuration root.
pub const PASSWD_FILE: &str = "etc/passwd";
/// `/etc/shadow`, relative to the configuration root.
pub const SHADOW_FILE: &str = "etc/shadow";
/// `/etc/group`, relative to the configuration root.
pub const GROUP_FILE: &str = "etc/group";
/// Persisted console autologin, relative to the configuration root.
pub const AUTOLOGIN_FILE: &str = "etc/raven/autologin.toml";
/// Lock file used while the three account files are replaced.
pub const LOCK_FILE: &str = "etc/.pwd.lock";
/// Shell for a new regular account.
pub const DEFAULT_SHELL: &str = "/bin/rsh";
/// The preserved superuser.
pub const ROOT_USER: &str = "root";
/// First regular uid/gid allocated by the form.
pub const MIN_REGULAR_ID: u32 = 1000;
/// Last regular uid/gid allocated by the form.
pub const MAX_REGULAR_ID: u32 = 59_999;
/// Longest account name.
pub const MAX_NAME: usize = 31;
/// Longest gecos field.
pub const MAX_GECOS: usize = 127;
/// Longest accepted password.
pub const MAX_PASSWORD: usize = 128;
/// Default shadow aging fields (days).
pub const SHADOW_MAX: i64 = 99_999;
/// Default shadow warning field (days).
pub const SHADOW_WARN: i64 = 7;

/// A user-management failure.
#[derive(Debug)]
pub enum UserError {
    /// A required account file is missing.
    Missing(String),
    /// A file could not be read or written.
    Io(String),
    /// A record could not be parsed.
    Parse {
        /// File name.
        file: String,
        /// One-based line.
        line: usize,
        /// Human detail.
        message: String,
    },
    /// The edited set is inconsistent.
    Validation(String),
    /// Another writer holds the account lock.
    Busy,
}

impl std::fmt::Display for UserError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Missing(path) => write!(formatter, "missing account file: {path}"),
            Self::Io(message) => write!(formatter, "{message}"),
            Self::Parse {
                file,
                line,
                message,
            } => write!(formatter, "{file}: line {line}: {message}"),
            Self::Validation(message) => formatter.write_str(message),
            Self::Busy => formatter.write_str("account database is locked"),
        }
    }
}

impl std::error::Error for UserError {}

/// One `/etc/passwd` record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserRecord {
    /// Login name.
    pub name: String,
    /// Numeric user id.
    pub uid: u32,
    /// Numeric primary group id.
    pub gid: u32,
    /// Free-form comment.
    pub gecos: String,
    /// Home directory.
    pub home: String,
    /// Login shell.
    pub shell: String,
}

impl UserRecord {
    fn parse(line: &str, line_number: usize) -> Result<Self, UserError> {
        let fields: Vec<&str> = line.split(':').collect();
        if fields.len() != 7 {
            return Err(parse_error(PASSWD_FILE, line_number, "expected 7 fields"));
        }
        let name = fields.first().copied().unwrap_or_default();
        if !valid_name(name) {
            return Err(parse_error(PASSWD_FILE, line_number, "invalid name"));
        }
        if fields.get(1).copied() != Some("x") {
            return Err(parse_error(
                PASSWD_FILE,
                line_number,
                "password field must be x",
            ));
        }
        let uid = parse_id(fields.get(2).copied(), PASSWD_FILE, line_number, "uid")?;
        let gid = parse_id(fields.get(3).copied(), PASSWD_FILE, line_number, "gid")?;
        let gecos = fields.get(4).copied().unwrap_or_default();
        if gecos.len() > MAX_GECOS || !clean_field(gecos) {
            return Err(parse_error(PASSWD_FILE, line_number, "invalid gecos"));
        }
        let home = fields.get(5).copied().unwrap_or_default();
        let shell = fields.get(6).copied().unwrap_or_default();
        if !valid_path(home) || !valid_path(shell) {
            return Err(parse_error(
                PASSWD_FILE,
                line_number,
                "invalid home or shell",
            ));
        }
        Ok(Self {
            name: name.to_string(),
            uid,
            gid,
            gecos: gecos.to_string(),
            home: home.to_string(),
            shell: shell.to_string(),
        })
    }

    fn render(&self) -> String {
        format!(
            "{}:x:{}:{}:{}:{}:{}",
            self.name, self.uid, self.gid, self.gecos, self.home, self.shell
        )
    }
}

/// One `/etc/shadow` record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShadowRecord {
    /// Account name.
    pub name: String,
    /// `$6$` hash, lock marker or empty.
    pub hash: String,
    /// Days since the epoch of the last change.
    pub last_change: i64,
    /// Minimum days between changes.
    pub min: i64,
    /// Maximum days a password remains valid.
    pub max: i64,
    /// Warning days before expiry.
    pub warn: i64,
    /// Days after expiry before disable.
    pub inactive: i64,
    /// Absolute expiry day.
    pub expire: i64,
    /// Reserved field.
    pub reserved: i64,
}

impl ShadowRecord {
    /// A locked record with the standard aging fields.
    #[must_use]
    pub fn locked(name: &str) -> Self {
        Self {
            name: name.to_string(),
            hash: String::from("!"),
            last_change: 0,
            min: 0,
            max: SHADOW_MAX,
            warn: SHADOW_WARN,
            inactive: -1,
            expire: -1,
            reserved: 0,
        }
    }

    /// A `$6$` record with the supplied hash and today's last-change day.
    #[must_use]
    pub fn hashed(name: &str, hash: &str) -> Self {
        Self {
            hash: hash.to_string(),
            last_change: today(),
            ..Self::locked(name)
        }
    }

    fn parse(line: &str, line_number: usize) -> Result<Self, UserError> {
        let fields: Vec<&str> = line.split(':').collect();
        if fields.len() != 9 {
            return Err(parse_error(SHADOW_FILE, line_number, "expected 9 fields"));
        }
        let name = fields.first().copied().unwrap_or_default();
        if !valid_name(name) {
            return Err(parse_error(SHADOW_FILE, line_number, "invalid name"));
        }
        let hash = fields.get(1).copied().unwrap_or_default();
        if !valid_hash(hash) {
            return Err(parse_error(SHADOW_FILE, line_number, "invalid hash"));
        }
        let number = |index: usize| -> Result<i64, UserError> {
            let field = fields.get(index).copied().unwrap_or_default();
            if field.is_empty() {
                return Ok(0);
            }
            field
                .parse::<i64>()
                .map_err(|_| parse_error(SHADOW_FILE, line_number, "invalid numeric field"))
        };
        let optional = |index: usize| -> Result<i64, UserError> {
            let field = fields.get(index).copied().unwrap_or_default();
            if field.is_empty() {
                return Ok(-1);
            }
            let value = field
                .parse::<i64>()
                .map_err(|_| parse_error(SHADOW_FILE, line_number, "invalid numeric field"))?;
            if value < -1 {
                return Err(parse_error(
                    SHADOW_FILE,
                    line_number,
                    "inactive/expire must be -1 or a day",
                ));
            }
            Ok(value)
        };
        Ok(Self {
            name: name.to_string(),
            hash: hash.to_string(),
            last_change: number(2)?,
            min: number(3)?,
            max: number(4)?,
            warn: number(5)?,
            inactive: optional(6)?,
            expire: optional(7)?,
            reserved: number(8)?,
        })
    }

    fn render(&self) -> String {
        format!(
            "{}:{}:{}:{}:{}:{}:{}:{}:{}",
            self.name,
            self.hash,
            self.last_change,
            self.min,
            self.max,
            self.warn,
            self.inactive,
            self.expire,
            self.reserved
        )
    }

    /// Whether this record carries a verifiable SHA-512-crypt hash.
    #[must_use]
    pub fn has_hash(&self) -> bool {
        self.hash.starts_with("$6$")
    }
}

/// One `/etc/group` record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroupRecord {
    /// Group name.
    pub name: String,
    /// Numeric group id.
    pub gid: u32,
    /// Supplementary members.
    pub members: Vec<String>,
}

impl GroupRecord {
    fn parse(line: &str, line_number: usize) -> Result<Self, UserError> {
        let fields: Vec<&str> = line.split(':').collect();
        if fields.len() != 4 {
            return Err(parse_error(GROUP_FILE, line_number, "expected 4 fields"));
        }
        let name = fields.first().copied().unwrap_or_default();
        if !valid_name(name) {
            return Err(parse_error(GROUP_FILE, line_number, "invalid name"));
        }
        if !matches!(fields.get(1).copied(), Some("x") | Some("*")) {
            return Err(parse_error(
                GROUP_FILE,
                line_number,
                "invalid password field",
            ));
        }
        let gid = parse_id(fields.get(2).copied(), GROUP_FILE, line_number, "gid")?;
        let mut members = Vec::new();
        for member in fields
            .get(3)
            .copied()
            .unwrap_or_default()
            .split(',')
            .filter(|name| !name.is_empty())
        {
            if !valid_name(member) || members.iter().any(|seen: &String| seen == member) {
                return Err(parse_error(GROUP_FILE, line_number, "invalid member"));
            }
            members.push(member.to_string());
        }
        Ok(Self {
            name: name.to_string(),
            gid,
            members,
        })
    }

    fn render(&self) -> String {
        format!("{}:x:{}:{}", self.name, self.gid, self.members.join(","))
    }
}

/// Persisted console autologin.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Autologin {
    /// Whether an automatic console session is enabled.
    pub enabled: bool,
    /// Account to start.
    pub user: String,
}

/// The parsed account database.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Accounts {
    /// `/etc/passwd` records.
    pub users: Vec<UserRecord>,
    /// `/etc/shadow` records.
    pub shadows: Vec<ShadowRecord>,
    /// `/etc/group` records.
    pub groups: Vec<GroupRecord>,
    /// Console autologin.
    pub autologin: Autologin,
}

impl Accounts {
    /// Read and parse the three account files plus the autologin file below `root`.
    ///
    /// A missing autologin file is the disabled default; the account files are required.
    pub fn load(root: &Path) -> Result<Self, UserError> {
        let passwd = read_required(root, PASSWD_FILE)?;
        let shadow = read_required(root, SHADOW_FILE)?;
        let group = read_required(root, GROUP_FILE)?;
        let autologin = match fs::read_to_string(root.join(AUTOLOGIN_FILE)) {
            Ok(text) => parse_autologin(&text),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Autologin::default(),
            Err(error) => {
                return Err(UserError::Io(format!(
                    "cannot read {}: {error}",
                    root.join(AUTOLOGIN_FILE).display()
                )));
            }
        };
        let mut accounts = Self {
            users: Vec::new(),
            shadows: Vec::new(),
            groups: Vec::new(),
            autologin,
        };
        for (index, line) in passwd.lines().enumerate() {
            if !line.is_empty() {
                accounts.users.push(UserRecord::parse(line, index + 1)?);
            }
        }
        for (index, line) in shadow.lines().enumerate() {
            if !line.is_empty() {
                accounts.shadows.push(ShadowRecord::parse(line, index + 1)?);
            }
        }
        for (index, line) in group.lines().enumerate() {
            if !line.is_empty() {
                accounts.groups.push(GroupRecord::parse(line, index + 1)?);
            }
        }
        Ok(accounts)
    }

    /// Find a user by login name.
    #[must_use]
    pub fn user(&self, name: &str) -> Option<&UserRecord> {
        self.users.iter().find(|user| user.name == name)
    }

    /// The shadow record for an account.
    #[must_use]
    pub fn shadow(&self, name: &str) -> Option<&ShadowRecord> {
        self.shadows.iter().find(|shadow| shadow.name == name)
    }

    /// The next free regular uid at or above [`MIN_REGULAR_ID`].
    #[must_use]
    pub fn next_uid(&self) -> u32 {
        next_id(self.users.iter().map(|user| user.uid))
    }

    /// The next free regular gid at or above [`MIN_REGULAR_ID`].
    #[must_use]
    pub fn next_gid(&self) -> u32 {
        next_id(self.groups.iter().map(|group| group.gid))
    }

    /// Whether `name` is the enabled console autologin account.
    #[must_use]
    pub fn is_autologin(&self, name: &str) -> bool {
        self.autologin.enabled && self.autologin.user == name
    }

    /// Cross-check names, ids, groups, shadow records and the root account.
    ///
    /// Shell executability and home existence are file-system checks; see [`check`].
    pub fn validate(&self) -> Result<(), UserError> {
        let mut seen_names = std::collections::BTreeSet::new();
        let mut seen_uids = std::collections::BTreeSet::new();
        for user in &self.users {
            if !seen_names.insert(user.name.clone()) {
                return Err(validation(format!("duplicate user `{}`", user.name)));
            }
            if !seen_uids.insert(user.uid) {
                return Err(validation(format!("duplicate uid {}", user.uid)));
            }
            if !self.groups.iter().any(|group| group.gid == user.gid) {
                return Err(validation(format!(
                    "user `{}` has no group with gid {}",
                    user.name, user.gid
                )));
            }
        }
        let root = self
            .user(ROOT_USER)
            .ok_or_else(|| validation(String::from("the root account is missing")))?;
        if root.uid != 0 || root.gid != 0 {
            return Err(validation(String::from("root must keep uid/gid 0")));
        }
        let mut group_names = std::collections::BTreeSet::new();
        let mut group_ids = std::collections::BTreeSet::new();
        for group in &self.groups {
            if !group_names.insert(group.name.clone()) {
                return Err(validation(format!("duplicate group `{}`", group.name)));
            }
            if !group_ids.insert(group.gid) {
                return Err(validation(format!("duplicate gid {}", group.gid)));
            }
            for member in &group.members {
                if self.user(member).is_none() {
                    return Err(validation(format!(
                        "group `{}` names unknown member `{member}`",
                        group.name
                    )));
                }
            }
        }
        let mut shadow_names = std::collections::BTreeSet::new();
        for shadow in &self.shadows {
            if !shadow_names.insert(shadow.name.clone()) {
                return Err(validation(format!(
                    "duplicate shadow record `{}`",
                    shadow.name
                )));
            }
            if self.user(&shadow.name).is_none() {
                return Err(validation(format!(
                    "shadow record `{}` has no passwd record",
                    shadow.name
                )));
            }
        }
        if self.autologin.enabled && self.user(&self.autologin.user).is_none() {
            return Err(validation(format!(
                "autologin account `{}` does not exist",
                self.autologin.user
            )));
        }
        Ok(())
    }

    fn render_passwd(&self) -> String {
        let mut text = String::new();
        for user in &self.users {
            text.push_str(&user.render());
            text.push('\n');
        }
        text
    }

    fn render_shadow(&self) -> String {
        let mut text = String::new();
        for shadow in &self.shadows {
            text.push_str(&shadow.render());
            text.push('\n');
        }
        text
    }

    fn render_group(&self) -> String {
        let mut text = String::new();
        for group in &self.groups {
            text.push_str(&group.render());
            text.push('\n');
        }
        text
    }
}

/// A requested user edit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserSpec {
    /// Login name.
    pub name: String,
    /// Numeric user id.
    pub uid: u32,
    /// Numeric primary group id.
    pub gid: u32,
    /// Home directory (empty means `/home/<name>`).
    pub home: String,
    /// Login shell.
    pub shell: String,
    /// New password; `None` keeps the current hash (edit only).
    pub password: Option<String>,
    /// Enable console autologin for this account.
    pub autologin: bool,
}

/// One Users-module save.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UserAction {
    /// Create the account.
    Add(UserSpec),
    /// Change an existing account.
    Edit {
        /// Existing login name.
        name: String,
        /// New values.
        spec: UserSpec,
    },
    /// Delete an existing account.
    Delete {
        /// Existing login name.
        name: String,
    },
}

/// A field-level validation failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpecError {
    /// Stable form key.
    pub key: &'static str,
    /// Human message.
    pub message: String,
}

/// Paths touched by a successful save.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApplyOutcome {
    /// `/etc/passwd`.
    pub passwd: PathBuf,
    /// `/etc/shadow`.
    pub shadow: PathBuf,
    /// `/etc/group`.
    pub group: PathBuf,
    /// `/etc/raven/autologin.toml`.
    pub autologin: PathBuf,
    /// Created home, when the action adds or moves one.
    pub home: Option<PathBuf>,
}

/// Validate one action against the loaded accounts and the file system below `root`.
pub fn validate_action(
    root: &Path,
    accounts: &Accounts,
    action: &UserAction,
) -> Result<(), Vec<SpecError>> {
    match action {
        UserAction::Add(spec) => validate_spec(root, accounts, spec, true, None),
        UserAction::Edit { name, spec } => validate_spec(root, accounts, spec, false, Some(name)),
        UserAction::Delete { name } => {
            if name == ROOT_USER {
                return Err(vec![SpecError {
                    key: "delete",
                    message: String::from("the root account cannot be deleted"),
                }]);
            }
            if accounts.user(name).is_none() {
                return Err(vec![SpecError {
                    key: "delete",
                    message: format!("user `{name}` does not exist"),
                }]);
            }
            Ok(())
        }
    }
}

fn validate_spec(
    root: &Path,
    accounts: &Accounts,
    spec: &UserSpec,
    adding: bool,
    editing: Option<&str>,
) -> Result<(), Vec<SpecError>> {
    let mut errors = Vec::new();
    if !valid_name(&spec.name) {
        errors.push(error(
            "username",
            "name must start with a letter or underscore and use letters, digits, . _ - (max 31)",
        ));
    }
    if let Some(existing) = editing {
        if spec.name != existing {
            errors.push(error("username", "the login name cannot be changed"));
        }
        if accounts.user(existing).is_none() {
            errors.push(error("username", "the user does not exist"));
        }
        if existing == ROOT_USER && (spec.uid != 0 || spec.gid != 0 || spec.home != "/root") {
            errors.push(error("uid", "root must keep uid 0, gid 0 and home /root"));
        }
    }
    if adding && accounts.user(&spec.name).is_some() {
        errors.push(error("username", "a user with this name already exists"));
    }
    if spec.name != ROOT_USER {
        if spec.uid == 0 {
            errors.push(error("uid", "uid 0 is reserved for root"));
        }
        if spec.uid < MIN_REGULAR_ID || spec.uid > MAX_REGULAR_ID {
            errors.push(error("uid", "regular uid must be between 1000 and 59999"));
        }
        if accounts
            .users
            .iter()
            .any(|user| user.uid == spec.uid && Some(user.name.as_str()) != editing)
        {
            errors.push(error("uid", "this uid is already in use"));
        }
        if spec.gid == 0 {
            errors.push(error("gid", "gid 0 is reserved for root"));
        }
        if spec.gid < MIN_REGULAR_ID || spec.gid > MAX_REGULAR_ID {
            errors.push(error("gid", "regular gid must be between 1000 and 59999"));
        }
    }
    let home = effective_home(spec);
    if !valid_path(&home) {
        errors.push(error("home", "home must be an absolute path"));
    }
    if !valid_path(&spec.shell) || spec.shell.is_empty() {
        errors.push(error("shell", "shell must be an absolute path"));
    } else if !shell_executable(root, &spec.shell) {
        errors.push(error("shell", "shell is not an executable file"));
    }
    if let Some(password) = &spec.password {
        if password.is_empty() {
            errors.push(error("password", "password must not be empty"));
        } else if password.len() > MAX_PASSWORD {
            errors.push(error("password", "password is too long"));
        } else if password.chars().any(char::is_control) {
            errors.push(error("password", "password contains a control character"));
        }
    } else if adding {
        errors.push(error("password", "a new user needs a password"));
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Apply one action: edit the in-memory database, validate, then replace the files atomically.
pub fn apply(root: &Path, action: &UserAction) -> Result<ApplyOutcome, UserError> {
    let mut accounts = Accounts::load(root)?;
    validate_action(root, &accounts, action).map_err(|errors| {
        validation(
            errors
                .iter()
                .map(|error| error.message.as_str())
                .collect::<Vec<&str>>()
                .join("; "),
        )
    })?;
    let mut home = None;
    match action {
        UserAction::Add(spec) => {
            let user = UserRecord {
                name: spec.name.clone(),
                uid: spec.uid,
                gid: spec.gid,
                gecos: String::new(),
                home: effective_home(spec),
                shell: spec.shell.clone(),
            };
            if !accounts.groups.iter().any(|group| group.gid == user.gid) {
                accounts.groups.push(GroupRecord {
                    name: user.name.clone(),
                    gid: user.gid,
                    members: Vec::new(),
                });
            }
            let password = spec.password.as_deref().unwrap_or_default();
            let hash = hash_password(password)?;
            accounts.users.push(user.clone());
            accounts
                .shadows
                .push(ShadowRecord::hashed(&user.name, &hash));
            if spec.autologin {
                accounts.autologin = Autologin {
                    enabled: true,
                    user: user.name.clone(),
                };
            }
            home = Some(create_home(root, &user)?);
        }
        UserAction::Edit { name, spec } => {
            let index = accounts
                .users
                .iter()
                .position(|user| &user.name == name)
                .ok_or_else(|| validation(format!("user `{name}` does not exist")))?;
            if name == ROOT_USER {
                // Only the password may change; validate_spec already pins uid/gid/home.
                if spec.uid != 0 || spec.gid != 0 || spec.home != "/root" {
                    return Err(validation(String::from("root must keep uid/gid 0")));
                }
            }
            let user = accounts
                .users
                .get_mut(index)
                .ok_or_else(|| validation(String::from("user record vanished")))?;
            if name != ROOT_USER {
                user.uid = spec.uid;
                user.gid = spec.gid;
                user.home = effective_home(spec);
                user.shell = spec.shell.clone();
            }
            if let Some(password) = spec.password.as_deref()
                && !password.is_empty()
            {
                let hash = hash_password(password)?;
                let record = ShadowRecord::hashed(name, &hash);
                if let Some(shadow) = accounts
                    .shadows
                    .iter_mut()
                    .find(|shadow| shadow.name.as_str() == name.as_str())
                {
                    shadow.hash = record.hash;
                    shadow.last_change = record.last_change;
                } else {
                    accounts.shadows.push(record);
                }
            }
            let user = accounts
                .users
                .get(index)
                .cloned()
                .ok_or_else(|| validation(String::from("user record vanished")))?;
            if name != ROOT_USER {
                home = Some(create_home(root, &user)?);
            }
            if spec.autologin {
                accounts.autologin = Autologin {
                    enabled: true,
                    user: name.clone(),
                };
            } else if accounts.is_autologin(name) {
                accounts.autologin = Autologin::default();
            }
        }
        UserAction::Delete { name } => {
            if name == ROOT_USER {
                return Err(validation(String::from(
                    "the root account cannot be deleted",
                )));
            }
            let Some(index) = accounts.users.iter().position(|user| &user.name == name) else {
                return Err(validation(format!("user `{name}` does not exist")));
            };
            let removed = accounts.users.remove(index);
            accounts
                .shadows
                .retain(|shadow| shadow.name != removed.name);
            for group in &mut accounts.groups {
                group.members.retain(|member| member != name);
            }
            let group_used = accounts.users.iter().any(|user| user.gid == removed.gid);
            if !group_used {
                accounts
                    .groups
                    .retain(|group| !(group.gid == removed.gid && group.name == removed.name));
            }
            if accounts.is_autologin(name) {
                accounts.autologin = Autologin::default();
            }
        }
    }
    accounts.validate()?;
    let outcome = write_accounts(root, &accounts)?;
    Ok(ApplyOutcome { home, ..outcome })
}

/// A read-only consistency report for `raven users check`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CheckReport {
    /// Number of passwd records.
    pub users: usize,
    /// Number of group records.
    pub groups: usize,
    /// Problems found; empty means healthy.
    pub errors: Vec<String>,
}

impl CheckReport {
    /// Whether the report is clean.
    #[must_use]
    pub fn ok(&self) -> bool {
        self.errors.is_empty()
    }
}

/// Validate the account files under `root`, including file modes, homes and shell executability.
pub fn check(root: &Path) -> Result<CheckReport, UserError> {
    let accounts = Accounts::load(root)?;
    let mut report = CheckReport {
        users: accounts.users.len(),
        groups: accounts.groups.len(),
        errors: Vec::new(),
    };
    if let Err(error) = accounts.validate() {
        report.errors.push(error.to_string());
    }
    for (file, expected) in [
        (PASSWD_FILE, 0o644),
        (GROUP_FILE, 0o644),
        (SHADOW_FILE, 0o600),
    ] {
        let path = root.join(file);
        match fs::metadata(&path) {
            Ok(metadata) => {
                let mode = metadata.permissions().mode() & 0o777;
                if mode != expected {
                    report
                        .errors
                        .push(format!("{file}: mode {mode:o}, expected {expected:o}"));
                }
            }
            Err(error) => report.errors.push(format!("{file}: cannot stat: {error}")),
        }
    }
    for user in &accounts.users {
        if user.shell.is_empty() {
            report.errors.push(format!("{}: empty shell", user.name));
        } else if !shell_executable(root, &user.shell) {
            report.errors.push(format!(
                "{}: shell {} is not executable",
                user.name, user.shell
            ));
        }
        if !user.home.is_empty() && !root.join(trim_root(&user.home)).is_dir() {
            report
                .errors
                .push(format!("{}: home {} is missing", user.name, user.home));
        }
        if accounts.shadow(&user.name).is_none() {
            report
                .errors
                .push(format!("{}: no shadow record", user.name));
        }
    }
    if accounts.autologin.enabled
        && !accounts.autologin.user.is_empty()
        && accounts.user(&accounts.autologin.user).is_none()
    {
        report.errors.push(format!(
            "autologin: user `{}` does not exist",
            accounts.autologin.user
        ));
    }
    Ok(report)
}

/// Render a dry-run plan for an action. The password is described, never printed.
#[must_use]
pub fn render_plan(action: &UserAction) -> String {
    let mut text = String::from("# raven config users (dry-run; nothing written)\n");
    match action {
        UserAction::Add(spec) => {
            text.push_str(&format!("action = \"add\"\nname = \"{}\"\n", spec.name));
            text.push_str(&format!("uid = {}\ngid = {}\n", spec.uid, spec.gid));
            text.push_str(&format!(
                "home = \"{}\"\nshell = \"{}\"\n",
                effective_home(spec),
                spec.shell
            ));
            text.push_str("password = \"<set>\"\n");
            if spec.autologin {
                text.push_str("autologin = true\n");
            }
        }
        UserAction::Edit { name, spec } => {
            text.push_str(&format!("action = \"edit\"\nname = \"{name}\"\n"));
            text.push_str(&format!("uid = {}\ngid = {}\n", spec.uid, spec.gid));
            text.push_str(&format!(
                "home = \"{}\"\nshell = \"{}\"\n",
                effective_home(spec),
                spec.shell
            ));
            text.push_str(if spec.password.is_some() {
                "password = \"<set>\"\n"
            } else {
                "password = \"<unchanged>\"\n"
            });
            text.push_str(&format!("autologin = {}\n", spec.autologin));
        }
        UserAction::Delete { name } => {
            text.push_str(&format!("action = \"delete\"\nname = \"{name}\"\n"));
        }
    }
    text
}

/// Render the autologin drop-in.
#[must_use]
pub fn render_autologin(autologin: &Autologin) -> String {
    format!(
        "# Managed by raven config (K229).\n[console]\nuser = \"{}\"\nenabled = {}\n",
        autologin.user,
        if autologin.enabled { "true" } else { "false" }
    )
}

/// Parse the autologin drop-in. Comments, sections and unknown keys are ignored; a malformed
/// file disables the feature instead of crashing the console.
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
                autologin.user = value
                    .strip_prefix('"')
                    .and_then(|value| value.strip_suffix('"'))
                    .unwrap_or(value)
                    .to_string();
            }
            "enabled" => autologin.enabled = value == "true",
            _ => {}
        }
    }
    if !valid_name(&autologin.user) {
        autologin.user.clear();
        autologin.enabled = false;
    }
    autologin
}

fn write_accounts(root: &Path, accounts: &Accounts) -> Result<ApplyOutcome, UserError> {
    let passwd_path = root.join(PASSWD_FILE);
    let shadow_path = root.join(SHADOW_FILE);
    let group_path = root.join(GROUP_FILE);
    let passwd_old = read_bytes(&passwd_path)?;
    let shadow_old = read_bytes(&shadow_path)?;
    let passwd = accounts.render_passwd().into_bytes();
    let shadow = accounts.render_shadow().into_bytes();
    let group = accounts.render_group().into_bytes();
    let lock = AccountLock::acquire(&root.join(LOCK_FILE))?;
    let result = (|| {
        stage(&passwd_path, &passwd, 0o644)?;
        stage(&shadow_path, &shadow, 0o600)?;
        stage(&group_path, &group, 0o644)?;
        if let Err(error) = fs::rename(temp_path(&passwd_path), &passwd_path) {
            cleanup(&[&passwd_path, &shadow_path, &group_path]);
            return Err(io_error(&passwd_path, error));
        }
        if let Err(error) = fs::rename(temp_path(&shadow_path), &shadow_path) {
            restore(&passwd_path, &passwd_old, 0o644);
            cleanup(&[&passwd_path, &shadow_path, &group_path]);
            return Err(io_error(&shadow_path, error));
        }
        if let Err(error) = fs::rename(temp_path(&group_path), &group_path) {
            restore(&passwd_path, &passwd_old, 0o644);
            restore(&shadow_path, &shadow_old, 0o600);
            cleanup(&[&passwd_path, &shadow_path, &group_path]);
            return Err(io_error(&group_path, error));
        }
        let autologin_path = root.join(AUTOLOGIN_FILE);
        write_autologin(&autologin_path, &accounts.autologin)?;
        Ok(ApplyOutcome {
            passwd: passwd_path.clone(),
            shadow: shadow_path.clone(),
            group: group_path.clone(),
            autologin: autologin_path,
            home: None,
        })
    })();
    drop(lock);
    result
}

fn write_autologin(path: &Path, autologin: &Autologin) -> Result<(), UserError> {
    if let Some(directory) = path.parent() {
        fs::create_dir_all(directory).map_err(|error| io_error(directory, error))?;
    }
    let temp = temp_path(path);
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o644)
        .open(&temp)
        .map_err(|error| io_error(&temp, error))?;
    file.write_all(render_autologin(autologin).as_bytes())
        .map_err(|error| io_error(&temp, error))?;
    file.sync_all().map_err(|error| io_error(&temp, error))?;
    drop(file);
    fs::rename(&temp, path).map_err(|error| {
        let _ = fs::remove_file(&temp);
        io_error(path, error)
    })?;
    Ok(())
}

fn create_home(root: &Path, user: &UserRecord) -> Result<PathBuf, UserError> {
    if user.home.is_empty() {
        return Ok(PathBuf::new());
    }
    let path = root.join(trim_root(&user.home));
    fs::create_dir_all(&path).map_err(|error| io_error(&path, error))?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))
        .map_err(|error| io_error(&path, error))?;
    match chown(&path, Some(user.uid), Some(user.gid)) {
        Ok(()) => {}
        // The release/UFS admin runs as root. A host test or a non-root preview root cannot
        // reassign ownership; keep the caller's owner and leave the mode correct.
        Err(error) if error.kind() == io::ErrorKind::PermissionDenied => {}
        Err(error) => {
            return Err(UserError::Io(format!(
                "cannot chown {} to {}:{}: {error}",
                path.display(),
                user.uid,
                user.gid
            )));
        }
    }
    Ok(path)
}

fn stage(path: &Path, data: &[u8], mode: u32) -> Result<(), UserError> {
    let temp = temp_path(path);
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(mode)
        .open(&temp)
        .map_err(|error| io_error(&temp, error))?;
    file.set_permissions(fs::Permissions::from_mode(mode))
        .map_err(|error| io_error(&temp, error))?;
    file.write_all(data)
        .map_err(|error| io_error(&temp, error))?;
    file.sync_all().map_err(|error| io_error(&temp, error))?;
    Ok(())
}

fn restore(path: &Path, data: &[u8], mode: u32) {
    let temp = temp_path(path);
    if let Ok(mut file) = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(mode)
        .open(&temp)
        && file.write_all(data).is_ok()
    {
        let _ = fs::set_permissions(&temp, fs::Permissions::from_mode(mode));
        let _ = file.sync_all();
        drop(file);
        let _ = fs::rename(&temp, path);
    }
}

fn cleanup(paths: &[&Path]) {
    for path in paths {
        let _ = fs::remove_file(temp_path(path));
    }
}

fn hash_password(password: &str) -> Result<String, UserError> {
    let mut random = [0u8; 16];
    File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(&mut random))
        .map_err(|error| UserError::Io(format!("cannot read /dev/urandom: {error}")))?;
    const ALPHABET: &[u8] = b"./0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
    let salt: Vec<u8> = random
        .iter()
        .map(|byte| ALPHABET[usize::from(*byte) % ALPHABET.len()])
        .collect();
    raven_auth::hash_password(password, &salt)
        .map_err(|error| UserError::Validation(format!("cannot hash the password: {error}")))
}

fn shell_executable(root: &Path, shell: &str) -> bool {
    if !valid_path(shell) || shell.is_empty() {
        return false;
    }
    let path = root.join(trim_root(shell));
    match fs::metadata(&path) {
        Ok(metadata) => metadata.is_file() && metadata.permissions().mode() & 0o111 != 0,
        Err(_) => false,
    }
}

fn effective_home(spec: &UserSpec) -> String {
    if spec.home.is_empty() {
        format!("/home/{}", spec.name)
    } else {
        spec.home.clone()
    }
}

fn today() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| (duration.as_secs() / 86_400) as i64)
        .unwrap_or(0)
}

fn read_required(root: &Path, relative: &str) -> Result<String, UserError> {
    let path = root.join(relative);
    match fs::read_to_string(&path) {
        Ok(text) => Ok(text),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            Err(UserError::Missing(path.display().to_string()))
        }
        Err(error) => Err(io_error(&path, error)),
    }
}

fn read_bytes(path: &Path) -> Result<Vec<u8>, UserError> {
    fs::read(path).map_err(|error| io_error(path, error))
}

fn io_error(path: &Path, error: io::Error) -> UserError {
    UserError::Io(format!("{}: {error}", path.display()))
}

fn parse_error(file: &str, line: usize, message: &str) -> UserError {
    UserError::Parse {
        file: file.to_string(),
        line,
        message: message.to_string(),
    }
}

fn validation(message: String) -> UserError {
    UserError::Validation(message)
}

fn error(key: &'static str, message: &str) -> SpecError {
    SpecError {
        key,
        message: message.to_string(),
    }
}

fn parse_id(field: Option<&str>, file: &str, line: usize, name: &str) -> Result<u32, UserError> {
    field
        .unwrap_or_default()
        .parse::<u32>()
        .map_err(|_| parse_error(file, line, &format!("invalid {name}")))
}

fn valid_name(name: &str) -> bool {
    if name.is_empty() || name.len() > MAX_NAME {
        return false;
    }
    let mut bytes = name.bytes();
    match bytes.next() {
        Some(first) if first.is_ascii_alphabetic() || first == b'_' => {}
        _ => return false,
    }
    bytes.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-'))
}

fn valid_path(path: &str) -> bool {
    if path.is_empty() {
        return true;
    }
    path.starts_with('/')
        && clean_field(path)
        && !path
            .split('/')
            .any(|component| component == ".." || component == ".")
}

fn clean_field(text: &str) -> bool {
    !text
        .bytes()
        .any(|byte| matches!(byte, b':' | b'\0' | b'\n' | b'\r'))
}

fn valid_hash(hash: &str) -> bool {
    if hash.is_empty() || hash.starts_with('*') {
        return true;
    }
    if let Some(rest) = hash.strip_prefix('!') {
        return rest.is_empty() || rest.starts_with("$6$");
    }
    hash.starts_with("$6$") && clean_field(hash)
}

fn next_id(ids: impl Iterator<Item = u32>) -> u32 {
    let highest = ids
        .filter(|id| (MIN_REGULAR_ID..=MAX_REGULAR_ID).contains(id))
        .max();
    highest.map_or(MIN_REGULAR_ID, |id| id.saturating_add(1))
}

fn trim_root(path: &str) -> &str {
    path.strip_prefix('/').unwrap_or(path)
}

fn temp_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(".tmp");
    PathBuf::from(name)
}

struct AccountLock {
    path: PathBuf,
}

impl AccountLock {
    fn acquire(path: &Path) -> Result<Self, UserError> {
        match OpenOptions::new().write(true).create_new(true).open(path) {
            Ok(mut file) => {
                let _ = writeln!(file, "{}", std::process::id());
                Ok(Self {
                    path: path.to_path_buf(),
                })
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Err(UserError::Busy),
            Err(error) => Err(io_error(path, error)),
        }
    }
}

impl Drop for AccountLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[cfg(test)]
#[path = "users_ut.rs"]
mod tests;

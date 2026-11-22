// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Data-driven module registry, network form model and atomic drop-in writer.
//!
//! The network schema is the K198 contract:
//! `[network] hostname/mode` plus `[network.static] address/prefix/gateway/dns`. K198 owns the
//! backend apply; this module validates the form and writes the dedicated drop-in.

use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use raven_config::users::{Accounts, UserAction, UserSpec, apply, render_plan, validate_action};

/// The dedicated network drop-in, relative to the configuration root.
pub const NETWORK_FILE: &str = "etc/raven/conf.d/network.toml";
/// `/etc/passwd`, relative to the configuration root (the Users dry-run target).
pub const PASSWD_FILE: &str = "etc/passwd";
/// Allowed network modes.
pub const MODE_CHOICES: &[&str] = &["dhcp", "static"];
/// Allowed yes/no choices.
pub const YES_NO: &[&str] = &["no", "yes"];

/// The kind of value a form field holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldKind {
    /// Free text.
    Text,
    /// A secret; the display is masked and the value is never printed.
    Secret,
    /// One of a fixed choice list.
    Choice(&'static [&'static str]),
    /// Read-only information.
    Info,
}

/// One form field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    /// Stable key used by the writer and tests.
    pub key: &'static str,
    /// Display label.
    pub label: &'static str,
    /// One-line help.
    pub help: &'static str,
    /// Field kind.
    pub kind: FieldKind,
    /// Value loaded from the current drop-in (or a default).
    pub current: String,
    /// Edit value; empty means "use current".
    pub value: String,
    /// Validation error for this field.
    pub error: Option<String>,
}

impl Field {
    fn new(
        key: &'static str,
        label: &'static str,
        help: &'static str,
        kind: FieldKind,
        current: String,
    ) -> Self {
        Self {
            key,
            label,
            help,
            kind,
            current,
            value: String::new(),
            error: None,
        }
    }

    /// The value the form would submit: the edit value when non-empty, otherwise the current
    /// value.
    #[must_use]
    pub fn effective(&self) -> String {
        if self.value.is_empty() {
            self.current.clone()
        } else {
            self.value.clone()
        }
    }

    /// Display value (the effective value), masked for secrets.
    #[must_use]
    pub fn display(&self) -> String {
        if self.kind == FieldKind::Secret {
            return if self.value.is_empty() {
                String::from("(not set)")
            } else {
                String::from("********")
            };
        }
        self.effective()
    }
}

/// What a module edits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModuleKind {
    /// The K198 network drop-in.
    Network,
    /// A new Users account.
    UsersAdd,
    /// An existing Users account.
    UsersEdit,
    /// Read-only information.
    Info,
}

/// One module in the left pane.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Module {
    /// Stable identifier.
    pub id: String,
    /// Left-pane title.
    pub title: String,
    /// One-line summary.
    pub summary: String,
    /// Form fields.
    pub fields: Vec<Field>,
    /// What the Save action edits.
    pub kind: ModuleKind,
    /// Existing login name for a Users edit.
    pub user: Option<String>,
    /// Whether the Save action writes something.
    pub writable: bool,
}

/// Current network values read from the drop-in (or defaults).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetworkCurrent {
    /// Hostname.
    pub hostname: String,
    /// `dhcp` or `static`.
    pub mode: String,
    /// Static address.
    pub address: String,
    /// Static prefix length.
    pub prefix: String,
    /// Static gateway.
    pub gateway: String,
    /// DNS servers.
    pub dns: Vec<String>,
}

impl Default for NetworkCurrent {
    fn default() -> Self {
        Self {
            hostname: String::from("raven"),
            mode: String::from("dhcp"),
            address: String::new(),
            prefix: String::from("24"),
            gateway: String::new(),
            dns: Vec::new(),
        }
    }
}

/// A validated network plan.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetworkPlan {
    /// Hostname.
    pub hostname: String,
    /// `dhcp` or `static`.
    pub mode: String,
    /// Static address (empty for DHCP).
    pub address: String,
    /// Static prefix length (0 for DHCP).
    pub prefix: u8,
    /// Static gateway (empty for DHCP).
    pub gateway: String,
    /// DNS servers (empty for DHCP unless supplied).
    pub dns: Vec<String>,
}

impl NetworkPlan {
    /// Render the K198 drop-in.
    #[must_use]
    pub fn to_toml(&self) -> String {
        let mut text = String::from("# Managed by raven config (K197).\n[network]\n");
        text.push_str(&format!("hostname = \"{}\"\n", toml_escape(&self.hostname)));
        text.push_str(&format!("mode = \"{}\"\n", toml_escape(&self.mode)));
        if self.mode == "static" {
            text.push_str("[network.static]\n");
            text.push_str(&format!("address = \"{}\"\n", toml_escape(&self.address)));
            text.push_str(&format!("prefix = {}\n", self.prefix));
            text.push_str(&format!("gateway = \"{}\"\n", toml_escape(&self.gateway)));
            text.push_str("dns = [");
            for (index, server) in self.dns.iter().enumerate() {
                if index > 0 {
                    text.push_str(", ");
                }
                text.push_str(&format!("\"{}\"", toml_escape(server)));
            }
            text.push_str("]\n");
        }
        text
    }
}

/// One field-level validation error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidationError {
    /// Field key.
    pub key: &'static str,
    /// Human message.
    pub message: String,
}

/// The values the registry is built from.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Current {
    /// Network drop-in values.
    pub network: NetworkCurrent,
    /// Files-backed accounts.
    pub accounts: Accounts,
}

/// Build the module registry: Network, Users, one entry per account, then the read-only modules.
#[must_use]
pub fn modules(current: &Current) -> Vec<Module> {
    let mut modules = base_modules(&current.network);
    modules.insert(1, users_add_module(&current.accounts));
    for (offset, user) in current.accounts.users.iter().enumerate() {
        modules.insert(2 + offset, users_edit_module(&current.accounts, user));
    }
    modules
}

fn users_add_module(accounts: &Accounts) -> Module {
    Module {
        id: String::from("users-add"),
        title: String::from("Users (add)"),
        summary: String::from("Add a user: name, uid/gid, home, shell, password, autologin"),
        fields: vec![
            Field::new(
                "username",
                "Username",
                "New login name; letters, digits, . _ -",
                FieldKind::Text,
                String::new(),
            ),
            Field::new(
                "uid",
                "UID",
                "Numeric user id; the next regular id by default",
                FieldKind::Text,
                accounts.next_uid().to_string(),
            ),
            Field::new(
                "gid",
                "GID",
                "Numeric primary group; a same-name group is created",
                FieldKind::Text,
                accounts.next_gid().to_string(),
            ),
            Field::new(
                "home",
                "Home",
                "Empty uses /home/<name>; created on save",
                FieldKind::Text,
                String::new(),
            ),
            Field::new(
                "shell",
                "Shell",
                "Absolute executable path; a TUI app is a valid shell",
                FieldKind::Text,
                String::from(raven_config::users::DEFAULT_SHELL),
            ),
            Field::new(
                "password",
                "Password",
                "Hashed as SHA-512 crypt $6$; never stored plaintext",
                FieldKind::Secret,
                String::new(),
            ),
            Field::new(
                "confirm",
                "Confirm",
                "Repeat the password",
                FieldKind::Secret,
                String::new(),
            ),
            Field::new(
                "autologin",
                "Autologin",
                "Start this account on the console without a password",
                FieldKind::Choice(YES_NO),
                String::from("no"),
            ),
        ],
        kind: ModuleKind::UsersAdd,
        user: None,
        writable: true,
    }
}

fn users_edit_module(accounts: &Accounts, user: &raven_config::users::UserRecord) -> Module {
    let root = user.name == raven_config::users::ROOT_USER;
    let text = |key: &'static str,
                label: &'static str,
                help: &'static str,
                value: String,
                kind: FieldKind| { Field::new(key, label, help, kind, value) };
    let field_kind = || {
        if root {
            FieldKind::Info
        } else {
            FieldKind::Text
        }
    };
    Module {
        id: format!("users:{}", user.name),
        title: format!("Users: {}", user.name),
        summary: format!("uid {} gid {} shell {}", user.uid, user.gid, user.shell),
        fields: vec![
            text(
                "username",
                "Username",
                "The login name cannot be changed",
                user.name.clone(),
                FieldKind::Info,
            ),
            text(
                "uid",
                "UID",
                "Root keeps uid 0",
                user.uid.to_string(),
                field_kind(),
            ),
            text(
                "gid",
                "GID",
                "Root keeps gid 0",
                user.gid.to_string(),
                field_kind(),
            ),
            text(
                "home",
                "Home",
                "Created on save when it is missing",
                user.home.clone(),
                field_kind(),
            ),
            text(
                "shell",
                "Shell",
                "Absolute executable path",
                user.shell.clone(),
                field_kind(),
            ),
            text(
                "password",
                "Password",
                "Empty keeps the current hash; otherwise set a new one",
                String::new(),
                FieldKind::Secret,
            ),
            text(
                "confirm",
                "Confirm",
                "Repeat a new password",
                String::new(),
                FieldKind::Secret,
            ),
            text(
                "autologin",
                "Autologin",
                "Console autologin; one account at a time",
                if accounts.is_autologin(&user.name) {
                    String::from("yes")
                } else {
                    String::from("no")
                },
                if root {
                    FieldKind::Info
                } else {
                    FieldKind::Choice(YES_NO)
                },
            ),
            text(
                "delete",
                "Delete",
                "Delete this account, its shadow record and group",
                String::from("no"),
                if root {
                    FieldKind::Info
                } else {
                    FieldKind::Choice(YES_NO)
                },
            ),
        ],
        kind: ModuleKind::UsersEdit,
        user: Some(user.name.clone()),
        writable: true,
    }
}

/// Build the network and read-only part of the registry.
#[must_use]
fn base_modules(current: &NetworkCurrent) -> Vec<Module> {
    let mode = if MODE_CHOICES.contains(&current.mode.as_str()) {
        current.mode.clone()
    } else {
        String::from("dhcp")
    };
    let network = Module {
        id: String::from("network"),
        title: String::from("Network"),
        summary: String::from("Hostname, DHCP or static IPv4 address"),
        kind: ModuleKind::Network,
        user: None,
        fields: vec![
            Field::new(
                "hostname",
                "Hostname",
                "Name applied at boot; K198/netd owns the apply",
                FieldKind::Text,
                current.hostname.clone(),
            ),
            Field::new(
                "mode",
                "Mode",
                "DHCP or a static address (left/right toggles)",
                FieldKind::Choice(MODE_CHOICES),
                mode,
            ),
            Field::new(
                "address",
                "Address",
                "Static IPv4 address (ignored for DHCP)",
                FieldKind::Text,
                current.address.clone(),
            ),
            Field::new(
                "prefix",
                "Prefix",
                "Static prefix length 1-32 (ignored for DHCP)",
                FieldKind::Text,
                current.prefix.clone(),
            ),
            Field::new(
                "gateway",
                "Gateway",
                "Static IPv4 gateway (ignored for DHCP)",
                FieldKind::Text,
                current.gateway.clone(),
            ),
            Field::new(
                "dns",
                "DNS",
                "Comma-separated IPv4 servers; K198 resolvd consumes it",
                FieldKind::Text,
                current.dns.join(","),
            ),
        ],
        writable: true,
    };
    vec![
        network,
        Module {
            id: String::from("services"),
            title: String::from("Services"),
            summary: String::from("Service enablement (read-only; K198/K76 own apply)"),
            kind: ModuleKind::Info,
            user: None,
            fields: vec![
                Field::new(
                    "core",
                    "Core services",
                    "Effective status core list",
                    FieldKind::Info,
                    String::from("logd, consd, vfsd"),
                ),
                Field::new(
                    "enable_file",
                    "Enable file",
                    "Service enablement drop-in",
                    FieldKind::Info,
                    String::from("/etc/raven/services.enable"),
                ),
            ],
            writable: false,
        },
        Module {
            id: String::from("logging"),
            title: String::from("Logging"),
            summary: String::from("Log level and destination (read-only; K76 owns apply)"),
            kind: ModuleKind::Info,
            user: None,
            fields: vec![
                Field::new(
                    "level",
                    "Level",
                    "Default log level",
                    FieldKind::Info,
                    String::from("info"),
                ),
                Field::new(
                    "destination",
                    "Destination",
                    "Console/file selection",
                    FieldKind::Info,
                    String::from("console"),
                ),
            ],
            writable: false,
        },
        Module {
            id: String::from("theme"),
            title: String::from("Theme"),
            summary: String::from("Colour policy and prompt (read-only)"),
            kind: ModuleKind::Info,
            user: None,
            fields: vec![
                Field::new(
                    "color",
                    "Colour",
                    "auto, always or never",
                    FieldKind::Info,
                    String::from("auto"),
                ),
                Field::new(
                    "prompt",
                    "Prompt",
                    "Shell prompt",
                    FieldKind::Info,
                    String::from("raven#"),
                ),
            ],
            writable: false,
        },
        Module {
            id: String::from("about"),
            title: String::from("About"),
            summary: String::from("raven config version and layout"),
            kind: ModuleKind::Info,
            user: None,
            fields: vec![
                Field::new(
                    "version",
                    "Version",
                    "Client version",
                    FieldKind::Info,
                    String::from(env!("CARGO_PKG_VERSION")),
                ),
                Field::new(
                    "layout",
                    "Layout",
                    "Two-pane searchable module list and form",
                    FieldKind::Info,
                    String::from("Crush-style"),
                ),
            ],
            writable: false,
        },
    ]
}

/// Path of the network drop-in for `root`.
#[must_use]
pub fn network_path(root: &Path) -> PathBuf {
    root.join(NETWORK_FILE)
}

/// Read the current network drop-in. A malformed or unreadable file returns the defaults plus a
/// warning; it never prevents the TUI from opening.
#[must_use]
pub fn load_network(root: &Path) -> (NetworkCurrent, Option<String>) {
    let path = network_path(root);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return (NetworkCurrent::default(), None);
        }
        Err(error) => {
            return (
                NetworkCurrent::default(),
                Some(format!("cannot read {}: {error}", path.display())),
            );
        }
    };
    match parse_current(&text) {
        Ok(current) => (current, None),
        Err(error) => (
            NetworkCurrent::default(),
            Some(format!("{}: {error}", path.display())),
        ),
    }
}

fn parse_current(text: &str) -> Result<NetworkCurrent, String> {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Section {
        None,
        Network,
        Static,
    }
    let mut current = NetworkCurrent::default();
    let mut section = Section::None;
    for (number, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            section = match line {
                "[network]" => Section::Network,
                "[network.static]" => Section::Static,
                _ => return Err(format!("line {}: unsupported section `{line}`", number + 1)),
            };
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            return Err(format!("line {}: expected `key = value`", number + 1));
        };
        let key = key.trim();
        let value = value.trim();
        match (section, key) {
            (Section::Network, "hostname") => current.hostname = parse_string(value)?,
            (Section::Network, "mode") => current.mode = parse_string(value)?,
            (Section::Static, "address") => current.address = parse_string(value)?,
            (Section::Static, "prefix") => {
                current.prefix = value.to_string();
            }
            (Section::Static, "gateway") => current.gateway = parse_string(value)?,
            (Section::Static, "dns") => current.dns = parse_array(value)?,
            _ => {}
        }
    }
    Ok(current)
}

fn parse_string(value: &str) -> Result<String, String> {
    let Some(inner) = value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
    else {
        return Err(format!("expected a quoted string, got `{value}`"));
    };
    let mut output = String::with_capacity(inner.len());
    let mut escaped = false;
    for character in inner.chars() {
        if escaped {
            output.push(match character {
                '"' => '"',
                '\\' => '\\',
                'n' => '\n',
                other => other,
            });
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else {
            output.push(character);
        }
    }
    if escaped {
        return Err(String::from("dangling backslash in string"));
    }
    Ok(output)
}

fn parse_array(value: &str) -> Result<Vec<String>, String> {
    let Some(inner) = value
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
    else {
        return Err(format!("expected an array, got `{value}`"));
    };
    let mut values = Vec::new();
    for item in inner.split(',') {
        let item = item.trim();
        if item.is_empty() {
            continue;
        }
        values.push(parse_string(item)?);
    }
    Ok(values)
}

/// Validate the form fields and build a plan, or return every field error.
pub fn plan_from_fields(fields: &[Field]) -> Result<NetworkPlan, Vec<ValidationError>> {
    let mut errors = Vec::new();
    let field = |key: &'static str| fields.iter().find(|field| field.key == key);
    let hostname = field("hostname").map_or(String::new(), Field::effective);
    let hostname = hostname.trim();
    if !valid_hostname(hostname) {
        errors.push(ValidationError {
            key: "hostname",
            message: String::from(
                "hostname must be 1-253 characters of letters, digits, dots and hyphens",
            ),
        });
    }
    let mode = field("mode").map_or(String::from("dhcp"), Field::effective);
    let mode = mode.trim().to_ascii_lowercase();
    if !MODE_CHOICES.contains(&mode.as_str()) {
        errors.push(ValidationError {
            key: "mode",
            message: String::from("mode must be dhcp or static"),
        });
    }
    let mut address = String::new();
    let mut prefix = 0u8;
    let mut gateway = String::new();
    let mut dns = Vec::new();
    if mode == "static" {
        let address_value = field("address").map_or(String::new(), Field::effective);
        let address_value = address_value.trim();
        if !valid_ipv4(address_value) || address_value == "0.0.0.0" {
            errors.push(ValidationError {
                key: "address",
                message: String::from("static address must be a non-zero IPv4 address"),
            });
        } else {
            address = address_value.to_string();
        }
        let prefix_value = field("prefix").map_or(String::new(), Field::effective);
        match prefix_value.trim().parse::<u8>() {
            Ok(value) if (1..=32).contains(&value) => prefix = value,
            _ => errors.push(ValidationError {
                key: "prefix",
                message: String::from("prefix must be 1-32"),
            }),
        }
        let gateway_value = field("gateway").map_or(String::new(), Field::effective);
        let gateway_value = gateway_value.trim();
        if !valid_ipv4(gateway_value) || gateway_value == "0.0.0.0" {
            errors.push(ValidationError {
                key: "gateway",
                message: String::from("static gateway must be a non-zero IPv4 address"),
            });
        } else {
            gateway = gateway_value.to_string();
        }
        let dns_value = field("dns").map_or(String::new(), Field::effective);
        for server in dns_value
            .split([',', ' ', '\t'])
            .filter(|item| !item.is_empty())
        {
            if !valid_ipv4(server) {
                errors.push(ValidationError {
                    key: "dns",
                    message: format!("`{server}` is not an IPv4 address"),
                });
            } else {
                dns.push(server.to_string());
            }
        }
    }
    if errors.is_empty() {
        Ok(NetworkPlan {
            hostname: hostname.to_string(),
            mode,
            address,
            prefix,
            gateway,
            dns,
        })
    } else {
        Err(errors)
    }
}

/// Load the account files for the Users modules. A read failure is a status warning; the
/// registry still opens so the operator can see why.
#[must_use]
pub fn load_accounts(root: &Path) -> (Accounts, Option<String>) {
    match Accounts::load(root) {
        Ok(accounts) => (accounts, None),
        Err(error) => (Accounts::default(), Some(format!("users: {error}"))),
    }
}

/// Parse a Users form into an action and validate it against the loaded accounts.
pub fn plan_user(
    root: &Path,
    module: &Module,
    fields: &[Field],
    accounts: &Accounts,
) -> Result<UserAction, Vec<ValidationError>> {
    let field = |key: &'static str| fields.iter().find(|field| field.key == key);
    let mut errors = Vec::new();
    let name = if module.kind == ModuleKind::UsersAdd {
        field("username")
            .map_or(String::new(), Field::effective)
            .trim()
            .to_string()
    } else {
        module.user.clone().unwrap_or_default()
    };
    let uid = parse_form_id(field("uid"), "uid", &mut errors);
    let gid = parse_form_id(field("gid"), "gid", &mut errors);
    let home = field("home").map_or(String::new(), Field::effective);
    let home = home.trim().to_string();
    let shell = field("shell").map_or(String::new(), Field::effective);
    let shell = shell.trim().to_string();
    let autologin = field("autologin").is_some_and(|field| field.effective() == "yes");
    let password = field("password").map_or(String::new(), Field::effective);
    let confirm = field("confirm").map_or(String::new(), Field::effective);
    let adding = module.kind == ModuleKind::UsersAdd;
    let password = if password.is_empty() && confirm.is_empty() {
        if adding {
            errors.push(ValidationError {
                key: "password",
                message: String::from("a new user needs a password"),
            });
        }
        None
    } else if password.is_empty() {
        errors.push(ValidationError {
            key: "password",
            message: String::from("password must not be empty"),
        });
        None
    } else if password != confirm {
        errors.push(ValidationError {
            key: "confirm",
            message: String::from("passwords do not match"),
        });
        None
    } else {
        Some(password)
    };
    if !errors.is_empty() {
        return Err(errors);
    }
    if !adding && field("delete").is_some_and(|field| field.effective() == "yes") {
        let action = UserAction::Delete { name };
        return validate_action(root, accounts, &action)
            .map(|()| action)
            .map_err(convert_spec_errors);
    }
    let spec = UserSpec {
        name: name.clone(),
        uid,
        gid,
        home,
        shell,
        password,
        autologin,
    };
    let action = if adding {
        UserAction::Add(spec)
    } else {
        UserAction::Edit { name, spec }
    };
    validate_action(root, accounts, &action)
        .map(|()| action)
        .map_err(convert_spec_errors)
}

fn parse_form_id(
    field: Option<&Field>,
    key: &'static str,
    errors: &mut Vec<ValidationError>,
) -> u32 {
    let text = field.map_or(String::new(), Field::effective);
    match text.trim().parse::<u32>() {
        Ok(value) => value,
        Err(_) => {
            errors.push(ValidationError {
                key,
                message: format!("{key} must be a number"),
            });
            0
        }
    }
}

fn convert_spec_errors(errors: Vec<raven_config::users::SpecError>) -> Vec<ValidationError> {
    errors
        .into_iter()
        .map(|error| ValidationError {
            key: error.key,
            message: error.message,
        })
        .collect()
}

/// Apply a validated Users action.
pub fn apply_user(
    root: &Path,
    action: &UserAction,
) -> Result<raven_config::users::ApplyOutcome, String> {
    apply(root, action).map_err(|error| error.to_string())
}

/// Render the Users dry-run plan (no secrets).
#[must_use]
pub fn user_plan(action: &UserAction) -> String {
    render_plan(action)
}

/// Write the plan atomically to the dedicated drop-in. The temporary file is in the same
/// directory, synced, chmod 0644 and renamed; no other configuration file is touched.
pub fn write_atomic(root: &Path, plan: &NetworkPlan) -> io::Result<PathBuf> {
    let path = network_path(root);
    let Some(directory) = path.parent() else {
        return Err(io::Error::other("network path has no parent"));
    };
    fs::create_dir_all(directory)?;
    let temporary = directory.join(format!(".network.toml.{}.tmp", std::process::id()));
    let result = (|| -> io::Result<()> {
        let mut file = fs::File::create(&temporary)?;
        file.write_all(plan.to_toml().as_bytes())?;
        file.sync_all()?;
        drop(file);
        fs::set_permissions(&temporary, fs::Permissions::from_mode(0o644))?;
        fs::rename(&temporary, &path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.map(|()| path)
}

fn valid_hostname(value: &str) -> bool {
    if value.is_empty() || value.len() > 253 {
        return false;
    }
    value.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '-')
    })
}

fn valid_ipv4(value: &str) -> bool {
    let parts: Vec<&str> = value.split('.').collect();
    parts.len() == 4
        && parts.iter().all(|part| {
            !part.is_empty()
                && part.len() <= 3
                && part.chars().all(|character| character.is_ascii_digit())
                && part.parse::<u8>().is_ok()
        })
}

fn toml_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
#[path = "model_ut.rs"]
mod tests;

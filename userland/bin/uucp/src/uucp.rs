// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `uucp` argument parsing and the bounded local/queue operations.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::config::Config;
use crate::job::{JobKind, NamedFile};
use crate::queue::{self, QueueError};
use crate::spool::{Spool, SpoolError};

/// Parsed `uucp` options.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options {
    /// `-c`/`-C`: spool a copy before transfer. The queue always embeds the payload.
    pub copy_spool: bool,
    /// `-d`: create missing local destination directories.
    pub create_dirs: bool,
    /// `-j`: print queued job ids.
    pub print_id: bool,
    /// `-m`: mail completion. Accepted but no mail transport exists.
    pub mail: bool,
    /// Grade byte, default `n`.
    pub grade: u8,
    /// `-n user`: requesting user.
    pub user: Option<String>,
    /// `-x debug`: parsed debug level, not interpreted.
    pub debug: Option<String>,
    /// Source operands followed by the destination.
    pub operands: Vec<String>,
}

/// Why a `uucp` command line is invalid.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UsageError {
    /// An option is unknown.
    UnknownOption(char),
    /// An option that takes a value has none.
    MissingValue(char),
    /// A grade is not one byte.
    BadGrade,
    /// Fewer than two operands were supplied.
    MissingOperands,
}

/// Why a `uucp` operation failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UucpError {
    /// The spool filesystem failed.
    Spool(SpoolError),
    /// The queue failed.
    Queue(QueueError),
    /// The destination system is not configured.
    UnknownSystem(String),
    /// A source or destination address is malformed.
    InvalidAddress,
    /// Multiple sources need an existing destination directory.
    NotDirectory,
    /// Remote-to-remote copy needs a transport that does not exist.
    CrossPeer,
}

impl core::fmt::Display for UucpError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Spool(error) => write!(formatter, "spool: {error:?}"),
            Self::Queue(error) => write!(formatter, "{error}"),
            Self::UnknownSystem(system) => write!(formatter, "unknown system `{system}`"),
            Self::InvalidAddress => formatter.write_str("invalid `system!path` address"),
            Self::NotDirectory => formatter.write_str("multiple sources need a directory"),
            Self::CrossPeer => formatter.write_str("remote-to-remote copy is not supported"),
        }
    }
}

impl From<SpoolError> for UucpError {
    fn from(error: SpoolError) -> Self {
        Self::Spool(error)
    }
}

impl From<QueueError> for UucpError {
    fn from(error: QueueError) -> Self {
        Self::Queue(error)
    }
}

/// One split `[system!]path` operand.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Address<'a> {
    /// Remote system, or `None` for a local path.
    pub system: Option<&'a str>,
    /// Path on that system.
    pub path: &'a str,
}

/// Split an operand at the first `!`.
///
/// # Errors
///
/// Returns [`UucpError::InvalidAddress`] for an empty system, an empty path or an empty operand.
pub fn split_address(value: &str) -> Result<Address<'_>, UucpError> {
    match value.find('!') {
        Some(index) => {
            let system = value.get(..index).ok_or(UucpError::InvalidAddress)?;
            let path = value
                .get(index.saturating_add(1)..)
                .ok_or(UucpError::InvalidAddress)?;
            if system.is_empty() || path.is_empty() || system.chars().any(char::is_whitespace) {
                return Err(UucpError::InvalidAddress);
            }
            Ok(Address {
                system: Some(system),
                path,
            })
        }
        None if value.is_empty() => Err(UucpError::InvalidAddress),
        None => Ok(Address {
            system: None,
            path: value,
        }),
    }
}

/// Parse `uucp [-cCdfjmrR] [-g grade] [-n user] [-x debug] source ... destination`.
///
/// # Errors
///
/// Returns [`UsageError`] for an unknown option, a missing option value, a multi-byte grade or
/// fewer than two operands.
pub fn parse_args<'a>(args: impl IntoIterator<Item = &'a str>) -> Result<Options, UsageError> {
    let arguments: Vec<&str> = args.into_iter().collect();
    let mut options = Options {
        copy_spool: false,
        create_dirs: false,
        print_id: false,
        mail: false,
        grade: b'n',
        user: None,
        debug: None,
        operands: Vec::new(),
    };
    let mut index = 0usize;
    let mut options_enabled = true;
    while index < arguments.len() {
        let argument = *arguments.get(index).ok_or(UsageError::MissingOperands)?;
        if options_enabled && argument == "--" {
            options_enabled = false;
            index = index.saturating_add(1);
            continue;
        }
        if options_enabled && argument.starts_with('-') && argument.len() > 1 {
            let bytes = argument.as_bytes();
            let mut offset = 1usize;
            while offset < bytes.len() {
                let option = char::from(*bytes.get(offset).ok_or(UsageError::MissingValue('?'))?);
                match option {
                    'c' | 'C' => options.copy_spool = true,
                    'd' => options.create_dirs = true,
                    'f' => options.create_dirs = false,
                    'j' => options.print_id = true,
                    'm' => options.mail = true,
                    'r' | 'R' => {}
                    'g' | 'n' | 'x' => {
                        let attached = argument.get(offset.saturating_add(1)..).unwrap_or("");
                        let value = if attached.is_empty() {
                            index = index.saturating_add(1);
                            *arguments
                                .get(index)
                                .ok_or(UsageError::MissingValue(option))?
                        } else {
                            attached
                        };
                        match option {
                            'g' => {
                                let grade = value.as_bytes();
                                if grade.len() != 1 {
                                    return Err(UsageError::BadGrade);
                                }
                                options.grade =
                                    grade.first().copied().ok_or(UsageError::BadGrade)?;
                            }
                            'n' => options.user = Some(value.to_string()),
                            'x' => options.debug = Some(value.to_string()),
                            _ => {}
                        }
                        offset = bytes.len();
                        continue;
                    }
                    _ => return Err(UsageError::UnknownOption(option)),
                }
                offset = offset.saturating_add(1);
            }
            index = index.saturating_add(1);
            continue;
        }
        options.operands.extend(
            arguments
                .get(index..)
                .ok_or(UsageError::MissingOperands)?
                .iter()
                .map(|value| (*value).to_string()),
        );
        break;
    }
    if options.operands.len() < 2 {
        return Err(UsageError::MissingOperands);
    }
    Ok(options)
}

/// Run one `uucp` invocation and return the ids of all queued jobs.
///
/// # Errors
///
/// Returns [`UucpError`] for an invalid address, an unknown peer, unsupported remote-to-remote
/// copy or a spool/queue failure.
pub fn run(
    options: &Options,
    config: &Config,
    spool: &mut dyn Spool,
    default_user: &str,
) -> Result<Vec<u64>, UucpError> {
    let (destination, sources) = options
        .operands
        .split_last()
        .ok_or(UucpError::InvalidAddress)?;
    if sources.is_empty() {
        return Err(UucpError::InvalidAddress);
    }
    let destination = split_address(destination)?;
    if let Some(system) = destination.system
        && !config.peer_known(system)
    {
        return Err(UucpError::UnknownSystem(system.to_string()));
    }
    let destination_is_dir = match destination.system {
        Some(_) => false,
        None => match spool.is_dir(destination.path) {
            Ok(value) => value,
            Err(SpoolError::NotFound) => false,
            Err(error) => return Err(error.into()),
        },
    };
    if destination.system.is_none() && sources.len() > 1 && !destination_is_dir {
        return Err(UucpError::NotDirectory);
    }
    let user = options.user.as_deref().unwrap_or(default_user);
    let mut ids = Vec::new();
    for source in sources {
        let source = split_address(source)?;
        match (source.system, destination.system) {
            (None, None) => {
                let target = if destination_is_dir || sources.len() > 1 {
                    join(destination.path, basename(source.path)?)?
                } else {
                    destination.path.to_string()
                };
                copy_local(spool, source.path, &target, options.create_dirs)?;
            }
            (None, Some(system)) => {
                let target = if sources.len() > 1 {
                    join(destination.path, basename(source.path)?)?
                } else {
                    destination.path.to_string()
                };
                let data = spool.read(source.path)?;
                let mode = spool.mode(source.path)?;
                let job = queue::enqueue(
                    spool,
                    config,
                    user,
                    system,
                    options.grade,
                    JobKind::Copy {
                        source: source.path.to_string(),
                        destination: target,
                        mode,
                        data,
                    },
                )?;
                ids.push(job.id);
            }
            (Some(system), None) => {
                if !config.peer_known(system) {
                    return Err(UucpError::UnknownSystem(system.to_string()));
                }
                let target = if destination_is_dir || sources.len() > 1 {
                    join(destination.path, basename(source.path)?)?
                } else {
                    destination.path.to_string()
                };
                let job = queue::enqueue(
                    spool,
                    config,
                    user,
                    system,
                    options.grade,
                    JobKind::Fetch {
                        source: source.path.to_string(),
                        destination: target,
                    },
                )?;
                ids.push(job.id);
            }
            (Some(source_system), Some(destination_system)) => {
                if !config.peer_known(source_system) {
                    return Err(UucpError::UnknownSystem(source_system.to_string()));
                }
                if !config.peer_known(destination_system) {
                    return Err(UucpError::UnknownSystem(destination_system.to_string()));
                }
                return Err(UucpError::CrossPeer);
            }
        }
    }
    Ok(ids)
}

fn copy_local(
    spool: &mut dyn Spool,
    source: &str,
    destination: &str,
    create_dirs: bool,
) -> Result<(), UucpError> {
    let data = spool.read(source)?;
    let mode = spool.mode(source)?;
    if create_dirs && let Some(parent) = parent(destination) {
        spool.create_dir_all(parent)?;
    }
    spool.write(destination, &data)?;
    spool.set_mode(destination, mode & 0o7777)?;
    Ok(())
}

fn basename(path: &str) -> Result<&str, UucpError> {
    let name = path.rsplit('/').next().unwrap_or("");
    if name.is_empty() || name == "." || name == ".." {
        return Err(UucpError::InvalidAddress);
    }
    Ok(name)
}

fn join(directory: &str, name: &str) -> Result<String, UucpError> {
    if name.is_empty() || name == "." || name == ".." || name.contains('/') {
        return Err(UucpError::InvalidAddress);
    }
    if directory == "/" {
        Ok(format!("/{name}"))
    } else if directory.ends_with('/') {
        Ok(format!("{directory}{name}"))
    } else {
        Ok(format!("{directory}/{name}"))
    }
}

fn parent(path: &str) -> Option<&str> {
    let index = path.rfind('/')?;
    if index == 0 {
        Some("/")
    } else {
        path.get(..index)
    }
}

/// Build one named-file payload for `uux`.
#[must_use]
pub fn named_file(name: &str, data: Vec<u8>) -> NamedFile {
    NamedFile {
        name: name.to_string(),
        data,
    }
}

// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Bounded local `mail`/`mailx` implementation.
//!
//! The tool sends a message to a local recipient by appending it to an mbox file or a Maildir,
//! lists message headers, reads one message and deletes one message. It intentionally has no
//! SMTP, network, MTA daemon or process-server integration; reply and forward are unsupported.

#![cfg_attr(target_os = "none", no_std)]
#![cfg(not(target_os = "none"))]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Successful exit status.
pub const EXIT_OK: i32 = 0;
/// Operational error.
pub const EXIT_ERROR: i32 = 1;
/// Usage error or unsupported option.
pub const EXIT_USAGE: i32 = 2;

/// What the command line asks the tool to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// List message headers.
    List,
    /// Read a one-based message index.
    Read(usize),
    /// Delete a one-based message index.
    Delete(usize),
    /// Send a message.
    Send,
    /// Exit 0 when the mailbox has at least one message.
    Exists,
    /// Print usage.
    Help,
}

/// Parsed command line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Invocation {
    /// Tool name used in diagnostics.
    pub tool: String,
    /// Explicit mailbox path.
    pub mailbox: Option<PathBuf>,
    /// Subject for a sent message.
    pub subject: Option<String>,
    /// Body file; `-` or `None` means standard input.
    pub body: Option<PathBuf>,
    /// Recipients for a sent message.
    pub recipients: Vec<String>,
    /// Requested action.
    pub action: Action,
}

/// One parsed message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    /// Header fields in source order.
    pub headers: Vec<(String, String)>,
    /// Body, with mbox `>From ` quoting removed.
    pub body: String,
    /// Original message text (without the mbox `From ` separator).
    pub raw: String,
}

impl Message {
    /// Return the first header value with a case-insensitive name.
    pub fn field(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

/// A local message store.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Store {
    /// One mbox file.
    Mbox(PathBuf),
    /// A Maildir directory with `new`, `cur` and `tmp`.
    Maildir(PathBuf),
}

/// No external dependency is used; all timestamps are formatted by `format_date`.
static UNIQUE: AtomicU64 = AtomicU64::new(0);

/// Parse the command line.
///
/// # Errors
/// Returns a usage message for an unknown option, a missing argument or an inconsistent action.
pub fn parse_args(tool: &str, args: &[String]) -> Result<Invocation, String> {
    let mut mailbox = None;
    let mut subject = None;
    let mut body = None;
    let mut recipients = Vec::new();
    let mut action = None;
    let mut index = 0usize;
    while index < args.len() {
        let argument = &args[index];
        match argument.as_str() {
            "-s" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "-s needs a subject".to_string())?;
                if value.contains('\n') || value.contains('\r') {
                    return Err("subject contains a newline".to_string());
                }
                subject = Some(value.clone());
            }
            "-f" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "-f needs a mailbox".to_string())?;
                mailbox = Some(PathBuf::from(value));
            }
            "-b" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "-b needs a body file".to_string())?;
                body = Some(PathBuf::from(value));
            }
            "-r" | "-d" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| format!("{argument} needs a message number"))?;
                let number = value
                    .parse::<usize>()
                    .map_err(|_| format!("{argument}: invalid message number `{value}`"))?;
                if number == 0 {
                    return Err("message numbers start at 1".to_string());
                }
                if action.is_some() {
                    return Err("only one action may be selected".to_string());
                }
                action = Some(if argument == "-r" {
                    Action::Read(number)
                } else {
                    Action::Delete(number)
                });
            }
            "-H" => {
                if action.is_some() {
                    return Err("only one action may be selected".to_string());
                }
                action = Some(Action::List);
            }
            "-e" => {
                if action.is_some() {
                    return Err("only one action may be selected".to_string());
                }
                action = Some(Action::Exists);
            }
            "-h" | "--help" => action = Some(Action::Help),
            "--" => {
                recipients.extend(args[index + 1..].iter().cloned());
                index = args.len();
                continue;
            }
            other if other.starts_with('-') && other != "-" => {
                return Err(format!("unsupported option `{other}`"));
            }
            other => recipients.push(other.to_string()),
        }
        index += 1;
    }

    let action = action.unwrap_or_else(|| {
        if recipients.is_empty() && subject.is_none() && body.is_none() {
            Action::List
        } else {
            Action::Send
        }
    });
    match action {
        Action::Send => {
            if recipients.is_empty() {
                return Err("send needs at least one recipient".to_string());
            }
        }
        Action::List | Action::Read(_) | Action::Delete(_) | Action::Exists => {
            if !recipients.is_empty() {
                return Err("recipients are only valid when sending".to_string());
            }
            if subject.is_some() || body.is_some() {
                return Err("-s and -b are only valid when sending".to_string());
            }
        }
        Action::Help => {}
    }
    Ok(Invocation {
        tool: tool.to_string(),
        mailbox,
        subject,
        body,
        recipients,
        action,
    })
}

/// Return the usage text for a tool name.
pub fn usage(tool: &str) -> String {
    format!(
        "usage: {tool} [-f mailbox] [-s subject] [-b body-file] recipient...\n       {tool} [-f mailbox] [-H | -e | -r N | -d N]\n\n\
         The bounded core supports local mbox and Maildir delivery, header listing, reading and\n\
         deletion. Reply, forward, SMTP, cc/bcc and attach are unsupported.\n"
    )
}

/// Run the tool and return its process exit status.
pub fn run(tool: &str, args: &[String]) -> i32 {
    let invocation = match parse_args(tool, args) {
        Ok(invocation) => invocation,
        Err(message) => {
            eprintln!("{tool}: {message}");
            return EXIT_USAGE;
        }
    };
    match execute(&invocation) {
        Ok(code) => code,
        Err(message) => {
            eprintln!("{tool}: {message}");
            EXIT_ERROR
        }
    }
}

fn execute(invocation: &Invocation) -> Result<i32, String> {
    if invocation.action == Action::Help {
        print!("{}", usage(&invocation.tool));
        return Ok(EXIT_OK);
    }
    let path = invocation.mailbox.clone().unwrap_or_else(default_mailbox);
    let store = Store::open(&path);
    match &invocation.action {
        Action::List => {
            let messages = store.list()?;
            if messages.is_empty() {
                println!("No mail for {}", user_name());
                return Ok(EXIT_OK);
            }
            for (index, message) in messages.iter().enumerate() {
                let subject = message.field("Subject").unwrap_or("(no subject)");
                let from = message.field("From").unwrap_or("unknown");
                println!("{} {} {}", index + 1, subject, from);
            }
            Ok(EXIT_OK)
        }
        Action::Read(number) => {
            let messages = store.list()?;
            let message = messages
                .get(number.saturating_sub(1))
                .ok_or_else(|| format!("no message {number}"))?;
            println!("{}", message.raw.trim_end());
            Ok(EXIT_OK)
        }
        Action::Delete(number) => {
            store.delete(number.saturating_sub(1))?;
            println!("Message {number} deleted.");
            Ok(EXIT_OK)
        }
        Action::Exists => {
            let messages = store.list()?;
            Ok(if messages.is_empty() {
                EXIT_ERROR
            } else {
                EXIT_OK
            })
        }
        Action::Send => {
            let body = read_body(invocation.body.as_deref())?;
            let from = sender_address();
            let date = format_date(
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_or(0, |value| value.as_secs()),
            );
            let message = compose(
                &from,
                &invocation.recipients,
                invocation.subject.as_deref(),
                &date,
                &body,
            );
            store.append(&message)?;
            Ok(EXIT_OK)
        }
        Action::Help => Ok(EXIT_OK),
    }
}

/// The mailbox selected when `-f` and `MAIL` are both absent.
pub fn default_mailbox() -> PathBuf {
    if let Some(value) = std::env::var_os("MAIL")
        && !value.is_empty()
    {
        return PathBuf::from(value);
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    for candidate in [home.join("Maildir"), home.join(".maildir")] {
        if candidate.is_dir() {
            return candidate;
        }
    }
    home.join("mbox")
}

fn user_name() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .unwrap_or_else(|_| "user".to_string())
}

fn sender_address() -> String {
    let name = user_name();
    if name.contains('@') {
        name
    } else {
        format!("{name}@localhost")
    }
}

fn read_body(path: Option<&Path>) -> Result<String, String> {
    let mut text = String::new();
    match path {
        Some(path) if path != Path::new("-") => {
            text =
                fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
        }
        _ => {
            std::io::stdin()
                .read_to_string(&mut text)
                .map_err(|error| format!("stdin: {error}"))?;
        }
    }
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    Ok(text)
}

/// Compose a message with the local headers used by both stores.
pub fn compose(
    from: &str,
    recipients: &[String],
    subject: Option<&str>,
    date: &str,
    body: &str,
) -> String {
    let mut message = String::new();
    message.push_str(&format!("From: {from}\n"));
    message.push_str(&format!("To: {}\n", recipients.join(", ")));
    message.push_str(&format!("Subject: {}\n", subject.unwrap_or("(no subject)")));
    message.push_str(&format!("Date: {date}\n\n"));
    message.push_str(body);
    if !message.ends_with('\n') {
        message.push('\n');
    }
    message
}

impl Store {
    /// Open a path as a Maildir when it is an existing directory, otherwise as mbox.
    pub fn open(path: &Path) -> Self {
        if path.is_dir() {
            Self::Maildir(path.to_path_buf())
        } else {
            Self::Mbox(path.to_path_buf())
        }
    }

    fn ensure(&self) -> Result<(), String> {
        match self {
            Self::Mbox(path) => {
                if let Some(parent) = path.parent()
                    && !parent.as_os_str().is_empty()
                {
                    fs::create_dir_all(parent)
                        .map_err(|error| format!("{}: {error}", parent.display()))?;
                }
                Ok(())
            }
            Self::Maildir(path) => {
                for child in ["new", "cur", "tmp"] {
                    let directory = path.join(child);
                    fs::create_dir_all(&directory)
                        .map_err(|error| format!("{}: {error}", directory.display()))?;
                }
                Ok(())
            }
        }
    }

    /// List all messages in delivery order.
    pub fn list(&self) -> Result<Vec<Message>, String> {
        self.ensure()?;
        match self {
            Self::Mbox(path) => {
                let text = match fs::read_to_string(path) {
                    Ok(text) => text,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
                    Err(error) => return Err(format!("{}: {error}", path.display())),
                };
                Ok(split_mbox(&text)
                    .iter()
                    .map(|raw| parse_message(raw, true))
                    .collect())
            }
            Self::Maildir(path) => {
                let mut files = maildir_files(path)?;
                files.sort();
                let mut messages = Vec::new();
                for file in files {
                    let text = fs::read_to_string(&file)
                        .map_err(|error| format!("{}: {error}", file.display()))?;
                    messages.push(parse_message(&text, false));
                }
                Ok(messages)
            }
        }
    }

    /// Delete a zero-based message.
    pub fn delete(&self, index: usize) -> Result<(), String> {
        self.ensure()?;
        match self {
            Self::Mbox(path) => {
                let text = fs::read_to_string(path).unwrap_or_default();
                let mut messages = split_mbox(&text);
                if index >= messages.len() {
                    return Err(format!("no message {}", index + 1));
                }
                messages.remove(index);
                let mut output = String::new();
                for message in messages {
                    output.push_str(&message);
                    if !output.ends_with('\n') {
                        output.push('\n');
                    }
                }
                write_atomic(path, &output)
            }
            Self::Maildir(path) => {
                let mut files = maildir_files(path)?;
                files.sort();
                let file = files
                    .get(index)
                    .ok_or_else(|| format!("no message {}", index + 1))?;
                fs::remove_file(file).map_err(|error| format!("{}: {error}", file.display()))
            }
        }
    }

    /// Append a composed message.
    pub fn append(&self, message: &str) -> Result<(), String> {
        self.ensure()?;
        match self {
            Self::Mbox(path) => {
                let mut file = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)
                    .map_err(|error| format!("{}: {error}", path.display()))?;
                let date = format_date(
                    SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map_or(0, |value| value.as_secs()),
                );
                let mut text = format!("From {} {}\n", sender_address(), date);
                text.push_str(&escape_mbox(message));
                if !text.ends_with('\n') {
                    text.push('\n');
                }
                text.push('\n');
                file.write_all(text.as_bytes())
                    .map_err(|error| format!("{}: {error}", path.display()))
            }
            Self::Maildir(path) => {
                let name = unique_name();
                let tmp = path.join("tmp").join(&name);
                let new = path.join("new").join(&name);
                fs::write(&tmp, message).map_err(|error| format!("{}: {error}", tmp.display()))?;
                fs::rename(&tmp, &new).map_err(|error| {
                    let _ = fs::remove_file(&tmp);
                    format!("{}: {error}", new.display())
                })
            }
        }
    }
}

fn maildir_files(path: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    for child in ["new", "cur"] {
        let directory = path.join(child);
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(format!("{}: {error}", directory.display())),
        };
        for entry in entries {
            let entry = entry.map_err(|error| format!("{}: {error}", directory.display()))?;
            if entry
                .file_type()
                .map(|kind| kind.is_file())
                .unwrap_or(false)
            {
                files.push(entry.path());
            }
        }
    }
    Ok(files)
}

fn unique_name() -> String {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |value| value.as_nanos());
    let sequence = UNIQUE.fetch_add(1, Ordering::Relaxed);
    format!("{stamp}.{}.{}", std::process::id(), sequence)
}

fn write_atomic(path: &Path, text: &str) -> Result<(), String> {
    let temporary = path.with_extension(format!("tmp.{}", std::process::id()));
    fs::write(&temporary, text).map_err(|error| format!("{}: {error}", temporary.display()))?;
    fs::rename(&temporary, path).map_err(|error| {
        let _ = fs::remove_file(&temporary);
        format!("{}: {error}", path.display())
    })
}

fn escape_mbox(message: &str) -> String {
    let mut output = String::new();
    for line in message.split_inclusive('\n') {
        if line.starts_with("From ") {
            output.push('>');
        }
        output.push_str(line);
    }
    output
}

fn unescape_mbox(message: &str) -> String {
    let mut output = String::new();
    for line in message.split_inclusive('\n') {
        if let Some(rest) = line.strip_prefix(">From ") {
            output.push_str("From ");
            output.push_str(rest);
        } else {
            output.push_str(line);
        }
    }
    output
}

/// Split an mbox file into raw messages, keeping the `From ` separator in each raw message.
pub fn split_mbox(text: &str) -> Vec<String> {
    let mut messages = Vec::new();
    let mut current = String::new();
    for line in text.split_inclusive('\n') {
        if line.starts_with("From ") && !current.is_empty() {
            messages.push(current);
            current = String::new();
        }
        current.push_str(line);
    }
    if !current.is_empty() {
        messages.push(current);
    }
    messages
}

/// Parse a raw message; `mbox` controls the leading `From ` line and quoting.
pub fn parse_message(raw: &str, mbox: bool) -> Message {
    let without_separator = if mbox {
        raw.split_once('\n').map_or("", |(_, rest)| rest)
    } else {
        raw
    };
    let (header_text, body) = match without_separator.split_once("\n\n") {
        Some((headers, body)) => (headers, body),
        None => (without_separator, ""),
    };
    let mut headers: Vec<(String, String)> = Vec::new();
    for line in header_text.lines() {
        if line.starts_with(' ') || line.starts_with('\t') {
            if let Some((_, value)) = headers.last_mut() {
                value.push(' ');
                value.push_str(line.trim());
            }
            continue;
        }
        if let Some((key, value)) = line.split_once(':') {
            headers.push((key.to_string(), value.trim().to_string()));
        }
    }
    let mut body = if mbox {
        unescape_mbox(body)
    } else {
        body.to_string()
    };
    if mbox && body.ends_with('\n') {
        // The mbox writer terminates every message with one blank separator line.
        body.pop();
    }
    Message {
        headers,
        body,
        raw: without_separator.to_string(),
    }
}

/// Format a UNIX timestamp as an RFC 2822 UTC date.
pub fn format_date(seconds: u64) -> String {
    const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let days = (seconds / 86_400) as i64;
    let remainder = seconds % 86_400;
    let (year, month, day) = civil_from_days(days);
    let hour = remainder / 3_600;
    let minute = (remainder % 3_600) / 60;
    let second = remainder % 60;
    let weekday = WEEKDAYS[((days + 4) % 7) as usize];
    format!(
        "{weekday}, {day:02} {} {year:04} {hour:02}:{minute:02}:{second:02} +0000",
        MONTHS[(month - 1) as usize]
    )
}

fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = if month_prime < 10 {
        month_prime + 3
    } else {
        month_prime - 9
    };
    if month <= 2 {
        year += 1;
    }
    (year, month, day)
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;

// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! POSIX `at`/`batch` queue model: time parsing, job serialization and spool operations.
//!
//! Supported time forms are `now`, `now + N minutes|hours|days` (or `+ N ...`), `HH:MM` for the
//! next occurrence, and `YYYY-MM-DD HH:MM` / `YYYY-MM-DDTHH:MM`. Every other POSIX and GNU form
//! is rejected explicitly. Commands are single program-plus-arguments lines; shell
//! metacharacters and multi-line scripts are unsupported because the target has no shell runner.

#![cfg_attr(target_os = "none", no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// Default spool directory.
pub const DEFAULT_SPOOL: &str = "/var/at";
/// Environment override used by the host tests.
pub const SPOOL_ENV: &str = "AT_SPOOL";
/// Maximum accepted command length.
pub const MAX_COMMAND: usize = 4096;

/// One civil time used by the scheduler.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Civil {
    /// Calendar year.
    pub year: i64,
    /// Month, 1..=12.
    pub month: u32,
    /// Day of month, 1..=31.
    pub day: u32,
    /// Hour, 0..=23.
    pub hour: u32,
    /// Minute, 0..=59.
    pub minute: u32,
}

/// One queued job.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Job {
    /// Job id, `<due>.<serial>`.
    pub id: String,
    /// Due time in Unix seconds.
    pub due: i64,
    /// Whether this is a `batch` job.
    pub batch: bool,
    /// Program plus arguments.
    pub command: String,
}

/// One parsed `at` action.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AtAction {
    /// List the queue.
    List,
    /// Remove the named jobs.
    Remove(Vec<String>),
    /// Schedule a command. A missing command means standard input.
    Schedule {
        /// Time expression.
        when: String,
        /// Command words supplied on the command line.
        command: Option<String>,
        /// File supplied with `-f`.
        file: Option<String>,
    },
}

/// Convert Unix seconds to a civil UTC time.
#[must_use]
pub fn civil_from_unix(seconds: i64) -> Civil {
    let days = seconds.div_euclid(86_400);
    let rem = seconds.rem_euclid(86_400);
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    Civil {
        year: if month <= 2 { y + 1 } else { y },
        month,
        day,
        hour: (rem / 3600) as u32,
        minute: ((rem % 3600) / 60) as u32,
    }
}

/// Convert a civil UTC time to Unix seconds.
#[must_use]
pub fn unix_from_civil(year: i64, month: u32, day: u32, hour: u32, minute: u32) -> i64 {
    let month = i64::from(month);
    let day = i64::from(day);
    let y = year - i64::from(month <= 2);
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = if month > 2 { month - 3 } else { month + 9 };
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    days * 86_400 + i64::from(hour) * 3600 + i64::from(minute) * 60
}

fn days_in_month(year: i64, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 {
                29
            } else {
                28
            }
        }
        _ => 0,
    }
}

fn parse_clock(value: &str) -> Result<(u32, u32), String> {
    let (hour, minute) = value
        .split_once(':')
        .ok_or_else(|| format!("invalid time `{value}`: expected HH:MM"))?;
    if minute.contains(':') {
        return Err(format!("invalid time `{value}`: expected HH:MM"));
    }
    let hour: u32 = hour
        .parse()
        .map_err(|_| format!("invalid hour in `{value}`"))?;
    let minute: u32 = minute
        .parse()
        .map_err(|_| format!("invalid minute in `{value}`"))?;
    if hour > 23 || minute > 59 {
        return Err(format!("invalid time `{value}`: outside 00:00..23:59"));
    }
    Ok((hour, minute))
}

fn parse_date(value: &str, time: &str) -> Result<i64, String> {
    let mut parts = value.split('-');
    let year = parts
        .next()
        .ok_or_else(|| format!("invalid date `{value}`"))?
        .parse::<i64>()
        .map_err(|_| format!("invalid year in `{value}`"))?;
    let month = parts
        .next()
        .ok_or_else(|| format!("invalid date `{value}`"))?
        .parse::<u32>()
        .map_err(|_| format!("invalid month in `{value}`"))?;
    let day = parts
        .next()
        .ok_or_else(|| format!("invalid date `{value}`"))?
        .parse::<u32>()
        .map_err(|_| format!("invalid day in `{value}`"))?;
    if parts.next().is_some() {
        return Err(format!("invalid date `{value}`"));
    }
    if !(1970..=9999).contains(&year) || !(1..=12).contains(&month) {
        return Err(format!("date `{value}` is outside 1970-01-01..9999-12-31"));
    }
    if day == 0 || day > days_in_month(year, month) {
        return Err(format!("date `{value}` has an invalid day"));
    }
    let (hour, minute) = parse_clock(time)?;
    Ok(unix_from_civil(year, month, day, hour, minute))
}

fn offset_seconds(count: &str, unit: &str) -> Result<i64, String> {
    let count: i64 = count
        .parse()
        .map_err(|_| format!("invalid offset count `{count}`"))?;
    let unit = match unit {
        "minute" | "minutes" => 60,
        "hour" | "hours" => 3600,
        "day" | "days" => 86_400,
        other => return Err(format!("unsupported offset unit `{other}`")),
    };
    count
        .checked_mul(unit)
        .ok_or_else(|| "offset is too large".to_string())
}

/// Parse a supported `at` time expression relative to `now` (Unix seconds).
pub fn parse_time(now: i64, spec: &str) -> Result<i64, String> {
    let spec = spec.trim();
    if spec.is_empty() {
        return Err("missing time".to_string());
    }
    let lower = spec.to_ascii_lowercase();
    let words = lower.split_whitespace().collect::<Vec<_>>();
    match words.as_slice() {
        ["now"] => Ok(now),
        ["now", "+", count, unit] | ["+", count, unit] => now
            .checked_add(offset_seconds(count, unit)?)
            .ok_or_else(|| "time is too large".to_string()),
        [clock] if clock.contains(':') && !clock.contains('-') => {
            let (hour, minute) = parse_clock(clock)?;
            let civil = civil_from_unix(now);
            let mut due = unix_from_civil(civil.year, civil.month, civil.day, hour, minute);
            if due <= now {
                due += 86_400;
            }
            Ok(due)
        }
        [date, time] => {
            let due = parse_date(date, time)?;
            if due < now {
                return Err(format!("time `{spec}` is in the past"));
            }
            Ok(due)
        }
        [datetime] if datetime.contains('t') => {
            let (date, time) = datetime
                .split_once('t')
                .ok_or_else(|| format!("invalid date-time `{spec}`"))?;
            let due = parse_date(date, time)?;
            if due < now {
                return Err(format!("time `{spec}` is in the past"));
            }
            Ok(due)
        }
        _ => Err(format!(
            "unsupported time `{spec}`; use now, now + N minutes, HH:MM or YYYY-MM-DD HH:MM"
        )),
    }
}

/// Validate and normalize one command line.
pub fn parse_command(text: &str) -> Result<String, String> {
    let command = text.trim();
    if command.is_empty() {
        return Err("empty command".to_string());
    }
    if command.len() > MAX_COMMAND {
        return Err("command is too long".to_string());
    }
    if command.contains('\n') || command.contains('\r') {
        return Err("multi-line commands are unsupported".to_string());
    }
    if command
        .chars()
        .any(|character| matches!(character, '>' | '<' | '|' | ';' | '&' | '\'' | '"'))
    {
        return Err("shell metacharacters are unsupported".to_string());
    }
    Ok(command.to_string())
}

/// Parse `at` arguments.
pub fn parse_at_args(args: &[String]) -> Result<AtAction, String> {
    let mut list = false;
    let mut remove = false;
    let mut file = None;
    let mut operands = Vec::new();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "-l" => list = true,
            "-r" => remove = true,
            "-f" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "-f needs a file operand".to_string())?;
                if file.is_some() {
                    return Err("-f given more than once".to_string());
                }
                file = Some(value.clone());
            }
            value if value.starts_with('-') => {
                return Err(format!("unsupported option {value}"));
            }
            value => operands.push(value.to_string()),
        }
        index += 1;
    }
    if list && remove {
        return Err("-l and -r are mutually exclusive".to_string());
    }
    if list {
        if !operands.is_empty() || file.is_some() {
            return Err("-l takes no operands".to_string());
        }
        return Ok(AtAction::List);
    }
    if remove {
        if file.is_some() {
            return Err("-r takes no -f operand".to_string());
        }
        if operands.is_empty() {
            return Err("-r needs at least one job id".to_string());
        }
        return Ok(AtAction::Remove(operands));
    }
    if operands.is_empty() {
        return Err("missing time operand".to_string());
    }
    let when = if operands[0].eq_ignore_ascii_case("now") {
        if operands.len() >= 4 && operands[1] == "+" {
            let when = format!("now + {} {}", operands[2], operands[3]);
            operands.drain(0..4);
            when
        } else {
            operands.remove(0)
        }
    } else if operands[0] == "+" {
        if operands.len() < 3 {
            return Err("incomplete offset time".to_string());
        }
        let when = format!("+ {} {}", operands[1], operands[2]);
        operands.drain(0..3);
        when
    } else if operands.len() >= 2
        && operands[0].contains('-')
        && !operands[0].contains(':')
        && operands[1].contains(':')
    {
        // Accept an unquoted ISO date and clock as two operands.
        let time = operands.remove(1);
        operands[0] = format!("{} {}", operands[0], time);
        operands.remove(0)
    } else {
        operands.remove(0)
    };
    if !operands.is_empty() && file.is_some() {
        return Err("command words and -f are mutually exclusive".to_string());
    }
    let command = if operands.is_empty() {
        None
    } else {
        Some(operands.join(" "))
    };
    Ok(AtAction::Schedule {
        when,
        command,
        file,
    })
}

/// Parse `batch` arguments: no options, optional command words.
pub fn parse_batch_args(args: &[String]) -> Result<Option<String>, String> {
    let mut words = Vec::new();
    for value in args {
        if value.starts_with('-') {
            return Err(format!("unsupported option {value}"));
        }
        words.push(value.clone());
    }
    if words.is_empty() {
        Ok(None)
    } else {
        Ok(Some(words.join(" ")))
    }
}

/// Storage operations used by the utilities, `atd` and their host tests.
pub trait Spool {
    /// List job ids.
    fn names(&mut self) -> Result<Vec<String>, String>;
    /// Read one job body.
    fn read(&mut self, id: &str) -> Result<String, String>;
    /// Atomically replace one job body.
    fn write_atomic(&mut self, id: &str, text: &str) -> Result<(), String>;
    /// Remove one job.
    fn remove(&mut self, id: &str) -> Result<(), String>;
}

fn check_id(id: &str) -> Result<(), String> {
    if id.is_empty()
        || !id
            .chars()
            .all(|character| character.is_ascii_digit() || character == '.')
    {
        return Err(format!("invalid job id `{id}`"));
    }
    Ok(())
}

fn job_text(job: &Job) -> String {
    format!(
        "due={}\nbatch={}\ncommand={}\n",
        job.due,
        u8::from(job.batch),
        job.command
    )
}

fn parse_job(id: &str, text: &str) -> Result<Job, String> {
    let mut lines = text.lines();
    let due = lines
        .next()
        .and_then(|line| line.strip_prefix("due="))
        .ok_or_else(|| format!("job `{id}` has no due line"))?
        .parse::<i64>()
        .map_err(|_| format!("job `{id}` has an invalid due time"))?;
    let batch = match lines.next().and_then(|line| line.strip_prefix("batch=")) {
        Some("0") => false,
        Some("1") => true,
        _ => return Err(format!("job `{id}` has an invalid batch flag")),
    };
    let command = lines
        .next()
        .and_then(|line| line.strip_prefix("command="))
        .ok_or_else(|| format!("job `{id}` has no command line"))?;
    if lines.next().is_some() {
        return Err(format!("job `{id}` has extra lines"));
    }
    Ok(Job {
        id: id.to_string(),
        due,
        batch,
        command: parse_command(command)?,
    })
}

fn next_serial(names: &[String]) -> u64 {
    names
        .iter()
        .filter_map(|name| {
            name.rsplit_once('.')
                .and_then(|(_, serial)| serial.parse().ok())
        })
        .max()
        .unwrap_or(0)
        + 1
}

/// Queue a job. `batch` marks it for the idle queue.
pub fn submit<S: Spool>(
    spool: &mut S,
    now: i64,
    when: &str,
    command: &str,
    batch: bool,
) -> Result<Job, String> {
    let due = parse_time(now, when)?;
    let command = parse_command(command)?;
    let names = spool.names()?;
    let mut serial = next_serial(&names);
    let id = loop {
        let candidate = format!("{due}.{serial}");
        if !names.contains(&candidate) {
            break candidate;
        }
        serial += 1;
    };
    let job = Job {
        id,
        due,
        batch,
        command,
    };
    spool.write_atomic(&job.id, &job_text(&job))?;
    Ok(job)
}

/// List all valid jobs, oldest first. Malformed and retired jobs are skipped.
pub fn list<S: Spool>(spool: &mut S) -> Result<Vec<Job>, String> {
    let mut jobs = Vec::new();
    for id in spool.names()? {
        if let Ok(text) = spool.read(&id)
            && let Ok(job) = parse_job(&id, &text)
        {
            if job.due < 0 {
                continue;
            }
            jobs.push(job);
        }
    }
    jobs.sort_by(|left, right| (left.due, &left.id).cmp(&(right.due, &right.id)));
    Ok(jobs)
}

fn retire<S: Spool>(spool: &mut S, id: &str) -> Result<(), String> {
    let text = spool.read(id)?;
    let mut job = parse_job(id, &text)?;
    job.due = -1;
    spool.write_atomic(id, &job_text(&job))
}

/// Remove one job, failing when it does not exist. Filesystems without unlink get a retired
/// tombstone instead (`due=-1`), which [`list`] hides.
pub fn remove<S: Spool>(spool: &mut S, id: &str) -> Result<(), String> {
    check_id(id)?;
    if !spool.names()?.iter().any(|name| name == id) {
        return Err(format!("no such job `{id}`"));
    }
    if spool.remove(id).is_ok() {
        return Ok(());
    }
    retire(spool, id)
}

/// Whether a job is due at `now`.
#[must_use]
pub fn is_due(job: &Job, now: i64) -> bool {
    job.due <= now
}

/// Split due jobs into regular and batch queues.
#[must_use]
pub fn due_queues(jobs: &[Job], now: i64) -> (Vec<Job>, Vec<Job>) {
    let mut regular = Vec::new();
    let mut batch = Vec::new();
    for job in jobs {
        if !is_due(job, now) {
            continue;
        }
        if job.batch {
            batch.push(job.clone());
        } else {
            regular.push(job.clone());
        }
    }
    (regular, batch)
}

/// The spool directory, honouring [`SPOOL_ENV`] on the host.
#[must_use]
pub fn spool_dir() -> String {
    #[cfg(not(target_os = "none"))]
    {
        std::env::var(SPOOL_ENV).unwrap_or_else(|_| DEFAULT_SPOOL.to_string())
    }
    #[cfg(target_os = "none")]
    {
        DEFAULT_SPOOL.to_string()
    }
}

/// Current Unix time.
#[must_use]
pub fn now_seconds() -> i64 {
    #[cfg(not(target_os = "none"))]
    {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs() as i64)
            .unwrap_or(0)
    }
    #[cfg(target_os = "none")]
    {
        raven_rt::clock_get(0).map(|time| time.tv_sec).unwrap_or(0)
    }
}

/// Host filesystem spool used by the utilities and tests.
#[cfg(not(target_os = "none"))]
#[derive(Clone, Debug)]
pub struct HostSpool {
    directory: String,
}

#[cfg(not(target_os = "none"))]
impl HostSpool {
    /// Create a spool rooted at `directory`.
    #[must_use]
    pub fn new(directory: &str) -> Self {
        Self {
            directory: directory.to_string(),
        }
    }

    fn path(&self, id: &str) -> String {
        format!("{}/{id}.job", self.directory)
    }
}

#[cfg(not(target_os = "none"))]
impl Spool for HostSpool {
    fn names(&mut self) -> Result<Vec<String>, String> {
        let entries = match std::fs::read_dir(&self.directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.to_string()),
        };
        let mut names = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|error| error.to_string())?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if let Some(id) = name.strip_suffix(".job") {
                names.push(id.to_string());
            }
        }
        names.sort();
        Ok(names)
    }

    fn read(&mut self, id: &str) -> Result<String, String> {
        check_id(id)?;
        std::fs::read_to_string(self.path(id)).map_err(|error| format!("job `{id}`: {error}"))
    }

    fn write_atomic(&mut self, id: &str, text: &str) -> Result<(), String> {
        check_id(id)?;
        std::fs::create_dir_all(&self.directory).map_err(|error| error.to_string())?;
        let temporary = format!("{}/{id}.tmp", self.directory);
        std::fs::write(&temporary, text).map_err(|error| error.to_string())?;
        std::fs::rename(&temporary, self.path(id)).map_err(|error| {
            let _ = std::fs::remove_file(&temporary);
            error.to_string()
        })
    }

    fn remove(&mut self, id: &str) -> Result<(), String> {
        check_id(id)?;
        std::fs::remove_file(self.path(id)).map_err(|error| error.to_string())
    }
}

/// Target filesystem spool used by the utilities, `atd` and the qtest fixture.
#[cfg(target_os = "none")]
#[derive(Clone, Copy, Debug, Default)]
pub struct TargetSpool;

#[cfg(target_os = "none")]
fn scheme(path: &str) -> String {
    if path.contains(':') {
        path.to_string()
    } else {
        format!("file:{}", raven_rt::fs::resolve(path))
    }
}

#[cfg(target_os = "none")]
impl Spool for TargetSpool {
    fn names(&mut self) -> Result<Vec<String>, String> {
        use raven_abi::errno::Errno;
        use raven_proto_fs::DT_REG;

        let directory = spool_dir();
        let entries = match raven_rt::fs::read_dir(&scheme(&directory)) {
            Ok(entries) => entries,
            Err(Errno::ENOENT) => return Ok(Vec::new()),
            Err(error) => {
                return Err(format!("read {}: {}", directory, error.description()));
            }
        };
        let mut names = Vec::new();
        for entry in entries {
            if entry.kind != DT_REG {
                continue;
            }
            if let Some(id) = entry.name.strip_suffix(".job") {
                names.push(id.to_string());
            }
        }
        names.sort();
        Ok(names)
    }

    fn read(&mut self, id: &str) -> Result<String, String> {
        check_id(id)?;
        let data = raven_rt::fs::read_to_vec(&scheme(&format!("{}/{id}.job", spool_dir())))
            .map_err(|error| error.description().to_string())?;
        String::from_utf8(data).map_err(|_| format!("job `{id}` is not UTF-8"))
    }

    fn write_atomic(&mut self, id: &str, text: &str) -> Result<(), String> {
        use raven_proto_fs::{O_CREAT, O_TRUNC, O_WRONLY};

        check_id(id)?;
        let directory = spool_dir();
        let _ = raven_rt::fs::mkdir(&scheme(&directory), 0o700);
        let temporary = format!("{directory}/{id}.tmp");
        let final_path = format!("{directory}/{id}.job");
        let mut file = raven_rt::fs::open(&scheme(&temporary), O_WRONLY | O_CREAT | O_TRUNC)
            .map_err(|error| format!("open {temporary}: {}", error.description()))?;
        file.write(text.as_bytes())
            .map_err(|error| format!("write {temporary}: {}", error.description()))?;
        file.close()
            .map_err(|error| format!("close {temporary}: {}", error.description()))?;
        match raven_rt::fs::rename(&scheme(&temporary), &scheme(&final_path)) {
            Ok(()) => Ok(()),
            Err(_) => {
                // ramfs does not implement rename. Fall back to a direct replace and drop the
                // temporary file; the queue remains readable because `atd` only scans `*.job`.
                let mut file =
                    raven_rt::fs::open(&scheme(&final_path), O_WRONLY | O_CREAT | O_TRUNC)
                        .map_err(|error| format!("open {final_path}: {}", error.description()))?;
                file.write(text.as_bytes())
                    .map_err(|error| format!("write {final_path}: {}", error.description()))?;
                file.close()
                    .map_err(|error| format!("close {final_path}: {}", error.description()))?;
                let _ = raven_rt::fs::unlink(&scheme(&temporary));
                Ok(())
            }
        }
    }

    fn remove(&mut self, id: &str) -> Result<(), String> {
        check_id(id)?;
        raven_rt::fs::unlink(&scheme(&format!("{}/{id}.job", spool_dir())))
            .map_err(|error| error.description().to_string())
    }
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;

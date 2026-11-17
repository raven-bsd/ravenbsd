// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Queue storage under the configured spool.

use alloc::collections::BTreeSet;
use alloc::format;
use alloc::string::ToString;
use alloc::vec::Vec;

use crate::config::Config;
use crate::job::{Job, JobError, JobKind};
use crate::spool::{Spool, SpoolError};

/// Why a queue operation failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QueueError {
    /// The spool filesystem failed.
    Spool(SpoolError),
    /// A job record is malformed.
    Job(JobError),
    /// A sequence number overflowed.
    BadId,
    /// The requested job does not exist.
    NotFound,
}

impl core::fmt::Display for QueueError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Spool(error) => write!(formatter, "spool: {error:?}"),
            Self::Job(error) => write!(formatter, "job: {error:?}"),
            Self::BadId => formatter.write_str("queue sequence overflow"),
            Self::NotFound => formatter.write_str("job not found"),
        }
    }
}

impl From<SpoolError> for QueueError {
    fn from(error: SpoolError) -> Self {
        Self::Spool(error)
    }
}

impl From<JobError> for QueueError {
    fn from(error: JobError) -> Self {
        Self::Job(error)
    }
}

/// Queue summary counts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Summary {
    /// Number of jobs.
    pub jobs: usize,
    /// Number of distinct destination systems.
    pub systems: usize,
    /// Number of distinct requesting users.
    pub users: usize,
}

/// Create the spool and queue directories.
///
/// # Errors
///
/// Returns [`QueueError::Spool`] when the filesystem cannot create a directory.
pub fn ensure(spool: &mut dyn Spool, config: &Config) -> Result<(), QueueError> {
    spool.create_dir_all(&config.spool)?;
    spool.create_dir_all(&config.queue_dir())?;
    Ok(())
}

/// Allocate the next sequence number.
///
/// # Errors
///
/// Returns a queue or sequence error when the filesystem fails or the number overflows.
pub fn next_id(spool: &mut dyn Spool, config: &Config) -> Result<u64, QueueError> {
    let path = config.sequence_path();
    let current = match spool.read(&path) {
        Ok(bytes) => parse_sequence(&bytes)?,
        Err(SpoolError::NotFound) => 0,
        Err(error) => return Err(error.into()),
    };
    let next = current.checked_add(1).ok_or(QueueError::BadId)?;
    spool.write(&path, next.to_string().as_bytes())?;
    Ok(next)
}

fn parse_sequence(bytes: &[u8]) -> Result<u64, QueueError> {
    let text = core::str::from_utf8(bytes).map_err(|_| QueueError::BadId)?;
    text.trim().parse::<u64>().map_err(|_| QueueError::BadId)
}

/// Store one new job and return it with its allocated id.
///
/// # Errors
///
/// Returns a queue error when the spool cannot be prepared, the sequence fails or the record
/// cannot be encoded/written.
pub fn enqueue(
    spool: &mut dyn Spool,
    config: &Config,
    user: &str,
    system: &str,
    grade: u8,
    kind: JobKind,
) -> Result<Job, QueueError> {
    ensure(spool, config)?;
    let id = next_id(spool, config)?;
    let job = Job {
        id,
        user: user.to_string(),
        system: system.to_string(),
        grade,
        kind,
    };
    let encoded = job.encode()?;
    spool.write(&config.job_path(id), &encoded)?;
    Ok(job)
}

/// Load every job in ascending id order.
///
/// # Errors
///
/// Returns a queue error for an unreadable queue directory, a malformed job or an I/O failure.
pub fn jobs(spool: &mut dyn Spool, config: &Config) -> Result<Vec<Job>, QueueError> {
    let names = match spool.list(&config.queue_dir()) {
        Ok(names) => names,
        Err(SpoolError::NotFound) => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let mut jobs = Vec::new();
    for name in names {
        if !name.ends_with(".job") {
            continue;
        }
        let path = format!("{}/{}", config.queue_dir(), name);
        let encoded = spool.read(&path)?;
        jobs.push(Job::decode(&encoded)?);
    }
    jobs.sort_by_key(|job| job.id);
    Ok(jobs)
}

/// Cancel one job by id.
///
/// # Errors
///
/// Returns [`QueueError::NotFound`] when the job is absent, or a filesystem error.
pub fn cancel(spool: &mut dyn Spool, config: &Config, id: u64) -> Result<(), QueueError> {
    match spool.remove(&config.job_path(id)) {
        Ok(()) => Ok(()),
        Err(SpoolError::NotFound) => Err(QueueError::NotFound),
        Err(error) => Err(error.into()),
    }
}

/// Count jobs, systems and users.
#[must_use]
pub fn summarize(jobs: &[Job]) -> Summary {
    let mut systems = BTreeSet::new();
    let mut users = BTreeSet::new();
    for job in jobs {
        systems.insert(job.system.as_str());
        users.insert(job.user.as_str());
    }
    Summary {
        jobs: jobs.len(),
        systems: systems.len(),
        users: users.len(),
    }
}

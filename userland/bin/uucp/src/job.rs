// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The first-party queue job format.
//!
//! The format is a bounded binary record stored in `/var/spool/uucp/queue/<id>.job`. It is a local
//! spool format, not a network protocol: integers are little-endian and every byte string is a
//! `u32` length followed by the bytes. A record starts with `uucp1\n`.

use alloc::string::String;
use alloc::vec::Vec;

/// The record magic.
pub const MAGIC: &[u8] = b"uucp1\n";
/// Largest accepted encoded job.
pub const MAX_JOB: usize = 4 * 1024 * 1024;

/// Why a job record is invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobError {
    /// The record is shorter than its declared fields.
    Truncated,
    /// The magic does not match.
    BadMagic,
    /// The kind byte is unknown.
    BadKind,
    /// A text field is not valid UTF-8.
    BadText,
    /// The encoded record exceeds [`MAX_JOB`].
    TooLarge,
    /// Trailing bytes remain after a complete record.
    TrailingData,
}

/// One file carried by a remote command job.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NamedFile {
    /// Name used in the command string.
    pub name: String,
    /// File bytes.
    pub data: Vec<u8>,
}

/// The work a queued job represents.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JobKind {
    /// Copy local bytes to a remote destination.
    Copy {
        /// Local source name.
        source: String,
        /// Remote destination name.
        destination: String,
        /// Source permission bits.
        mode: u32,
        /// Source bytes.
        data: Vec<u8>,
    },
    /// Fetch a remote file to a local destination.
    Fetch {
        /// Remote source name.
        source: String,
        /// Local destination name.
        destination: String,
    },
    /// Run a command on a remote system.
    Command {
        /// Command text after `system!`.
        command: String,
        /// Standard input bytes, if the command reads `< -`.
        stdin: Vec<u8>,
        /// Local input files named by the command.
        files: Vec<NamedFile>,
        /// Output names named by the command redirections.
        outputs: Vec<String>,
    },
}

impl JobKind {
    /// Stable label used by `uustat`.
    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            Self::Copy { .. } => "copy",
            Self::Fetch { .. } => "fetch",
            Self::Command { .. } => "command",
        }
    }
}

/// One queued job.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Job {
    /// Queue sequence number and file stem.
    pub id: u64,
    /// Requesting user.
    pub user: String,
    /// Destination system name.
    pub system: String,
    /// POSIX grade byte.
    pub grade: u8,
    /// Job payload.
    pub kind: JobKind,
}

impl Job {
    /// Encode the record into a new buffer.
    ///
    /// # Errors
    ///
    /// Returns [`JobError::TooLarge`] when a field does not fit the bounded format, or
    /// [`JobError::BadText`] when a text field is not valid UTF-8.
    pub fn encode(&self) -> Result<Vec<u8>, JobError> {
        let mut out = Vec::new();
        out.extend_from_slice(MAGIC);
        put_u64(&mut out, self.id);
        put_str(&mut out, &self.user)?;
        put_str(&mut out, &self.system)?;
        out.push(self.grade);
        match &self.kind {
            JobKind::Copy {
                source,
                destination,
                mode,
                data,
            } => {
                out.push(1);
                put_str(&mut out, source)?;
                put_str(&mut out, destination)?;
                put_u32(&mut out, *mode);
                put_bytes(&mut out, data)?;
            }
            JobKind::Fetch {
                source,
                destination,
            } => {
                out.push(2);
                put_str(&mut out, source)?;
                put_str(&mut out, destination)?;
            }
            JobKind::Command {
                command,
                stdin,
                files,
                outputs,
            } => {
                out.push(3);
                put_str(&mut out, command)?;
                put_bytes(&mut out, stdin)?;
                put_u32(
                    &mut out,
                    u32::try_from(files.len()).map_err(|_| JobError::TooLarge)?,
                );
                for file in files {
                    put_str(&mut out, &file.name)?;
                    put_bytes(&mut out, &file.data)?;
                }
                put_u32(
                    &mut out,
                    u32::try_from(outputs.len()).map_err(|_| JobError::TooLarge)?,
                );
                for output in outputs {
                    put_str(&mut out, output)?;
                }
            }
        }
        if out.len() > MAX_JOB {
            return Err(JobError::TooLarge);
        }
        Ok(out)
    }

    /// Decode one record.
    ///
    /// # Errors
    ///
    /// Returns [`JobError`] for a truncated, oversized or malformed record.
    pub fn decode(bytes: &[u8]) -> Result<Self, JobError> {
        if bytes.len() > MAX_JOB {
            return Err(JobError::TooLarge);
        }
        if !bytes.starts_with(MAGIC) {
            return Err(JobError::BadMagic);
        }
        let mut cursor = Cursor {
            bytes,
            position: MAGIC.len(),
        };
        let id = cursor.u64()?;
        let user = cursor.string()?;
        let system = cursor.string()?;
        let grade = cursor.u8()?;
        let kind = match cursor.u8()? {
            1 => {
                let source = cursor.string()?;
                let destination = cursor.string()?;
                let mode = cursor.u32()?;
                let data = cursor.bytes()?.to_vec();
                JobKind::Copy {
                    source,
                    destination,
                    mode,
                    data,
                }
            }
            2 => {
                let source = cursor.string()?;
                let destination = cursor.string()?;
                JobKind::Fetch {
                    source,
                    destination,
                }
            }
            3 => {
                let command = cursor.string()?;
                let stdin = cursor.bytes()?.to_vec();
                let count = cursor.u32()? as usize;
                let mut files = Vec::new();
                for _ in 0..count {
                    let name = cursor.string()?;
                    let data = cursor.bytes()?.to_vec();
                    files.push(NamedFile { name, data });
                }
                let output_count = cursor.u32()? as usize;
                let mut outputs = Vec::new();
                for _ in 0..output_count {
                    outputs.push(cursor.string()?);
                }
                JobKind::Command {
                    command,
                    stdin,
                    files,
                    outputs,
                }
            }
            _ => return Err(JobError::BadKind),
        };
        if cursor.position != bytes.len() {
            return Err(JobError::TrailingData);
        }
        Ok(Self {
            id,
            user,
            system,
            grade,
            kind,
        })
    }
}

fn put_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_bytes(out: &mut Vec<u8>, value: &[u8]) -> Result<(), JobError> {
    put_u32(
        out,
        u32::try_from(value.len()).map_err(|_| JobError::TooLarge)?,
    );
    out.extend_from_slice(value);
    Ok(())
}

fn put_str(out: &mut Vec<u8>, value: &str) -> Result<(), JobError> {
    put_bytes(out, value.as_bytes())
}

struct Cursor<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> Cursor<'a> {
    fn take(&mut self, count: usize) -> Result<&'a [u8], JobError> {
        let end = self
            .position
            .checked_add(count)
            .ok_or(JobError::Truncated)?;
        let slice = self
            .bytes
            .get(self.position..end)
            .ok_or(JobError::Truncated)?;
        self.position = end;
        Ok(slice)
    }

    fn u8(&mut self) -> Result<u8, JobError> {
        let slice = self.take(1)?;
        slice.first().copied().ok_or(JobError::Truncated)
    }

    fn u32(&mut self) -> Result<u32, JobError> {
        let slice = self.take(4)?;
        let array: [u8; 4] = slice.try_into().map_err(|_| JobError::Truncated)?;
        Ok(u32::from_le_bytes(array))
    }

    fn u64(&mut self) -> Result<u64, JobError> {
        let slice = self.take(8)?;
        let array: [u8; 8] = slice.try_into().map_err(|_| JobError::Truncated)?;
        Ok(u64::from_le_bytes(array))
    }

    fn bytes(&mut self) -> Result<&'a [u8], JobError> {
        let count = self.u32()? as usize;
        self.take(count)
    }

    fn string(&mut self) -> Result<String, JobError> {
        let bytes = self.bytes()?;
        String::from_utf8(bytes.to_vec()).map_err(|_| JobError::BadText)
    }
}

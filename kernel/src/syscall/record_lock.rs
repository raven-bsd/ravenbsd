// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! POSIX advisory byte-range record locks (`RECORD_LOCK`, raw number `0x0C`).
//!
//! Locks are owned by `(process id, server process id, inode)`. The inode comes from the kernel's
//! inline `FSTAT` path, so two descriptors for the same file share one lock set while different
//! servers with the same inode number stay separate. A close of any descriptor for the file
//! releases the process's locks for that file; process exit/terminate releases all of its locks.
//! `F_SETLKW` blocks on a global wait queue and retries after every range change.

#[cfg(target_arch = "aarch64")]
use crate::arch::aarch64::ipc::{self, SchemeFile, router};
#[cfg(target_arch = "x86_64")]
use crate::ipc::{self, SchemeFile, router};

use alloc::sync::Arc;
use alloc::vec::Vec;

use raven_abi::errno::Errno;
use raven_abi::types::fcntl::{GETLK, RDLCK, RecordLock, SETLK, SETLKW, UNLCK, WRLCK};
use raven_abi::types::{Handle, Rights};
use raven_usercopy::UserSlice;
use spin::Mutex;

use crate::process::{self, Process};
use crate::sched;
use crate::sched::wait::WaitQueue;
use crate::syscall::SyscallFrame;
use crate::usercopy::{copy_from_user, copy_to_user};

/// `SEEK_SET` for `l_whence`.
const SEEK_SET: i16 = 0;
/// `SEEK_END` for `l_whence`.
const SEEK_END: i16 = 2;

/// Global lock table.
static TABLE: Mutex<Table> = Mutex::new(Table::new());
/// All `F_SETLKW` waiters; every unlock wakes them to retry.
static WAIT: WaitQueue = WaitQueue::new();

/// One inclusive byte range. `end == u64::MAX` means to EOF.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Range {
    start: u64,
    end: u64,
}

impl Range {
    fn overlaps(self, other: Self) -> bool {
        self.start <= other.end && other.start <= self.end
    }
}

/// One owned lock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Entry {
    owner: u64,
    server: u64,
    ino: u64,
    kind: i16,
    range: Range,
}

/// The lock table. A flat vector is enough: a process holds only a few ranges.
struct Table {
    entries: Vec<Entry>,
}

impl Table {
    const fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    fn has_owner(&self, owner: u64) -> bool {
        self.entries.iter().any(|entry| entry.owner == owner)
    }

    /// The first conflicting entry for `owner`/`range`, if any.
    ///
    /// Same-process locks never conflict. Two read locks are compatible; anything involving a
    /// write lock conflicts.
    fn conflict(
        &self,
        owner: u64,
        server: u64,
        ino: u64,
        kind: i16,
        range: Range,
    ) -> Option<Entry> {
        let mut found: Option<Entry> = None;
        for entry in &self.entries {
            if entry.owner == owner || entry.server != server || entry.ino != ino {
                continue;
            }
            if !entry.range.overlaps(range) || (kind == RDLCK && entry.kind == RDLCK) {
                continue;
            }
            if found
                .as_ref()
                .map(|current: &Entry| entry.range.start < current.range.start)
                .unwrap_or(true)
            {
                found = Some(*entry);
            }
        }
        found
    }

    /// Replace the owner's locks in `range` with `kind`.
    fn set(&mut self, entry: Entry) {
        let mut kept = Vec::new();
        for current in self.entries.drain(..) {
            if current.owner != entry.owner
                || current.server != entry.server
                || current.ino != entry.ino
                || !current.range.overlaps(entry.range)
            {
                kept.push(current);
                continue;
            }
            if current.range.start < entry.range.start {
                kept.push(Entry {
                    range: Range {
                        start: current.range.start,
                        end: entry.range.start.saturating_sub(1),
                    },
                    ..current
                });
            }
            if current.range.end > entry.range.end && entry.range.end < u64::MAX {
                kept.push(Entry {
                    range: Range {
                        start: entry.range.end.saturating_add(1),
                        end: current.range.end,
                    },
                    ..current
                });
            }
        }
        kept.push(entry);
        self.entries = kept;
    }

    /// Remove the owner's locks in `range`.
    fn unlock(&mut self, owner: u64, server: u64, ino: u64, range: Range) {
        let mut kept = Vec::new();
        for current in self.entries.drain(..) {
            if current.owner != owner
                || current.server != server
                || current.ino != ino
                || !current.range.overlaps(range)
            {
                kept.push(current);
                continue;
            }
            if current.range.start < range.start {
                kept.push(Entry {
                    range: Range {
                        start: current.range.start,
                        end: range.start.saturating_sub(1),
                    },
                    ..current
                });
            }
            if current.range.end > range.end && range.end < u64::MAX {
                kept.push(Entry {
                    range: Range {
                        start: range.end.saturating_add(1),
                        end: current.range.end,
                    },
                    ..current
                });
            }
        }
        self.entries = kept;
    }

    fn remove_file(&mut self, owner: u64, server: u64, ino: u64) {
        self.entries
            .retain(|entry| entry.owner != owner || entry.server != server || entry.ino != ino);
    }

    fn remove_process(&mut self, owner: u64) {
        self.entries.retain(|entry| entry.owner != owner);
    }
}

fn resolve_range(lock: &RecordLock, size: u64) -> Result<Range, Errno> {
    let base = match lock.l_whence {
        SEEK_SET => 0i128,
        SEEK_END => i128::from(size),
        _ => return Err(Errno::EINVAL),
    };
    let start = base
        .checked_add(i128::from(lock.l_start))
        .ok_or(Errno::EINVAL)?;
    if start < 0 || start > i128::from(u64::MAX) {
        return Err(Errno::EINVAL);
    }
    let start = start as u64;
    if lock.l_len < 0 {
        return Err(Errno::EINVAL);
    }
    if lock.l_len == 0 {
        return Ok(Range {
            start,
            end: u64::MAX,
        });
    }
    let end = start
        .checked_add((lock.l_len as u64).saturating_sub(1))
        .ok_or(Errno::EINVAL)?;
    Ok(Range { start, end })
}

fn decode(raw: &[u8; 32]) -> RecordLock {
    RecordLock {
        l_type: i16::from_le_bytes([raw[0], raw[1]]),
        l_whence: i16::from_le_bytes([raw[2], raw[3]]),
        l_start: i64::from_le_bytes([
            raw[8], raw[9], raw[10], raw[11], raw[12], raw[13], raw[14], raw[15],
        ]),
        l_len: i64::from_le_bytes([
            raw[16], raw[17], raw[18], raw[19], raw[20], raw[21], raw[22], raw[23],
        ]),
        l_pid: i32::from_le_bytes([raw[24], raw[25], raw[26], raw[27]]),
    }
}

fn encode(lock: &RecordLock, raw: &mut [u8; 32]) {
    raw[0..2].copy_from_slice(&lock.l_type.to_le_bytes());
    raw[2..4].copy_from_slice(&lock.l_whence.to_le_bytes());
    raw[4..8].fill(0);
    raw[8..16].copy_from_slice(&lock.l_start.to_le_bytes());
    raw[16..24].copy_from_slice(&lock.l_len.to_le_bytes());
    raw[24..28].copy_from_slice(&lock.l_pid.to_le_bytes());
    raw[28..32].fill(0);
}

/// Try to set or clear a lock without blocking.
fn try_set(owner: u64, server: u64, ino: u64, kind: i16, range: Range) -> Result<bool, Errno> {
    let mut table = TABLE.lock();
    if kind != UNLCK && table.conflict(owner, server, ino, kind, range).is_some() {
        return Ok(false);
    }
    if kind == UNLCK {
        table.unlock(owner, server, ino, range);
    } else {
        table.set(Entry {
            owner,
            server,
            ino,
            kind,
            range,
        });
    }
    drop(table);
    WAIT.wake(0, 0, usize::MAX);
    Ok(true)
}

/// `RECORD_LOCK`: `record_lock(file: Handle, cmd: u32, lock: *mut RecordLock)`.
pub fn record_lock(frame: &SyscallFrame) -> Result<usize, Errno> {
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let handle = Handle(u32::try_from(frame.rdi).map_err(|_| Errno::EBADF)?);
    let cmd = u32::try_from(frame.rsi).map_err(|_| Errno::EINVAL)?;
    if !matches!(cmd, GETLK | SETLK | SETLKW) {
        return Err(Errno::EINVAL);
    }
    let mut raw = [0u8; 32];
    let source = UserSlice::new(frame.rdx, 32)?;
    copy_from_user(&mut raw, source)?;
    let lock = decode(&raw);
    if !matches!(lock.l_type, RDLCK | WRLCK | UNLCK) {
        return Err(Errno::EINVAL);
    }

    let file: Arc<SchemeFile> = ipc::file_from_handle(&caller, handle, Rights::NONE)?;
    let stat = router::fstat_kernel(&caller, &file)?;
    let ino = u64::from_le_bytes([
        stat[0], stat[1], stat[2], stat[3], stat[4], stat[5], stat[6], stat[7],
    ]);
    let size = u64::from_le_bytes([
        stat[8], stat[9], stat[10], stat[11], stat[12], stat[13], stat[14], stat[15],
    ]);
    let owner = caller.id;
    let server = file.server.id;
    let range = resolve_range(&lock, size)?;

    match cmd {
        GETLK => {
            let conflict = TABLE
                .lock()
                .conflict(owner, server, ino, lock.l_type, range);
            let result = match conflict {
                Some(entry) => RecordLock {
                    l_type: entry.kind,
                    l_whence: SEEK_SET,
                    l_start: entry.range.start as i64,
                    l_len: if entry.range.end == u64::MAX {
                        0
                    } else {
                        i64::try_from(
                            entry
                                .range
                                .end
                                .saturating_sub(entry.range.start)
                                .saturating_add(1),
                        )
                        .unwrap_or(0)
                    },
                    l_pid: entry.owner as i32,
                },
                None => RecordLock {
                    l_type: UNLCK,
                    l_whence: SEEK_SET,
                    l_start: 0,
                    l_len: 0,
                    l_pid: 0,
                },
            };
            let mut out = [0u8; 32];
            encode(&result, &mut out);
            let destination = UserSlice::new(frame.rdx, 32)?;
            copy_to_user(destination, &out)?;
            Ok(0)
        }
        SETLK => {
            if try_set(owner, server, ino, lock.l_type, range)? {
                Ok(0)
            } else {
                Err(Errno::EAGAIN)
            }
        }
        SETLKW => loop {
            if try_set(owner, server, ino, lock.l_type, range)? {
                return Ok(0);
            }
            let token = sched::begin_block_interruptible()?;
            if sched::take_kick() {
                sched::end_block(token);
                return Err(Errno::EINTR);
            }
            let queued = WAIT.enqueue_if(0, 0, || {
                let table = TABLE.lock();
                Ok(table
                    .conflict(owner, server, ino, lock.l_type, range)
                    .is_some())
            })?;
            if !queued {
                sched::end_block(token);
                continue;
            }
            let status = sched::block_current();
            sched::end_block(token);
            if status == Errno::EINTR.to_ret() {
                return Err(Errno::EINTR);
            }
        },
        _ => Err(Errno::EINVAL),
    }
}

/// Whether `owner` holds any lock; used to keep close fast.
pub fn has_locks_for(owner: u64) -> bool {
    TABLE.lock().has_owner(owner)
}

/// Release every lock `owner` holds on `file` (any descriptor close).
pub fn release_file_for(caller: &Arc<Process>, file: &Arc<SchemeFile>) {
    if !has_locks_for(caller.id) {
        return;
    }
    let Ok(stat) = router::fstat_kernel(caller, file) else {
        return;
    };
    let ino = u64::from_le_bytes([
        stat[0], stat[1], stat[2], stat[3], stat[4], stat[5], stat[6], stat[7],
    ]);
    let key = (caller.id, file.server.id, ino);
    TABLE.lock().remove_file(key.0, key.1, key.2);
    WAIT.wake(0, 0, usize::MAX);
}

/// Release every lock `owner` holds (process exit or terminate).
pub fn release_process(owner: u64) {
    let mut table = TABLE.lock();
    if !table.has_owner(owner) {
        return;
    }
    table.remove_process(owner);
    drop(table);
    WAIT.wake(0, 0, usize::MAX);
}

#[cfg(test)]
#[path = "record_lock_ut.rs"]
mod tests;

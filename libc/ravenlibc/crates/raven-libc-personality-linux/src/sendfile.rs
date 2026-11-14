// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `sendfile` and `copy_file_range` (`X73`, `11` 3.9 T2).
//!
//! One 64 KiB bounce buffer per call (`Vec`, `ENOMEM` on a failed reservation; never a 64 KiB
//! stack array on a 16 KiB thread stack). `sendfile` clamps the count to `0x7ffff000`, uses
//! `pread`/`pwrite` when an explicit offset is given (needs `PG-22`) and advances the pointer
//! without moving the descriptor position, otherwise the plain `read`/`write` pair. A short
//! read ends the copy; an error after some bytes returns the byte count, never losing the data
//! that already moved. `copy_file_range` never returns `EXDEV`: both paths are the same loop.

use alloc::vec::Vec;

use crate::seams::{Errno, FileIo};

/// The Linux `sendfile` count limit (`0x7ffff000`).
pub const SEND_COUNT_MAX: usize = 0x7fff_f000;
/// The bounce-buffer size.
pub const BOUNCE: usize = 65536;

fn bounce() -> Result<Vec<u8>, Errno> {
    let mut buffer = Vec::new();
    buffer
        .try_reserve_exact(BOUNCE)
        .map_err(|_| Errno::ENOMEM)?;
    buffer.resize(BOUNCE, 0);
    Ok(buffer)
}

/// `sendfile(out_fd, in_fd, offset, count)`.
pub fn sendfile<F: FileIo>(
    f: &mut F,
    out_fd: i32,
    in_fd: i32,
    offset: Option<&mut i64>,
    count: usize,
) -> Result<usize, Errno> {
    let remaining_total = count.min(SEND_COUNT_MAX);
    if remaining_total == 0 {
        return Ok(0);
    }
    let mut buffer = bounce()?;
    let mut offset = offset;
    let mut position = match offset.as_deref_mut() {
        Some(value) => {
            if *value < 0 {
                return Err(Errno::EINVAL);
            }
            Some(*value)
        }
        None => None,
    };
    let mut total = 0usize;
    let mut remaining = remaining_total;
    while remaining > 0 {
        let chunk = remaining.min(BOUNCE);
        let Some(window) = buffer.get_mut(..chunk) else {
            break;
        };
        let read = match position.as_mut() {
            Some(pos) => f.pread(in_fd, window, *pos),
            None => f.read(in_fd, window),
        };
        let read = match read {
            Ok(value) => value,
            Err(error) => {
                if total > 0 {
                    write_back(offset, position)?;
                    return Ok(total);
                }
                return Err(error);
            }
        };
        if read == 0 {
            break;
        }
        let Some(source) = buffer.get(..read) else {
            break;
        };
        let mut written = 0usize;
        while written < source.len() {
            let Some(piece) = source.get(written..) else {
                break;
            };
            let step = match f.write(out_fd, piece) {
                Ok(value) => value,
                Err(error) => {
                    if total > 0 {
                        write_back(offset, position)?;
                        return Ok(total);
                    }
                    return Err(error);
                }
            };
            if step == 0 {
                if total > 0 {
                    write_back(offset, position)?;
                    return Ok(total);
                }
                return Err(Errno::EIO);
            }
            written = written.saturating_add(step);
        }
        if let Some(pos) = position.as_mut() {
            *pos = pos
                .checked_add(i64::try_from(read).unwrap_or(0))
                .ok_or(Errno::EOVERFLOW)?;
        }
        total = total.saturating_add(read);
        remaining = remaining.saturating_sub(read);
        if read < chunk {
            break;
        }
    }
    write_back(offset, position)?;
    Ok(total)
}

fn write_back(offset: Option<&mut i64>, position: Option<i64>) -> Result<(), Errno> {
    if let (Some(offset), Some(position)) = (offset, position) {
        *offset = position;
    }
    Ok(())
}

/// Whether two half-open ranges overlap.
#[must_use]
pub fn ranges_overlap(a: i64, b: i64, len: i64) -> bool {
    let a_end = a.saturating_add(len);
    let b_end = b.saturating_add(len);
    a < b_end && b < a_end
}

/// `copy_file_range(fd_in, off_in, fd_out, off_out, len, flags)`.
pub fn copy_file_range<F: FileIo>(
    f: &mut F,
    fd_in: i32,
    off_in: Option<&mut i64>,
    fd_out: i32,
    off_out: Option<&mut i64>,
    len: usize,
    flags: u32,
) -> Result<usize, Errno> {
    if flags != 0 {
        return Err(Errno::EINVAL);
    }
    if len == 0 {
        return Ok(0);
    }
    if fd_in == fd_out {
        // Without a way to observe the current cursor through this seam a same-descriptor
        // copy cannot be proven non-overlapping; Linux's answer for overlap is EINVAL.
        return Err(Errno::EINVAL);
    }
    let mut buffer = bounce()?;
    let mut off_in = off_in;
    let mut off_out = off_out;
    let mut pos_in = match off_in.as_deref_mut() {
        Some(value) if *value < 0 => return Err(Errno::EINVAL),
        Some(value) => Some(*value),
        None => None,
    };
    let mut pos_out = match off_out.as_deref_mut() {
        Some(value) if *value < 0 => return Err(Errno::EINVAL),
        Some(value) => Some(*value),
        None => None,
    };
    let mut total = 0usize;
    let mut remaining = len;
    while remaining > 0 {
        let chunk = remaining.min(BOUNCE);
        let Some(window) = buffer.get_mut(..chunk) else {
            break;
        };
        let read = match pos_in.as_mut() {
            Some(pos) => f.pread(fd_in, window, *pos),
            None => f.read(fd_in, window),
        };
        let read = match read {
            Ok(value) => value,
            Err(error) => {
                if total > 0 {
                    return total.with_offsets(off_in, pos_in, off_out, pos_out);
                }
                return Err(error);
            }
        };
        if read == 0 {
            break;
        }
        let Some(source) = buffer.get(..read) else {
            break;
        };
        let mut written = 0usize;
        while written < source.len() {
            let Some(piece) = source.get(written..) else {
                break;
            };
            let step = match pos_out.as_mut() {
                Some(pos) => f.pwrite(fd_out, piece, *pos),
                None => f.write(fd_out, piece),
            };
            let step = match step {
                Ok(value) => value,
                Err(error) => {
                    if total > 0 {
                        return total.with_offsets(off_in, pos_in, off_out, pos_out);
                    }
                    return Err(error);
                }
            };
            if step == 0 {
                if total > 0 {
                    return total.with_offsets(off_in, pos_in, off_out, pos_out);
                }
                return Err(Errno::EIO);
            }
            if let Some(pos) = pos_out.as_mut() {
                *pos = pos
                    .checked_add(i64::try_from(step).unwrap_or(0))
                    .ok_or(Errno::EOVERFLOW)?;
            }
            written = written.saturating_add(step);
        }
        if let Some(pos) = pos_in.as_mut() {
            *pos = pos
                .checked_add(i64::try_from(read).unwrap_or(0))
                .ok_or(Errno::EOVERFLOW)?;
        }
        total = total.saturating_add(read);
        remaining = remaining.saturating_sub(read);
        if read < chunk {
            break;
        }
    }
    total.with_offsets(off_in, pos_in, off_out, pos_out)
}

trait OffsetResult {
    fn with_offsets(
        self,
        off_in: Option<&mut i64>,
        pos_in: Option<i64>,
        off_out: Option<&mut i64>,
        pos_out: Option<i64>,
    ) -> Result<usize, Errno>;
}

impl OffsetResult for usize {
    fn with_offsets(
        self,
        off_in: Option<&mut i64>,
        pos_in: Option<i64>,
        off_out: Option<&mut i64>,
        pos_out: Option<i64>,
    ) -> Result<usize, Errno> {
        if let (Some(offset), Some(position)) = (off_in, pos_in) {
            *offset = position;
        }
        if let (Some(offset), Some(position)) = (off_out, pos_out) {
            *offset = position;
        }
        Ok(self)
    }
}

#[cfg(test)]
#[path = "sendfile_ut.rs"]
mod tests;

// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Versioned control requests and responses carried by the S11 `CALL` opcode.
//!
//! A request starts with `version: u16, op: u16, flags: u32, arg0: u32, arg1: u32`, followed by
//! up to two length-prefixed paths. The number of paths and the meaning of `arg0`/`arg1` is fixed
//! by the operation. Unknown versions and operations are reported as protocol errors; a server
//! maps them to `-ENOSYS`.

use crate::ProtoError;
use crate::path::MAX_PATH;
use crate::wire::{Reader, Writer};

/// Encoded size of the fixed [`CallRequest`] header in bytes.
pub const CALL_HEADER_LEN: usize = 16;
/// Maximum number of paths in one control request.
pub const MAX_CALL_PATHS: usize = 2;

/// `MKDIR(mode)` followed by one path.
pub const OP_MKDIR: u16 = 1;
/// `UNLINK` followed by one path.
pub const OP_UNLINK: u16 = 2;
/// `RMDIR` followed by one path.
pub const OP_RMDIR: u16 = 3;
/// `RENAME(path_from, path_to)`.
pub const OP_RENAME: u16 = 4;
/// `TRUNCATE(size)` on the open file handle; no paths.
pub const OP_TRUNCATE: u16 = 5;
/// `SYMLINK(path, target)`.
pub const OP_SYMLINK: u16 = 6;
/// `READLINK(path)` followed by one path.
pub const OP_READLINK: u16 = 7;
/// `CHMOD(mode)` followed by one path.
pub const OP_CHMOD: u16 = 8;
/// `SYNC`; no paths.
pub const OP_SYNC: u16 = 9;
/// `MOUNT(path, scheme)`; handled by `vfsd`.
pub const OP_MOUNT: u16 = 10;
/// `UMOUNT(path)`; handled by `vfsd`.
pub const OP_UMOUNT: u16 = 11;
/// `STATFS`; no paths.
pub const OP_STATFS: u16 = 12;

/// Encoded size of a [`StatFs`] payload in bytes.
pub const STATFS_LEN: usize = 36;

/// One typed control operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Call<'a> {
    /// Create a directory with `mode` at `path`.
    Mkdir {
        /// Permission bits for the new directory.
        mode: u32,
        /// Path relative to the directory handle.
        path: &'a [u8],
    },
    /// Remove one file at `path`.
    Unlink {
        /// Path relative to the directory handle.
        path: &'a [u8],
    },
    /// Remove one empty directory at `path`.
    Rmdir {
        /// Path relative to the directory handle.
        path: &'a [u8],
    },
    /// Rename `from` to `to`.
    Rename {
        /// Source path.
        from: &'a [u8],
        /// Destination path.
        to: &'a [u8],
    },
    /// Truncate the open file to `size` bytes.
    Truncate {
        /// New file length.
        size: u64,
    },
    /// Create a symbolic link at `path` pointing to `target`.
    Symlink {
        /// Link path.
        path: &'a [u8],
        /// Link target.
        target: &'a [u8],
    },
    /// Read the target of the symbolic link at `path`.
    Readlink {
        /// Link path.
        path: &'a [u8],
    },
    /// Change permission bits to `mode` at `path`.
    Chmod {
        /// New permission bits.
        mode: u32,
        /// Path relative to the directory handle.
        path: &'a [u8],
    },
    /// Flush pending data to stable storage.
    Sync,
    /// Attach `scheme` at the absolute mount point `path`.
    Mount {
        /// Absolute mount point.
        path: &'a [u8],
        /// Scheme name such as `ramfs:`.
        scheme: &'a [u8],
    },
    /// Detach the mount at the absolute mount point `path`.
    Umount {
        /// Absolute mount point.
        path: &'a [u8],
    },
    /// Read filesystem statistics.
    Statfs,
}

impl Call<'_> {
    /// Wire operation code.
    pub const fn op(&self) -> u16 {
        match self {
            Self::Mkdir { .. } => OP_MKDIR,
            Self::Unlink { .. } => OP_UNLINK,
            Self::Rmdir { .. } => OP_RMDIR,
            Self::Rename { .. } => OP_RENAME,
            Self::Truncate { .. } => OP_TRUNCATE,
            Self::Symlink { .. } => OP_SYMLINK,
            Self::Readlink { .. } => OP_READLINK,
            Self::Chmod { .. } => OP_CHMOD,
            Self::Sync => OP_SYNC,
            Self::Mount { .. } => OP_MOUNT,
            Self::Umount { .. } => OP_UMOUNT,
            Self::Statfs => OP_STATFS,
        }
    }

    /// `arg0` and `arg1` for this operation.
    const fn args(&self) -> (u32, u32) {
        match self {
            Self::Mkdir { mode, .. } | Self::Chmod { mode, .. } => (*mode, 0),
            Self::Truncate { size } => (*size as u32, (*size >> 32) as u32),
            _ => (0, 0),
        }
    }

    /// The paths carried by this operation, in wire order.
    const fn paths(&self) -> (Option<&[u8]>, Option<&[u8]>) {
        match self {
            Self::Mkdir { path, .. }
            | Self::Unlink { path }
            | Self::Rmdir { path }
            | Self::Readlink { path }
            | Self::Chmod { path, .. }
            | Self::Umount { path } => (Some(path), None),
            Self::Rename { from, to } => (Some(from), Some(to)),
            Self::Symlink { path, target } => (Some(path), Some(target)),
            Self::Mount { path, scheme } => (Some(path), Some(scheme)),
            Self::Truncate { .. } | Self::Sync | Self::Statfs => (None, None),
        }
    }
}

/// One control request: the typed operation plus its reserved `flags` word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CallRequest<'a> {
    /// Reserved flags word; send zero in this protocol version.
    pub flags: u32,
    /// Typed operation.
    pub call: Call<'a>,
}

impl<'a> CallRequest<'a> {
    /// Build a request with zero flags.
    pub const fn new(call: Call<'a>) -> Self {
        Self { flags: 0, call }
    }

    /// The operation code.
    pub const fn op(&self) -> u16 {
        self.call.op()
    }

    /// Encode the request into `out`; returns the number of bytes written.
    pub fn encode(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        let (arg0, arg1) = self.call.args();
        let mut writer = Writer::new(out);
        writer.put_u16(crate::VERSION)?;
        writer.put_u16(self.call.op())?;
        writer.put_u32(self.flags)?;
        writer.put_u32(arg0)?;
        writer.put_u32(arg1)?;
        let (first, second) = self.call.paths();
        if let Some(path) = first {
            write_path(&mut writer, path)?;
        }
        if let Some(path) = second {
            write_path(&mut writer, path)?;
        }
        Ok(writer.position())
    }

    /// Decode one complete control request.
    pub fn decode(input: &'a [u8]) -> Result<Self, ProtoError> {
        let mut reader = Reader::new(input);
        let version = reader.get_u16()?;
        if version != crate::VERSION {
            return Err(ProtoError::UnsupportedVersion);
        }
        let op = reader.get_u16()?;
        let flags = reader.get_u32()?;
        let arg0 = reader.get_u32()?;
        let arg1 = reader.get_u32()?;
        let path_count = path_count(op).ok_or(ProtoError::UnknownOp(op))?;
        let first = if path_count >= 1 {
            Some(read_path(&mut reader)?)
        } else {
            None
        };
        let second = if path_count >= 2 {
            Some(read_path(&mut reader)?)
        } else {
            None
        };
        if !reader.is_empty() {
            return Err(ProtoError::TrailingBytes);
        }
        let call = match op {
            OP_MKDIR => Call::Mkdir {
                mode: arg0,
                path: first.ok_or(ProtoError::Truncated)?,
            },
            OP_UNLINK => Call::Unlink {
                path: first.ok_or(ProtoError::Truncated)?,
            },
            OP_RMDIR => Call::Rmdir {
                path: first.ok_or(ProtoError::Truncated)?,
            },
            OP_RENAME => Call::Rename {
                from: first.ok_or(ProtoError::Truncated)?,
                to: second.ok_or(ProtoError::Truncated)?,
            },
            OP_TRUNCATE => Call::Truncate {
                size: u64::from(arg0) | (u64::from(arg1) << 32),
            },
            OP_SYMLINK => Call::Symlink {
                path: first.ok_or(ProtoError::Truncated)?,
                target: second.ok_or(ProtoError::Truncated)?,
            },
            OP_READLINK => Call::Readlink {
                path: first.ok_or(ProtoError::Truncated)?,
            },
            OP_CHMOD => Call::Chmod {
                mode: arg0,
                path: first.ok_or(ProtoError::Truncated)?,
            },
            OP_SYNC => Call::Sync,
            OP_MOUNT => Call::Mount {
                path: first.ok_or(ProtoError::Truncated)?,
                scheme: second.ok_or(ProtoError::Truncated)?,
            },
            OP_UMOUNT => Call::Umount {
                path: first.ok_or(ProtoError::Truncated)?,
            },
            OP_STATFS => Call::Statfs,
            _ => return Err(ProtoError::UnknownOp(op)),
        };
        Ok(Self { flags, call })
    }
}

/// Filesystem statistics payload (`STATFS` success).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StatFs {
    /// Preferred block size in bytes.
    pub block_size: u32,
    /// Total blocks.
    pub blocks: u64,
    /// Free blocks.
    pub free_blocks: u64,
    /// Total inodes.
    pub files: u64,
    /// Free inodes.
    pub free_files: u64,
}

impl StatFs {
    /// Encode the 36-byte payload.
    pub fn encode(&self, out: &mut [u8; STATFS_LEN]) -> Result<(), ProtoError> {
        let mut writer = Writer::new(out);
        writer.put_u32(self.block_size)?;
        writer.put_u64(self.blocks)?;
        writer.put_u64(self.free_blocks)?;
        writer.put_u64(self.files)?;
        writer.put_u64(self.free_files)?;
        Ok(())
    }

    /// Decode exactly one 36-byte payload.
    pub fn decode(input: &[u8]) -> Result<Self, ProtoError> {
        if input.len() != STATFS_LEN {
            return Err(if input.len() < STATFS_LEN {
                ProtoError::Truncated
            } else {
                ProtoError::TrailingBytes
            });
        }
        let mut reader = Reader::new(input);
        Ok(Self {
            block_size: reader.get_u32()?,
            blocks: reader.get_u64()?,
            free_blocks: reader.get_u64()?,
            files: reader.get_u64()?,
            free_files: reader.get_u64()?,
        })
    }
}

/// One control response.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallResponse<'a> {
    /// Success or failure without a payload.
    Status(i32),
    /// `READLINK` success with the link target.
    Readlink {
        /// Zero on success.
        status: i32,
        /// Link target bytes.
        target: &'a [u8],
    },
    /// `STATFS` success with the filesystem record.
    StatFs {
        /// Zero on success.
        status: i32,
        /// Filesystem statistics.
        statfs: StatFs,
    },
}

impl<'a> CallResponse<'a> {
    /// Response status (zero or a negative errno).
    pub const fn status(&self) -> i32 {
        match self {
            Self::Status(status) | Self::Readlink { status, .. } | Self::StatFs { status, .. } => {
                *status
            }
        }
    }

    /// Encode the response into `out`; returns the number of bytes written.
    pub fn encode(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        validate_status(self.status())?;
        let mut writer = Writer::new(out);
        writer.put_i32(self.status())?;
        match self {
            Self::Status(_) => {}
            Self::Readlink { target, .. } => {
                validate_path(target)?;
                writer.put_u16(
                    u16::try_from(target.len()).map_err(|_| ProtoError::LengthOutOfRange)?,
                )?;
                writer.put_bytes(target)?;
            }
            Self::StatFs { statfs, .. } => {
                writer.put_u32(statfs.block_size)?;
                writer.put_u64(statfs.blocks)?;
                writer.put_u64(statfs.free_blocks)?;
                writer.put_u64(statfs.files)?;
                writer.put_u64(statfs.free_files)?;
            }
        }
        Ok(writer.position())
    }

    /// Decode one response for the request operation `op`.
    pub fn decode(op: u16, input: &'a [u8]) -> Result<Self, ProtoError> {
        let mut reader = Reader::new(input);
        let status = reader.get_i32()?;
        validate_status(status)?;
        if status != 0 {
            if !reader.is_empty() {
                return Err(ProtoError::TrailingBytes);
            }
            return Ok(Self::Status(status));
        }
        match op {
            OP_READLINK => {
                let length = reader.get_u16()? as usize;
                if length == 0 || length > MAX_PATH {
                    return Err(ProtoError::LengthOutOfRange);
                }
                let target = reader.get_bytes(length)?;
                validate_path(target)?;
                if !reader.is_empty() {
                    return Err(ProtoError::TrailingBytes);
                }
                Ok(Self::Readlink { status, target })
            }
            OP_STATFS => {
                let bytes = reader.get_bytes(STATFS_LEN)?;
                if !reader.is_empty() {
                    return Err(ProtoError::TrailingBytes);
                }
                Ok(Self::StatFs {
                    status,
                    statfs: StatFs::decode(bytes)?,
                })
            }
            OP_MKDIR | OP_UNLINK | OP_RMDIR | OP_RENAME | OP_TRUNCATE | OP_SYMLINK | OP_CHMOD
            | OP_SYNC | OP_MOUNT | OP_UMOUNT => {
                if !reader.is_empty() {
                    return Err(ProtoError::TrailingBytes);
                }
                Ok(Self::Status(status))
            }
            _ => Err(ProtoError::UnknownOp(op)),
        }
    }
}

const fn path_count(op: u16) -> Option<usize> {
    match op {
        OP_MKDIR | OP_UNLINK | OP_RMDIR | OP_READLINK | OP_CHMOD | OP_UMOUNT => Some(1),
        OP_RENAME | OP_SYMLINK | OP_MOUNT => Some(2),
        OP_TRUNCATE | OP_SYNC | OP_STATFS => Some(0),
        _ => None,
    }
}

fn write_path(writer: &mut Writer<'_>, path: &[u8]) -> Result<(), ProtoError> {
    validate_path(path)?;
    writer.put_u16(u16::try_from(path.len()).map_err(|_| ProtoError::LengthOutOfRange)?)?;
    writer.put_bytes(path)?;
    Ok(())
}

fn read_path<'a>(reader: &mut Reader<'a>) -> Result<&'a [u8], ProtoError> {
    let length = reader.get_u16()? as usize;
    if length == 0 || length > MAX_PATH {
        return Err(ProtoError::LengthOutOfRange);
    }
    let path = reader.get_bytes(length)?;
    validate_path(path)?;
    Ok(path)
}

fn validate_path(path: &[u8]) -> Result<(), ProtoError> {
    if path.is_empty() || path.len() > MAX_PATH {
        return Err(ProtoError::LengthOutOfRange);
    }
    if path.contains(&0) {
        return Err(ProtoError::Malformed);
    }
    core::str::from_utf8(path).map_err(|_| ProtoError::InvalidUtf8)?;
    Ok(())
}

const fn validate_status(status: i32) -> Result<(), ProtoError> {
    if status <= 0 && status >= -4095 {
        Ok(())
    } else {
        Err(ProtoError::BadStatus)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encoded(request: &CallRequest<'_>) -> Vec<u8> {
        let mut buffer = [0u8; 128];
        let length = request.encode(&mut buffer).unwrap();
        buffer[..length].to_vec()
    }

    #[test]
    fn mkdir_golden_bytes() {
        assert_eq!(
            encoded(&CallRequest::new(Call::Mkdir {
                mode: 0o755,
                path: b"hello"
            })),
            [
                0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0xed, 0x01, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x05, 0x00, b'h', b'e', b'l', b'l', b'o'
            ]
        );
    }

    #[test]
    fn unlink_golden_bytes() {
        assert_eq!(
            encoded(&CallRequest::new(Call::Unlink { path: b"x" })),
            [
                0x01, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x01, 0x00, b'x'
            ]
        );
    }

    #[test]
    fn rmdir_golden_bytes() {
        assert_eq!(
            encoded(&CallRequest::new(Call::Rmdir { path: b"d" })),
            [
                0x01, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x01, 0x00, b'd'
            ]
        );
    }

    #[test]
    fn rename_golden_bytes() {
        assert_eq!(
            encoded(&CallRequest::new(Call::Rename {
                from: b"a",
                to: b"b"
            })),
            [
                0x01, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x01, 0x00, b'a', 0x01, 0x00, b'b'
            ]
        );
    }

    #[test]
    fn truncate_golden_bytes() {
        assert_eq!(
            encoded(&CallRequest::new(Call::Truncate {
                size: 0x0000_0001_0000_0002
            })),
            [
                0x01, 0x00, 0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x01, 0x00,
                0x00, 0x00
            ]
        );
    }

    #[test]
    fn symlink_golden_bytes() {
        assert_eq!(
            encoded(&CallRequest::new(Call::Symlink {
                path: b"l",
                target: b"t"
            })),
            [
                0x01, 0x00, 0x06, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x01, 0x00, b'l', 0x01, 0x00, b't'
            ]
        );
    }

    #[test]
    fn readlink_golden_bytes() {
        assert_eq!(
            encoded(&CallRequest::new(Call::Readlink { path: b"l" })),
            [
                0x01, 0x00, 0x07, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x01, 0x00, b'l'
            ]
        );
    }

    #[test]
    fn chmod_golden_bytes() {
        assert_eq!(
            encoded(&CallRequest::new(Call::Chmod {
                mode: 0o600,
                path: b"f"
            })),
            [
                0x01, 0x00, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0x01, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x01, 0x00, b'f'
            ]
        );
    }

    #[test]
    fn sync_golden_bytes() {
        assert_eq!(
            encoded(&CallRequest::new(Call::Sync)),
            [
                0x01, 0x00, 0x09, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00
            ]
        );
    }

    #[test]
    fn mount_golden_bytes() {
        assert_eq!(
            encoded(&CallRequest::new(Call::Mount {
                path: b"/tmp",
                scheme: b"ramfs:"
            })),
            [
                0x01, 0x00, 0x0a, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x04, 0x00, b'/', b't', b'm', b'p', 0x06, 0x00, b'r', b'a', b'm', b'f',
                b's', b':'
            ]
        );
    }

    #[test]
    fn umount_golden_bytes() {
        assert_eq!(
            encoded(&CallRequest::new(Call::Umount { path: b"/tmp" })),
            [
                0x01, 0x00, 0x0b, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x04, 0x00, b'/', b't', b'm', b'p'
            ]
        );
    }

    #[test]
    fn statfs_golden_bytes() {
        assert_eq!(
            encoded(&CallRequest::new(Call::Statfs)),
            [
                0x01, 0x00, 0x0c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00
            ]
        );
    }

    #[test]
    fn every_call_roundtrips() {
        let calls = [
            Call::Mkdir {
                mode: 0o755,
                path: b"hello",
            },
            Call::Unlink { path: b"x" },
            Call::Rmdir { path: b"d" },
            Call::Rename {
                from: b"a",
                to: b"b",
            },
            Call::Truncate {
                size: 0x1234_5678_9abc_def0,
            },
            Call::Symlink {
                path: b"l",
                target: b"t",
            },
            Call::Readlink { path: b"l" },
            Call::Chmod {
                mode: 0o600,
                path: b"f",
            },
            Call::Sync,
            Call::Mount {
                path: b"/tmp",
                scheme: b"ramfs:",
            },
            Call::Umount { path: b"/tmp" },
            Call::Statfs,
        ];
        for call in calls {
            let request = CallRequest {
                flags: 0x1122_3344,
                call,
            };
            let bytes = encoded(&request);
            assert_eq!(CallRequest::decode(&bytes).unwrap(), request);
        }
    }

    #[test]
    fn truncated_request_is_rejected() {
        let bytes = encoded(&CallRequest::new(Call::Sync));
        assert_eq!(
            CallRequest::decode(&bytes[..CALL_HEADER_LEN - 1]),
            Err(ProtoError::Truncated)
        );
    }

    #[test]
    fn trailing_request_bytes_are_rejected() {
        let mut bytes = encoded(&CallRequest::new(Call::Sync));
        bytes.push(0);
        assert_eq!(CallRequest::decode(&bytes), Err(ProtoError::TrailingBytes));
    }

    #[test]
    fn unknown_version_is_rejected() {
        let mut bytes = encoded(&CallRequest::new(Call::Sync));
        bytes[0] = 2;
        assert_eq!(
            CallRequest::decode(&bytes),
            Err(ProtoError::UnsupportedVersion)
        );
    }

    #[test]
    fn unknown_operation_is_rejected() {
        let mut bytes = [0u8; CALL_HEADER_LEN];
        bytes[0] = 1;
        bytes[2] = 99;
        assert_eq!(CallRequest::decode(&bytes), Err(ProtoError::UnknownOp(99)));
    }

    #[test]
    fn empty_and_oversized_paths_are_rejected() {
        let mut empty = encoded(&CallRequest::new(Call::Unlink { path: b"x" }));
        empty[CALL_HEADER_LEN] = 0;
        empty[CALL_HEADER_LEN + 1] = 0;
        assert_eq!(
            CallRequest::decode(&empty),
            Err(ProtoError::LengthOutOfRange)
        );

        let mut oversized = encoded(&CallRequest::new(Call::Unlink { path: b"x" }));
        oversized[CALL_HEADER_LEN] = 0;
        oversized[CALL_HEADER_LEN + 1] = 1;
        assert_eq!(
            CallRequest::decode(&oversized),
            Err(ProtoError::LengthOutOfRange)
        );
    }

    #[test]
    fn bad_path_bytes_are_rejected() {
        let mut invalid = encoded(&CallRequest::new(Call::Unlink { path: b"x" }));
        invalid[CALL_HEADER_LEN + 2] = 0xff;
        assert_eq!(CallRequest::decode(&invalid), Err(ProtoError::InvalidUtf8));

        let mut nul = encoded(&CallRequest::new(Call::Unlink { path: b"x" }));
        nul[CALL_HEADER_LEN + 2] = 0;
        assert_eq!(CallRequest::decode(&nul), Err(ProtoError::Malformed));
    }

    #[test]
    fn encoding_rejects_invalid_paths() {
        let mut buffer = [0u8; 128];
        assert_eq!(
            CallRequest::new(Call::Unlink { path: b"" }).encode(&mut buffer),
            Err(ProtoError::LengthOutOfRange)
        );
        assert_eq!(
            CallRequest::new(Call::Unlink { path: b"a\0b" }).encode(&mut buffer),
            Err(ProtoError::Malformed)
        );
    }

    #[test]
    fn response_status_golden_bytes() {
        let mut buffer = [0u8; 8];
        let length = CallResponse::Status(-2).encode(&mut buffer).unwrap();
        assert_eq!(&buffer[..length], &[0xfe, 0xff, 0xff, 0xff]);
    }

    #[test]
    fn readlink_response_golden_bytes() {
        let mut buffer = [0u8; 16];
        let response = CallResponse::Readlink {
            status: 0,
            target: b"abc",
        };
        let length = response.encode(&mut buffer).unwrap();
        assert_eq!(
            &buffer[..length],
            &[0x00, 0x00, 0x00, 0x00, 0x03, 0x00, b'a', b'b', b'c']
        );
    }

    #[test]
    fn statfs_response_golden_bytes() {
        let mut buffer = [0u8; 48];
        let response = CallResponse::StatFs {
            status: 0,
            statfs: StatFs {
                block_size: 4096,
                blocks: 10,
                free_blocks: 5,
                files: 7,
                free_files: 3,
            },
        };
        let length = response.encode(&mut buffer).unwrap();
        assert_eq!(length, 40);
        assert_eq!(
            &buffer[..length],
            &[
                0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x00, 0x0a, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x07, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00
            ]
        );
    }

    #[test]
    fn responses_roundtrip() {
        let responses = [
            CallResponse::Status(0),
            CallResponse::Status(-2),
            CallResponse::Readlink {
                status: 0,
                target: b"target",
            },
            CallResponse::StatFs {
                status: 0,
                statfs: StatFs {
                    block_size: 4096,
                    blocks: 1,
                    free_blocks: 2,
                    files: 3,
                    free_files: 4,
                },
            },
        ];
        for response in responses {
            let op = match response {
                CallResponse::Readlink { .. } => OP_READLINK,
                CallResponse::StatFs { .. } => OP_STATFS,
                CallResponse::Status(_) => OP_SYNC,
            };
            let mut buffer = [0u8; 64];
            let length = response.encode(&mut buffer).unwrap();
            assert_eq!(
                CallResponse::decode(op, &buffer[..length]).unwrap(),
                response
            );
        }
    }

    #[test]
    fn negative_status_has_no_payload() {
        let mut buffer = [0u8; 8];
        let length = CallResponse::Status(-2).encode(&mut buffer).unwrap();
        assert_eq!(
            CallResponse::decode(OP_READLINK, &buffer[..length]).unwrap(),
            CallResponse::Status(-2)
        );
    }

    #[test]
    fn success_readlink_requires_a_target() {
        let bytes = [0u8; 4];
        assert_eq!(
            CallResponse::decode(OP_READLINK, &bytes),
            Err(ProtoError::Truncated)
        );
    }

    #[test]
    fn success_statfs_requires_a_record() {
        let bytes = [0u8; 4];
        assert_eq!(
            CallResponse::decode(OP_STATFS, &bytes),
            Err(ProtoError::Truncated)
        );
    }

    #[test]
    fn response_status_range_is_checked() {
        let mut buffer = [0u8; 8];
        assert_eq!(
            CallResponse::Status(1).encode(&mut buffer),
            Err(ProtoError::BadStatus)
        );
        assert_eq!(
            CallResponse::decode(OP_SYNC, &[0x00, 0xf0, 0xff, 0xff]),
            Err(ProtoError::BadStatus)
        );
    }

    #[test]
    fn response_trailing_bytes_are_rejected() {
        let bytes = [0u8; 5];
        assert_eq!(
            CallResponse::decode(OP_SYNC, &bytes),
            Err(ProtoError::TrailingBytes)
        );
    }

    #[test]
    fn statfs_record_roundtrips() {
        let record = StatFs {
            block_size: 4096,
            blocks: 10,
            free_blocks: 5,
            files: 7,
            free_files: 3,
        };
        let mut bytes = [0u8; STATFS_LEN];
        record.encode(&mut bytes).unwrap();
        assert_eq!(StatFs::decode(&bytes).unwrap(), record);
        assert_eq!(StatFs::decode(&bytes[..35]), Err(ProtoError::Truncated));
        assert_eq!(
            StatFs::decode(&[0u8; STATFS_LEN + 1]),
            Err(ProtoError::TrailingBytes)
        );
    }

    #[test]
    fn path_count_table_is_complete() {
        for op in [
            OP_MKDIR,
            OP_UNLINK,
            OP_RMDIR,
            OP_RENAME,
            OP_TRUNCATE,
            OP_SYMLINK,
            OP_READLINK,
            OP_CHMOD,
            OP_SYNC,
            OP_MOUNT,
            OP_UMOUNT,
            OP_STATFS,
        ] {
            assert!(path_count(op).is_some());
        }
        assert_eq!(path_count(0), None);
        assert_eq!(path_count(13), None);
    }
}

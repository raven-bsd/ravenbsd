// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Linux-valued constants used by the file protocol.
//!
//! The numbers are copied from the Linux x86-64 ABI (DECISIONS.md DR-13) instead of depending on
//! `raven-linux-abi`: only this small, stable subset is needed on the wire.

/// Open for reading only.
pub const O_RDONLY: u32 = 0;
/// Open for writing only.
pub const O_WRONLY: u32 = 1;
/// Open for reading and writing.
pub const O_RDWR: u32 = 2;
/// Access-mode mask (`O_RDONLY` .. `O_RDWR`).
pub const O_ACCMODE: u32 = 0o3;
/// Create the file if it does not exist.
pub const O_CREAT: u32 = 0o100;
/// Fail when `O_CREAT` is used and the file already exists.
pub const O_EXCL: u32 = 0o200;
/// Truncate the file to zero length on open.
pub const O_TRUNC: u32 = 0o1000;
/// Append writes at the end of the file.
pub const O_APPEND: u32 = 0o2000;
/// Require the path to name a directory.
pub const O_DIRECTORY: u32 = 0o200000;
/// Do not follow a final symbolic link.
pub const O_NOFOLLOW: u32 = 0o400000;

/// Mask of every open flag this protocol version defines.
pub const OPEN_FLAGS_MASK: u32 =
    O_ACCMODE | O_CREAT | O_EXCL | O_TRUNC | O_APPEND | O_DIRECTORY | O_NOFOLLOW;

/// File-type mask in a `Stat::mode` value.
pub const S_IFMT: u32 = 0o170000;
/// Regular file.
pub const S_IFREG: u32 = 0o100000;
/// Directory.
pub const S_IFDIR: u32 = 0o040000;
/// Symbolic link.
pub const S_IFLNK: u32 = 0o120000;
/// Character device.
pub const S_IFCHR: u32 = 0o020000;
/// Block device.
pub const S_IFBLK: u32 = 0o060000;
/// FIFO.
pub const S_IFIFO: u32 = 0o010000;
/// Socket.
pub const S_IFSOCK: u32 = 0o140000;

/// Directory-entry type: unknown.
pub const DT_UNKNOWN: u8 = 0;
/// Directory-entry type: FIFO.
pub const DT_FIFO: u8 = 1;
/// Directory-entry type: character device.
pub const DT_CHR: u8 = 2;
/// Directory-entry type: directory.
pub const DT_DIR: u8 = 4;
/// Directory-entry type: block device.
pub const DT_BLK: u8 = 6;
/// Directory-entry type: regular file.
pub const DT_REG: u8 = 8;
/// Directory-entry type: symbolic link.
pub const DT_LNK: u8 = 10;
/// Directory-entry type: socket.
pub const DT_SOCK: u8 = 12;

/// Map a `Stat::mode` value onto a directory-entry `kind`.
///
/// Unknown file types become [`DT_UNKNOWN`].
pub const fn dirent_kind(mode: u32) -> u8 {
    match mode & S_IFMT {
        S_IFIFO => DT_FIFO,
        S_IFCHR => DT_CHR,
        S_IFDIR => DT_DIR,
        S_IFBLK => DT_BLK,
        S_IFREG => DT_REG,
        S_IFLNK => DT_LNK,
        S_IFSOCK => DT_SOCK,
        _ => DT_UNKNOWN,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_flags_match_linux_x86_64() {
        assert_eq!(O_RDONLY, 0);
        assert_eq!(O_WRONLY, 1);
        assert_eq!(O_RDWR, 2);
        assert_eq!(O_CREAT, 0o100);
        assert_eq!(O_EXCL, 0o200);
        assert_eq!(O_TRUNC, 0o1000);
        assert_eq!(O_APPEND, 0o2000);
        assert_eq!(O_DIRECTORY, 0o200000);
        assert_eq!(O_NOFOLLOW, 0o400000);
        assert_eq!(OPEN_FLAGS_MASK, 0o603303);
    }

    #[test]
    fn mode_constants_match_linux_x86_64() {
        assert_eq!(S_IFMT, 0o170000);
        assert_eq!(S_IFREG, 0o100000);
        assert_eq!(S_IFDIR, 0o040000);
        assert_eq!(S_IFLNK, 0o120000);
        assert_eq!(S_IFCHR, 0o020000);
        assert_eq!(S_IFBLK, 0o060000);
        assert_eq!(S_IFIFO, 0o010000);
        assert_eq!(S_IFSOCK, 0o140000);
    }

    #[test]
    fn directory_entry_kinds_are_stable() {
        assert_eq!(DT_UNKNOWN, 0);
        assert_eq!(DT_FIFO, 1);
        assert_eq!(DT_CHR, 2);
        assert_eq!(DT_DIR, 4);
        assert_eq!(DT_BLK, 6);
        assert_eq!(DT_REG, 8);
        assert_eq!(DT_LNK, 10);
        assert_eq!(DT_SOCK, 12);
    }

    #[test]
    fn mode_maps_to_dirent_kind() {
        assert_eq!(dirent_kind(S_IFREG | 0o644), DT_REG);
        assert_eq!(dirent_kind(S_IFDIR | 0o755), DT_DIR);
        assert_eq!(dirent_kind(S_IFLNK | 0o777), DT_LNK);
        assert_eq!(dirent_kind(S_IFCHR | 0o666), DT_CHR);
        assert_eq!(dirent_kind(S_IFBLK | 0o660), DT_BLK);
        assert_eq!(dirent_kind(S_IFIFO | 0o600), DT_FIFO);
        assert_eq!(dirent_kind(S_IFSOCK | 0o666), DT_SOCK);
        assert_eq!(dirent_kind(0), DT_UNKNOWN);
        assert_eq!(dirent_kind(S_IFMT), DT_UNKNOWN);
    }
}

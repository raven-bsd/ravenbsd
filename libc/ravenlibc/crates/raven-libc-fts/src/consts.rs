// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The `fts.h`/`ftw.h` flag and status values and the crate's own limits.
//!
//! The numbers are facts read from `freebsd:include/{fts,ftw}.h` at
//! `133344405f532bd54ff3629a77d4c4e062f01fdb`. `FTS_NAMEONLY` is outside `FTS_OPTIONMASK`
//! (trap list) and only valid for `fts_children`.

/// `FTS_COMFOLLOW`: follow a command-line symbolic link.
pub const FTS_COMFOLLOW: u32 = 0x0001;
/// `FTS_LOGICAL`: logical walk (follow every symbolic link).
pub const FTS_LOGICAL: u32 = 0x0002;
/// `FTS_NOCHDIR`: never change the process directory. Forced by H-FTS-1.
pub const FTS_NOCHDIR: u32 = 0x0004;
/// `FTS_NOSTAT`: do not request `stat` information.
pub const FTS_NOSTAT: u32 = 0x0008;
/// `FTS_PHYSICAL`: physical walk (do not follow symbolic links).
pub const FTS_PHYSICAL: u32 = 0x0010;
/// `FTS_SEEDOT`: return `.` and `..`.
pub const FTS_SEEDOT: u32 = 0x0020;
/// `FTS_XDEV`: do not cross a mount point.
pub const FTS_XDEV: u32 = 0x0040;
/// `FTS_WHITEOUT`: accepted, no effect (there are no whiteouts).
pub const FTS_WHITEOUT: u32 = 0x0080;
/// `FTS_COMFOLLOWDIR`: follow a command-line symbolic link only when it is a directory.
pub const FTS_COMFOLLOWDIR: u32 = 0x0400;
/// `FTS_NOSTAT_TYPE`: use `d_type` instead of `stat` under `FTS_NOSTAT`.
pub const FTS_NOSTAT_TYPE: u32 = 0x0800;
/// The user-option mask of `fts_open`/`fts_openat`.
pub const FTS_OPTIONMASK: u32 = 0x0cff;

/// `FTS_NAMEONLY`: `fts_children` returns names without `stat`.
pub const FTS_NAMEONLY: u32 = 0x0100;

/// Internal: an unrecoverable error stopped the walk.
pub const FTS_STOP: u32 = 0x0100_0000;

/// `FTS_D`: preorder directory.
pub const FTS_D: i32 = 1;
/// `FTS_DC`: a directory that causes a cycle.
pub const FTS_DC: i32 = 2;
/// `FTS_DEFAULT`: none of the other types.
pub const FTS_DEFAULT: i32 = 3;
/// `FTS_DNR`: unreadable directory.
pub const FTS_DNR: i32 = 4;
/// `FTS_DOT`: `.` or `..`.
pub const FTS_DOT: i32 = 5;
/// `FTS_DP`: postorder directory.
pub const FTS_DP: i32 = 6;
/// `FTS_ERR`: an error; `errno` is set.
pub const FTS_ERR: i32 = 7;
/// `FTS_F`: regular file.
pub const FTS_F: i32 = 8;
/// `FTS_INIT`: initialized only (the dummy node).
pub const FTS_INIT: i32 = 9;
/// `FTS_NS`: `stat` failed.
pub const FTS_NS: i32 = 10;
/// `FTS_NSOK`: no `stat` requested.
pub const FTS_NSOK: i32 = 11;
/// `FTS_SL`: symbolic link.
pub const FTS_SL: i32 = 12;
/// `FTS_SLNONE`: symbolic link without a target.
pub const FTS_SLNONE: i32 = 13;
/// `FTS_W`: whiteout object (never produced).
pub const FTS_W: i32 = 14;

/// `FTS_DONTCHDIR`: no descriptor is held for this directory.
pub const FTS_DONTCHDIR: u32 = 0x01;
/// `FTS_SYMFOLLOW`: the node was reached through a followed link.
pub const FTS_SYMFOLLOW: u32 = 0x02;
/// `FTS_ISW`: a whiteout object (never produced).
pub const FTS_ISW: u32 = 0x04;

/// `FTS_AGAIN`: re-stat and return the node again.
pub const FTS_AGAIN: i32 = 1;
/// `FTS_FOLLOW`: follow the symbolic link and return the node again.
pub const FTS_FOLLOW: i32 = 2;
/// `FTS_NOINSTR`: no instruction.
pub const FTS_NOINSTR: i32 = 3;
/// `FTS_SKIP`: discard the node and its children.
pub const FTS_SKIP: i32 = 4;

/// `FTS_ROOTPARENTLEVEL`.
pub const FTS_ROOTPARENTLEVEL: i64 = -1;
/// `FTS_ROOTLEVEL`.
pub const FTS_ROOTLEVEL: i64 = 0;

/// The musl `__dirstream` buffer size (`__dirent.h:10`).
pub const DIRSTREAM_BUFSIZE: usize = 2048;

/// `PATH_MAX` (4096) from `raven-libc-abi`.
pub const PATH_MAX: usize = 4096;
/// `NAME_MAX` (255) from `raven-libc-abi`.
pub const NAME_MAX: usize = 255;
/// `SYMLOOP_MAX` (40) from `raven-libc-abi`.
pub const SYMLOOP_MAX: u32 = 40;

/// The dirfd value of a node without a descriptor.
pub const NO_FD: i32 = -1;
/// The default `FTS` path-buffer slab (the donor's initial 1 KiB, rounded to `PATH_MAX`).
pub const FTS_INITIAL_PATH: usize = PATH_MAX;

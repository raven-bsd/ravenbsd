// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 1982, 1985, 1993 The Regents of the University of California.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: table mechanism transpiled from musl:src/errno/__strerror.h @ 7813e9119eb5fe681b6750b8a033a0490bca0941
// (no per-file header; covered by musl/COPYRIGHT, H-1). Message strings are the FreeBSD native
// domain texts transcribed from freebsd:lib/libc/gen/errlst.c @ 133344405f532bd54ff3629a77d4c4e062f01fdb
// (BSD-3-Clause, DR-69); slot 0 keeps the RD-13 "Success" deviation.
//
// Permission is hereby granted, free of charge, to any person obtaining
// a copy of this software and associated documentation files (the
// "Software"), to deal in the Software without restriction, including
// without limitation the rights to use, copy, modify, merge, publish,
// distribute, sublicense, and/or sell copies of the Software, and to
// permit persons to whom the Software is furnished to do so, subject to
// the following conditions:
//
// The above copyright notice and this permission notice shall be
// included in all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
// EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
// MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
// IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
// CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT,
// TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE
// SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

//! Error-message blob and `u16` offsets (R-12).
//!
//! Slot 0 is `"Success"` (RD-13). Slots 1..=97 are the FreeBSD `sys_errlist` texts of the
//! native errno domain (DR-69); `ERRMSG_COUNT` is FreeBSD's `sys_nerr` (`ELAST + 1`), so a
//! number at or above it takes the numbered unknown path.

/// Number of `errno` slots: FreeBSD `sys_nerr` (0..=97).
pub(crate) const ERRMSG_COUNT: usize = 98;

/// One NUL-separated blob, indexed by the native `errno` number (R-12).
pub(crate) const ERRMSG_BLOB: &[u8] = b"Success\x00Operation not permitted\x00No such file or directory\x00No such process\x00Interrupted system call\x00Input/output error\x00Device not configured\x00Argument list too long\x00Exec format error\x00Bad file descriptor\x00No child processes\x00Resource deadlock avoided\x00Cannot allocate memory\x00Permission denied\x00Bad address\x00Block device required\x00Device busy\x00File exists\x00Cross-device link\x00Operation not supported by device\x00Not a directory\x00Is a directory\x00Invalid argument\x00Too many open files in system\x00Too many open files\x00Inappropriate ioctl for device\x00Text file busy\x00File too large\x00No space left on device\x00Illegal seek\x00Read-only file system\x00Too many links\x00Broken pipe\x00Numerical argument out of domain\x00Result too large\x00Resource temporarily unavailable\x00Operation now in progress\x00Operation already in progress\x00Socket operation on non-socket\x00Destination address required\x00Message too long\x00Protocol wrong type for socket\x00Protocol not available\x00Protocol not supported\x00Socket type not supported\x00Operation not supported\x00Protocol family not supported\x00Address family not supported by protocol family\x00Address already in use\x00Can't assign requested address\x00Network is down\x00Network is unreachable\x00Network dropped connection on reset\x00Software caused connection abort\x00Connection reset by peer\x00No buffer space available\x00Socket is already connected\x00Socket is not connected\x00Can't send after socket shutdown\x00Too many references: can't splice\x00Operation timed out\x00Connection refused\x00Too many levels of symbolic links\x00File name too long\x00Host is down\x00No route to host\x00Directory not empty\x00Too many processes\x00Too many users\x00Disc quota exceeded\x00Stale NFS file handle\x00Too many levels of remote in path\x00RPC struct is bad\x00RPC version wrong\x00RPC prog. not avail\x00Program version wrong\x00Bad procedure for program\x00No locks available\x00Function not implemented\x00Inappropriate file type or format\x00Authentication error\x00Need authenticator\x00Identifier removed\x00No message of desired type\x00Value too large to be stored in data type\x00Operation canceled\x00Illegal byte sequence\x00Attribute not found\x00Programming error\x00Bad message\x00Multihop attempted\x00Link has been severed\x00Protocol error\x00Capabilities insufficient\x00Not permitted in capability mode\x00State not recoverable\x00Previous owner died\x00Integrity check failed\x00";

/// Byte offset of slot `n` in [`ERRMSG_BLOB`].
pub(crate) const ERRMSG_OFF: [u16; ERRMSG_COUNT] = [
    0, 8, 32, 58, 74, 98, 117, 139, 162, 180, 200, 219, 245, 268, 286, 298, 320, 332, 344, 362,
    396, 412, 427, 444, 474, 494, 525, 540, 555, 579, 592, 614, 629, 641, 674, 691, 724, 750,
    780, 811, 840, 857, 888, 911, 934, 960, 984, 1014, 1062, 1085, 1116, 1132, 1155, 1191, 1224,
    1249, 1275, 1303, 1327, 1360, 1394, 1414, 1433, 1467, 1486, 1499, 1516, 1536, 1555, 1570,
    1590, 1612, 1646, 1664, 1682, 1702, 1724, 1750, 1769, 1794, 1828, 1849, 1868, 1887, 1914,
    1956, 1975, 1997, 2017, 2035, 2047, 2066, 2088, 2103, 2129, 2162, 2184, 2204,
];

/// Text of slot `n` without its NUL, when `n` is in range.
#[must_use]
pub(crate) fn text(n: usize) -> Option<&'static [u8]> {
    if n >= ERRMSG_COUNT {
        return None;
    }
    let start = usize::from(*ERRMSG_OFF.get(n)?);
    let rest = ERRMSG_BLOB.get(start..)?;
    let mut end = 0usize;
    loop {
        if *rest.get(end)? == 0 {
            break;
        }
        end = end.wrapping_add(1);
    }
    rest.get(..end)
}

#[cfg(test)]
#[path = "tables_ut.rs"]
mod tests;

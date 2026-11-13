// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/errno/__strerror.h @ 7813e9119eb5fe681b6750b8a033a0490bca0941
// (no per-file header; covered by musl/COPYRIGHT, H-1)
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
//! Slot 0 is `"Success"` (RD-13) and the in-range gaps (41, 58) use the musl catch-all
//! `"No error information"`. The Linux names musl omits get original one-line English
//! texts (CLEANROOM); the glibc `sysdeps/gnu/errlist.c` was read for behaviour only.

/// Number of `errno` slots: Linux numbers 0..=133.
pub(crate) const ERRMSG_COUNT: usize = 134;

/// One NUL-separated blob, indexed by the Linux `errno` number (R-12).
pub(crate) const ERRMSG_BLOB: &[u8] = b"Success\x00Operation not permitted\x00No such file or directory\x00No such process\x00Interrupted system call\x00I/O error\x00No such device or address\x00Argument list too long\x00Exec format error\x00Bad file descriptor\x00No child process\x00Resource temporarily unavailable\x00Out of memory\x00Permission denied\x00Bad address\x00Block device required\x00Resource busy\x00File exists\x00Cross-device link\x00No such device\x00Not a directory\x00Is a directory\x00Invalid argument\x00Too many open files in system\x00No file descriptors available\x00Not a tty\x00Text file busy\x00File too large\x00No space left on device\x00Invalid seek\x00Read-only file system\x00Too many links\x00Broken pipe\x00Domain error\x00Result not representable\x00Resource deadlock would occur\x00Filename too long\x00No locks available\x00Function not implemented\x00Directory not empty\x00Symbolic link loop\x00No error information\x00No message of desired type\x00Identifier removed\x00Channel number out of range\x00Level 2 not synchronised\x00Level 3 halted\x00Level 3 reset\x00Link number out of range\x00Protocol driver not attached\x00No CSI structure available\x00Level 2 halted\x00Invalid exchange\x00Invalid request descriptor\x00Exchange full\x00No anode\x00Invalid request code\x00Invalid slot\x00No error information\x00Bad font file format\x00Device not a stream\x00No data available\x00Device timeout\x00Out of streams resources\x00Machine is not on the network\x00Package not installed\x00Object is remote\x00Link has been severed\x00Advertise error\x00Srmount error\x00Communication error on send\x00Protocol error\x00Multihop attempted\x00RFS specific error\x00Bad message\x00Value too large for data type\x00Name not unique on network\x00File descriptor in bad state\x00Remote address changed\x00Cannot access a needed shared library\x00Accessing a corrupted shared library\x00.lib section in a.out corrupted\x00Attempting to link in too many shared libraries\x00Cannot exec a shared library directly\x00Illegal byte sequence\x00Interrupted system call should be restarted\x00Streams pipe error\x00Too many users\x00Not a socket\x00Destination address required\x00Message too large\x00Protocol wrong type for socket\x00Protocol not available\x00Protocol not supported\x00Socket type not supported\x00Not supported\x00Protocol family not supported\x00Address family not supported by protocol\x00Address in use\x00Address not available\x00Network is down\x00Network unreachable\x00Connection reset by network\x00Connection aborted\x00Connection reset by peer\x00No buffer space available\x00Socket is connected\x00Socket not connected\x00Cannot send after socket shutdown\x00Too many references: cannot splice\x00Operation timed out\x00Connection refused\x00Host is down\x00Host is unreachable\x00Operation already in progress\x00Operation in progress\x00Stale file handle\x00Data consistency error\x00Not a named type file\x00Resource not available\x00Is a named type file\x00Remote I/O error\x00Quota exceeded\x00No medium found\x00Wrong medium type\x00Operation canceled\x00Required key not available\x00Key has expired\x00Key has been revoked\x00Key was rejected by service\x00Previous owner died\x00State not recoverable\x00Operation not possible due to RF-kill\x00Memory page has hardware error\x00";

/// Byte offset of slot `n` in [`ERRMSG_BLOB`].
pub(crate) const ERRMSG_OFF: [u16; ERRMSG_COUNT] = [
    0, 8, 32, 58, 74, 98, 108, 134, 157, 175, 195, 212, 245, 259, 277, 289, 311, 325, 337, 355,
    370, 386, 401, 418, 448, 478, 488, 503, 518, 542, 555, 577, 592, 604, 617, 642, 672, 690, 709,
    734, 754, 773, 794, 821, 840, 868, 893, 908, 922, 947, 976, 1003, 1018, 1035, 1062, 1076, 1085,
    1106, 1119, 1140, 1161, 1181, 1199, 1214, 1239, 1269, 1291, 1308, 1330, 1346, 1360, 1388, 1403,
    1422, 1441, 1453, 1483, 1510, 1539, 1562, 1600, 1637, 1669, 1717, 1755, 1777, 1821, 1840, 1855,
    1868, 1897, 1915, 1946, 1969, 1992, 2018, 2032, 2062, 2103, 2118, 2140, 2156, 2176, 2204, 2223,
    2248, 2274, 2294, 2315, 2349, 2384, 2404, 2423, 2436, 2456, 2486, 2508, 2526, 2549, 2571, 2594,
    2615, 2632, 2647, 2663, 2681, 2700, 2727, 2743, 2764, 2792, 2812, 2834, 2872,
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

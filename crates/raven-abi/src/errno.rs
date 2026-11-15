// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Error numbers. Values are Linux's (DECISIONS.md DR-13). A syscall returns `>= 0` on success and
//! `-errno` on failure.

/// A positive error number.
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct Errno(pub u16);

macro_rules! errnos {
    ($($name:ident = $val:expr, $text:expr;)*) => {
        impl Errno {
            $(#[doc = $text] pub const $name: Errno = Errno($val);)*

            /// Short description, or `"unknown error"`.
            pub const fn description(self) -> &'static str {
                match self.0 {
                    $($val => $text,)*
                    _ => "unknown error",
                }
            }
        }
    };
}

errnos! {
    EPERM = 1, "operation not permitted";
    ENOENT = 2, "no such file or directory";
    ESRCH = 3, "no such process";
    EINTR = 4, "interrupted";
    EIO = 5, "input/output error";
    ENXIO = 6, "no such device or address";
    E2BIG = 7, "argument list too long";
    ENOEXEC = 8, "exec format error";
    EBADF = 9, "bad handle or file descriptor";
    ECHILD = 10, "no child process";
    EAGAIN = 11, "resource temporarily unavailable";
    ENOMEM = 12, "out of memory";
    EACCES = 13, "permission denied";
    EFAULT = 14, "bad address";
    ENOTBLK = 15, "block device required";
    EBUSY = 16, "resource busy";
    EEXIST = 17, "already exists";
    EXDEV = 18, "cross-device link";
    ENODEV = 19, "no such device";
    ENOTDIR = 20, "not a directory";
    EISDIR = 21, "is a directory";
    EINVAL = 22, "invalid argument";
    ENFILE = 23, "too many open files in system";
    EMFILE = 24, "too many open files";
    ENOTTY = 25, "not a terminal";
    ETXTBSY = 26, "text file busy";
    EFBIG = 27, "file too large";
    ENOSPC = 28, "no space left on device";
    ESPIPE = 29, "illegal seek";
    EROFS = 30, "read-only file system";
    EMLINK = 31, "too many links";
    EPIPE = 32, "broken pipe";
    EDOM = 33, "numerical argument out of domain";
    ERANGE = 34, "result out of range";
    EDEADLK = 35, "resource deadlock avoided";
    ENAMETOOLONG = 36, "file name too long";
    ENOLCK = 37, "no locks available";
    ENOSYS = 38, "function not implemented";
    ENOTEMPTY = 39, "directory not empty";
    ELOOP = 40, "too many levels of symbolic links";
    ENOMSG = 42, "no message of desired type";
    EIDRM = 43, "identifier removed";
    ENOSTR = 60, "not a stream";
    ENODATA = 61, "no data available";
    ETIME = 62, "timer expired";
    ENOSR = 63, "out of streams resources";
    ENOLINK = 67, "link has been severed";
    EPROTO = 71, "protocol error";
    EBADMSG = 74, "bad message";
    EOVERFLOW = 75, "value too large for data type";
    EILSEQ = 84, "illegal byte sequence";
    ENOTSOCK = 88, "not a socket";
    EDESTADDRREQ = 89, "destination address required";
    EMSGSIZE = 90, "message too long";
    EPROTOTYPE = 91, "protocol wrong type for socket";
    ENOPROTOOPT = 92, "protocol not available";
    EPROTONOSUPPORT = 93, "protocol not supported";
    ESOCKTNOSUPPORT = 94, "socket type not supported";
    EOPNOTSUPP = 95, "operation not supported";
    EPFNOSUPPORT = 96, "protocol family not supported";
    EAFNOSUPPORT = 97, "address family not supported";
    EADDRINUSE = 98, "address in use";
    EADDRNOTAVAIL = 99, "address not available";
    ENETDOWN = 100, "network is down";
    ENETUNREACH = 101, "network unreachable";
    ENETRESET = 102, "network dropped connection on reset";
    ECONNABORTED = 103, "connection aborted";
    ECONNRESET = 104, "connection reset";
    ENOBUFS = 105, "no buffer space available";
    EISCONN = 106, "already connected";
    ENOTCONN = 107, "not connected";
    ESHUTDOWN = 108, "cannot send after transport endpoint shutdown";
    ETOOMANYREFS = 109, "too many references: cannot splice";
    ETIMEDOUT = 110, "timed out";
    ECONNREFUSED = 111, "connection refused";
    EHOSTDOWN = 112, "host is down";
    EHOSTUNREACH = 113, "host unreachable";
    EALREADY = 114, "operation already in progress";
    EINPROGRESS = 115, "operation in progress";
    ESTALE = 116, "stale file handle";
    EDQUOT = 122, "disk quota exceeded";
    ECANCELED = 125, "operation canceled";
    EOWNERDEAD = 130, "owner died";
    ENOTRECOVERABLE = 131, "state not recoverable";
}

impl Errno {
    /// Operation would block (alias of `EAGAIN`).
    pub const EWOULDBLOCK: Errno = Self::EAGAIN;
    /// Operation not supported (alias of `EOPNOTSUPP`).
    pub const ENOTSUP: Errno = Self::EOPNOTSUPP;
    /// Resource deadlock avoided (alias of `EDEADLK`).
    pub const EDEADLOCK: Errno = Self::EDEADLK;

    /// Encode as a syscall return value (`-errno`).
    #[inline]
    pub const fn to_ret(self) -> isize {
        -(self.0 as isize)
    }

    /// Decode a raw syscall return value: `>= 0` is `Ok`, `-4095..=-1` is `Err`.
    /// Values below `-4095` are treated as success (they are large addresses), as on Linux.
    #[inline]
    pub const fn from_ret(ret: isize) -> Result<usize, Errno> {
        if ret < 0 && ret >= -4095 {
            Err(Errno((-ret) as u16))
        } else {
            Ok(ret as usize)
        }
    }
}

#[cfg(test)]
#[path = "errno_ut.rs"]
mod tests;

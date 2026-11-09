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
    EFBIG = 27, "file too large";
    ENOSPC = 28, "no space left on device";
    ESPIPE = 29, "illegal seek";
    EROFS = 30, "read-only file system";
    EMLINK = 31, "too many links";
    EPIPE = 32, "broken pipe";
    EDOM = 33, "numerical argument out of domain";
    ERANGE = 34, "result out of range";
    ENAMETOOLONG = 36, "file name too long";
    ENOSYS = 38, "function not implemented";
    ENOTEMPTY = 39, "directory not empty";
    ELOOP = 40, "too many levels of symbolic links";
    EPROTO = 71, "protocol error";
    EOVERFLOW = 75, "value too large for data type";
    ENOTSOCK = 88, "not a socket";
    EOPNOTSUPP = 95, "operation not supported";
    EAFNOSUPPORT = 97, "address family not supported";
    EADDRINUSE = 98, "address in use";
    EADDRNOTAVAIL = 99, "address not available";
    ENETUNREACH = 101, "network unreachable";
    ECONNABORTED = 103, "connection aborted";
    ECONNRESET = 104, "connection reset";
    ENOBUFS = 105, "no buffer space available";
    EISCONN = 106, "already connected";
    ENOTCONN = 107, "not connected";
    ETIMEDOUT = 110, "timed out";
    ECONNREFUSED = 111, "connection refused";
    EHOSTUNREACH = 113, "host unreachable";
    EALREADY = 114, "operation already in progress";
    EINPROGRESS = 115, "operation in progress";
}

impl Errno {
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
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        assert_eq!(Errno::from_ret(Errno::ENOENT.to_ret()), Err(Errno::ENOENT));
        assert_eq!(Errno::from_ret(0), Ok(0));
        assert_eq!(Errno::from_ret(42), Ok(42));
    }

    #[test]
    fn linux_values() {
        assert_eq!(Errno::EPERM.0, 1);
        assert_eq!(Errno::EAGAIN.0, 11);
        assert_eq!(Errno::ENOSYS.0, 38);
        assert_eq!(Errno::ETIMEDOUT.0, 110);
    }

    #[test]
    fn high_addresses_are_not_errors() {
        // A mapping returned at 0xFFFF_FFFF_FFFF_F000 is -4096 as isize: success.
        assert_eq!(Errno::from_ret(-4096), Ok((-4096isize) as usize));
        assert_eq!(Errno::from_ret(-4095), Err(Errno(4095)));
    }

    #[test]
    fn descriptions() {
        assert_eq!(Errno::ENOENT.description(), "no such file or directory");
        assert_eq!(Errno(4000).description(), "unknown error");
    }
}

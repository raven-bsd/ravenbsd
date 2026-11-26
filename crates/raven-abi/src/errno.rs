// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Native error numbers. Values are FreeBSD's (DR-69, `sys/sys/errno.h` at the S700 pin);
//! Linux values exist only inside the Linux personality and convert at its boundaries.
//! A syscall returns `>= 0` on success and `-errno` on failure.

/// A positive native error number.
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

// FreeBSD `sys/sys/errno.h` values, in that header's order. `ELAST` is the last value and is
// not itself an error. The Linux personality has its own table in `raven-linux-abi`.
errnos! {
    EPERM = 1, "operation not permitted";
    ENOENT = 2, "no such file or directory";
    ESRCH = 3, "no such process";
    EINTR = 4, "interrupted system call";
    EIO = 5, "input/output error";
    ENXIO = 6, "device not configured";
    E2BIG = 7, "argument list too long";
    ENOEXEC = 8, "exec format error";
    EBADF = 9, "bad file descriptor";
    ECHILD = 10, "no child processes";
    EDEADLK = 11, "resource deadlock avoided";
    ENOMEM = 12, "cannot allocate memory";
    EACCES = 13, "permission denied";
    EFAULT = 14, "bad address";
    ENOTBLK = 15, "block device required";
    EBUSY = 16, "device busy";
    EEXIST = 17, "file exists";
    EXDEV = 18, "cross-device link";
    ENODEV = 19, "operation not supported by device";
    ENOTDIR = 20, "not a directory";
    EISDIR = 21, "is a directory";
    EINVAL = 22, "invalid argument";
    ENFILE = 23, "too many open files in system";
    EMFILE = 24, "too many open files";
    ENOTTY = 25, "inappropriate ioctl for device";
    ETXTBSY = 26, "text file busy";
    EFBIG = 27, "file too large";
    ENOSPC = 28, "no space left on device";
    ESPIPE = 29, "illegal seek";
    EROFS = 30, "read-only file system";
    EMLINK = 31, "too many links";
    EPIPE = 32, "broken pipe";
    EDOM = 33, "numerical argument out of domain";
    ERANGE = 34, "result too large";
    EAGAIN = 35, "resource temporarily unavailable";
    EINPROGRESS = 36, "operation now in progress";
    EALREADY = 37, "operation already in progress";
    ENOTSOCK = 38, "socket operation on non-socket";
    EDESTADDRREQ = 39, "destination address required";
    EMSGSIZE = 40, "message too long";
    EPROTOTYPE = 41, "protocol wrong type for socket";
    ENOPROTOOPT = 42, "protocol not available";
    EPROTONOSUPPORT = 43, "protocol not supported";
    ESOCKTNOSUPPORT = 44, "socket type not supported";
    EOPNOTSUPP = 45, "operation not supported";
    EPFNOSUPPORT = 46, "protocol family not supported";
    EAFNOSUPPORT = 47, "address family not supported by protocol family";
    EADDRINUSE = 48, "address already in use";
    EADDRNOTAVAIL = 49, "can't assign requested address";
    ENETDOWN = 50, "network is down";
    ENETUNREACH = 51, "network is unreachable";
    ENETRESET = 52, "network dropped connection on reset";
    ECONNABORTED = 53, "software caused connection abort";
    ECONNRESET = 54, "connection reset by peer";
    ENOBUFS = 55, "no buffer space available";
    EISCONN = 56, "socket is already connected";
    ENOTCONN = 57, "socket is not connected";
    ESHUTDOWN = 58, "can't send after socket shutdown";
    ETOOMANYREFS = 59, "too many references: can't splice";
    ETIMEDOUT = 60, "operation timed out";
    ECONNREFUSED = 61, "connection refused";
    ELOOP = 62, "too many levels of symbolic links";
    ENAMETOOLONG = 63, "file name too long";
    EHOSTDOWN = 64, "host is down";
    EHOSTUNREACH = 65, "no route to host";
    ENOTEMPTY = 66, "directory not empty";
    EPROCLIM = 67, "too many processes";
    EUSERS = 68, "too many users";
    EDQUOT = 69, "disc quota exceeded";
    ESTALE = 70, "stale NFS file handle";
    EREMOTE = 71, "too many levels of remote in path";
    EBADRPC = 72, "RPC struct is bad";
    ERPCMISMATCH = 73, "RPC version wrong";
    EPROGUNAVAIL = 74, "RPC prog. not avail";
    EPROGMISMATCH = 75, "program version wrong";
    EPROCUNAVAIL = 76, "bad procedure for program";
    ENOLCK = 77, "no locks available";
    ENOSYS = 78, "function not implemented";
    EFTYPE = 79, "inappropriate file type or format";
    EAUTH = 80, "authentication error";
    ENEEDAUTH = 81, "need authenticator";
    EIDRM = 82, "identifier removed";
    ENOMSG = 83, "no message of desired type";
    EOVERFLOW = 84, "value too large to be stored in data type";
    ECANCELED = 85, "operation canceled";
    EILSEQ = 86, "illegal byte sequence";
    ENOATTR = 87, "attribute not found";
    EDOOFUS = 88, "programming error";
    EBADMSG = 89, "bad message";
    EMULTIHOP = 90, "multihop attempted";
    ENOLINK = 91, "link has been severed";
    EPROTO = 92, "protocol error";
    ENOTCAPABLE = 93, "capabilities insufficient";
    ECAPMODE = 94, "not permitted in capability mode";
    ENOTRECOVERABLE = 95, "state not recoverable";
    EOWNERDEAD = 96, "previous owner died";
    EINTEGRITY = 97, "integrity check failed";
}

impl Errno {
    /// Operation would block (alias of `EAGAIN`).
    pub const EWOULDBLOCK: Errno = Self::EAGAIN;
    /// Operation not supported (alias of `EOPNOTSUPP`).
    pub const ENOTSUP: Errno = Self::EOPNOTSUPP;
    /// Resource deadlock avoided (alias of `EDEADLK`).
    pub const EDEADLOCK: Errno = Self::EDEADLK;
    /// The largest native error number (`ELAST` in the donor header).
    pub const ELAST: u16 = Self::EINTEGRITY.0;

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

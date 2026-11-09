// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Linux errno names and values.
//!
//! Values are the Linux UAPI values as exposed by `linux-raw-sys` 0.12.1
//! (`src/x86_64/errno.rs`, Apache-2.0 OR MIT); the generic architectures
//! use the same numbering. The table is sorted by name.

#![allow(missing_docs)] // Linux errno names are their own documentation.

/// `EPERM` = 1.
pub const EPERM: u16 = 1;
/// `ENOENT` = 2.
pub const ENOENT: u16 = 2;
/// `ESRCH` = 3.
pub const ESRCH: u16 = 3;
/// `EINTR` = 4.
pub const EINTR: u16 = 4;
/// `EIO` = 5.
pub const EIO: u16 = 5;
/// `ENXIO` = 6.
pub const ENXIO: u16 = 6;
/// `E2BIG` = 7.
pub const E2BIG: u16 = 7;
/// `ENOEXEC` = 8.
pub const ENOEXEC: u16 = 8;
/// `EBADF` = 9.
pub const EBADF: u16 = 9;
/// `ECHILD` = 10.
pub const ECHILD: u16 = 10;
/// `EAGAIN` = 11.
pub const EAGAIN: u16 = 11;
/// `ENOMEM` = 12.
pub const ENOMEM: u16 = 12;
/// `EACCES` = 13.
pub const EACCES: u16 = 13;
/// `EFAULT` = 14.
pub const EFAULT: u16 = 14;
/// `ENOTBLK` = 15.
pub const ENOTBLK: u16 = 15;
/// `EBUSY` = 16.
pub const EBUSY: u16 = 16;
/// `EEXIST` = 17.
pub const EEXIST: u16 = 17;
/// `EXDEV` = 18.
pub const EXDEV: u16 = 18;
/// `ENODEV` = 19.
pub const ENODEV: u16 = 19;
/// `ENOTDIR` = 20.
pub const ENOTDIR: u16 = 20;
/// `EISDIR` = 21.
pub const EISDIR: u16 = 21;
/// `EINVAL` = 22.
pub const EINVAL: u16 = 22;
/// `ENFILE` = 23.
pub const ENFILE: u16 = 23;
/// `EMFILE` = 24.
pub const EMFILE: u16 = 24;
/// `ENOTTY` = 25.
pub const ENOTTY: u16 = 25;
/// `ETXTBSY` = 26.
pub const ETXTBSY: u16 = 26;
/// `EFBIG` = 27.
pub const EFBIG: u16 = 27;
/// `ENOSPC` = 28.
pub const ENOSPC: u16 = 28;
/// `ESPIPE` = 29.
pub const ESPIPE: u16 = 29;
/// `EROFS` = 30.
pub const EROFS: u16 = 30;
/// `EMLINK` = 31.
pub const EMLINK: u16 = 31;
/// `EPIPE` = 32.
pub const EPIPE: u16 = 32;
/// `EDOM` = 33.
pub const EDOM: u16 = 33;
/// `ERANGE` = 34.
pub const ERANGE: u16 = 34;
/// `EDEADLK` = 35.
pub const EDEADLK: u16 = 35;
/// `ENAMETOOLONG` = 36.
pub const ENAMETOOLONG: u16 = 36;
/// `ENOLCK` = 37.
pub const ENOLCK: u16 = 37;
/// `ENOSYS` = 38.
pub const ENOSYS: u16 = 38;
/// `ENOTEMPTY` = 39.
pub const ENOTEMPTY: u16 = 39;
/// `ELOOP` = 40.
pub const ELOOP: u16 = 40;
/// `EWOULDBLOCK` = 11.
pub const EWOULDBLOCK: u16 = 11;
/// `ENOMSG` = 42.
pub const ENOMSG: u16 = 42;
/// `EIDRM` = 43.
pub const EIDRM: u16 = 43;
/// `ECHRNG` = 44.
pub const ECHRNG: u16 = 44;
/// `EL2NSYNC` = 45.
pub const EL2NSYNC: u16 = 45;
/// `EL3HLT` = 46.
pub const EL3HLT: u16 = 46;
/// `EL3RST` = 47.
pub const EL3RST: u16 = 47;
/// `ELNRNG` = 48.
pub const ELNRNG: u16 = 48;
/// `EUNATCH` = 49.
pub const EUNATCH: u16 = 49;
/// `ENOCSI` = 50.
pub const ENOCSI: u16 = 50;
/// `EL2HLT` = 51.
pub const EL2HLT: u16 = 51;
/// `EBADE` = 52.
pub const EBADE: u16 = 52;
/// `EBADR` = 53.
pub const EBADR: u16 = 53;
/// `EXFULL` = 54.
pub const EXFULL: u16 = 54;
/// `ENOANO` = 55.
pub const ENOANO: u16 = 55;
/// `EBADRQC` = 56.
pub const EBADRQC: u16 = 56;
/// `EBADSLT` = 57.
pub const EBADSLT: u16 = 57;
/// `EDEADLOCK` = 35.
pub const EDEADLOCK: u16 = 35;
/// `EBFONT` = 59.
pub const EBFONT: u16 = 59;
/// `ENOSTR` = 60.
pub const ENOSTR: u16 = 60;
/// `ENODATA` = 61.
pub const ENODATA: u16 = 61;
/// `ETIME` = 62.
pub const ETIME: u16 = 62;
/// `ENOSR` = 63.
pub const ENOSR: u16 = 63;
/// `ENONET` = 64.
pub const ENONET: u16 = 64;
/// `ENOPKG` = 65.
pub const ENOPKG: u16 = 65;
/// `EREMOTE` = 66.
pub const EREMOTE: u16 = 66;
/// `ENOLINK` = 67.
pub const ENOLINK: u16 = 67;
/// `EADV` = 68.
pub const EADV: u16 = 68;
/// `ESRMNT` = 69.
pub const ESRMNT: u16 = 69;
/// `ECOMM` = 70.
pub const ECOMM: u16 = 70;
/// `EPROTO` = 71.
pub const EPROTO: u16 = 71;
/// `EMULTIHOP` = 72.
pub const EMULTIHOP: u16 = 72;
/// `EDOTDOT` = 73.
pub const EDOTDOT: u16 = 73;
/// `EBADMSG` = 74.
pub const EBADMSG: u16 = 74;
/// `EOVERFLOW` = 75.
pub const EOVERFLOW: u16 = 75;
/// `ENOTUNIQ` = 76.
pub const ENOTUNIQ: u16 = 76;
/// `EBADFD` = 77.
pub const EBADFD: u16 = 77;
/// `EREMCHG` = 78.
pub const EREMCHG: u16 = 78;
/// `ELIBACC` = 79.
pub const ELIBACC: u16 = 79;
/// `ELIBBAD` = 80.
pub const ELIBBAD: u16 = 80;
/// `ELIBSCN` = 81.
pub const ELIBSCN: u16 = 81;
/// `ELIBMAX` = 82.
pub const ELIBMAX: u16 = 82;
/// `ELIBEXEC` = 83.
pub const ELIBEXEC: u16 = 83;
/// `EILSEQ` = 84.
pub const EILSEQ: u16 = 84;
/// `ERESTART` = 85.
pub const ERESTART: u16 = 85;
/// `ESTRPIPE` = 86.
pub const ESTRPIPE: u16 = 86;
/// `EUSERS` = 87.
pub const EUSERS: u16 = 87;
/// `ENOTSOCK` = 88.
pub const ENOTSOCK: u16 = 88;
/// `EDESTADDRREQ` = 89.
pub const EDESTADDRREQ: u16 = 89;
/// `EMSGSIZE` = 90.
pub const EMSGSIZE: u16 = 90;
/// `EPROTOTYPE` = 91.
pub const EPROTOTYPE: u16 = 91;
/// `ENOPROTOOPT` = 92.
pub const ENOPROTOOPT: u16 = 92;
/// `EPROTONOSUPPORT` = 93.
pub const EPROTONOSUPPORT: u16 = 93;
/// `ESOCKTNOSUPPORT` = 94.
pub const ESOCKTNOSUPPORT: u16 = 94;
/// `EOPNOTSUPP` = 95.
pub const EOPNOTSUPP: u16 = 95;
/// `EPFNOSUPPORT` = 96.
pub const EPFNOSUPPORT: u16 = 96;
/// `EAFNOSUPPORT` = 97.
pub const EAFNOSUPPORT: u16 = 97;
/// `EADDRINUSE` = 98.
pub const EADDRINUSE: u16 = 98;
/// `EADDRNOTAVAIL` = 99.
pub const EADDRNOTAVAIL: u16 = 99;
/// `ENETDOWN` = 100.
pub const ENETDOWN: u16 = 100;
/// `ENETUNREACH` = 101.
pub const ENETUNREACH: u16 = 101;
/// `ENETRESET` = 102.
pub const ENETRESET: u16 = 102;
/// `ECONNABORTED` = 103.
pub const ECONNABORTED: u16 = 103;
/// `ECONNRESET` = 104.
pub const ECONNRESET: u16 = 104;
/// `ENOBUFS` = 105.
pub const ENOBUFS: u16 = 105;
/// `EISCONN` = 106.
pub const EISCONN: u16 = 106;
/// `ENOTCONN` = 107.
pub const ENOTCONN: u16 = 107;
/// `ESHUTDOWN` = 108.
pub const ESHUTDOWN: u16 = 108;
/// `ETOOMANYREFS` = 109.
pub const ETOOMANYREFS: u16 = 109;
/// `ETIMEDOUT` = 110.
pub const ETIMEDOUT: u16 = 110;
/// `ECONNREFUSED` = 111.
pub const ECONNREFUSED: u16 = 111;
/// `EHOSTDOWN` = 112.
pub const EHOSTDOWN: u16 = 112;
/// `EHOSTUNREACH` = 113.
pub const EHOSTUNREACH: u16 = 113;
/// `EALREADY` = 114.
pub const EALREADY: u16 = 114;
/// `EINPROGRESS` = 115.
pub const EINPROGRESS: u16 = 115;
/// `ESTALE` = 116.
pub const ESTALE: u16 = 116;
/// `EUCLEAN` = 117.
pub const EUCLEAN: u16 = 117;
/// `ENOTNAM` = 118.
pub const ENOTNAM: u16 = 118;
/// `ENAVAIL` = 119.
pub const ENAVAIL: u16 = 119;
/// `EISNAM` = 120.
pub const EISNAM: u16 = 120;
/// `EREMOTEIO` = 121.
pub const EREMOTEIO: u16 = 121;
/// `EDQUOT` = 122.
pub const EDQUOT: u16 = 122;
/// `ENOMEDIUM` = 123.
pub const ENOMEDIUM: u16 = 123;
/// `EMEDIUMTYPE` = 124.
pub const EMEDIUMTYPE: u16 = 124;
/// `ECANCELED` = 125.
pub const ECANCELED: u16 = 125;
/// `ENOKEY` = 126.
pub const ENOKEY: u16 = 126;
/// `EKEYEXPIRED` = 127.
pub const EKEYEXPIRED: u16 = 127;
/// `EKEYREVOKED` = 128.
pub const EKEYREVOKED: u16 = 128;
/// `EKEYREJECTED` = 129.
pub const EKEYREJECTED: u16 = 129;
/// `EOWNERDEAD` = 130.
pub const EOWNERDEAD: u16 = 130;
/// `ENOTRECOVERABLE` = 131.
pub const ENOTRECOVERABLE: u16 = 131;
/// `ERFKILL` = 132.
pub const ERFKILL: u16 = 132;
/// `EHWPOISON` = 133.
pub const EHWPOISON: u16 = 133;

/// All Linux errno names and values, sorted by name.
pub static ERRNOS: &[(&str, u16)] = &[
    ("E2BIG", 7),
    ("EACCES", 13),
    ("EADDRINUSE", 98),
    ("EADDRNOTAVAIL", 99),
    ("EADV", 68),
    ("EAFNOSUPPORT", 97),
    ("EAGAIN", 11),
    ("EALREADY", 114),
    ("EBADE", 52),
    ("EBADF", 9),
    ("EBADFD", 77),
    ("EBADMSG", 74),
    ("EBADR", 53),
    ("EBADRQC", 56),
    ("EBADSLT", 57),
    ("EBFONT", 59),
    ("EBUSY", 16),
    ("ECANCELED", 125),
    ("ECHILD", 10),
    ("ECHRNG", 44),
    ("ECOMM", 70),
    ("ECONNABORTED", 103),
    ("ECONNREFUSED", 111),
    ("ECONNRESET", 104),
    ("EDEADLK", 35),
    ("EDEADLOCK", 35),
    ("EDESTADDRREQ", 89),
    ("EDOM", 33),
    ("EDOTDOT", 73),
    ("EDQUOT", 122),
    ("EEXIST", 17),
    ("EFAULT", 14),
    ("EFBIG", 27),
    ("EHOSTDOWN", 112),
    ("EHOSTUNREACH", 113),
    ("EHWPOISON", 133),
    ("EIDRM", 43),
    ("EILSEQ", 84),
    ("EINPROGRESS", 115),
    ("EINTR", 4),
    ("EINVAL", 22),
    ("EIO", 5),
    ("EISCONN", 106),
    ("EISDIR", 21),
    ("EISNAM", 120),
    ("EKEYEXPIRED", 127),
    ("EKEYREJECTED", 129),
    ("EKEYREVOKED", 128),
    ("EL2HLT", 51),
    ("EL2NSYNC", 45),
    ("EL3HLT", 46),
    ("EL3RST", 47),
    ("ELIBACC", 79),
    ("ELIBBAD", 80),
    ("ELIBEXEC", 83),
    ("ELIBMAX", 82),
    ("ELIBSCN", 81),
    ("ELNRNG", 48),
    ("ELOOP", 40),
    ("EMEDIUMTYPE", 124),
    ("EMFILE", 24),
    ("EMLINK", 31),
    ("EMSGSIZE", 90),
    ("EMULTIHOP", 72),
    ("ENAMETOOLONG", 36),
    ("ENAVAIL", 119),
    ("ENETDOWN", 100),
    ("ENETRESET", 102),
    ("ENETUNREACH", 101),
    ("ENFILE", 23),
    ("ENOANO", 55),
    ("ENOBUFS", 105),
    ("ENOCSI", 50),
    ("ENODATA", 61),
    ("ENODEV", 19),
    ("ENOENT", 2),
    ("ENOEXEC", 8),
    ("ENOKEY", 126),
    ("ENOLCK", 37),
    ("ENOLINK", 67),
    ("ENOMEDIUM", 123),
    ("ENOMEM", 12),
    ("ENOMSG", 42),
    ("ENONET", 64),
    ("ENOPKG", 65),
    ("ENOPROTOOPT", 92),
    ("ENOSPC", 28),
    ("ENOSR", 63),
    ("ENOSTR", 60),
    ("ENOSYS", 38),
    ("ENOTBLK", 15),
    ("ENOTCONN", 107),
    ("ENOTDIR", 20),
    ("ENOTEMPTY", 39),
    ("ENOTNAM", 118),
    ("ENOTRECOVERABLE", 131),
    ("ENOTSOCK", 88),
    ("ENOTTY", 25),
    ("ENOTUNIQ", 76),
    ("ENXIO", 6),
    ("EOPNOTSUPP", 95),
    ("EOVERFLOW", 75),
    ("EOWNERDEAD", 130),
    ("EPERM", 1),
    ("EPFNOSUPPORT", 96),
    ("EPIPE", 32),
    ("EPROTO", 71),
    ("EPROTONOSUPPORT", 93),
    ("EPROTOTYPE", 91),
    ("ERANGE", 34),
    ("EREMCHG", 78),
    ("EREMOTE", 66),
    ("EREMOTEIO", 121),
    ("ERESTART", 85),
    ("ERFKILL", 132),
    ("EROFS", 30),
    ("ESHUTDOWN", 108),
    ("ESOCKTNOSUPPORT", 94),
    ("ESPIPE", 29),
    ("ESRCH", 3),
    ("ESRMNT", 69),
    ("ESTALE", 116),
    ("ESTRPIPE", 86),
    ("ETIME", 62),
    ("ETIMEDOUT", 110),
    ("ETOOMANYREFS", 109),
    ("ETXTBSY", 26),
    ("EUCLEAN", 117),
    ("EUNATCH", 49),
    ("EUSERS", 87),
    ("EWOULDBLOCK", 11),
    ("EXDEV", 18),
    ("EXFULL", 54),
];

/// Look up an errno name by value.
pub fn name(value: u16) -> Option<&'static str> {
    ERRNOS
        .iter()
        .find(|(_, candidate)| *candidate == value)
        .map(|(name, _)| *name)
}

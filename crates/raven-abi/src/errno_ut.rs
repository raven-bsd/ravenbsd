// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the errno table.
//!
//! Expected values come from FreeBSD `sys/sys/errno.h` at `13334440` (the S700 pin), not from
//! the implementation. The independent cross-check is `raven-libc-hdrgen verify --profile
//! freebsd --families errno`, which measures the donor headers with clang.

use super::*;

#[test]
fn roundtrip() {
    assert_eq!(Errno::from_ret(Errno::ENOENT.to_ret()), Err(Errno::ENOENT));
    assert_eq!(Errno::from_ret(0), Ok(0));
    assert_eq!(Errno::from_ret(42), Ok(42));
}

#[test]
fn freebsd_values() {
    // FreeBSD sys/sys/errno.h: EDEADLK is 11 (11 was EAGAIN) and EAGAIN is 35.
    assert_eq!(Errno::EPERM.0, 1);
    assert_eq!(Errno::EDEADLK.0, 11);
    assert_eq!(Errno::EAGAIN.0, 35);
    assert_eq!(Errno::ENOSYS.0, 78);
    assert_eq!(Errno::EADDRINUSE.0, 48);
    assert_eq!(Errno::EOVERFLOW.0, 84);
    assert_eq!(Errno::EILSEQ.0, 86);
    assert_eq!(Errno::EOPNOTSUPP.0, 45);
    assert_eq!(Errno::ETIMEDOUT.0, 60);
}

#[test]
fn every_donor_value_is_the_donor_number() {
    // One row per FreeBSD errno, measured from the pinned header; this is the exhaustive map
    // that a blanket numeric renumbering or a single mistyped row would break.
    let donor: [(Errno, u16); 97] = [
        (Errno::EPERM, 1),
        (Errno::ENOENT, 2),
        (Errno::ESRCH, 3),
        (Errno::EINTR, 4),
        (Errno::EIO, 5),
        (Errno::ENXIO, 6),
        (Errno::E2BIG, 7),
        (Errno::ENOEXEC, 8),
        (Errno::EBADF, 9),
        (Errno::ECHILD, 10),
        (Errno::EDEADLK, 11),
        (Errno::ENOMEM, 12),
        (Errno::EACCES, 13),
        (Errno::EFAULT, 14),
        (Errno::ENOTBLK, 15),
        (Errno::EBUSY, 16),
        (Errno::EEXIST, 17),
        (Errno::EXDEV, 18),
        (Errno::ENODEV, 19),
        (Errno::ENOTDIR, 20),
        (Errno::EISDIR, 21),
        (Errno::EINVAL, 22),
        (Errno::ENFILE, 23),
        (Errno::EMFILE, 24),
        (Errno::ENOTTY, 25),
        (Errno::ETXTBSY, 26),
        (Errno::EFBIG, 27),
        (Errno::ENOSPC, 28),
        (Errno::ESPIPE, 29),
        (Errno::EROFS, 30),
        (Errno::EMLINK, 31),
        (Errno::EPIPE, 32),
        (Errno::EDOM, 33),
        (Errno::ERANGE, 34),
        (Errno::EAGAIN, 35),
        (Errno::EINPROGRESS, 36),
        (Errno::EALREADY, 37),
        (Errno::ENOTSOCK, 38),
        (Errno::EDESTADDRREQ, 39),
        (Errno::EMSGSIZE, 40),
        (Errno::EPROTOTYPE, 41),
        (Errno::ENOPROTOOPT, 42),
        (Errno::EPROTONOSUPPORT, 43),
        (Errno::ESOCKTNOSUPPORT, 44),
        (Errno::EOPNOTSUPP, 45),
        (Errno::EPFNOSUPPORT, 46),
        (Errno::EAFNOSUPPORT, 47),
        (Errno::EADDRINUSE, 48),
        (Errno::EADDRNOTAVAIL, 49),
        (Errno::ENETDOWN, 50),
        (Errno::ENETUNREACH, 51),
        (Errno::ENETRESET, 52),
        (Errno::ECONNABORTED, 53),
        (Errno::ECONNRESET, 54),
        (Errno::ENOBUFS, 55),
        (Errno::EISCONN, 56),
        (Errno::ENOTCONN, 57),
        (Errno::ESHUTDOWN, 58),
        (Errno::ETOOMANYREFS, 59),
        (Errno::ETIMEDOUT, 60),
        (Errno::ECONNREFUSED, 61),
        (Errno::ELOOP, 62),
        (Errno::ENAMETOOLONG, 63),
        (Errno::EHOSTDOWN, 64),
        (Errno::EHOSTUNREACH, 65),
        (Errno::ENOTEMPTY, 66),
        (Errno::EPROCLIM, 67),
        (Errno::EUSERS, 68),
        (Errno::EDQUOT, 69),
        (Errno::ESTALE, 70),
        (Errno::EREMOTE, 71),
        (Errno::EBADRPC, 72),
        (Errno::ERPCMISMATCH, 73),
        (Errno::EPROGUNAVAIL, 74),
        (Errno::EPROGMISMATCH, 75),
        (Errno::EPROCUNAVAIL, 76),
        (Errno::ENOLCK, 77),
        (Errno::ENOSYS, 78),
        (Errno::EFTYPE, 79),
        (Errno::EAUTH, 80),
        (Errno::ENEEDAUTH, 81),
        (Errno::EIDRM, 82),
        (Errno::ENOMSG, 83),
        (Errno::EOVERFLOW, 84),
        (Errno::ECANCELED, 85),
        (Errno::EILSEQ, 86),
        (Errno::ENOATTR, 87),
        (Errno::EDOOFUS, 88),
        (Errno::EBADMSG, 89),
        (Errno::EMULTIHOP, 90),
        (Errno::ENOLINK, 91),
        (Errno::EPROTO, 92),
        (Errno::ENOTCAPABLE, 93),
        (Errno::ECAPMODE, 94),
        (Errno::ENOTRECOVERABLE, 95),
        (Errno::EOWNERDEAD, 96),
        (Errno::EINTEGRITY, 97),
    ];
    assert_eq!(donor.len(), usize::from(Errno::ELAST));
    assert_eq!(Errno::ELAST, 97);
    for (index, (error, value)) in donor.iter().enumerate() {
        assert_eq!(error.0, *value, "row {index}");
    }
}

#[test]
fn aliases_keep_the_canonical_values() {
    assert_eq!(Errno::EWOULDBLOCK, Errno::EAGAIN);
    assert_eq!(Errno::ENOTSUP, Errno::EOPNOTSUPP);
    assert_eq!(Errno::EDEADLOCK, Errno::EDEADLK);
    // FreeBSD defines ENOATTR as its own number, not as Linux's ENODATA.
    assert_eq!(Errno::ENOATTR.0, 87);
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
    assert_eq!(Errno::EILSEQ.description(), "illegal byte sequence");
    assert_eq!(Errno(4000).description(), "unknown error");
}

#[test]
fn every_defined_value_has_a_description_and_no_gap() {
    for value in 1..=Errno::ELAST {
        assert_ne!(
            Errno(value).description(),
            "unknown error",
            "native errno {value} is not described"
        );
    }
}

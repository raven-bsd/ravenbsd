// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! NetBSD C11/C23 vector tables, `C` and `en_US.UTF-8` rows only (A-ctype-14).
//!
//! Sources: `netbsd:tests/lib/libc/locale/t_mbrtoc8.c`, `t_c8rtomb.c`, `t_mbrtoc16.c`,
//! `t_c16rtomb.c`, `t_mbrtoc32.c`, `t_c32rtomb.c`. The non-UTF-8 locale rows are skipped by
//! design; the c16rtomb NUL-interruption row conflicts with the musl donor and is recorded in
//! RD-new D-ct-8.

use raven_libc_ctype::*;

macro_rules! same {
    ($left:expr, $right:expr $(,)?) => {
        assert!($left == $right, "{} != {}", stringify!($left), stringify!($right))
    };
    ($left:expr, $right:expr, $($arg:tt)+) => {
        assert!($left == $right, $($arg)+)
    };
}

#[test]
fn mbrtoc8_c_locale_vectors() {
    let mut state = MbState::INITIAL;
    // "": return 0, c8 = 0.
    same!(
        mbrtoc8(&mut state, MbMode::Byte, b"\0"),
        Mbr8::Char { unit: 0, used: 1 }
    );
    // "A": return 1, c8 = 'A'.
    same!(
        mbrtoc8(&mut state, MbMode::Byte, b"A"),
        Mbr8::Char {
            unit: b'A',
            used: 1
        }
    );
    // Zero length: -2, untouched.
    let mut state = MbState::INITIAL;
    same!(mbrtoc8(&mut state, MbMode::Byte, &[]), Mbr8::Incomplete);
    // No read-ahead: "AB" consumes one byte, then "C" decodes separately.
    let mut state = MbState::INITIAL;
    same!(
        mbrtoc8(&mut state, MbMode::Byte, b"AB"),
        Mbr8::Char {
            unit: b'A',
            used: 1
        }
    );
    same!(
        mbrtoc8(&mut state, MbMode::Byte, b"C"),
        Mbr8::Char {
            unit: b'C',
            used: 1
        }
    );
}

#[test]
fn mbrtoc8_utf8_vectors() {
    let mut state = MbState::INITIAL;
    same!(
        mbrtoc8(&mut state, MbMode::Utf8, b"\0"),
        Mbr8::Char { unit: 0, used: 1 }
    );
    same!(
        mbrtoc8(&mut state, MbMode::Utf8, b"A"),
        Mbr8::Char {
            unit: b'A',
            used: 1
        }
    );
    // Truncated double byte: -2.
    same!(mbrtoc8(&mut state, MbMode::Utf8, &[0xc3]), Mbr8::Incomplete);
    // Complete: lead then continuation.
    state = MbState::INITIAL;
    same!(
        mbrtoc8(&mut state, MbMode::Utf8, &[0xc3, 0x84]),
        Mbr8::Char {
            unit: 0xc3,
            used: 2
        }
    );
    same!(
        mbrtoc8(&mut state, MbMode::Utf8, &[]),
        Mbr8::Continuation { unit: 0x84 }
    );
    // Restarting: partial, then the second byte.
    same!(mbrtoc8(&mut state, MbMode::Utf8, &[0xc3]), Mbr8::Incomplete);
    same!(
        mbrtoc8(&mut state, MbMode::Utf8, &[0xb7]),
        Mbr8::Char {
            unit: 0xc3,
            used: 1
        }
    );
    same!(
        mbrtoc8(&mut state, MbMode::Utf8, &[]),
        Mbr8::Continuation { unit: 0xb7 }
    );
    // Four-byte sequence.
    same!(
        mbrtoc8(&mut state, MbMode::Utf8, &[0xf0, 0x9f, 0x92, 0xa9]),
        Mbr8::Char {
            unit: 0xf0,
            used: 4
        }
    );
    for unit in [0x9f, 0x92, 0xa9] {
        same!(
            mbrtoc8(&mut state, MbMode::Utf8, &[]),
            Mbr8::Continuation { unit }
        );
    }
    // e + combining acute.
    same!(
        mbrtoc8(&mut state, MbMode::Utf8, &[0x65, 0xcc, 0x81]),
        Mbr8::Char {
            unit: 0x65,
            used: 1
        }
    );
    same!(
        mbrtoc8(&mut state, MbMode::Utf8, &[0xcc, 0x81]),
        Mbr8::Char {
            unit: 0xcc,
            used: 2
        }
    );
    same!(
        mbrtoc8(&mut state, MbMode::Utf8, &[]),
        Mbr8::Continuation { unit: 0x81 }
    );
}

#[test]
fn c8rtomb_c_locale_vectors() {
    let mut state = MbState::INITIAL;
    let mut out = [0xccu8; 4];
    same!(c8rtomb(&mut state, MbMode::Byte, 0, &mut out), Ok(1));
    same!(out[0], 0);
    same!(out[1], 0xcc);
    same!(c8rtomb(&mut state, MbMode::Byte, b'A', &mut out), Ok(1));
    same!(out[0], b'A');
    same!(out[1], 0xcc);
    // Pile of poo is not representable in the single-byte locale.
    out = [0xccu8; 4];
    same!(c8rtomb(&mut state, MbMode::Byte, 0xf0, &mut out), Ok(0));
    same!(c8rtomb(&mut state, MbMode::Byte, 0x9f, &mut out), Ok(0));
    same!(c8rtomb(&mut state, MbMode::Byte, 0x92, &mut out), Ok(0));
    same!(
        c8rtomb(&mut state, MbMode::Byte, 0xa9, &mut out),
        Err(MbError::Ilseq)
    );
    same!(out[0], 0xcc);
    // NUL discards the pending sequence.
    same!(c8rtomb(&mut state, MbMode::Byte, 0xf0, &mut out), Ok(0));
    same!(c8rtomb(&mut state, MbMode::Byte, 0, &mut out), Ok(1));
    same!(out[0], 0);
    same!(out[1], 0xcc);
}

#[test]
fn c8rtomb_utf8_vectors() {
    let mut state = MbState::INITIAL;
    let mut out = [0xccu8; 4];
    same!(c8rtomb(&mut state, MbMode::Utf8, 0xf0, &mut out), Ok(0));
    same!(c8rtomb(&mut state, MbMode::Utf8, 0x9f, &mut out), Ok(0));
    same!(c8rtomb(&mut state, MbMode::Utf8, 0x92, &mut out), Ok(0));
    same!(c8rtomb(&mut state, MbMode::Utf8, 0xa9, &mut out), Ok(4));
    same!(out, [0xf0, 0x9f, 0x92, 0xa9]);
    // Missing last byte.
    out = [0xccu8; 4];
    same!(c8rtomb(&mut state, MbMode::Utf8, 0xf0, &mut out), Ok(0));
    same!(c8rtomb(&mut state, MbMode::Utf8, 0x9f, &mut out), Ok(0));
    same!(c8rtomb(&mut state, MbMode::Utf8, 0x92, &mut out), Ok(0));
    same!(
        c8rtomb(&mut state, MbMode::Utf8, b'A', &mut out),
        Err(MbError::Ilseq)
    );
    same!(out[0], 0xcc);

    let mut state = MbState::INITIAL;
    same!(
        c8rtomb(&mut state, MbMode::Utf8, 0x9f, &mut out),
        Err(MbError::Ilseq)
    );

    for length in 0..=3 {
        let mut state = MbState::INITIAL;
        let mut out = [0xccu8; 4];
        for byte in [0xf0, 0x9f, 0x92, 0xa9].into_iter().take(length) {
            same!(c8rtomb(&mut state, MbMode::Utf8, byte, &mut out), Ok(0));
        }
        same!(c8rtomb(&mut state, MbMode::Utf8, 0, &mut out), Ok(1));
        same!(out[0], 0);
        same!(out[1], 0xcc);
    }
}

#[test]
fn mbrtoc16_c_locale_vectors() {
    let mut state = MbState::INITIAL;
    same!(
        mbrtoc16(&mut state, MbMode::Byte, b"\0"),
        Mbr16::Char { unit: 0, used: 1 }
    );
    same!(
        mbrtoc16(&mut state, MbMode::Byte, b"A"),
        Mbr16::Char {
            unit: u16::from(b'A'),
            used: 1
        }
    );
    // Zero length: -2, untouched.
    same!(mbrtoc16(&mut state, MbMode::Byte, &[]), Mbr16::Incomplete);
    // No read-ahead.
    same!(
        mbrtoc16(&mut state, MbMode::Byte, b"AB"),
        Mbr16::Char {
            unit: u16::from(b'A'),
            used: 1
        }
    );
    same!(
        mbrtoc16(&mut state, MbMode::Byte, b"C"),
        Mbr16::Char {
            unit: u16::from(b'C'),
            used: 1
        }
    );
}

#[test]
fn mbrtoc16_utf8_vectors() {
    let mut state = MbState::INITIAL;
    same!(
        mbrtoc16(&mut state, MbMode::Utf8, b"\0"),
        Mbr16::Char { unit: 0, used: 1 }
    );
    same!(
        mbrtoc16(&mut state, MbMode::Utf8, b"A"),
        Mbr16::Char {
            unit: u16::from(b'A'),
            used: 1
        }
    );
    same!(
        mbrtoc16(&mut state, MbMode::Utf8, &[0xc3]),
        Mbr16::Incomplete
    );
    state = MbState::INITIAL;
    same!(
        mbrtoc16(&mut state, MbMode::Utf8, &[0xc3, 0x84]),
        Mbr16::Char {
            unit: 0x00c4,
            used: 2
        }
    );
    // Restarting.
    same!(
        mbrtoc16(&mut state, MbMode::Utf8, &[0xc3]),
        Mbr16::Incomplete
    );
    same!(
        mbrtoc16(&mut state, MbMode::Utf8, &[0xb7]),
        Mbr16::Char {
            unit: 0x00f7,
            used: 1
        }
    );
    // Surrogate pair.
    same!(
        mbrtoc16(&mut state, MbMode::Utf8, &[0xf0, 0x9f, 0x92, 0xa9]),
        Mbr16::Char {
            unit: 0xd83d,
            used: 4
        }
    );
    same!(
        mbrtoc16(&mut state, MbMode::Utf8, &[]),
        Mbr16::PendingLow { unit: 0xdca9 }
    );
    // Precomposed and combined e-acute.
    same!(
        mbrtoc16(&mut state, MbMode::Utf8, &[0xc3, 0xa9]),
        Mbr16::Char {
            unit: 0x00e9,
            used: 2
        }
    );
    same!(
        mbrtoc16(&mut state, MbMode::Utf8, &[0x65, 0xcc, 0x81]),
        Mbr16::Char {
            unit: 0x0065,
            used: 1
        }
    );
    same!(
        mbrtoc16(&mut state, MbMode::Utf8, &[0xcc, 0x81]),
        Mbr16::Char {
            unit: 0x0301,
            used: 2
        }
    );
}

#[test]
fn c16rtomb_c_locale_vectors() {
    let mut state = MbState::INITIAL;
    let mut out = [0xccu8; 4];
    same!(c16rtomb(&mut state, MbMode::Byte, 0, &mut out), Ok(1));
    same!(out[0], 0);
    same!(out[1], 0xcc);
    same!(
        c16rtomb(&mut state, MbMode::Byte, u16::from(b'A'), &mut out),
        Ok(1)
    );
    same!(out[0], b'A');
    // Pile of poo is not representable in the single-byte locale.
    out = [0xccu8; 4];
    same!(c16rtomb(&mut state, MbMode::Byte, 0xd83d, &mut out), Ok(0));
    same!(
        c16rtomb(&mut state, MbMode::Byte, 0xdca9, &mut out),
        Err(MbError::Ilseq)
    );
    same!(out[0], 0xcc);
}

#[test]
fn c16rtomb_utf8_vectors() {
    let mut state = MbState::INITIAL;
    let mut out = [0xccu8; 4];
    same!(c16rtomb(&mut state, MbMode::Utf8, 0xd83d, &mut out), Ok(0));
    same!(c16rtomb(&mut state, MbMode::Utf8, 0xdca9, &mut out), Ok(4));
    same!(out, [0xf0, 0x9f, 0x92, 0xa9]);
    out = [0xccu8; 4];
    same!(c16rtomb(&mut state, MbMode::Utf8, 0xd83d, &mut out), Ok(0));
    same!(
        c16rtomb(&mut state, MbMode::Utf8, u16::from(b'A'), &mut out),
        Err(MbError::Ilseq)
    );
    same!(out[0], 0xcc);
    same!(
        c16rtomb(&mut state, MbMode::Utf8, 0xdca9, &mut out),
        Err(MbError::Ilseq)
    );
    same!(c16rtomb(&mut state, MbMode::Utf8, 0xd83d, &mut out), Ok(0));
    // NetBSD expects 1 for a NUL that discards the pending surrogate; musl c16rtomb
    // (the donor of 5.A row 11) returns EILSEQ. RD-new D-ct-8.
    same!(
        c16rtomb(&mut state, MbMode::Utf8, 0, &mut out),
        Err(MbError::Ilseq)
    );
}

#[test]
fn mbrtoc32_and_c32rtomb_null_vectors() {
    let mut state = MbState::INITIAL;
    // s == NULL is the shell's call mbrtoc32(..., "\0", 1, ps).
    same!(
        mbrtoc32(&mut state, MbMode::Byte, b"\0"),
        Mbr::Char { wc: 0, used: 1 }
    );
    let mut out = [0u8; 4];
    same!(c32rtomb(MbMode::Utf8, u32::from(b'x'), &mut out), Ok(1));
    same!(out[0], b'x');
}

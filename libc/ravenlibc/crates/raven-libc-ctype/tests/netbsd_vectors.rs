// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! NetBSD locale vector tables: `t_mbrtowc.c` C and UTF-8 rows, `t_mbtowc.c`,
//! `t_wctype.c` C and UTF-8 rows, `t_toupper.c` C row (A-ctype-14).

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
fn mbrtowc_c_locale_table() {
    let data = b"ABCD01234_\\\0";
    let wchars = [
        0x41u32, 0x42, 0x43, 0x44, 0x30, 0x31, 0x32, 0x33, 0x34, 0x5f, 0x5c,
    ];
    for n in (1..=9usize).rev() {
        let mut state = MbState::INITIAL;
        let mut source = 0usize;
        let mut index = 0usize;
        let mut width = 0usize;
        loop {
            if source >= data.len() {
                break;
            }
            let end = data.len().min(source + n);
            match mbrtowc(&mut state, MbMode::Byte, &data[source..end]) {
                Mbr::Char { wc, used } => {
                    width += used;
                    source += used;
                    if wc == 0 {
                        break;
                    }
                    same!(wc, wchars[index], "n={n} index={index}");
                    same!(width, 1, "n={n} index={index}");
                    index += 1;
                    width = 0;
                }
                Mbr::Incomplete => {
                    source = data.len().min(source + n);
                    width += n;
                }
                Mbr::Ilseq => panic!("C locale rejected the table at n={n}"),
            }
        }
        same!(index, wchars.len(), "n={n}");
    }
}

#[test]
fn mbrtowc_utf8_table() {
    let data: &[u8] =
        b"[\x01\x7f][\xc2\x80\xdf\xbf][\xe0\xa0\x80\xef\xbf\xbf][\xf0\x90\x80\x80\xf4\x8f\xbf\xbf]";
    let wchars = [
        0x5bu32, 0x01, 0x7f, 0x5d, 0x5b, 0x80, 0x7ff, 0x5d, 0x5b, 0x800, 0xffff, 0x5d, 0x5b,
        0x10000, 0x10ffff, 0x5d,
    ];
    let widths = [1usize, 1, 1, 1, 1, 2, 2, 1, 1, 3, 3, 1, 1, 4, 4, 1];
    for n in (1..=9usize).rev() {
        let mut state = MbState::INITIAL;
        let mut source = 0usize;
        let mut index = 0usize;
        let mut width = 0usize;
        loop {
            if source >= data.len() {
                break;
            }
            let end = data.len().min(source + n);
            match mbrtowc(&mut state, MbMode::Utf8, &data[source..end]) {
                Mbr::Char { wc, used } => {
                    width += used;
                    source += used;
                    if wc == 0 {
                        break;
                    }
                    same!(wc, wchars[index]);
                    same!(width, widths[index]);
                    index += 1;
                    width = 0;
                }
                Mbr::Incomplete => {
                    source = data.len().min(source + n);
                    width += n;
                }
                Mbr::Ilseq => panic!("UTF-8 rejected the table at n={n}"),
            }
        }
        same!(index, wchars.len(), "n={n}");
    }

    let mut with_nul = data.to_vec();
    with_nul.push(0);
    let mut out = [0u32; 32];
    let conv = mbsrtowcs(None, MbMode::Utf8, &with_nul, Some(&mut out[..]));
    same!(conv.ret, Ok(wchars.len()));
    same!(conv.src, SrcPos::Nul);
    same!(&out[..wchars.len()], &wchars);
}

#[test]
fn mbtowc_basic_utf8_rows() {
    same!(mbtowc(MbMode::Utf8, &[0xa0]), Err(MbError::Ilseq));
    same!(mbtowc(MbMode::Utf8, &[0xc2, 0xa0]), Ok((0xa0, 2)));
    same!(mbtowc(MbMode::Utf8, &[0xe4]), Err(MbError::Ilseq));
    same!(mblen(MbMode::Utf8, &[]), Err(MbError::Ilseq));
}

fn check_class(name: &[u8], in_bytes: &[u8], out_bytes: &[u8]) {
    let class = wctype(name);
    assert_ne!(class, 0, "wctype({name:?})");
    for &byte in in_bytes {
        let wc = btowc(MbMode::Utf8, i32::from(byte));
        assert!(
            iswctype(wc, class),
            "{} 0x{byte:02x}",
            String::from_utf8_lossy(name)
        );
    }
    for &byte in out_bytes {
        let wc = btowc(MbMode::Utf8, i32::from(byte));
        assert!(
            !iswctype(wc, class),
            "{} 0x{byte:02x}",
            String::from_utf8_lossy(name)
        );
    }
}

#[test]
fn wctype_c_and_utf8_rows() {
    let classes: [(&[u8], &[u8], &[u8]); 12] = [
        (b"alnum", b"abc123", b"&^%$"),
        (b"alpha", b"ABCabc", b"123*&^"),
        (b"blank", b" \t", b"abc123%$^&\r\n"),
        (b"cntrl", b"\x07", b"abc123^&*"),
        (b"digit", b"123", b"abc*()"),
        (b"graph", b"abc123ABC[];?~", b" "),
        (b"lower", b"abc", b"ABC123&*("),
        (b"print", b"abcABC123%^&* ", b"\r\n\t"),
        (b"punct", b".,;:!?", b"abc123 \n"),
        (b"space", b" \t\r\n", b"abc123ABC&*("),
        (b"upper", b"ABC", b"abc123&*("),
        (b"xdigit", b"123abcABC", b"xyz"),
    ];
    for (name, inside, outside) in classes {
        check_class(name, inside, outside);
    }
}

#[test]
fn toupper_c_row() {
    let lower = b"abcde12345";
    let upper = b"ABCDE12345";
    for (lower, upper) in lower.iter().zip(upper.iter()) {
        same!(toupper(i32::from(*lower)), i32::from(*upper));
        same!(tolower(i32::from(*upper)), i32::from(*lower));
    }
}

// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Differential test: `raven-libc-ctype` against the musl multibyte originals.
//!
//! The C is built with the recipe `ctype-multibyte`; `MB_CUR_MAX` reads the
//! `RAVEN_ORACLE_MB_CUR_MAX` environment variable (the C35 `shim:` include path is relative to the
//! build directory and does not resolve, so the recipe uses a `getenv` expression instead; RD-new
//! D-ct-10). The test sets the variable before each mode's phase.

#![cfg(feature = "oracle-ctype-multibyte")]
#![allow(clippy::undocumented_unsafe_blocks)] // every unsafe call is a pure scalar/slice oracle call

use std::ffi::{c_char, c_int, c_uint};

use raven_libc_ctype::*;
use serde::Deserialize;

#[derive(Deserialize)]
struct FixedCorpus {
    case: Vec<FixedCase>,
}

#[derive(Deserialize)]
struct FixedCase {
    bytes_hex: String,
}

fn run_fixed_corpus(mode: MbMode) {
    let corpus: FixedCorpus =
        toml::from_str(include_str!("../corpus/ctype/mbrtowc.toml")).expect("fixed corpus");
    for case in corpus.case {
        let mut bytes = Vec::new();
        let text = case.bytes_hex.as_bytes();
        let mut index = 0;
        while index + 1 < text.len() {
            let pair = [text[index], text[index + 1]];
            let hex = core::str::from_utf8(&pair).expect("hex");
            bytes.push(u8::from_str_radix(hex, 16).expect("hex byte"));
            index += 2;
        }
        assert!(mbr_matches(&bytes, mode), "fixed corpus {bytes:02x?}");
    }
}

#[repr(C)]
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
struct State {
    w0: u32,
    w1: u32,
}

#[link(name = "oracle_ctype_multibyte", kind = "dylib")]
unsafe extern "C" {
    fn oracle_mbrtowc(wc: *mut c_int, src: *const c_char, n: usize, st: *mut State) -> usize;
    fn oracle_mbtowc(wc: *mut c_int, src: *const c_char, n: usize) -> c_int;
    fn oracle_mblen(src: *const c_char, n: usize) -> c_int;
    fn oracle_mbrlen(src: *const c_char, n: usize, st: *mut State) -> usize;
    fn oracle_wcrtomb(dst: *mut c_char, wc: c_int, st: *mut State) -> usize;
    fn oracle_wctomb(dst: *mut c_char, wc: c_int) -> c_int;
    fn oracle_btowc(c: c_int) -> c_uint;
    fn oracle_wctob(c: c_uint) -> c_int;
    fn oracle_mbsrtowcs(
        dst: *mut c_int,
        src: *mut *const c_char,
        wn: usize,
        st: *mut State,
    ) -> usize;
    fn oracle_mbsnrtowcs(
        dst: *mut c_int,
        src: *mut *const c_char,
        nms: usize,
        wn: usize,
        st: *mut State,
    ) -> usize;
    fn oracle_wcsrtombs(
        dst: *mut c_char,
        src: *mut *const c_int,
        n: usize,
        st: *mut State,
    ) -> usize;
    fn oracle_wcsnrtombs(
        dst: *mut c_char,
        src: *mut *const c_int,
        wn: usize,
        n: usize,
        st: *mut State,
    ) -> usize;
    fn oracle_mbrtoc16(dst: *mut u16, src: *const c_char, n: usize, st: *mut State) -> usize;
    fn oracle_c16rtomb(dst: *mut c_char, c16: u16, st: *mut State) -> usize;
    fn oracle_mbrtoc32(dst: *mut u32, src: *const c_char, n: usize, st: *mut State) -> usize;
    fn oracle_c32rtomb(dst: *mut c_char, c32: u32, st: *mut State) -> usize;
}

fn set_mb_cur_max(value: u8) {
    // The donor C uses the host stdlib's MB_CUR_MAX, which follows the host LC_CTYPE. The
    // vendored musl code only distinguishes 1 (C/POSIX) from >1 (UTF-8).
    let locale: &[u8] = if value == 1 { b"C\0" } else { b"C.UTF-8\0" };
    // SAFETY: LC_CTYPE is the host glibc value 0; `locale` is NUL-terminated and static.
    let result = unsafe { setlocale(0, locale.as_ptr().cast::<c_char>()) };
    assert!(!result.is_null(), "setlocale failed for {locale:?}");
}

unsafe extern "C" {
    fn setlocale(category: c_int, locale: *const c_char) -> *mut c_char;
}

fn as_c(bytes: &[u8]) -> *const c_char {
    bytes.as_ptr().cast::<c_char>()
}

fn mbr_matches(bytes: &[u8], mode: MbMode) -> bool {
    let mut rust_state = MbState::INITIAL;
    let rust = mbrtowc(&mut rust_state, mode, bytes);
    let mut c_state = State::default();
    let mut c_wc = 0i32;
    // SAFETY: `bytes` is valid for `bytes.len()` reads and both output pointers are valid.
    let c_ret = unsafe { oracle_mbrtowc(&mut c_wc, as_c(bytes), bytes.len(), &mut c_state) };
    if c_ret as isize != rust.c_return() {
        eprintln!(
            "ret mismatch byte_mode={} bytes={bytes:02x?} c={c_ret} rust={} c_wc={c_wc} c_state={:?} r_state=({},{})",
            mode == MbMode::Byte,
            rust.c_return(),
            c_state,
            rust_state.opaque1,
            rust_state.opaque2
        );
        return false;
    }
    if let Mbr::Char { wc, .. } = rust
        && c_wc as u32 != wc
    {
        eprintln!(
            "wc mismatch byte_mode={} c_wc={c_wc} rust={wc}",
            mode == MbMode::Byte
        );
        return false;
    }
    if !(c_state.w0 == rust_state.opaque1 && c_state.w1 == rust_state.opaque2) {
        eprintln!(
            "state mismatch byte_mode={} bytes={bytes:02x?} c={:?} rust=({},{})",
            mode == MbMode::Byte,
            c_state,
            rust_state.opaque1,
            rust_state.opaque2
        );
        return false;
    }
    true
}

fn mbrtowc_domain(mode: MbMode) -> u64 {
    let mut count = 0u64;
    for first in 0..=255u8 {
        for second in 0..=255u8 {
            for third in 0..=255u8 {
                if !mbr_matches(&[first, second, third], mode) {
                    panic!("mbrtowc mismatch {first:02x} {second:02x} {third:02x}");
                }
                count += 1;
            }
        }
    }
    for lead in 0xf0..=0xf7u8 {
        for second in 0..=255u8 {
            for third in 0..=255u8 {
                for fourth in 0..=255u8 {
                    if !mbr_matches(&[lead, second, third, fourth], mode) {
                        panic!("mbrtowc mismatch {lead:02x} {second:02x} {third:02x} {fourth:02x}");
                    }
                    count += 1;
                }
            }
        }
    }
    count
}

fn stateless_domain(mode: MbMode) -> u64 {
    let mut count = 0u64;
    for first in 0..=255u8 {
        for second in 0..=255u8 {
            let bytes = [first, second];
            let mut c_wc = 0i32;
            // SAFETY: valid slice pointer and output pointer.
            let c_ret = unsafe { oracle_mbtowc(&mut c_wc, as_c(&bytes), 2) };
            let rust = mbtowc(mode, &bytes);
            let rust_ret = match rust {
                Ok((_, used)) => used as i32,
                Err(_) => -1,
            };
            assert!(c_ret == rust_ret, "mbtowc ret {first:02x}{second:02x}");
            if let Ok((wc, _)) = rust {
                assert!(c_wc as u32 == wc, "mbtowc wc");
            }
            // SAFETY: valid slice pointer.
            let c_len = unsafe { oracle_mblen(as_c(&bytes), 2) };
            let rust_len = match mblen(mode, &bytes) {
                Ok(used) => used as i32,
                Err(_) => -1,
            };
            assert!(c_len == rust_len, "mblen");
            let mut c_state = State::default();
            // SAFETY: valid slice/state pointers.
            let c_rlen = unsafe { oracle_mbrlen(as_c(&bytes), 2, &mut c_state) };
            let mut rust_state = MbState::INITIAL;
            let rust_rlen = mbrlen(&mut rust_state, mode, &bytes).c_return();
            assert!(c_rlen as isize == rust_rlen, "mbrlen ret");
            assert!(c_state.w0 == rust_state.opaque1 && c_state.w1 == rust_state.opaque2);
            count += 1;
        }
    }
    count
}

fn encoder_domain(mode: MbMode) -> u64 {
    let mut count = 0u64;
    let ranges = [
        0u32..=0x10ffff,
        0x110000..=0x1100ff,
        0x7fff_ff00..=0x8000_00ff,
        0xffff_ff00..=u32::MAX,
    ];
    for range in ranges {
        for cp in range {
            let mut rust_out = [0xccu8; 4];
            let rust = wcrtomb(mode, cp, &mut rust_out);
            let mut c_out = [0xccu8; 8];
            let mut c_state = State::default();
            // SAFETY: output buffer is large enough and the state pointer is valid.
            let c_ret = unsafe {
                oracle_wcrtomb(
                    c_out.as_mut_ptr().cast::<c_char>(),
                    cp as c_int,
                    &mut c_state,
                )
            };
            let rust_ret = match rust {
                Ok(length) => length as isize,
                Err(_) => -1,
            };
            assert!(c_ret as isize == rust_ret, "wcrtomb ret U+{cp:04X}");
            if rust.is_ok() {
                assert_eq!(&c_out[..4], &rust_out, "wcrtomb bytes U+{cp:04X}");
            }
            assert_eq!(&c_out[4..], &[0xcc, 0xcc, 0xcc, 0xcc]);
            assert!(c_state.w0 == 0 && c_state.w1 == 0);
            let mut wctomb_out = [0xccu8; 4];
            let mut c_wctomb = [0xccu8; 8];
            let rust_wctomb = wctomb(mode, cp, &mut wctomb_out);
            // SAFETY: output buffer is valid.
            let c_wctomb_ret =
                unsafe { oracle_wctomb(c_wctomb.as_mut_ptr().cast::<c_char>(), cp as c_int) };
            assert!(
                c_wctomb_ret as isize
                    == match rust_wctomb {
                        Ok(length) => length as isize,
                        Err(_) => -1,
                    }
            );
            count += 1;
        }
    }
    count
}

fn btowc_wctob_domain(mode: MbMode) -> u64 {
    let mut count = 0u64;
    for c in -300..=300i32 {
        // SAFETY: pure function.
        let c_ret = unsafe { oracle_btowc(c) };
        assert!(c_ret == btowc(mode, c), "btowc {c}");
        count += 1;
    }
    for c in [i32::MIN, i32::MIN + 1, -2, EOF, i32::MAX - 1, i32::MAX] {
        // SAFETY: pure function.
        let c_ret = unsafe { oracle_btowc(c) };
        assert!(c_ret == btowc(mode, c), "btowc extreme {c}");
        count += 1;
    }
    for cp in 0..=0x10ffffu32 {
        // SAFETY: pure function.
        let c_ret = unsafe { oracle_wctob(cp) };
        assert!(c_ret == wctob(mode, cp), "wctob U+{cp:04X}");
        count += 1;
    }
    count
}

fn xorshift(state: &mut u64) -> u64 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *state = x;
    x
}

fn strings_domain(mode: MbMode) -> u64 {
    let alphabet = [
        0u8, 0x41, 0x7f, 0xc2, 0xdf, 0xe0, 0xed, 0xf0, 0xf4, 0x80, 0xbf, 0xff,
    ];
    let mut seed = 1u64;
    let mut count = 0u64;
    for _ in 0..1_000_000u32 {
        let len = (xorshift(&mut seed) % 9) as usize;
        let mut bytes = Vec::with_capacity(len + 1);
        for _ in 0..len {
            bytes.push(alphabet[(xorshift(&mut seed) as usize) % alphabet.len()]);
        }
        bytes.push(0);
        for wn in 0..=8usize {
            let mut rust_dst = vec![0xcccc_ccccu32; wn];
            let rust = mbsrtowcs(None, mode, &bytes, Some(&mut rust_dst[..]));
            let mut c_dst = vec![0xcccc_ccccu32; 12];
            let mut source = bytes.as_ptr().cast::<c_char>();
            let mut c_state = State::default();
            // SAFETY: buffers are valid for the requested sizes; the source pointer is
            // updated by the C only inside the source allocation.
            let c_ret = unsafe {
                oracle_mbsrtowcs(
                    c_dst.as_mut_ptr().cast::<c_int>(),
                    &mut source,
                    wn,
                    &mut c_state,
                )
            };
            let rust_ret = match rust.ret {
                Ok(value) => value as isize,
                Err(_) => -1,
            };
            assert!(c_ret as isize == rust_ret, "mbsrtowcs ret");
            assert_eq!(&c_dst[..wn], &rust_dst[..]);
            let c_at_nul = source.is_null();
            let rust_at_nul = rust.src == SrcPos::Nul;
            assert!(c_at_nul == rust_at_nul, "mbsrtowcs src");
            if !c_at_nul {
                let offset = source as usize - bytes.as_ptr() as usize;
                assert!(rust.src == SrcPos::At(offset), "mbsrtowcs offset");
            }
            assert!(c_state.w0 == rust.state.opaque1 && c_state.w1 == rust.state.opaque2);
            count += 1;
        }
        for nms in 0..=bytes.len() {
            let mut rust_dst = [0xcccc_ccccu32; 4];
            let rust = mbsnrtowcs(None, mode, &bytes[..nms], Some(&mut rust_dst[..]));
            let mut c_dst = vec![0xcccc_ccccu32; 12];
            let mut source = bytes.as_ptr().cast::<c_char>();
            let mut c_state = State::default();
            // SAFETY: as above.
            let c_ret = unsafe {
                oracle_mbsnrtowcs(
                    c_dst.as_mut_ptr().cast::<c_int>(),
                    &mut source,
                    nms,
                    4,
                    &mut c_state,
                )
            };
            let rust_ret = match rust.ret {
                Ok(value) => value as isize,
                Err(_) => -1,
            };
            assert!(c_ret as isize == rust_ret, "mbsnrtowcs ret");
            assert_eq!(&c_dst[..4], &rust_dst[..]);
            assert!(c_state.w0 == rust.state.opaque1 && c_state.w1 == rust.state.opaque2);
            let c_at_nul = source.is_null();
            assert!(c_at_nul == (rust.src == SrcPos::Nul), "mbsnrtowcs src");
            if !c_at_nul {
                let offset = source as usize - bytes.as_ptr() as usize;
                assert!(rust.src == SrcPos::At(offset), "mbsnrtowcs offset");
            }
            count += 1;
        }
    }
    count
}

fn uchar_domain(mode: MbMode) -> u64 {
    let mut count = 0u64;
    for first in 0..=255u8 {
        for second in 0..=255u8 {
            let bytes = [first, second];
            let mut rust_state = MbState::INITIAL;
            let rust = mbrtoc16(&mut rust_state, mode, &bytes);
            let mut c_state = State::default();
            let mut c_unit = 0u16;
            // SAFETY: valid slice/state/output pointers.
            let c_ret = unsafe { oracle_mbrtoc16(&mut c_unit, as_c(&bytes), 2, &mut c_state) };
            assert!(c_ret as isize == rust.c_return());
            if let Mbr16::Char { unit, .. } = rust {
                assert!(c_unit == unit);
            }
            assert!(c_state.w0 == rust_state.opaque1);
            let mut rust32 = MbState::INITIAL;
            let rust32_result = mbrtoc32(&mut rust32, mode, &bytes);
            let mut c32_state = State::default();
            let mut c32 = 0u32;
            // SAFETY: valid pointers.
            let c32_ret = unsafe { oracle_mbrtoc32(&mut c32, as_c(&bytes), 2, &mut c32_state) };
            assert!(c32_ret as isize == rust32_result.c_return());
            if let Mbr::Char { wc, .. } = rust32_result {
                assert!(c32 == wc);
            }
            assert!(c32_state.w0 == rust32.opaque1);
            count += 1;
        }
    }
    for first in 0..=0xffffu16 {
        let mut c_out = [0xccu8; 8];
        let mut rust_out = [0xccu8; 4];
        let mut c_state = State::default();
        let mut rust_state = MbState::INITIAL;
        // SAFETY: valid output/state pointers.
        let c_ret =
            unsafe { oracle_c16rtomb(c_out.as_mut_ptr().cast::<c_char>(), first, &mut c_state) };
        let rust_ret = c16rtomb(&mut rust_state, mode, first, &mut rust_out);
        assert!(
            c_ret as isize
                == match rust_ret {
                    Ok(length) => length as isize,
                    Err(_) => -1,
                }
        );
        assert!(c_state.w0 == rust_state.opaque1);
        count += 1;
        let mut c32_out = [0xccu8; 8];
        let mut rust32_out = [0xccu8; 4];
        // SAFETY: valid output pointer.
        let c32_ret = unsafe {
            oracle_c32rtomb(
                c32_out.as_mut_ptr().cast::<c_char>(),
                u32::from(first),
                null_mut(),
            )
        };
        let rust32_ret = c32rtomb(mode, u32::from(first), &mut rust32_out);
        assert!(
            c32_ret as isize
                == match rust32_ret {
                    Ok(length) => length as isize,
                    Err(_) => -1,
                }
        );
        count += 1;
    }
    count
}

fn null_mut() -> *mut State {
    core::ptr::null_mut()
}

fn wide_strings_domain(mode: MbMode) -> u64 {
    let alphabet = [0u32, 0x41, 0x7f, 0x20ac, 0x1f600, 0xd800, 0x10ffff];
    let mut seed = 0x1234_5678u64;
    let mut count = 0u64;
    for _ in 0..200_000u32 {
        let len = (xorshift(&mut seed) % 9) as usize;
        let mut wchars = Vec::with_capacity(len + 1);
        for _ in 0..len {
            wchars.push(alphabet[(xorshift(&mut seed) as usize) % alphabet.len()]);
        }
        wchars.push(0);
        for n in 0..=8usize {
            let mut rust_dst = vec![0xccu8; n];
            let rust = wcsrtombs(None, mode, &wchars, Some(&mut rust_dst[..]));
            let mut c_dst = vec![0xccu8; 12];
            let mut source = wchars.as_ptr().cast::<c_int>();
            let mut c_state = State::default();
            // SAFETY: buffers are valid and the source pointer stays inside `wchars`.
            let c_ret = unsafe {
                oracle_wcsrtombs(
                    c_dst.as_mut_ptr().cast::<c_char>(),
                    &mut source,
                    n,
                    &mut c_state,
                )
            };
            let rust_ret = match rust.ret {
                Ok(value) => value as isize,
                Err(_) => -1,
            };
            assert!(c_ret as isize == rust_ret, "wcsrtombs ret");
            assert_eq!(&c_dst[..n], &rust_dst[..]);
            let c_at_nul = source.is_null();
            assert!(c_at_nul == (rust.src == SrcPos::Nul), "wcsrtombs src");
            if !c_at_nul {
                let offset =
                    (source as usize - wchars.as_ptr() as usize) / core::mem::size_of::<u32>();
                assert!(rust.src == SrcPos::At(offset));
            }
            count += 1;
        }
        for wn in 0..=wchars.len() {
            for n in 0..=8usize {
                let mut rust_dst = vec![0xccu8; n];
                let rust = wcsnrtombs(None, mode, &wchars[..wn], Some(&mut rust_dst[..]));
                let mut c_dst = vec![0xccu8; 12];
                let mut source = wchars.as_ptr().cast::<c_int>();
                let mut c_state = State::default();
                // SAFETY: as above.
                let c_ret = unsafe {
                    oracle_wcsnrtombs(
                        c_dst.as_mut_ptr().cast::<c_char>(),
                        &mut source,
                        wn,
                        n,
                        &mut c_state,
                    )
                };
                let rust_ret = match rust.ret {
                    Ok(value) => value as isize,
                    Err(_) => -1,
                };
                assert!(
                    c_ret as isize == rust_ret,
                    "wcsnrtombs ret c={c_ret} rust={rust_ret} wn={wn} n={n} wchars={wchars:?}"
                );
                assert_eq!(&c_dst[..n], &rust_dst[..]);
                let c_at_nul = source.is_null();
                assert!(c_at_nul == (rust.src == SrcPos::Nul), "wcsnrtombs src");
                if !c_at_nul {
                    let offset =
                        (source as usize - wchars.as_ptr() as usize) / core::mem::size_of::<u32>();
                    assert!(rust.src == SrcPos::At(offset), "wcsnrtombs offset");
                }
                count += 1;
            }
        }
    }
    count
}

#[test]
fn differential_ctype_multibyte() {
    let mut total = 0u64;
    for mode in [MbMode::Byte, MbMode::Utf8] {
        set_mb_cur_max(if mode == MbMode::Byte { 1 } else { 4 });
        run_fixed_corpus(mode);
        let count = mbrtowc_domain(mode);
        println!(
            "mbrtowc cases ({mode_is_byte}): {count}",
            mode_is_byte = mode == MbMode::Byte
        );
        total += count;
        let count = stateless_domain(mode);
        println!("stateless cases: {count}");
        total += count;
        let count = encoder_domain(mode);
        println!("encoder cases: {count}");
        total += count;
        let count = btowc_wctob_domain(mode);
        println!("btowc/wctob cases: {count}");
        total += count;
        let count = strings_domain(mode);
        println!("string cases: {count}");
        total += count;
        let count = wide_strings_domain(mode);
        println!("wide string cases: {count}");
        total += count;
        let count = uchar_domain(mode);
        println!("uchar cases: {count}");
        total += count;
    }
    println!("total observations: {total}");
}

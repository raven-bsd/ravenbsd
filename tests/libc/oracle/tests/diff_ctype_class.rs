// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Differential test: `raven-libc-ctype` against the musl ctype/wctype originals.
//!
//! The data-dependent classes are Unicode 15.1.0 here and Unicode 12.1 in musl, so those
//! functions record mismatches into `target/ctype-musl-delta.txt` and are checked against the
//! reviewed budget in `tests/musl_delta_budget.toml`. The hard-coded classes and the char ctype
//! functions require exact equality.

#![cfg(feature = "oracle-ctype-class")]
#![allow(clippy::undocumented_unsafe_blocks)] // every unsafe call is a pure scalar oracle call

use std::collections::HashMap;
use std::ffi::{c_char, c_int, c_uint};
use std::path::Path;

use raven_libc_ctype::*;
use serde::Deserialize;

#[link(name = "oracle_ctype_class", kind = "dylib")]
unsafe extern "C" {
    fn oracle_isalnum(c: c_int) -> c_int;
    fn oracle_isalpha(c: c_int) -> c_int;
    fn oracle_isascii(c: c_int) -> c_int;
    fn oracle_isblank(c: c_int) -> c_int;
    fn oracle_iscntrl(c: c_int) -> c_int;
    fn oracle_isdigit(c: c_int) -> c_int;
    fn oracle_isgraph(c: c_int) -> c_int;
    fn oracle_islower(c: c_int) -> c_int;
    fn oracle_isprint(c: c_int) -> c_int;
    fn oracle_ispunct(c: c_int) -> c_int;
    fn oracle_isspace(c: c_int) -> c_int;
    fn oracle_isupper(c: c_int) -> c_int;
    fn oracle_isxdigit(c: c_int) -> c_int;
    fn oracle_toascii(c: c_int) -> c_int;
    fn oracle_tolower(c: c_int) -> c_int;
    fn oracle_toupper(c: c_int) -> c_int;

    fn oracle_iswspace(c: c_uint) -> c_int;
    fn oracle_iswprint(c: c_uint) -> c_int;
    fn oracle_iswcntrl(c: c_uint) -> c_int;
    fn oracle_iswdigit(c: c_uint) -> c_int;
    fn oracle_iswxdigit(c: c_uint) -> c_int;
    fn oracle_iswblank(c: c_uint) -> c_int;
    fn oracle_iswgraph(c: c_uint) -> c_int;
    fn oracle_iswalpha(c: c_uint) -> c_int;
    fn oracle_iswalnum(c: c_uint) -> c_int;
    fn oracle_iswpunct(c: c_uint) -> c_int;
    fn oracle_iswlower(c: c_uint) -> c_int;
    fn oracle_iswupper(c: c_uint) -> c_int;
    fn oracle_towupper(c: c_uint) -> c_uint;
    fn oracle_towlower(c: c_uint) -> c_uint;
    fn oracle_wcwidth(c: c_int) -> c_int;
    fn oracle_wcswidth(s: *const c_int, n: usize) -> c_int;
    fn oracle_wctype(name: *const c_char) -> u64;
    fn oracle_iswctype(c: c_uint, t: u64) -> c_int;
    fn oracle_wctrans(name: *const c_char) -> u64;
    fn oracle_towctrans(c: c_uint, t: u64) -> c_uint;
}

#[derive(Deserialize)]
struct BudgetFile {
    budget: HashMap<String, u64>,
    #[serde(default)]
    alpha_allow: Vec<u32>,
}

#[derive(Deserialize)]
struct FixedCorpus {
    case: Vec<FixedCase>,
}

#[derive(Deserialize)]
struct FixedCase {
    c: i32,
}

fn run_fixed_corpus() {
    let corpus: FixedCorpus =
        toml::from_str(include_str!("../corpus/ctype/ctype.toml")).expect("fixed corpus");
    for case in corpus.case {
        let c = case.c;
        assert!((unsafe { oracle_isalpha(c) } != 0) == isalpha(c));
        assert!((unsafe { oracle_isdigit(c) } != 0) == isdigit(c));
        assert!((unsafe { oracle_isspace(c) } != 0) == isspace(c));
        assert!((unsafe { oracle_toupper(c) }) == toupper(c));
        assert!((unsafe { oracle_tolower(c) }) == tolower(c));
    }
}

fn target_file(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../target")
        .join(name)
}

#[test]
fn differential_ctype_class() {
    run_fixed_corpus();
    // Char ctype: exact over -300..=300 and the extremes.
    let mut char_cases = 0u64;
    for c in (-300..=300i32).chain([i32::MIN, i32::MIN + 1, EOF, i32::MAX - 1, i32::MAX]) {
        // SAFETY: every oracle function is pure.
        let c_isalnum = unsafe { oracle_isalnum(c) } != 0;
        assert!(c_isalnum == isalnum(c), "isalnum({c})");
        assert!(
            (unsafe { oracle_isalpha(c) } != 0) == isalpha(c),
            "isalpha({c})"
        );
        assert!(
            (unsafe { oracle_isascii(c) } != 0) == isascii(c),
            "isascii({c})"
        );
        assert!(
            (unsafe { oracle_isblank(c) } != 0) == isblank(c),
            "isblank({c})"
        );
        assert!(
            (unsafe { oracle_iscntrl(c) } != 0) == iscntrl(c),
            "iscntrl({c})"
        );
        assert!(
            (unsafe { oracle_isdigit(c) } != 0) == isdigit(c),
            "isdigit({c})"
        );
        assert!(
            (unsafe { oracle_isgraph(c) } != 0) == isgraph(c),
            "isgraph({c})"
        );
        assert!(
            (unsafe { oracle_islower(c) } != 0) == islower(c),
            "islower({c})"
        );
        assert!(
            (unsafe { oracle_isprint(c) } != 0) == isprint(c),
            "isprint({c})"
        );
        assert!(
            (unsafe { oracle_ispunct(c) } != 0) == ispunct(c),
            "ispunct({c})"
        );
        assert!(
            (unsafe { oracle_isspace(c) } != 0) == isspace(c),
            "isspace({c})"
        );
        assert!(
            (unsafe { oracle_isupper(c) } != 0) == isupper(c),
            "isupper({c})"
        );
        assert!(
            (unsafe { oracle_isxdigit(c) } != 0) == isxdigit(c),
            "isxdigit({c})"
        );
        assert!((unsafe { oracle_toascii(c) }) == toascii(c), "toascii({c})");
        assert!((unsafe { oracle_tolower(c) }) == tolower(c), "tolower({c})");
        assert!((unsafe { oracle_toupper(c) }) == toupper(c), "toupper({c})");
        char_cases += 1;
    }
    println!("char ctype cases: {char_cases}");

    // Hard-coded wide classes: zero mismatches allowed.
    let mut wide_cases = 0u64;
    for cp in (0..=0x110010u32).chain(0xffff_fff0..=u32::MAX) {
        assert!(
            (unsafe { oracle_iswspace(cp) } != 0) == iswspace(cp),
            "iswspace U+{cp:04X}"
        );
        assert!(
            (unsafe { oracle_iswprint(cp) } != 0) == iswprint(cp),
            "iswprint U+{cp:04X}"
        );
        assert!(
            (unsafe { oracle_iswcntrl(cp) } != 0) == iswcntrl(cp),
            "iswcntrl U+{cp:04X}"
        );
        assert!(
            (unsafe { oracle_iswdigit(cp) } != 0) == iswdigit(cp),
            "iswdigit U+{cp:04X}"
        );
        assert!(
            (unsafe { oracle_iswxdigit(cp) } != 0) == iswxdigit(cp),
            "iswxdigit U+{cp:04X}"
        );
        assert!(
            (unsafe { oracle_iswblank(cp) } != 0) == iswblank(cp),
            "iswblank U+{cp:04X}"
        );
        assert!(
            (unsafe { oracle_iswgraph(cp) } != 0) == iswgraph(cp),
            "iswgraph U+{cp:04X}"
        );
        wide_cases += 1;
    }
    println!("hard-coded wide cases: {wide_cases}");

    // Data-dependent functions: mismatch budget.
    let mut deltas: HashMap<&'static str, u64> = HashMap::new();
    let mut delta_file = String::new();
    let mut alpha_allow_list = Vec::new();
    fn record(
        deltas: &mut HashMap<&'static str, u64>,
        delta_file: &mut String,
        alpha_allow: &mut Vec<u32>,
        name: &'static str,
        cp: u32,
        musl: bool,
        ours: bool,
    ) {
        if musl != ours {
            *deltas.entry(name).or_insert(0) += 1;
            if name == "iswalpha" && musl && !ours && cp < 0x2000 {
                alpha_allow.push(cp);
            }
            delta_file.push_str(&format!("{name} U+{cp:04X} musl={musl} ours={ours}\n"));
        }
    }
    for cp in 0..0x110000u32 {
        // SAFETY: pure functions.
        let musl_alpha = unsafe { oracle_iswalpha(cp) } != 0;
        let musl_alnum = unsafe { oracle_iswalnum(cp) } != 0;
        let musl_punct = unsafe { oracle_iswpunct(cp) } != 0;
        let musl_lower = unsafe { oracle_iswlower(cp) } != 0;
        let musl_upper = unsafe { oracle_iswupper(cp) } != 0;
        let musl_toupper = unsafe { oracle_towupper(cp) };
        let musl_tolower = unsafe { oracle_towlower(cp) };
        let musl_width = unsafe { oracle_wcwidth(cp as c_int) };
        record(
            &mut deltas,
            &mut delta_file,
            &mut alpha_allow_list,
            "iswalpha",
            cp,
            musl_alpha,
            iswalpha(cp),
        );
        record(
            &mut deltas,
            &mut delta_file,
            &mut alpha_allow_list,
            "iswalnum",
            cp,
            musl_alnum,
            iswalnum(cp),
        );
        record(
            &mut deltas,
            &mut delta_file,
            &mut alpha_allow_list,
            "iswpunct",
            cp,
            musl_punct,
            iswpunct(cp),
        );
        record(
            &mut deltas,
            &mut delta_file,
            &mut alpha_allow_list,
            "iswlower",
            cp,
            musl_lower,
            iswlower(cp),
        );
        record(
            &mut deltas,
            &mut delta_file,
            &mut alpha_allow_list,
            "iswupper",
            cp,
            musl_upper,
            iswupper(cp),
        );
        if musl_toupper != towupper(cp) {
            *deltas.entry("towupper").or_insert(0) += 1;
            delta_file.push_str(&format!(
                "towupper U+{cp:04X} musl={musl_toupper:04X} ours={:04X}\n",
                towupper(cp)
            ));
        }
        if musl_tolower != towlower(cp) {
            *deltas.entry("towlower").or_insert(0) += 1;
            delta_file.push_str(&format!(
                "towlower U+{cp:04X} musl={musl_tolower:04X} ours={:04X}\n",
                towlower(cp)
            ));
        }
        if musl_width != wcwidth(cp) {
            *deltas.entry("wcwidth").or_insert(0) += 1;
            delta_file.push_str(&format!(
                "wcwidth U+{cp:04X} musl={musl_width} ours={}\n",
                wcwidth(cp)
            ));
        }
        if cp < 0x100 {
            assert!(musl_alpha == iswalpha(cp), "iswalpha latin-1 U+{cp:04X}");
            assert!(musl_punct == iswpunct(cp), "iswpunct latin-1 U+{cp:04X}");
            // musl 12.1 maps U+00DF to U+1E9E; Unicode 15.1 leaves the simple uppercase
            // mapping empty (5.C asserts towupper(0xDF) == 0xDF). RD-new D-ct-11.
            if cp != 0x00df {
                assert!(musl_toupper == towupper(cp), "towupper latin-1 U+{cp:04X}");
            }
            assert!(musl_tolower == towlower(cp), "towlower latin-1 U+{cp:04X}");
        }
    }
    std::fs::write(target_file("ctype-musl-delta.txt"), &delta_file).expect("write delta file");
    let budget_text = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/musl_delta_budget.toml"),
    )
    .expect("tests/musl_delta_budget.toml");
    let budget: BudgetFile = toml::from_str(&budget_text).expect("budget TOML");
    let mut summary = Vec::new();
    for name in [
        "iswalpha", "iswalnum", "iswpunct", "iswlower", "iswupper", "towupper", "towlower",
        "wcwidth",
    ] {
        let count = deltas.get(name).copied().unwrap_or(0);
        let allowed = budget.budget.get(name).copied().unwrap_or(0);
        summary.push(format!("{name}={count}/{allowed}"));
    }
    println!("musl delta: {}", summary.join(" "));
    for name in [
        "iswalpha", "iswalnum", "iswpunct", "iswlower", "iswupper", "towupper", "towlower",
        "wcwidth",
    ] {
        let count = deltas.get(name).copied().unwrap_or(0);
        let allowed = budget.budget.get(name).copied().unwrap_or(0);
        assert!(
            count <= allowed,
            "{name} mismatches {count} exceed the budget {allowed}"
        );
    }
    assert!(
        alpha_allow_list == budget.alpha_allow,
        "iswalpha below U+2000 allow-list changed: actual {alpha_allow_list:?}"
    );

    // wcswidth on short slices.
    let alphabet = [
        0u32, 0x41, 0x4e00, 0x0300, 0x0a, 0x1f600, 0x1f1e6, 0x10ffff, 0xfffe,
    ];
    let mut seq = 0u64;
    let mut state = 0x9e37_79b9u64;
    for _ in 0..10_000u32 {
        let len = (state % 9) as usize;
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        let mut slice = Vec::with_capacity(len);
        for _ in 0..len {
            slice.push(alphabet[(state as usize) % alphabet.len()]);
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        }
        for n in 0..=slice.len() {
            let mut c_slice: Vec<c_int> = slice[..n].iter().map(|&value| value as c_int).collect();
            // SAFETY: `c_slice` is valid for `n` elements.
            let c_ret = unsafe { oracle_wcswidth(c_slice.as_mut_ptr(), n) };
            assert!(c_ret == wcswidth(&slice[..n]), "wcswidth {slice:?} n={n}");
            seq += 1;
        }
    }
    println!("wcswidth cases: {seq}");

    // wctype/wctrans names and results.
    for name in [
        b"alnum".as_slice(),
        b"alpha",
        b"blank",
        b"cntrl",
        b"digit",
        b"graph",
        b"lower",
        b"print",
        b"punct",
        b"space",
        b"upper",
        b"xdigit",
    ] {
        let mut c_name = name.to_vec();
        c_name.push(0);
        // SAFETY: NUL-terminated name.
        let c_type = unsafe { oracle_wctype(c_name.as_ptr().cast::<c_char>()) };
        assert!(c_type == wctype(name), "wctype {name:?}");
        for cp in [0u32, 0x20, 0x41, 0x61, 0x300, 0x4e00, 0x1f600, u32::MAX] {
            // SAFETY: pure functions.
            let c_result = unsafe { oracle_iswctype(cp, c_type) } != 0;
            assert!(
                c_result == iswctype(cp, c_type),
                "iswctype {name:?} U+{cp:04X}"
            );
        }
    }
    for name in [b"toupper".as_slice(), b"tolower"] {
        let mut c_name = name.to_vec();
        c_name.push(0);
        // SAFETY: NUL-terminated name.
        let c_trans = unsafe { oracle_wctrans(c_name.as_ptr().cast::<c_char>()) };
        assert!(c_trans == wctrans(name), "wctrans {name:?}");
        for cp in [0u32, 0x41, 0x61, 0x1c5, 0x1f600, u32::MAX] {
            // SAFETY: pure functions.
            let c_result = unsafe { oracle_towctrans(cp, c_trans) };
            assert!(c_result == towctrans(cp, c_trans), "towctrans {name:?}");
        }
    }
}

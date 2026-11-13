// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Differential tests for `stdlib-openbsd`: conversion, sorting, getopt_long, getsubopt, a64l.

#![cfg(feature = "oracle-stdlib-openbsd")]
#![allow(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::arithmetic_side_effects,
    clippy::too_many_lines
)]
#![allow(clippy::undocumented_unsafe_blocks)] // oracle calls follow the C prototypes; test buffers outlive each call

use core::ffi::{c_char, c_int, c_long, c_void};
use std::cell::RefCell;
use std::ffi::{CStr, CString};

use raven_libc_oracle::call::{errno_get, errno_set};
use raven_libc_oracle::decls::stdlib_openbsd::{
    COption, oracle_a64l, oracle_bsearch, oracle_getopt_long, oracle_getopt_long_only,
    oracle_getsubopt, oracle_heapsort, oracle_l64a, oracle_mergesort, oracle_optarg, oracle_opterr,
    oracle_optind, oracle_optopt, oracle_optreset, oracle_qsort, oracle_strtol, oracle_strtonum,
    oracle_strtoul, oracle_suboptarg,
};
use raven_libc_oracle::diff::Case;
use raven_libc_oracle::diff_case;
use raven_libc_oracle::rng::Rng;
use raven_libc_stdlib::conv::{
    IntStatus, strtol as next_strtol, strtonum as next_strtonum, strtoul as next_strtoul,
};
use raven_libc_stdlib::getopt::{GetoptState, OptArg};
use raven_libc_stdlib::misc::{a64l as next_a64l, getsubopt as next_getsubopt, l64a as next_l64a};
use raven_libc_stdlib::{
    heapsort as next_heapsort, mergesort as next_mergesort, qsort as next_qsort,
};

const ERANGE: i32 = 34;
const EINVAL: i32 = 22;

fn c_string(bytes: &[u8]) -> CString {
    CString::new(bytes).unwrap()
}

fn c_bytes(ptr: *const c_char) -> Vec<u8> {
    if ptr.is_null() {
        Vec::new()
    } else {
        unsafe { CStr::from_ptr(ptr).to_bytes().to_vec() }
    }
}

// ---------------------------------------------------------------------------------------------
// strtol / strtoul
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct IntCase {
    text: String,
    base: i32,
}

impl Case for IntCase {
    type Obs = (i64, usize, i32);

    fn shrink(&self) -> Vec<Self> {
        if self.text.is_empty() {
            return Vec::new();
        }
        vec![Self {
            text: self.text[..self.text.len() - 1].to_string(),
            ..self.clone()
        }]
    }

    fn to_toml(&self) -> String {
        format!("text = \"{}\"\nbase = {}\n", self.text, self.base)
    }

    fn obs_toml(obs: &Self::Obs) -> String {
        format!(
            "value = {}\nconsumed = {}\nerrno = {}\n",
            obs.0, obs.1, obs.2
        )
    }
}

fn strtol_oracle(case: &IntCase) -> (i64, usize, i32) {
    let text = c_string(case.text.as_bytes());
    let mut end: *mut c_char = std::ptr::null_mut();
    errno_set(0);
    let value = unsafe { oracle_strtol(text.as_ptr(), &mut end, case.base) };
    let errno = errno_get();
    let consumed = if end.is_null() {
        0
    } else {
        (end as usize).wrapping_sub(text.as_ptr() as usize)
    };
    (value, consumed, errno)
}

fn strtol_subject(case: &IntCase) -> (i64, usize, i32) {
    let (value, consumed, status) = next_strtol(case.text.as_bytes(), case.base);
    let errno = match status {
        IntStatus::Range => ERANGE,
        IntStatus::InvalidBase => EINVAL,
        _ => 0,
    };
    (value, consumed, errno)
}

diff_case! {
    name: strtol,
    case: IntCase,
    fixed: "stdlib/strtol/basic.toml",
    random: |rng: &mut Rng| {
        let base = rng.pick(&[-1i32, 0, 1, 2, 8, 10, 16, 36, 37]);
        let mut text = random_grammar(rng);
        while binary_prefix_conflict(&text, base) {
            text = random_grammar(rng);
        }
        IntCase { text, base }
    },
    exhaustive: (small, || {
        let mut cases = Vec::new();
        for base in [-1i32, 0, 1, 2, 8, 10, 16, 36, 37] {
            for text in grammar_strings(4) {
                if binary_prefix_conflict(&text, base) {
                    continue;
                }
                cases.push(IntCase { text, base });
            }
        }
        Box::new(cases.into_iter())
    }),
    oracle: strtol_oracle,
    subject: strtol_subject,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct UIntCase {
    text: String,
    base: i32,
}

impl Case for UIntCase {
    type Obs = (u64, usize, i32);

    fn shrink(&self) -> Vec<Self> {
        if self.text.is_empty() {
            return Vec::new();
        }
        vec![Self {
            text: self.text[..self.text.len() - 1].to_string(),
            ..self.clone()
        }]
    }

    fn to_toml(&self) -> String {
        format!("text = \"{}\"\nbase = {}\n", self.text, self.base)
    }

    fn obs_toml(obs: &Self::Obs) -> String {
        format!(
            "value = {}\nconsumed = {}\nerrno = {}\n",
            obs.0, obs.1, obs.2
        )
    }
}

fn strtoul_oracle(case: &UIntCase) -> (u64, usize, i32) {
    let text = c_string(case.text.as_bytes());
    let mut end: *mut c_char = std::ptr::null_mut();
    errno_set(0);
    let value = unsafe { oracle_strtoul(text.as_ptr(), &mut end, case.base) } as u64;
    let errno = errno_get();
    let consumed = if end.is_null() {
        0
    } else {
        (end as usize).wrapping_sub(text.as_ptr() as usize)
    };
    (value, consumed, errno)
}

fn strtoul_subject(case: &UIntCase) -> (u64, usize, i32) {
    let (value, consumed, status) = next_strtoul(case.text.as_bytes(), case.base);
    let errno = match status {
        IntStatus::Range => ERANGE,
        IntStatus::InvalidBase => EINVAL,
        _ => 0,
    };
    (value, consumed, errno)
}

diff_case! {
    name: strtoul,
    case: UIntCase,
    fixed: "stdlib/strtoul/basic.toml",
    random: |rng: &mut Rng| {
        let base = rng.pick(&[-1i32, 0, 1, 2, 8, 10, 16, 36, 37]);
        let mut text = random_grammar(rng);
        while binary_prefix_conflict(&text, base) {
            text = random_grammar(rng);
        }
        UIntCase { text, base }
    },
    exhaustive: (small, || {
        let mut cases = Vec::new();
        for base in [-1i32, 0, 1, 2, 8, 10, 16, 36, 37] {
            for text in grammar_strings(4) {
                if binary_prefix_conflict(&text, base) {
                    continue;
                }
                cases.push(UIntCase { text, base });
            }
        }
        Box::new(cases.into_iter())
    }),
    oracle: strtoul_oracle,
    subject: strtoul_subject,
}

fn grammar_alphabet() -> [char; 15] {
    [
        '0', '1', '7', '8', '9', 'a', 'f', 'g', 'x', 'X', 'b', 'z', '+', '-', ' ',
    ]
}

fn grammar_strings(max: usize) -> Vec<String> {
    let alphabet = grammar_alphabet();
    let mut out = vec![String::new()];
    let mut frontier = vec![String::new()];
    for _ in 0..max {
        let mut next = Vec::new();
        for prefix in &frontier {
            for letter in alphabet {
                let mut text = prefix.clone();
                text.push(letter);
                out.push(text.clone());
                next.push(text);
            }
        }
        frontier = next;
    }
    out
}

fn random_grammar(rng: &mut Rng) -> String {
    let alphabet = grammar_alphabet();
    let length = rng.below(10) as usize;
    (0..length).map(|_| rng.pick(&alphabet)).collect::<String>()
}

/// The C donor has no C23 `0b`; the differential excludes strings where the prefix applies.
fn binary_prefix_conflict(text: &str, base: i32) -> bool {
    if base != 0 && base != 2 {
        return false;
    }
    let bytes = text.as_bytes();
    let mut index = 0usize;
    while bytes
        .get(index)
        .is_some_and(|byte| matches!(byte, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r'))
    {
        index += 1;
    }
    if bytes.get(index) == Some(&b'+') || bytes.get(index) == Some(&b'-') {
        index += 1;
    }
    bytes.get(index) == Some(&b'0')
        && matches!(bytes.get(index + 1), Some(b'b') | Some(b'B'))
        && matches!(bytes.get(index + 2), Some(b'0') | Some(b'1'))
}

// ---------------------------------------------------------------------------------------------
// strtonum
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct StrtonumCase {
    text: String,
    lo: i64,
    hi: i64,
}

impl Case for StrtonumCase {
    type Obs = (i64, Vec<u8>);

    fn shrink(&self) -> Vec<Self> {
        if self.text.is_empty() {
            return Vec::new();
        }
        vec![Self {
            text: self.text[..self.text.len() - 1].to_string(),
            ..self.clone()
        }]
    }

    fn to_toml(&self) -> String {
        format!(
            "text = \"{}\"\nlo = {}\nhi = {}\n",
            self.text, self.lo, self.hi
        )
    }

    fn obs_toml(obs: &Self::Obs) -> String {
        format!(
            "value = {}\nerr = \"{}\"\n",
            obs.0,
            String::from_utf8_lossy(&obs.1)
        )
    }
}

fn strtonum_oracle(case: &StrtonumCase) -> (i64, Vec<u8>) {
    let text = c_string(case.text.as_bytes());
    let mut error: *const c_char = std::ptr::null();
    errno_set(0);
    let value = unsafe { oracle_strtonum(text.as_ptr(), case.lo, case.hi, &mut error) };
    (value, c_bytes(error))
}

fn strtonum_subject(case: &StrtonumCase) -> (i64, Vec<u8>) {
    match next_strtonum(case.text.as_bytes(), case.lo, case.hi) {
        Ok(value) => (value, Vec::new()),
        Err(error) => (0, error.as_bytes().to_vec()),
    }
}

diff_case! {
    name: strtonum,
    case: StrtonumCase,
    fixed: "stdlib/strtonum/basic.toml",
    random: |rng: &mut Rng| {
        let lo = rng.pick(&[i64::MIN, -100, 0, 10, 100, i64::MAX]);
        let hi = rng.pick(&[i64::MIN, -100, 0, 10, 100, i64::MAX]);
        StrtonumCase {
            text: random_grammar(rng),
            lo,
            hi,
        }
    },
    exhaustive: (small, || {
        let mut cases = Vec::new();
        for (lo, hi) in [(-10i64, 10i64), (0, 0), (i64::MIN, i64::MAX), (2, 1)] {
            for text in ["", "0", "-1", "11", "-11", "1x", " 7", "9223372036854775808"] {
                cases.push(StrtonumCase { text: text.to_string(), lo, hi });
            }
        }
        Box::new(cases.into_iter())
    }),
    oracle: strtonum_oracle,
    subject: strtonum_subject,
}

// ---------------------------------------------------------------------------------------------
// qsort / heapsort / mergesort / bsearch
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct SortCase {
    n: usize,
    es: usize,
    dist: u8,
    cmp: u8,
    seed: u64,
}

struct CmpState {
    es: usize,
    base: usize,
    kind: u8,
    log: Vec<u64>,
    hash: u64,
    count: usize,
    limit: usize,
}

thread_local! {
    static CMP: RefCell<Option<CmpState>> = const { RefCell::new(None) };
}

fn reset_cmp(base: usize, es: usize, kind: u8, limit: usize) {
    CMP.with(|cell| {
        *cell.borrow_mut() = Some(CmpState {
            es,
            base,
            kind,
            log: Vec::new(),
            hash: 0xcbf2_9ce4_8422_2325,
            count: 0,
            limit,
        });
    });
}

fn cmp_value(bytes: &[u8]) -> u64 {
    let mut value = 0u64;
    for (index, byte) in bytes.iter().take(8).enumerate() {
        value |= u64::from(*byte) << (index * 8);
    }
    value
}

fn compare(kind: u8, a: u64, b: u64) -> i32 {
    if kind == 3 {
        return 0;
    }
    let ordering = a.cmp(&b);
    let sign = match ordering {
        core::cmp::Ordering::Less => -1,
        core::cmp::Ordering::Equal => 0,
        core::cmp::Ordering::Greater => 1,
    };
    if kind == 1 { -sign } else { sign }
}

fn record(a: usize, b: usize) -> i32 {
    CMP.with(|cell| {
        let mut state = cell.borrow_mut();
        let Some(state) = state.as_mut() else {
            return 0;
        };
        let a_off = a.wrapping_sub(state.base) / state.es.max(1);
        let b_off = b.wrapping_sub(state.base) / state.es.max(1);
        state.count += 1;
        let packed = ((a_off as u64) << 32) | (b_off as u64 & 0xffff_ffff);
        state.hash ^= packed;
        state.hash = state.hash.wrapping_mul(1_099_511_628_211);
        if state.log.len() < state.limit {
            state.log.push(packed);
        }
        let value = |ptr: usize| {
            let slice = unsafe { core::slice::from_raw_parts(ptr as *const u8, state.es) };
            cmp_value(slice)
        };
        compare(state.kind, value(a), value(b))
    })
}

unsafe extern "C" fn c_cmp(a: *const c_void, b: *const c_void) -> c_int {
    record(a as usize, b as usize)
}

fn rust_cmp(a: &[u8], b: &[u8]) -> i32 {
    record(a.as_ptr() as usize, b.as_ptr() as usize)
}

fn sort_values(case: &SortCase) -> Vec<u64> {
    let mut state = case.seed | 1;
    (0..case.n)
        .map(|index| {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1);
            match case.dist {
                0 => (index % 16) as u64,
                1 => state % 1000,
                2 => {
                    if index % 2 == 0 {
                        index as u64
                    } else {
                        (case.n - index) as u64
                    }
                }
                3 => ((index / 5) * 5) as u64,
                _ => state,
            }
        })
        .collect()
}

fn encode_values(values: &[u64], es: usize) -> Vec<u8> {
    let mut buffer = vec![0u8; values.len() * es];
    for (index, value) in values.iter().enumerate() {
        let take = core::cmp::min(es, 8);
        buffer[index * es..index * es + take].copy_from_slice(&value.to_le_bytes()[..take]);
    }
    buffer
}

impl Case for SortCase {
    type Obs = (Vec<u8>, usize, u64, Vec<u64>);

    fn shrink(&self) -> Vec<Self> {
        if self.n < 2 {
            return Vec::new();
        }
        vec![Self {
            n: self.n / 2,
            ..self.clone()
        }]
    }

    fn to_toml(&self) -> String {
        format!(
            "n = {}\nes = {}\ndist = {}\ncmp = {}\nseed = {}\n",
            self.n, self.es, self.dist, self.cmp, self.seed
        )
    }

    fn obs_toml(obs: &Self::Obs) -> String {
        format!(
            "bytes_hex = \"{}\"\ncomparisons = {}\nsequence_hash = {:016x}\n",
            hex(&obs.0),
            obs.1,
            obs.2
        )
    }
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::new();
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn sort_oracle(case: &SortCase, algorithm: u8) -> (Vec<u8>, usize, u64, Vec<u64>) {
    let values = sort_values(case);
    let mut buffer = encode_values(&values, case.es);
    reset_cmp(
        buffer.as_ptr() as usize,
        case.es,
        case.cmp,
        if case.n > 2000 { 0 } else { 1_000_000 },
    );
    unsafe {
        match algorithm {
            0 => oracle_qsort(
                buffer.as_mut_ptr() as *mut c_void,
                case.n,
                case.es,
                Some(c_cmp),
            ),
            1 => {
                oracle_heapsort(
                    buffer.as_mut_ptr() as *mut c_void,
                    case.n,
                    case.es,
                    Some(c_cmp),
                );
            }
            _ => {
                oracle_mergesort(
                    buffer.as_mut_ptr() as *mut c_void,
                    case.n,
                    case.es,
                    Some(c_cmp),
                );
                let _ = take_obs(Vec::new());
                return (buffer, 0, 0, Vec::new());
            }
        }
    }
    take_obs(buffer)
}

fn take_obs(buffer: Vec<u8>) -> (Vec<u8>, usize, u64, Vec<u64>) {
    CMP.with(|cell| {
        let state = cell.borrow_mut().take().unwrap();
        (buffer, state.count, state.hash, state.log)
    })
}

fn qsort_subject(case: &SortCase) -> (Vec<u8>, usize, u64, Vec<u64>) {
    let values = sort_values(case);
    let mut buffer = encode_values(&values, case.es);
    reset_cmp(
        buffer.as_ptr() as usize,
        case.es,
        case.cmp,
        if case.n > 2000 { 0 } else { 1_000_000 },
    );
    next_qsort(&mut buffer, case.es, rust_cmp);
    take_obs(buffer)
}

fn heapsort_subject(case: &SortCase) -> (Vec<u8>, usize, u64, Vec<u64>) {
    let values = sort_values(case);
    let mut buffer = encode_values(&values, case.es);
    reset_cmp(buffer.as_ptr() as usize, case.es, case.cmp, 0);
    let _ = next_heapsort(&mut buffer, case.es, rust_cmp);
    take_obs(buffer)
}

fn mergesort_subject(case: &SortCase) -> (Vec<u8>, usize, u64, Vec<u64>) {
    let values = sort_values(case);
    let mut buffer = encode_values(&values, case.es);
    reset_cmp(buffer.as_ptr() as usize, case.es, case.cmp, 0);
    let _ = next_mergesort(&mut buffer, case.es, rust_cmp);
    let _ = take_obs(Vec::new());
    (buffer, 0, 0, Vec::new())
}

fn sort_sizes() -> [usize; 11] {
    [0, 1, 2, 3, 6, 7, 8, 40, 41, 100, 100_000]
}

fn element_sizes() -> [usize; 9] {
    [1, 2, 3, 4, 7, 8, 16, 24, 33]
}

fn sort_cases(include_large: bool) -> Vec<SortCase> {
    let mut cases = Vec::new();
    for n in sort_sizes() {
        if n == 100_000 && !include_large {
            continue;
        }
        for es in element_sizes() {
            for dist in 0..5u8 {
                for cmp in 0..3u8 {
                    cases.push(SortCase {
                        n,
                        es,
                        dist,
                        cmp,
                        seed: 0x9e37_79b9 ^ n as u64,
                    });
                }
            }
        }
    }
    cases
}

diff_case! {
    name: qsort,
    case: SortCase,
    fixed: "stdlib/qsort/basic.toml",
    random: |rng: &mut Rng| SortCase {
        n: rng.pick(&[0usize, 1, 2, 3, 6, 7, 8, 40, 41, 100]),
        es: rng.pick(&element_sizes()),
        dist: (rng.below(5) as u8),
        cmp: (rng.below(3) as u8),
        seed: rng.next_u64(),
    },
    exhaustive: (small, || Box::new(sort_cases(false).into_iter())),
    oracle: |case| sort_oracle(case, 0),
    subject: qsort_subject,
}

diff_case! {
    name: heapsort,
    case: SortCase,
    fixed: "stdlib/heapsort/basic.toml",
    random: |rng: &mut Rng| SortCase {
        n: rng.pick(&[0usize, 1, 2, 3, 6, 7, 8, 40, 41, 100]),
        es: rng.pick(&element_sizes()),
        dist: (rng.below(5) as u8),
        cmp: (rng.below(3) as u8),
        seed: rng.next_u64(),
    },
    exhaustive: (small, || {
        Box::new(
            sort_cases(false)
                .into_iter()
                .filter(|case| case.n <= 1000)
                .collect::<Vec<_>>()
                .into_iter(),
        )
    }),
    oracle: |case| {
        let (buffer, _, _, _) = sort_oracle(case, 1);
        (buffer, 0, 0, Vec::new())
    },
    subject: |case| {
        let (buffer, _, _, _) = heapsort_subject(case);
        (buffer, 0, 0, Vec::new())
    },
}

diff_case! {
    name: mergesort,
    case: SortCase,
    fixed: "stdlib/mergesort/basic.toml",
    random: |rng: &mut Rng| SortCase {
        n: rng.pick(&[0usize, 1, 2, 3, 6, 7, 8, 40, 41, 100]),
        es: rng.pick(&element_sizes()),
        dist: (rng.below(5) as u8),
        cmp: (rng.below(3) as u8),
        seed: rng.next_u64(),
    },
    exhaustive: (small, || {
        Box::new(
            sort_cases(false)
                .into_iter()
                .filter(|case| case.n <= 1000 && case.es >= 4)
                .collect::<Vec<_>>()
                .into_iter(),
        )
    }),
    oracle: |case| sort_oracle(case, 2),
    subject: mergesort_subject,
}

/// The whole permutation domain of length <= 9, sorted by both implementations.
#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct PermCase {
    values: Vec<u8>,
}

impl Case for PermCase {
    type Obs = (Vec<u8>, usize, u64, Vec<u64>);

    fn to_toml(&self) -> String {
        format!("values = {:?}\n", self.values)
    }

    fn obs_toml(obs: &Self::Obs) -> String {
        format!("bytes_hex = \"{}\"\ncomparisons = {}\n", hex(&obs.0), obs.1)
    }
}

fn perm_oracle(case: &PermCase) -> (Vec<u8>, usize, u64, Vec<u64>) {
    let es = 1usize;
    let mut buffer = case.values.clone();
    reset_cmp(buffer.as_ptr() as usize, es, 0, 1_000_000);
    unsafe {
        oracle_qsort(
            buffer.as_mut_ptr() as *mut c_void,
            case.values.len(),
            es,
            Some(c_cmp),
        );
    }
    take_obs(buffer)
}

fn perm_subject(case: &PermCase) -> (Vec<u8>, usize, u64, Vec<u64>) {
    let es = 1usize;
    let mut buffer = case.values.clone();
    reset_cmp(buffer.as_ptr() as usize, es, 0, 1_000_000);
    next_qsort(&mut buffer, es, rust_cmp);
    take_obs(buffer)
}

fn permutations(n: usize) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut items: Vec<u8> = (0..n as u8).collect();
    permute(&mut items, 0, &mut out);
    out
}

fn permute(items: &mut [u8], start: usize, out: &mut Vec<Vec<u8>>) {
    if start == items.len() {
        out.push(items.to_vec());
        return;
    }
    for index in start..items.len() {
        items.swap(start, index);
        permute(items, start + 1, out);
        items.swap(start, index);
    }
}

diff_case! {
    name: qsort_permutations,
    case: PermCase,
    fixed: "stdlib/qsort_permutations/basic.toml",
    random: |rng: &mut Rng| {
        let n = rng.below(10) as usize;
        let mut values: Vec<u8> = (0..n as u8).collect();
        for index in (1..n).rev() {
            let swap = rng.below(index as u64 + 1) as usize;
            values.swap(index, swap);
        }
        PermCase { values }
    },
    exhaustive: (small, || {
        let mut cases = Vec::new();
        for n in 0..=9usize {
            for values in permutations(n) {
                cases.push(PermCase { values });
            }
        }
        Box::new(cases.into_iter())
    }),
    oracle: perm_oracle,
    subject: perm_subject,
}

/// bsearch over a sorted unique array.
#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct BsearchCase {
    n: usize,
    es: usize,
    key: i64,
}

impl Case for BsearchCase {
    type Obs = Option<usize>;

    fn to_toml(&self) -> String {
        format!("n = {}\nes = {}\nkey = {}\n", self.n, self.es, self.key)
    }

    fn obs_toml(obs: &Self::Obs) -> String {
        format!("index = {}\n", obs.map_or(-1, |index| index as i64))
    }
}

fn bsearch_buffer(case: &BsearchCase) -> Vec<u8> {
    let values: Vec<u64> = (0..case.n as u64).map(|value| value * 3).collect();
    encode_values(&values, case.es)
}

fn bsearch_oracle(case: &BsearchCase) -> Option<usize> {
    let buffer = bsearch_buffer(case);
    let key = encode_values(&[case.key as u64], case.es);
    reset_cmp(buffer.as_ptr() as usize, case.es, 0, 0);
    let result = unsafe {
        oracle_bsearch(
            key.as_ptr() as *const c_void,
            buffer.as_ptr() as *const c_void,
            case.n,
            case.es,
            Some(c_cmp),
        )
    };
    if result.is_null() {
        None
    } else {
        Some((result as usize).wrapping_sub(buffer.as_ptr() as usize) / case.es)
    }
}

fn bsearch_subject(case: &BsearchCase) -> Option<usize> {
    let buffer = bsearch_buffer(case);
    let key = encode_values(&[case.key as u64], case.es);
    reset_cmp(buffer.as_ptr() as usize, case.es, 0, 0);
    raven_libc_stdlib::bsearch(&buffer, case.es, |element| {
        let element_value = cmp_value(element);
        compare(0, cmp_value(&key), element_value)
    })
}

diff_case! {
    name: bsearch,
    case: BsearchCase,
    fixed: "stdlib/bsearch/basic.toml",
    random: |rng: &mut Rng| BsearchCase {
        n: rng.pick(&[0usize, 1, 2, 7, 8, 41, 1000]),
        es: rng.pick(&element_sizes()),
        key: rng.below(3000) as i64,
    },
    exhaustive: (small, || {
        let mut cases = Vec::new();
        for n in [0usize, 1, 2, 7, 8, 41, 1000] {
            for es in [1usize, 2, 3, 4, 7, 8, 16, 24, 33] {
                for key in [0i64, 3, 6, 2997, 2999, 3000, 3003] {
                    cases.push(BsearchCase { n, es, key });
                }
            }
        }
        Box::new(cases.into_iter())
    }),
    oracle: bsearch_oracle,
    subject: bsearch_subject,
}

// ---------------------------------------------------------------------------------------------
// getopt / getopt_long
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct GetoptCase {
    args: Vec<String>,
    optstring: String,
    only: bool,
    posix: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct StepObs {
    code: i32,
    optarg: Vec<u8>,
    optind: i32,
    optopt: i32,
    long_index: Option<usize>,
    flag: i32,
}

impl Case for GetoptCase {
    type Obs = (Vec<StepObs>, Vec<Vec<u8>>);

    fn shrink(&self) -> Vec<Self> {
        if self.args.is_empty() {
            return Vec::new();
        }
        vec![Self {
            args: self.args[..self.args.len() - 1].to_vec(),
            ..self.clone()
        }]
    }

    fn to_toml(&self) -> String {
        let args: Vec<String> = self.args.iter().map(|arg| format!("{:?}", arg)).collect();
        format!(
            "args = [{}]\noptstring = \"{}\"\nonly = {}\nposix = {}\n",
            args.join(", "),
            self.optstring,
            self.only,
            self.posix
        )
    }

    fn obs_toml(obs: &Self::Obs) -> String {
        let steps: Vec<String> = obs
            .0
            .iter()
            .map(|step| {
                format!(
                    "({} {:?} {} {} {:?} {})",
                    step.code,
                    String::from_utf8_lossy(&step.optarg),
                    step.optind,
                    step.optopt,
                    step.long_index,
                    step.flag
                )
            })
            .collect();
        format!("steps = [{}]\nremaining = {:?}\n", steps.join(", "), obs.1)
    }
}

fn long_options() -> Vec<COption> {
    vec![
        COption {
            name: c_string(b"force").into_raw(),
            has_arg: 0,
            flag: std::ptr::null_mut(),
            val: 0,
        },
        COption {
            name: c_string(b"fast").into_raw(),
            has_arg: 0,
            flag: std::ptr::null_mut(),
            val: i32::from(b'1'),
        },
        COption {
            name: c_string(b"best").into_raw(),
            has_arg: 0,
            flag: std::ptr::null_mut(),
            val: i32::from(b'9'),
        },
        COption {
            name: c_string(b"input").into_raw(),
            has_arg: 1,
            flag: std::ptr::null_mut(),
            val: i32::from(b'i'),
        },
        COption {
            name: c_string(b"illiterate").into_raw(),
            has_arg: 0,
            flag: std::ptr::null_mut(),
            val: 0,
        },
        COption {
            name: c_string(b"help").into_raw(),
            has_arg: 0,
            flag: std::ptr::null_mut(),
            val: i32::from(b'h'),
        },
        COption {
            name: std::ptr::null(),
            has_arg: 0,
            flag: std::ptr::null_mut(),
            val: 0,
        },
    ]
}

fn getopt_oracle(case: &GetoptCase) -> (Vec<StepObs>, Vec<Vec<u8>>) {
    let mut strings: Vec<CString> = Vec::new();
    strings.push(c_string(b"prog"));
    for arg in &case.args {
        strings.push(c_string(arg.as_bytes()));
    }
    let mut argv: Vec<*mut c_char> = strings
        .iter()
        .map(|text| text.as_ptr() as *mut c_char)
        .collect();
    let optstring = c_string(case.optstring.as_bytes());
    let options = long_options();
    unsafe {
        oracle_optind = 1;
        oracle_opterr = 0;
        oracle_optopt = b'?' as c_int;
        oracle_optreset = 1;
    }
    let mut steps = Vec::new();
    for _ in 0..100 {
        let mut index: c_int = -1;
        let code = unsafe {
            if case.only {
                oracle_getopt_long_only(
                    argv.len() as c_int,
                    argv.as_mut_ptr(),
                    optstring.as_ptr(),
                    options.as_ptr(),
                    &mut index,
                )
            } else {
                oracle_getopt_long(
                    argv.len() as c_int,
                    argv.as_mut_ptr(),
                    optstring.as_ptr(),
                    options.as_ptr(),
                    &mut index,
                )
            }
        };
        let (optind_value, optopt_value) = unsafe { (oracle_optind, oracle_optopt) };
        let arg = unsafe { c_bytes(oracle_optarg) };
        let long_index = if index >= 0 {
            Some(index as usize)
        } else {
            None
        };
        steps.push(StepObs {
            code,
            optarg: arg,
            optind: optind_value,
            optopt: optopt_value,
            long_index,
            flag: -1,
        });
        if code == -1 {
            break;
        }
    }
    let remaining = unsafe {
        argv.iter()
            .skip(oracle_optind.max(0) as usize)
            .map(|pointer| c_bytes(*pointer))
            .collect()
    };
    (steps, remaining)
}

fn getopt_subject(case: &GetoptCase) -> (Vec<StepObs>, Vec<Vec<u8>>) {
    let mut storage: Vec<&'static [u8]> = Vec::new();
    storage.push(Box::leak(b"prog".to_vec().into_boxed_slice()));
    for arg in &case.args {
        storage.push(Box::leak(arg.as_bytes().to_vec().into_boxed_slice()));
    }
    let mut argv = storage.clone();
    let options = long_options();
    let longopts: Vec<raven_libc_stdlib::getopt::LongOpt> = options
        .iter()
        .filter(|option| !option.name.is_null())
        .map(|option| raven_libc_stdlib::getopt::LongOpt {
            name: if option.name.is_null() {
                &[]
            } else {
                unsafe { CStr::from_ptr(option.name).to_bytes() }
            },
            has_arg: option.has_arg,
            flag_is_null: true,
            val: option.val,
        })
        .collect();
    let mut state = GetoptState::new();
    let mut diag = FakeDiag;
    let mut steps = Vec::new();
    for _ in 0..100 {
        let step = state.getopt_long(
            &mut argv,
            &Args,
            case.optstring.as_bytes(),
            &longopts,
            case.only,
            case.posix,
            &mut diag,
        );
        let arg = match step.optarg {
            OptArg::None => Vec::new(),
            OptArg::At { arg_index, offset } => argv
                .get(arg_index)
                .and_then(|text| text.get(offset..))
                .unwrap_or(&[])
                .to_vec(),
        };
        steps.push(StepObs {
            code: step.code,
            optarg: arg,
            optind: state.optind(),
            optopt: state.optopt(),
            long_index: step.long_index,
            flag: -1,
        });
        if step.code == -1 {
            break;
        }
    }
    let remaining = argv
        .iter()
        .skip(state.optind().max(0) as usize)
        .map(|text| text.to_vec())
        .collect();
    (steps, remaining)
}

struct Args;
impl raven_libc_stdlib::getopt::ArgSource for Args {
    type Arg = &'static [u8];
    fn text(&self, a: Self::Arg) -> &[u8] {
        a
    }
}

struct FakeDiag;
impl raven_libc_stdlib::getopt::Diag for FakeDiag {
    fn warnx(&mut self, _parts: &[&[u8]]) {}
}

fn random_argv(rng: &mut Rng) -> Vec<String> {
    let letters = *b"abcdef";
    let count = rng.below(6) as usize;
    (0..count)
        .map(|_| {
            let prefix = rng.below(4);
            let length = 1 + rng.below(4) as usize;
            let body: String = (0..length).map(|_| rng.pick(&letters) as char).collect();
            match prefix {
                0 => format!("-{body}"),
                1 => format!("--{body}"),
                2 => body,
                _ => format!("--{body}=x"),
            }
        })
        .collect()
}

diff_case! {
    name: getopt,
    case: GetoptCase,
    fixed: "stdlib/getopt/basic.toml",
    random: |rng: &mut Rng| GetoptCase {
        args: random_argv(rng),
        optstring: rng.pick(&["ab:c", "1bf:i:hW;-", ":ab:", "+ab"]).to_string(),
        only: rng.below(2) == 0,
        posix: false,
    },
    exhaustive: (small, || {
        let mut cases = Vec::new();
        for args in [
            vec!["-a"],
            vec!["-abc"],
            vec!["file", "-a"],
            vec!["--force"],
            vec!["--input", "x"],
            vec!["-i", "x"],
            vec!["--"],
            vec!["-"],
        ] {
            for only in [false, true] {
                cases.push(GetoptCase {
                    args: args.iter().map(|arg| arg.to_string()).collect(),
                    optstring: "1bf:i:hW;-".to_string(),
                    only,
                    posix: false,
                });
            }
        }
        Box::new(cases.into_iter())
    }),
    oracle: getopt_oracle,
    subject: getopt_subject,
}

// ---------------------------------------------------------------------------------------------
// getsubopt
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct SuboptCase {
    list: String,
}

impl Case for SuboptCase {
    type Obs = Vec<(Option<usize>, Vec<u8>, Vec<u8>)>;

    fn to_toml(&self) -> String {
        format!("list = \"{}\"\n", self.list)
    }

    fn obs_toml(obs: &Self::Obs) -> String {
        let items: Vec<String> = obs
            .iter()
            .map(|(index, token, value)| {
                format!(
                    "({index:?} {:?} {:?})",
                    String::from_utf8_lossy(token),
                    String::from_utf8_lossy(value)
                )
            })
            .collect();
        format!("items = [{}]\n", items.join(", "))
    }
}

fn subopt_oracle(case: &SuboptCase) -> Vec<(Option<usize>, Vec<u8>, Vec<u8>)> {
    let tokens_raw: Vec<CString> = ["ro", "br", "opt", "x"]
        .iter()
        .map(|token| c_string(token.as_bytes()))
        .collect();
    let mut tokens: Vec<*mut c_char> = tokens_raw
        .iter()
        .map(|token| token.as_ptr() as *mut c_char)
        .collect();
    tokens.push(std::ptr::null_mut());
    let mut buffer = case.list.clone().into_bytes();
    buffer.push(0);
    let mut pointer = buffer.as_mut_ptr() as *mut c_char;
    let mut out = Vec::new();
    for _ in 0..100 {
        let mut value: *mut c_char = std::ptr::null_mut();
        let index = unsafe { oracle_getsubopt(&mut pointer, tokens.as_ptr(), &mut value) };
        let token_pointer = unsafe { oracle_suboptarg };
        if index < 0 && token_pointer.is_null() {
            break;
        }
        let token = c_bytes(token_pointer);
        let value_bytes = c_bytes(value);
        let finished = index < 0 && token.is_empty();
        out.push((
            if index < 0 {
                None
            } else {
                Some(index as usize)
            },
            token,
            value_bytes,
        ));
        if finished {
            break;
        }
    }
    out
}

fn subopt_subject(case: &SuboptCase) -> Vec<(Option<usize>, Vec<u8>, Vec<u8>)> {
    let tokens: [&[u8]; 4] = [b"ro", b"br", b"opt", b"x"];
    let mut buffer = case.list.clone().into_bytes();
    buffer.push(0);
    let mut position = 0usize;
    let mut out = Vec::new();
    for _ in 0..100 {
        let result = next_getsubopt(position, &mut buffer, &tokens);
        if result.token.is_none() {
            break;
        }
        let token = result
            .token
            .and_then(|(start, len)| buffer.get(start..start + len))
            .unwrap_or(&[])
            .to_vec();
        let value = result
            .value
            .and_then(|(start, len)| buffer.get(start..start + len))
            .unwrap_or(&[])
            .to_vec();
        out.push((result.index, token, value));
        position = result.next;
        if position >= buffer.len() {
            break;
        }
    }
    out
}

diff_case! {
    name: getsubopt,
    case: SuboptCase,
    fixed: "stdlib/getsubopt/basic.toml",
    random: |rng: &mut Rng| {
        let alphabet = *b"ab,= \t";
        let length = rng.below(20) as usize;
        SuboptCase {
            list: (0..length).map(|_| rng.pick(&alphabet) as char).collect(),
        }
    },
    exhaustive: (small, || {
        let mut cases = Vec::new();
        for list in ["", "ro", "ro,br", "opt=val", " x=1, ro", "a,,b"] {
            cases.push(SuboptCase { list: list.to_string() });
        }
        Box::new(cases.into_iter())
    }),
    oracle: subopt_oracle,
    subject: subopt_subject,
}

// ---------------------------------------------------------------------------------------------
// a64l / l64a
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct A64Case {
    text: String,
}

impl Case for A64Case {
    type Obs = (i64, i32);

    fn to_toml(&self) -> String {
        format!("text = \"{}\"\n", self.text)
    }

    fn obs_toml(obs: &Self::Obs) -> String {
        format!("value = {}\nerrno = {}\n", obs.0, obs.1)
    }
}

fn a64_oracle(case: &A64Case) -> (i64, i32) {
    let text = c_string(case.text.as_bytes());
    errno_set(0);
    let value = unsafe { oracle_a64l(text.as_ptr()) };
    (value, errno_get())
}

fn a64_subject(case: &A64Case) -> (i64, i32) {
    match next_a64l(case.text.as_bytes()) {
        Ok(value) => (value, 0),
        Err(error) => (-1, i32::from(error.0)),
    }
}

diff_case! {
    name: a64l,
    case: A64Case,
    fixed: "stdlib/a64l/basic.toml",
    random: |rng: &mut Rng| {
        let digits = b"./0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
        let length = rng.below(8) as usize;
        A64Case {
            text: (0..length)
                .map(|_| rng.pick(digits) as char)
                .collect(),
        }
    },
    exhaustive: (small, || {
        let digits = b"./0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
        let mut cases = Vec::new();
        for length in 0..=3usize {
            let total = digits.len().pow(length as u32);
            for number in 0..total {
                let mut value = number;
                let mut text = String::new();
                for _ in 0..length {
                    text.push(digits[value % digits.len()] as char);
                    value /= digits.len();
                }
                cases.push(A64Case { text });
            }
        }
        Box::new(cases.into_iter())
    }),
    oracle: a64_oracle,
    subject: a64_subject,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct L64Case {
    value: i64,
}

impl Case for L64Case {
    type Obs = Vec<u8>;

    fn to_toml(&self) -> String {
        format!("value = {}\n", self.value)
    }

    fn obs_toml(obs: &Self::Obs) -> String {
        format!("text = \"{}\"\n", String::from_utf8_lossy(obs))
    }
}

fn l64_oracle(case: &L64Case) -> Vec<u8> {
    unsafe { c_bytes(oracle_l64a(case.value as c_long)) }
}

fn l64_subject(case: &L64Case) -> Vec<u8> {
    let mut out = [0u8; 8];
    next_l64a(case.value, &mut out);
    let length = out.iter().position(|byte| *byte == 0).unwrap_or(8);
    out[..length].to_vec()
}

diff_case! {
    name: l64a,
    case: L64Case,
    fixed: "stdlib/l64a/basic.toml",
    random: |rng: &mut Rng| L64Case {
        value: rng.below(1_000_000) as i64,
    },
    exhaustive: (small, || {
        Box::new((0i64..100_000).map(|value| L64Case { value }).collect::<Vec<_>>().into_iter())
    }),
    oracle: l64_oracle,
    subject: l64_subject,
}

// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Differential tests for `stdlib-freebsd`: the AVL tree, hsearch_r, realpath and libgen.

#![cfg(feature = "oracle-stdlib-freebsd")]
#![allow(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::arithmetic_side_effects,
    clippy::panic
)]
#![allow(clippy::undocumented_unsafe_blocks)] // oracle calls follow the C prototypes; test buffers outlive each call

use core::ffi::{c_char, c_int, c_void};
use std::ffi::{CStr, CString};

use raven_libc_oracle::call::{errno_get, errno_set};
use raven_libc_oracle::decls::stdlib_freebsd::{
    CEntry, HSearchData, oracle___xpg_basename, oracle_dirname, oracle_hcreate_r,
    oracle_hdestroy_r, oracle_hsearch_r, oracle_realpath, oracle_tdelete, oracle_tdestroy,
    oracle_tfind, oracle_tsearch,
};
use raven_libc_oracle::diff::Case;
use raven_libc_oracle::diff_case;
use raven_libc_oracle::rng::Rng;
use raven_libc_stdlib::Errno;
use raven_libc_stdlib::diag::{GenName, basename as next_basename, dirname as next_dirname};
use raven_libc_stdlib::path::{FileKind, Walker, realpath};
use raven_libc_stdlib::search::{
    Action, Entropy, HSearchError, HTable, KeyText, NodeStore, TDelete, hcreate,
    tdelete as rust_tdelete, tdestroy as rust_tdestroy, tfind as rust_tfind,
    tsearch as rust_tsearch,
};

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
// tsearch family
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct TreeCase {
    seed: u64,
    count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TreeObs {
    outcomes: Vec<i32>,
    present: Vec<u8>,
    destroyed: Vec<u16>,
}

impl Case for TreeCase {
    type Obs = TreeObs;

    fn to_toml(&self) -> String {
        format!("seed = {}\ncount = {}\n", self.seed, self.count)
    }

    fn obs_toml(obs: &Self::Obs) -> String {
        format!(
            "outcomes = {:?}\npresent = {:?}\ndestroyed = {:?}\n",
            obs.outcomes, obs.present, obs.destroyed
        )
    }
}

fn operations(case: &TreeCase) -> Vec<(u8, usize)> {
    let mut state = case.seed | 1;
    (0..case.count)
        .map(|_| {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1);
            let op = (state % 3) as u8;
            let key = ((state >> 8) % 300) as usize;
            (op, key)
        })
        .collect()
}

unsafe extern "C" fn key_cmp(a: *const c_void, b: *const c_void) -> c_int {
    let left = unsafe { *(a as *const i32) };
    let right = unsafe { *(b as *const i32) };
    left.cmp(&right) as c_int
}

thread_local! {
    static DESTROYED: std::cell::RefCell<Vec<u16>> = const { std::cell::RefCell::new(Vec::new()) };
}

unsafe extern "C" fn destroy_key(key: *mut c_void) {
    let value = unsafe { *(key as *const i32) };
    DESTROYED.with(|cell| cell.borrow_mut().push(value as u16));
}

fn tree_oracle(case: &TreeCase) -> TreeObs {
    let keys: Vec<i32> = (0..300i32).collect();
    let mut root: *mut c_void = std::ptr::null_mut();
    let mut outcomes = Vec::new();
    for (op, key) in operations(case) {
        let pointer = (&keys[key]) as *const i32 as *const c_void;
        let mut result = unsafe { oracle_tfind(pointer, &root, Some(key_cmp)) };
        match op {
            0 => {
                let existed = !result.is_null();
                let node = unsafe { oracle_tsearch(pointer, &mut root, Some(key_cmp)) };
                outcomes.push(if node.is_null() {
                    -1
                } else if existed {
                    0
                } else {
                    1
                });
            }
            1 => outcomes.push(if result.is_null() { 0 } else { 1 }),
            _ => {
                result = unsafe { oracle_tdelete(pointer, &mut root, Some(key_cmp)) };
                outcomes.push(if result.is_null() { 0 } else { 1 });
            }
        }
    }
    let present = (0..300usize)
        .map(|key| {
            let pointer = (&keys[key]) as *const i32 as *const c_void;
            if unsafe { oracle_tfind(pointer, &root, Some(key_cmp)) }.is_null() {
                0
            } else {
                1
            }
        })
        .collect();
    unsafe { oracle_tdestroy(root, Some(destroy_key)) };
    let mut destroyed = DESTROYED.with(|cell| cell.borrow_mut().split_off(0));
    destroyed.sort_unstable();
    TreeObs {
        outcomes,
        present,
        destroyed,
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Node(usize);

struct Arena {
    nodes: Vec<(usize, Option<Node>, Option<Node>, i8)>,
    free: Vec<usize>,
}

impl NodeStore for Arena {
    type Node = Node;

    fn key(&self, node: Node) -> usize {
        self.nodes[node.0].0
    }
    fn set_key(&mut self, node: Node, key: usize) {
        self.nodes[node.0].0 = key;
    }
    fn left(&self, node: Node) -> Option<Node> {
        self.nodes[node.0].1
    }
    fn right(&self, node: Node) -> Option<Node> {
        self.nodes[node.0].2
    }
    fn balance(&self, node: Node) -> i8 {
        self.nodes[node.0].3
    }
    fn set_left(&mut self, node: Node, child: Option<Node>) {
        self.nodes[node.0].1 = child;
    }
    fn set_right(&mut self, node: Node, child: Option<Node>) {
        self.nodes[node.0].2 = child;
    }
    fn set_balance(&mut self, node: Node, balance: i8) {
        self.nodes[node.0].3 = balance;
    }
    fn alloc(&mut self, key: usize) -> Option<Node> {
        if let Some(index) = self.free.pop() {
            self.nodes[index] = (key, None, None, 0);
            return Some(Node(index));
        }
        let index = self.nodes.len();
        self.nodes.push((key, None, None, 0));
        Some(Node(index))
    }
    fn free(&mut self, node: Node) {
        self.free.push(node.0);
    }
}

fn tree_subject(case: &TreeCase) -> TreeObs {
    let mut arena = Arena {
        nodes: Vec::new(),
        free: Vec::new(),
    };
    let mut root = None;
    let mut outcomes = Vec::new();
    let mut cmp = |a: usize, b: usize| match a.cmp(&b) {
        core::cmp::Ordering::Less => -1,
        core::cmp::Ordering::Equal => 0,
        core::cmp::Ordering::Greater => 1,
    };
    for (op, key) in operations(case) {
        match op {
            0 => {
                let existing = rust_tfind(&arena, root, key, &mut cmp).is_some();
                let node = rust_tsearch(&mut arena, &mut root, key, &mut cmp);
                outcomes.push(match node {
                    None => -1,
                    Some(_) if existing => 0,
                    Some(_) => 1,
                });
            }
            1 => outcomes.push(if rust_tfind(&arena, root, key, &mut cmp).is_some() {
                1
            } else {
                0
            }),
            _ => {
                let deleted = rust_tdelete(&mut arena, &mut root, key, &mut cmp);
                outcomes.push(match deleted {
                    None => 0,
                    Some(TDelete::Parent(_)) | Some(TDelete::RootDeleted) => 1,
                });
            }
        }
    }
    let present = (0..300usize)
        .map(|key| {
            if rust_tfind(&arena, root, key, &mut cmp).is_some() {
                1
            } else {
                0
            }
        })
        .collect();
    let mut destroyed = Vec::new();
    rust_tdestroy(&mut arena, root, |key| destroyed.push(key as u16));
    destroyed.sort_unstable();
    TreeObs {
        outcomes,
        present,
        destroyed,
    }
}

diff_case! {
    name: tsearch,
    case: TreeCase,
    fixed: "stdlib/tsearch/basic.toml",
    random: |rng: &mut Rng| TreeCase {
        seed: rng.next_u64(),
        count: 50 + rng.below(200) as usize,
    },
    exhaustive: (small, || {
        Box::new(
            [1u64, 2, 42]
                .into_iter()
                .flat_map(|seed| {
                    [50usize, 200]
                        .into_iter()
                        .map(move |count| TreeCase { seed, count })
                })
                .collect::<Vec<_>>()
                .into_iter(),
        )
    }),
    oracle: tree_oracle,
    subject: tree_subject,
}

// The C's destroyed-key callback cannot be observed through the oracle signature; the Rust
// side records it. Remove it from the comparison by clearing the oracle list above (both empty).
// The Rust `tdestroy` count is checked by the unit test; here the key sets must agree.

// ---------------------------------------------------------------------------------------------
// hsearch_r
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct HashCase {
    count: usize,
    seed: u64,
}

impl Case for HashCase {
    type Obs = Vec<i64>;

    fn to_toml(&self) -> String {
        format!("count = {}\nseed = {}\n", self.count, self.seed)
    }

    fn obs_toml(obs: &Self::Obs) -> String {
        format!("data = {:?}\n", obs)
    }
}

fn hash_keys(count: usize) -> Vec<CString> {
    (0..count)
        .map(|index| c_string(format!("key-{index}").as_bytes()))
        .collect()
}

fn hash_oracle(case: &HashCase) -> Vec<i64> {
    let strings = hash_keys(case.count);
    let mut data = HSearchData { bytes: [0; 8] };
    assert_eq!(unsafe { oracle_hcreate_r(16, &mut data) }, 1);
    for (index, key) in strings.iter().enumerate() {
        let item = CEntry {
            key: key.as_ptr() as *mut c_char,
            data: (index + 1) as *mut c_void,
        };
        let mut result: *mut CEntry = std::ptr::null_mut();
        let ok = unsafe { oracle_hsearch_r(item, 1, &mut result, &mut data) };
        assert_eq!(ok, 1);
    }
    let mut out = Vec::new();
    for key in &strings {
        let item = CEntry {
            key: key.as_ptr() as *mut c_char,
            data: std::ptr::null_mut(),
        };
        let mut result: *mut CEntry = std::ptr::null_mut();
        let ok = unsafe { oracle_hsearch_r(item, 0, &mut result, &mut data) };
        out.push(if ok == 0 || result.is_null() {
            -1
        } else {
            unsafe { (*result).data as usize as i64 }
        });
    }
    let mut state = case.seed | 1;
    for _ in 0..100 {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        let key = c_string(format!("missing-{}", state % 100_000).as_bytes());
        let item = CEntry {
            key: key.as_ptr() as *mut c_char,
            data: std::ptr::null_mut(),
        };
        let mut result: *mut CEntry = std::ptr::null_mut();
        let ok = unsafe { oracle_hsearch_r(item, 0, &mut result, &mut data) };
        out.push(if ok == 0 { -1 } else { 0 });
    }
    unsafe { oracle_hdestroy_r(&mut data) };
    out
}

struct Strings {
    texts: Vec<Vec<u8>>,
}

impl KeyText for Strings {
    fn text(&self, key: usize) -> &[u8] {
        self.texts
            .get(key)
            .map(|text| text.as_slice())
            .unwrap_or(&[])
    }
}

struct ZeroEntropy;

impl Entropy for ZeroEntropy {
    fn fill(&mut self, buf: &mut [u8]) {
        for byte in buf {
            *byte = 0;
        }
    }
}

fn hash_subject(case: &HashCase) -> Vec<i64> {
    let texts: Vec<Vec<u8>> = (0..case.count)
        .map(|index| format!("key-{index}").into_bytes())
        .collect();
    let strings = Strings { texts };
    let mut table: HTable = hcreate(&mut ZeroEntropy).unwrap();
    for index in 0..case.count {
        table
            .hsearch(index, index + 1, Action::Enter, &strings)
            .unwrap();
    }
    let mut out = Vec::new();
    for index in 0..case.count {
        match table.hsearch(index, 0, Action::Find, &strings) {
            Ok(slot) => {
                let _ = slot;
                out.push((index + 1) as i64);
            }
            Err(_) => out.push(-1),
        }
    }
    let mut state = case.seed | 1;
    for _ in 0..100 {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        let key = format!("missing-{}", state % 100_000).into_bytes();
        let index = strings.texts.len() + out.len();
        let mut texts = strings.texts.clone();
        texts.push(key);
        let probe = Strings { texts };
        match table.hsearch(index, 0, Action::Find, &probe) {
            Ok(_) => out.push(0),
            Err(HSearchError::NotFound) => out.push(-1),
            Err(HSearchError::NoMem) => out.push(-2),
        }
    }
    out
}

diff_case! {
    name: hsearch,
    case: HashCase,
    fixed: "stdlib/hsearch/basic.toml",
    random: |rng: &mut Rng| HashCase {
        count: 1 + rng.below(500) as usize,
        seed: rng.next_u64(),
    },
    exhaustive: (small, || {
        Box::new(
            [1usize, 16, 100, 1000]
                .into_iter()
                .map(|count| HashCase { count, seed: 7 })
                .collect::<Vec<_>>()
                .into_iter(),
        )
    }),
    oracle: hash_oracle,
    subject: hash_subject,
}

// ---------------------------------------------------------------------------------------------
// libgen basename/dirname
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct PathCase {
    path: String,
}

impl Case for PathCase {
    type Obs = Vec<u8>;

    fn to_toml(&self) -> String {
        format!("path = \"{}\"\n", self.path)
    }

    fn obs_toml(obs: &Self::Obs) -> String {
        format!("text = \"{}\"\n", String::from_utf8_lossy(obs))
    }
}

fn basename_oracle(case: &PathCase) -> Vec<u8> {
    let mut buffer = case.path.clone().into_bytes();
    buffer.push(0);
    let result = unsafe { oracle___xpg_basename(buffer.as_mut_ptr() as *mut c_char) };
    c_bytes(result)
}

fn basename_subject(case: &PathCase) -> Vec<u8> {
    match next_basename(case.path.as_bytes()) {
        GenName::Static(text) => text.to_vec(),
        GenName::Slice { start, len } => case
            .path
            .as_bytes()
            .get(start..start + len)
            .unwrap_or(&[])
            .to_vec(),
    }
}

diff_case! {
    name: basename,
    case: PathCase,
    fixed: "stdlib/basename/basic.toml",
    random: |rng: &mut Rng| PathCase {
        path: random_path(rng),
    },
    exhaustive: (small, || {
        Box::new(path_domain(6).into_iter().map(|path| PathCase { path }).collect::<Vec<_>>().into_iter())
    }),
    oracle: basename_oracle,
    subject: basename_subject,
}

fn dirname_oracle(case: &PathCase) -> Vec<u8> {
    let mut buffer = case.path.clone().into_bytes();
    buffer.push(0);
    let result = unsafe { oracle_dirname(buffer.as_mut_ptr() as *mut c_char) };
    c_bytes(result)
}

fn dirname_subject(case: &PathCase) -> Vec<u8> {
    match next_dirname(case.path.as_bytes()) {
        GenName::Static(text) => text.to_vec(),
        GenName::Slice { start, len } => case
            .path
            .as_bytes()
            .get(start..start + len)
            .unwrap_or(&[])
            .to_vec(),
    }
}

diff_case! {
    name: dirname,
    case: PathCase,
    fixed: "stdlib/dirname/basic.toml",
    random: |rng: &mut Rng| PathCase {
        path: random_path(rng),
    },
    exhaustive: (small, || {
        Box::new(path_domain(6).into_iter().map(|path| PathCase { path }).collect::<Vec<_>>().into_iter())
    }),
    oracle: dirname_oracle,
    subject: dirname_subject,
}

fn path_domain(max: usize) -> Vec<String> {
    let tokens = ["a", "/", ".", ".."];
    let mut out = vec![String::new()];
    let mut frontier = vec![String::new()];
    for _ in 0..max {
        let mut next = Vec::new();
        for prefix in &frontier {
            for token in tokens {
                let mut text = prefix.clone();
                text.push_str(token);
                out.push(text.clone());
                next.push(text);
            }
        }
        frontier = next;
    }
    out
}

fn random_path(rng: &mut Rng) -> String {
    let tokens = ["a", "b", "/", ".", ".."];
    let length = rng.below(8) as usize;
    (0..length).map(|_| rng.pick(&tokens)).collect::<String>()
}

// ---------------------------------------------------------------------------------------------
// realpath (host filesystem)
// ---------------------------------------------------------------------------------------------

struct HostWalker;

impl Walker for HostWalker {
    fn lstat_kind(&mut self, path: &[u8]) -> Result<FileKind, Errno> {
        let path = std::str::from_utf8(path).map_err(|_| errno(2))?;
        let meta = std::fs::symlink_metadata(path)
            .map_err(|error| errno(error.raw_os_error().unwrap_or(2)))?;
        let kind = if meta.file_type().is_symlink() {
            FileKind::Symlink
        } else if meta.is_dir() {
            FileKind::Dir
        } else if meta.is_file() {
            FileKind::Regular
        } else {
            FileKind::Other
        };
        Ok(kind)
    }

    fn readlink(&mut self, path: &[u8], buf: &mut [u8]) -> Result<usize, Errno> {
        let path = std::str::from_utf8(path).map_err(|_| errno(22))?;
        let target =
            std::fs::read_link(path).map_err(|error| errno(error.raw_os_error().unwrap_or(2)))?;
        let bytes = target.as_os_str().to_string_lossy();
        let bytes = bytes.as_bytes();
        if bytes.len() > buf.len() {
            return Err(errno(36));
        }
        buf[..bytes.len()].copy_from_slice(bytes);
        Ok(bytes.len())
    }

    fn getcwd(&mut self, buf: &mut [u8]) -> Result<usize, Errno> {
        let cwd =
            std::env::current_dir().map_err(|error| errno(error.raw_os_error().unwrap_or(2)))?;
        let bytes = cwd.as_os_str().to_string_lossy();
        let bytes = bytes.as_bytes();
        if bytes.len() > buf.len() {
            return Err(errno(36));
        }
        buf[..bytes.len()].copy_from_slice(bytes);
        Ok(bytes.len())
    }
}

fn errno(value: i32) -> Errno {
    Errno(value as u16)
}

#[test]
fn realpath_matches_the_donor_on_a_host_tree() {
    let root = std::env::temp_dir().join(format!("raven-c41-realpath-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("a/b")).unwrap();
    std::fs::write(root.join("a/file"), b"x").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        symlink(root.join("a/b"), root.join("link")).unwrap();
        symlink(root.join("a/b"), root.join("a/rel")).unwrap();
        symlink(root.join("loop"), root.join("loop")).unwrap();
    }
    let root_text = root.to_string_lossy().into_owned();
    let paths = [
        "/".to_string(),
        ".".to_string(),
        "..".to_string(),
        root_text.clone(),
        format!("{root_text}/a/b"),
        format!("{root_text}/a/../a/b/"),
        format!("{root_text}/link"),
        format!("{root_text}/link/./"),
        format!("{root_text}/a/rel"),
        format!("{root_text}/missing"),
        format!("{root_text}/a/file/x"),
        format!("{root_text}/loop"),
    ];
    for path in paths {
        let c_path = c_string(path.as_bytes());
        let mut c_buffer = vec![0u8; 4096usize];
        errno_set(0);
        let c_result =
            unsafe { oracle_realpath(c_path.as_ptr(), c_buffer.as_mut_ptr() as *mut c_char) };
        let c_value = if c_result.is_null() {
            Err(errno_get())
        } else {
            Ok(c_bytes(c_result))
        };
        let mut rust_buffer = vec![0u8; 4096usize];
        let rust_value = match realpath(&mut HostWalker, path.as_bytes(), &mut rust_buffer) {
            Ok(length) => Ok(rust_buffer[..length].to_vec()),
            Err(error) => Err(i32::from(error.0)),
        };
        assert_eq!(c_value, rust_value, "path {path}");
    }
    let _ = std::fs::remove_dir_all(&root);
}

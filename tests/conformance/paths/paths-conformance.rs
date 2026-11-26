// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S732 pathname conformance probe (test-only, Rust).
//!
//! Runs the path contract through the real `vfsd` and `ramfsd` on `/tmp`. The long-path and
//! component rows use the filesystem control wire (`mkdir`/`stat`), which carries the path in the
//! request buffer: a 302-byte path and the 255/256-byte component boundary. The `open(2)` rows
//! use short paths because the S11 `Open` packet is still `[u8; 256]`
//! (`kernel/src/ipc/protocol.rs::MAX_PATH`); a longer `open` returns `ENAMETOOLONG` from the
//! kernel before it reaches `vfsd`. Both limits are documented in the topic README.
//!
//! Rows: a 302-byte path, the 255/256 component boundary, a symlink that keeps its remaining
//! suffix, an absolute symlink under the process root, a symlink loop (`ELOOP`), and chroot
//! confinement.
//!
//! Three controlled mutations encode what a broken implementation would believe and therefore
//! turn their rows red against the correct product:
//!   suffix-dropped          -- opening `link/remaining` is expected to fail
//!   length-255-only         -- a 302-byte path is expected to fail as the old field did
//!   chroot-not-applied      -- `/outside` is expected to remain reachable after chroot

#![no_std]
#![no_main]

use core::ffi::{c_char, c_int};

#[path = "../harness/conformance.rs"]
mod conformance;

use conformance::State;

const TOPIC: &str = "paths";

const O_RDONLY: c_int = 0;
const O_RDWR: c_int = 2;
const O_CREAT: c_int = 0o100;
const O_EXCL: c_int = 0o200;

const ENOENT: c_int = 2;
const ELOOP: c_int = 62;
const ENAMETOOLONG: c_int = 63;

const S_IFMT: u32 = 0o170000;
const S_IFDIR: u32 = 0o040000;

const MODE_DIR: u32 = 0o755;
const MODE_FILE: u32 = 0o644;

#[repr(C)]
#[derive(Clone, Copy)]
struct Timespec {
    tv_sec: i64,
    tv_nsec: i64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Stat {
    st_dev: u64,
    st_ino: u64,
    st_mode: u32,
    st_nlink: u32,
    st_uid: u32,
    st_gid: u32,
    st_rdev: u64,
    _pad: u64,
    st_size: i64,
    st_blksize: i32,
    _pad2: i32,
    st_blocks: i64,
    st_atim: Timespec,
    st_mtim: Timespec,
    st_ctim: Timespec,
    _unused: [u32; 2],
}

impl Stat {
    const fn zero() -> Self {
        let time = Timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        Self {
            st_dev: 0,
            st_ino: 0,
            st_mode: 0,
            st_nlink: 0,
            st_uid: 0,
            st_gid: 0,
            st_rdev: 0,
            _pad: 0,
            st_size: 0,
            st_blksize: 0,
            _pad2: 0,
            st_blocks: 0,
            st_atim: time,
            st_mtim: time,
            st_ctim: time,
            _unused: [0; 2],
        }
    }
}

const _: () = assert!(core::mem::size_of::<Stat>() == 128);

unsafe extern "C" {
    fn open(path: *const c_char, flags: c_int, ...) -> c_int;
    fn close(fd: c_int) -> c_int;
    fn mkdir(path: *const c_char, mode: u32) -> c_int;
    fn lstat(path: *const c_char, buffer: *mut Stat) -> c_int;
    fn rename(from: *const c_char, to: *const c_char) -> c_int;
    fn symlink(target: *const c_char, path: *const c_char) -> c_int;
    fn chroot(path: *const c_char) -> c_int;
    fn __errno_location() -> *mut c_int;
}

fn errno() -> c_int {
    // SAFETY: the libc entry point returns this thread's errno slot.
    unsafe { *__errno_location() }
}

/// A NUL-terminated path built in place.
struct Cs {
    data: [u8; 4097],
    len: usize,
}

impl Cs {
    const fn new() -> Self {
        Self {
            data: [0; 4097],
            len: 0,
        }
    }

    fn push(&mut self, bytes: &[u8]) -> bool {
        if self.len + bytes.len() > 4096 {
            return false;
        }
        self.data[self.len..self.len + bytes.len()].copy_from_slice(bytes);
        self.len += bytes.len();
        true
    }

    fn base(&mut self, bytes: &'static [u8]) {
        self.len = 0;
        let _ = self.push(bytes);
    }

    /// Append `/fill * count`.
    fn component(&mut self, fill: u8, count: usize) -> bool {
        if self.len > 0 && self.data[self.len - 1] != b'/' && !self.push(b"/") {
            return false;
        }
        for _ in 0..count {
            if !self.push(&[fill]) {
                return false;
            }
        }
        true
    }

    /// Append `/component`.
    fn name(&mut self, bytes: &[u8]) -> bool {
        if self.len > 0 && self.data[self.len - 1] != b'/' && !self.push(b"/") {
            return false;
        }
        self.push(bytes)
    }

    fn ptr(&mut self) -> *const c_char {
        self.data[self.len] = 0;
        self.data.as_ptr().cast()
    }
}

fn make_dir(path: &mut Cs) -> bool {
    // SAFETY: `path.ptr()` is a NUL-terminated array owned by `path`.
    unsafe { mkdir(path.ptr(), MODE_DIR) == 0 }
}

fn make_file(path: &mut Cs) -> bool {
    // SAFETY: `path.ptr()` is a NUL-terminated array owned by `path`.
    let fd = unsafe { open(path.ptr(), O_CREAT | O_RDWR | O_EXCL, MODE_FILE) };
    if fd < 0 {
        return false;
    }
    // SAFETY: `fd` is an open descriptor returned above.
    let _ = unsafe { close(fd) };
    true
}

fn open_only(path: &mut Cs) -> c_int {
    // SAFETY: `path.ptr()` is a NUL-terminated array owned by `path`.
    let fd = unsafe { open(path.ptr(), O_RDONLY) };
    if fd >= 0 {
        // SAFETY: `fd` is an open descriptor returned above.
        let _ = unsafe { close(fd) };
    }
    fd
}

/// `lstat` goes through the one-call `Lstat` control request, so it is not bounded by the S11
/// `Open` packet and exercises the filesystem control wire for a long path.
fn lstat_path(path: &mut Cs) -> Option<Stat> {
    let mut record = Stat::zero();
    // SAFETY: `path.ptr()` is NUL-terminated and `record` is one 128-byte `struct stat`.
    let result = unsafe { lstat(path.ptr(), &mut record) };
    (result == 0).then_some(record)
}

fn link_to(target: &[u8], path: &mut Cs) -> bool {
    let mut bytes = [0u8; 65];
    if target.is_empty() || target.len() >= bytes.len() {
        return false;
    }
    bytes[..target.len()].copy_from_slice(target);
    // SAFETY: both pointers reference NUL-terminated arrays that outlive the call.
    unsafe { symlink(bytes.as_ptr().cast(), path.ptr()) == 0 }
}

/// Fill the long path with `a/b/c`, 300 bytes of names plus separators.
fn long_path(path: &mut Cs) -> bool {
    path.base(b"/tmp/paths");
    path.component(b'a', 100) && path.component(b'b', 100) && path.component(b'c', 100)
}

/// Build the tree every row and mutation reads. Returns whether all steps succeeded.
fn prepare() -> bool {
    let mut base = Cs::new();
    base.base(b"/tmp/paths");
    if !make_dir(&mut base) {
        return false;
    }
    let mut level = Cs::new();
    level.base(b"/tmp/paths");
    if !(level.component(b'a', 100) && make_dir(&mut level)) {
        return false;
    }
    if !(level.component(b'b', 100) && make_dir(&mut level)) {
        return false;
    }
    if !(level.component(b'c', 100) && make_dir(&mut level)) {
        return false;
    }
    let mut max = Cs::new();
    max.base(b"/tmp/paths");
    if !(max.component(b'd', 255) && make_dir(&mut max)) {
        return false;
    }
    let mut dir = Cs::new();
    dir.base(b"/tmp/paths/d");
    if !make_dir(&mut dir) {
        return false;
    }
    let mut inner = Cs::new();
    inner.base(b"/tmp/paths/d/f");
    if !make_file(&mut inner) {
        return false;
    }
    let mut link = Cs::new();
    link.base(b"/tmp/paths/l");
    if !link_to(b"d", &mut link) {
        return false;
    }
    let mut absolute = Cs::new();
    absolute.base(b"/tmp/paths/abs");
    if !link_to(b"/tmp/paths/d/f", &mut absolute) {
        return false;
    }
    let mut loop_a = Cs::new();
    loop_a.base(b"/tmp/paths/loopa");
    let mut loop_b = Cs::new();
    loop_b.base(b"/tmp/paths/loopb");
    if !(link_to(b"loopb", &mut loop_a) && link_to(b"loopa", &mut loop_b)) {
        return false;
    }
    let mut root = Cs::new();
    root.base(b"/tmp/paths/root");
    let mut inside = Cs::new();
    inside.base(b"/tmp/paths/root/inside");
    let mut outside = Cs::new();
    outside.base(b"/tmp/paths/outside");
    make_dir(&mut root) && make_file(&mut inside) && make_file(&mut outside)
}

fn make_levels(fill: u8, widths: &[usize]) -> Option<usize> {
    let mut path = Cs::new();
    path.base(b"/tmp/paths");
    for width in widths {
        if !path.component(fill, *width) || !make_dir(&mut path) {
            return None;
        }
    }
    Some(path.len)
}

fn row_levels(state: &mut State, name: &str, fill: u8, widths: &[usize]) {
    let created = make_levels(fill, widths);
    let mut path = Cs::new();
    path.base(b"/tmp/paths");
    for width in widths {
        let _ = path.component(fill, *width);
    }
    let length = path.len;
    let directory =
        lstat_path(&mut path).is_some_and(|stat| stat.st_mode & S_IFMT == S_IFDIR);
    state.check(
        name,
        created == Some(length) && directory,
        format_args!(
            "length={length} created={} dir={directory} errno={}",
            created.is_some(),
            errno()
        ),
    );
}

fn row_long_path(state: &mut State) {
    let mut path = Cs::new();
    let _ = long_path(&mut path);
    let length = path.len;
    let result = lstat_path(&mut path);
    let directory = result.is_some_and(|stat| stat.st_mode & S_IFMT == S_IFDIR);
    state.check(
        "long-path",
        directory && length > 255,
        format_args!("length={length} dir={directory} errno={}", errno()),
    );
}

fn row_rename_max(state: &mut State) {
    let mut source = Cs::new();
    source.base(b"/tmp/paths");
    for _ in 0..15 {
        let _ = source.component(b'q', 255);
    }
    let _ = source.component(b'q', 244);
    let mut destination = Cs::new();
    destination.base(b"/tmp/paths");
    for _ in 0..15 {
        let _ = destination.component(b'q', 255);
    }
    let _ = destination.component(b'z', 244);
    let source_len = source.len;
    let destination_len = destination.len;
    // SAFETY: both pointers are NUL-terminated arrays owned by the two builders.
    let renamed = unsafe { rename(source.ptr(), destination.ptr()) } == 0;
    let error = errno();
    let moved = lstat_path(&mut destination)
        .is_some_and(|stat| stat.st_mode & S_IFMT == S_IFDIR);
    let gone = lstat_path(&mut source).is_none();
    state.check(
        "rename-max",
        renamed && moved && gone,
        format_args!(
            "renamed={renamed} from={source_len} to={destination_len} moved={moved} gone={gone} errno={error}"
        ),
    );
}

fn row_component_limit(state: &mut State) {
    let mut max = Cs::new();
    max.base(b"/tmp/paths");
    let _ = max.component(b'd', 255);
    let max_dir = lstat_path(&mut max).is_some_and(|stat| stat.st_mode & S_IFMT == S_IFDIR);
    let mut over = Cs::new();
    over.base(b"/tmp/paths");
    let _ = over.component(b'e', 256);
    let too_long = make_dir(&mut over);
    let error = errno();
    state.check(
        "component-255",
        max_dir && !too_long && error == ENAMETOOLONG,
        format_args!("max255={max_dir} m256={too_long} errno={error}"),
    );
}

fn row_symlink_suffix(state: &mut State) {
    let mut path = Cs::new();
    path.base(b"/tmp/paths/l/f");
    let fd = open_only(&mut path);
    state.check(
        "symlink-suffix",
        fd >= 0,
        format_args!("open={fd} errno={}", errno()),
    );
}

fn row_absolute_symlink(state: &mut State) {
    let mut path = Cs::new();
    path.base(b"/tmp/paths/abs");
    let fd = open_only(&mut path);
    state.check(
        "absolute-symlink",
        fd >= 0,
        format_args!("open={fd} errno={}", errno()),
    );
}

fn row_symlink_loop(state: &mut State) {
    let mut path = Cs::new();
    path.base(b"/tmp/paths/loopa");
    let fd = open_only(&mut path);
    let error = errno();
    state.check(
        "symlink-loop",
        fd < 0 && error == ELOOP,
        format_args!("open={fd} errno={error}"),
    );
}

fn row_chroot(state: &mut State) {
    let mut root = Cs::new();
    root.base(b"/tmp/paths/root");
    // SAFETY: `root.ptr()` is a NUL-terminated array owned by `root`.
    let entered = unsafe { chroot(root.ptr()) } == 0;
    let mut inside = Cs::new();
    inside.base(b"/inside");
    let inside_fd = open_only(&mut inside);
    let mut outside = Cs::new();
    outside.base(b"/outside");
    let outside_fd = open_only(&mut outside);
    let error = errno();
    state.check(
        "chroot-confinement",
        entered && inside_fd >= 0 && outside_fd < 0 && error == ENOENT,
        format_args!(
            "entered={entered} inside={inside_fd} outside={outside_fd} errno={error}"
        ),
    );
}

fn run_normal(prepared: bool) -> i32 {
    let mut state = State::begin(TOPIC);
    state.check(
        "prepare",
        prepared,
        format_args!("prepare={prepared} errno={}", errno()),
    );
    row_long_path(&mut state);
    row_levels(&mut state, "long-path-1000", b'p', &[249; 4]);
    let mut max_widths = [255usize; 16];
    max_widths[15] = 244;
    row_levels(&mut state, "long-path-max", b'q', &max_widths);
    row_rename_max(&mut state);
    row_component_limit(&mut state);
    row_symlink_suffix(&mut state);
    row_absolute_symlink(&mut state);
    row_symlink_loop(&mut state);
    row_chroot(&mut state);
    state.finish()
}

fn run_mutation(name: &str) -> i32 {
    let mut state = State {
        topic: TOPIC,
        total: 0,
        passed: 0,
        failed: 0,
    };
    conformance::mutation_begin(TOPIC, name);
    match name {
        // A broken walker drops the components after the link; against the correct product the
        // open succeeds, so this wrong expectation makes the row red.
        "suffix-dropped" => {
            let mut path = Cs::new();
            path.base(b"/tmp/paths/l/f");
            let fd = open_only(&mut path);
            state.check(
                "symlink-suffix",
                fd < 0 && errno() == ENOENT,
                format_args!("open={fd} errno={}", errno()),
            );
        }
        // The old filesystem field stopped at 255 bytes; a 302-byte path must reach the server
        // now, so expecting ENAMETOOLONG makes the row red against the correct product.
        "length-255-only" => {
            let mut path = Cs::new();
            let _ = long_path(&mut path);
            let result = lstat_path(&mut path);
            let error = errno();
            state.check(
                "long-path",
                result.is_none() && error == ENAMETOOLONG,
                format_args!("lstat={} errno={error}", result.is_some()),
            );
        }
        // A broken encoder accepts any component length; the correct product returns
        // ENAMETOOLONG, so expecting success makes the row red.
        "component-limit-ignored" => {
            let mut path = Cs::new();
            path.base(b"/tmp/paths");
            let _ = path.component(b'z', 256);
            let made = make_dir(&mut path);
            state.check(
                "component-255",
                made,
                format_args!("m256={made} errno={}", errno()),
            );
        }
        // A lookuper that ignores the process root still reaches the old namespace; the correct
        // product returns ENOENT, so expecting a descriptor makes the row red.
        "chroot-not-applied" => {
            let mut path = Cs::new();
            path.base(b"/outside");
            let fd = open_only(&mut path);
            state.check(
                "chroot-confinement",
                fd >= 0,
                format_args!("open={fd} errno={}", errno()),
            );
        }
        _ => state.check(name, false, format_args!("unknown mutation")),
    }
    conformance::mutation_end(&state, name)
}

fn c_str_eq(pointer: *const c_char, expected: &[u8]) -> bool {
    if pointer.is_null() {
        return false;
    }
    for (index, byte) in expected.iter().enumerate() {
        // SAFETY: the caller passes a NUL-terminated argument vector; stop at the NUL.
        if unsafe { *pointer.add(index) } as u8 != *byte {
            return false;
        }
    }
    // SAFETY: the expected string plus its terminating NUL must fit the argument.
    unsafe { *pointer.add(expected.len()) == 0 }
}

#[unsafe(no_mangle)]
pub extern "C" fn main(argc: c_int, argv: *const *const c_char) -> c_int {
    let prepared = prepare();
    let mut failures = 0;
    let mut all = false;
    let mut index = 1;
    while index < argc {
        // SAFETY: index is inside argv.
        if c_str_eq(unsafe { *argv.add(index as usize) }, b"--all") {
            all = true;
        }
        index += 1;
    }
    if all {
        failures |= run_mutation("suffix-dropped");
        failures |= run_mutation("length-255-only");
        failures |= run_mutation("component-limit-ignored");
        failures |= run_mutation("chroot-not-applied");
    }
    failures |= run_normal(prepared);
    if failures == 0 { 0 } else { 1 }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    conformance::emit(format_args!("CONFORM {TOPIC} panic\n"));
    loop {
        core::hint::spin_loop();
    }
}

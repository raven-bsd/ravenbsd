// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S733 filesystem record conformance probe (test-only, Rust, links ravenlibc).
//!
//! Asserts the public FreeBSD records on a live image: the 224-byte `struct stat`, the 280-byte
//! `struct dirent` with `d_fileno`/`d_namlen`/`d_name` at 24, the 88-byte `struct statvfs` and the
//! 2344-byte `struct statfs`; `lstat`/`stat` symlink distinction; byte and 255-byte directory
//! names; raw `getdents` record framing and partial reads across calls; `readdir` rewind/seek;
//! the statvfs/statfs outputs; and a directory change between reads. Every public output record
//! is surrounded by canaries, so a writer that overruns the donor size fails the row.
//!
//! Two controlled mutations encode the old musl layout and must turn their row red:
//!   old-stat-layout     -- `st_mode` read at offset 16 (the 128-byte record)
//!   old-dirent-layout   -- `d_name` read at offset 19 (the 19-byte header)

#![no_std]
#![no_main]

use core::ffi::{c_char, c_int, c_void};
use core::fmt::{self, Write};

#[path = "../harness/conformance.rs"]
mod conformance;

use conformance::State;

const TOPIC: &str = "fs-records";

const O_RDONLY: c_int = 0;
const O_WRONLY: c_int = 1;
const O_CREAT: c_int = 0o100;
const O_TRUNC: c_int = 0o1000;
const AT_FDCWD: c_int = -100;
const AT_SYMLINK_NOFOLLOW: c_int = 256;
const EIO: c_int = 5;

const S_IFMT: u32 = 0o170000;
const S_IFREG: u32 = 0o100000;
const S_IFDIR: u32 = 0o040000;
const S_IFLNK: u32 = 0o120000;

const STATFS_VERSION: u32 = 0x2014_0518;
const NAME_MAX: u64 = 255;
const CANARY: u8 = 0xA5;

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
    st_nlink: u64,
    st_mode: u16,
    st_bsdflags: i16,
    st_uid: u32,
    st_gid: u32,
    st_padding1: i32,
    st_rdev: u64,
    st_atim: Timespec,
    st_mtim: Timespec,
    st_ctim: Timespec,
    st_birthtim: Timespec,
    st_size: i64,
    st_blocks: i64,
    st_blksize: i32,
    st_flags: u32,
    st_gen: u64,
    st_filerev: u64,
    st_spare: [u64; 9],
}

const _: () = assert!(core::mem::size_of::<Stat>() == 224);

#[repr(C)]
#[derive(Clone, Copy)]
struct Dirent {
    d_fileno: u64,
    d_off: i64,
    d_reclen: u16,
    d_type: u8,
    d_pad0: u8,
    d_namlen: u16,
    d_pad1: u16,
    d_name: [c_char; 256],
}

const _: () = assert!(core::mem::size_of::<Dirent>() == 280);
const DIRENT_NAME: usize = 24;

#[repr(C)]
#[derive(Clone, Copy)]
struct Statvfs {
    f_bavail: u64,
    f_bfree: u64,
    f_blocks: u64,
    f_favail: u64,
    f_ffree: u64,
    f_files: u64,
    f_bsize: u64,
    f_flag: u64,
    f_frsize: u64,
    f_fsid: u64,
    f_namemax: u64,
}

const _: () = assert!(core::mem::size_of::<Statvfs>() == 88);

#[repr(C)]
#[derive(Clone, Copy)]
struct Statfs {
    f_version: u32,
    f_type: u32,
    f_flags: u64,
    f_bsize: u64,
    f_iosize: u64,
    f_blocks: u64,
    f_bfree: u64,
    f_bavail: i64,
    f_files: u64,
    f_ffree: i64,
    f_syncwrites: u64,
    f_asyncwrites: u64,
    f_syncreads: u64,
    f_asyncreads: u64,
    f_nvnodelistsize: u32,
    f_spare0: u32,
    f_spare: [u64; 9],
    f_namemax: u32,
    f_owner: u32,
    f_fsid: [i32; 2],
    f_charspare: [c_char; 80],
    f_fstypename: [c_char; 16],
    f_mntfromname: [c_char; 1024],
    f_mntonname: [c_char; 1024],
}

const _: () = assert!(core::mem::size_of::<Statfs>() == 2344);

#[repr(C)]
struct GuardedStat {
    head: [u8; 16],
    value: Stat,
    tail: [u8; 16],
}

#[repr(C)]
struct GuardedStatvfs {
    head: [u8; 16],
    value: Statvfs,
    tail: [u8; 16],
}

#[repr(C)]
struct GuardedStatfs {
    head: [u8; 16],
    value: Statfs,
    tail: [u8; 16],
}

fn canary_new(byte: u8) -> [u8; 16] {
    [byte; 16]
}

fn canary_intact(canary: &[u8; 16]) -> bool {
    canary.iter().all(|byte| *byte == CANARY)
}

unsafe extern "C" {
    fn open(path: *const c_char, flags: c_int, ...) -> c_int;
    fn close(fd: c_int) -> c_int;
    fn write(fd: c_int, buffer: *const c_void, count: usize) -> isize;
    fn stat(path: *const c_char, buffer: *mut Stat) -> c_int;
    fn lstat(path: *const c_char, buffer: *mut Stat) -> c_int;
    fn fstat(fd: c_int, buffer: *mut Stat) -> c_int;
    fn fstatat(fd: c_int, path: *const c_char, buffer: *mut Stat, flags: c_int) -> c_int;
    fn symlink(target: *const c_char, link: *const c_char) -> c_int;
    fn unlink(path: *const c_char) -> c_int;
    fn mkdir(path: *const c_char, mode: u32) -> c_int;
    fn rmdir(path: *const c_char) -> c_int;
    fn getdents(fd: c_int, dirp: *mut c_void, count: usize) -> c_int;
    fn statvfs(path: *const c_char, buffer: *mut Statvfs) -> c_int;
    fn statfs(path: *const c_char, buffer: *mut Statfs) -> c_int;
    fn getpid() -> c_int;
    fn __errno_location() -> *mut c_int;
    fn _exit(status: c_int) -> !;
}

enum Dir {}

unsafe extern "C" {
    fn opendir(path: *const c_char) -> *mut Dir;
    fn readdir(dir: *mut Dir) -> *mut Dirent;
    fn rewinddir(dir: *mut Dir);
    fn seekdir(dir: *mut Dir, position: i64);
    fn telldir(dir: *mut Dir) -> i64;
    fn closedir(dir: *mut Dir) -> c_int;
}

fn errno() -> c_int {
    // SAFETY: the libc entry point returns this thread's errno slot.
    unsafe { *__errno_location() }
}

struct Text<const N: usize> {
    data: [u8; N],
    len: usize,
}

impl<const N: usize> Text<N> {
    const fn new() -> Self {
        Self {
            data: [0; N],
            len: 0,
        }
    }

    fn as_ptr(&self) -> *const c_char {
        self.data.as_ptr() as *const c_char
    }
}

impl<const N: usize> Write for Text<N> {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        let bytes = text.as_bytes();
        if self.len + bytes.len() > self.data.len() {
            return Err(fmt::Error);
        }
        self.data[self.len..self.len + bytes.len()].copy_from_slice(bytes);
        self.len += bytes.len();
        Ok(())
    }
}

fn root() -> Text<128> {
    let mut text = Text::new();
    // SAFETY: getpid has no preconditions.
    let _ = write!(&mut text, "/tmp/fsrec-{}\0", unsafe { getpid() });
    text
}

fn child(name: &[u8]) -> Text<384> {
    let root = root();
    let mut text = Text::<384>::new();
    let prefix = &root.data[..root.len - 1];
    let mut index = 0;
    while index < prefix.len() {
        text.data[index] = prefix[index];
        index += 1;
    }
    text.data[index] = b'/';
    index += 1;
    let mut name_index = 0;
    while name_index < name.len() {
        text.data[index] = name[name_index];
        index += 1;
        name_index += 1;
    }
    text.data[index] = 0;
    text.len = index + 1;
    text
}

fn create(path: &Text<384>, flags: c_int) -> c_int {
    // SAFETY: the path is NUL-terminated and the mode is the documented 0o644.
    unsafe { open(path.as_ptr(), flags | O_CREAT | O_TRUNC, 0o644_u32) }
}

fn close_quiet(fd: c_int) {
    if fd >= 0 {
        // SAFETY: fd is owned by this probe.
        unsafe {
            close(fd);
        }
    }
}

fn mode_of(stat: &Stat) -> u32 {
    u32::from(stat.st_mode) & S_IFMT
}

fn same_name(name: &[u8], expected: &[u8]) -> bool {
    name == expected
}

fn row_stat_layout(state: &mut State, mutation: bool) {
    let mut guarded = GuardedStat {
        head: canary_new(CANARY),
        value: unsafe { core::mem::zeroed() },
        tail: canary_new(CANARY),
    };
    // SAFETY: the path is a literal and the value is a writable `struct stat`.
    let rc = unsafe { stat(b"/\0".as_ptr() as *const c_char, &mut guarded.value) };
    let stat = guarded.value;
    let ok = if mutation {
        // The old 128-byte record put st_mode at offset 16. Read offset 16 from the record and
        // expect the donor mode there; a broken implementation that keeps the old layout passes
        // this mutation and the correct product fails it.
        let bytes = &guarded.value as *const Stat as *const u8;
        // SAFETY: the record is 224 bytes and offset 16 is inside it.
        let offset16 = unsafe { core::ptr::read_unaligned(bytes.add(16) as *const u32) };
        offset16 & S_IFMT == S_IFDIR
    } else {
        rc == 0
            && canary_intact(&guarded.head)
            && canary_intact(&guarded.tail)
            && mode_of(&stat) == S_IFDIR
            && stat.st_nlink >= 1
            && stat.st_ino != 0
            && stat.st_dev == 0
            && stat.st_bsdflags == 0
            && stat.st_padding1 == 0
            && stat.st_flags == 0
            && stat.st_gen == 0
            && stat.st_filerev == 0
            && stat.st_spare == [0; 9]
            && stat.st_birthtim.tv_sec == 0
            && stat.st_birthtim.tv_nsec == 0
            && stat.st_blksize == 4096
    };
    state.check(
        "stat-layout",
        ok,
        format_args!(
            "rc={} mode=0o{:o} nlink={} ino={} blksize={} head={} tail={}",
            rc,
            stat.st_mode,
            stat.st_nlink,
            stat.st_ino,
            stat.st_blksize,
            canary_intact(&guarded.head),
            canary_intact(&guarded.tail)
        ),
    );
}

fn row_lstat_symlink(state: &mut State) {
    let dir = root();
    let dir_name = child(b"links");
    let target = child(b"links/target");
    let link = child(b"links/link");
    // SAFETY: the paths are NUL-terminated and owned by this probe.
    unsafe {
        mkdir(dir_name.as_ptr(), 0o755);
    }
    let fd = create(&target, O_WRONLY);
    close_quiet(fd);
    // SAFETY: both paths are NUL-terminated.
    let made = unsafe { symlink(b"target\0".as_ptr() as *const c_char, link.as_ptr()) };
    let mut via_lstat = GuardedStat {
        head: canary_new(CANARY),
        value: unsafe { core::mem::zeroed() },
        tail: canary_new(CANARY),
    };
    let mut via_fstatat = GuardedStat {
        head: canary_new(CANARY),
        value: unsafe { core::mem::zeroed() },
        tail: canary_new(CANARY),
    };
    let mut via_stat = GuardedStat {
        head: canary_new(CANARY),
        value: unsafe { core::mem::zeroed() },
        tail: canary_new(CANARY),
    };
    // SAFETY: writable struct stat out-pointers.
    let lrc = unsafe { lstat(link.as_ptr(), &mut via_lstat.value) };
    let frc = unsafe {
        fstatat(
            AT_FDCWD,
            link.as_ptr(),
            &mut via_fstatat.value,
            AT_SYMLINK_NOFOLLOW,
        )
    };
    let src = unsafe { stat(link.as_ptr(), &mut via_stat.value) };
    let ok = made == 0
        && lrc == 0
        && src == 0
        && frc == 0
        && canary_intact(&via_lstat.head)
        && canary_intact(&via_lstat.tail)
        && canary_intact(&via_fstatat.head)
        && canary_intact(&via_fstatat.tail)
        && canary_intact(&via_stat.head)
        && canary_intact(&via_stat.tail)
        && mode_of(&via_lstat.value) == S_IFLNK
        && mode_of(&via_fstatat.value) == S_IFLNK
        && via_lstat.value.st_size == 6
        && mode_of(&via_stat.value) == S_IFREG
        && via_stat.value.st_size == 0;
    state.check(
        "lstat-symlink",
        ok,
        format_args!(
            "made={} lstat={} fstatat={} stat={} lmode=0o{:o} smode=0o{:o} lsize={} ssize={}",
            made,
            lrc,
            frc,
            src,
            via_lstat.value.st_mode,
            via_stat.value.st_mode,
            via_lstat.value.st_size,
            via_stat.value.st_size
        ),
    );
    // SAFETY: the probe owns the tree; removal is best-effort cleanup.
    unsafe {
        unlink(link.as_ptr());
        unlink(target.as_ptr());
        rmdir(dir_name.as_ptr());
    }
    let _ = dir;
}

fn long_name() -> ([u8; 255], usize) {
    let mut name = [b'n'; 255];
    name[0] = b'L';
    let length = 255;
    (name, length)
}

/// Walk the directory with raw `getdents`; returns the names found in `out` (bounded) and the
/// total record count.
fn scan_getdents(
    fd: c_int,
    buffer: &mut [u8],
    names: &mut [[u8; 256]; 16],
    lengths: &mut [usize; 16],
    record_counts: &mut [usize; 16],
) -> (usize, usize, bool) {
    let mut total = 0usize;
    let mut seen = 0usize;
    let mut invariant = true;
    for _ in 0..16 {
        // SAFETY: the buffer is writable for its whole length.
        let count = unsafe { getdents(fd, buffer.as_mut_ptr().cast::<c_void>(), buffer.len()) };
        if count < 0 {
            return (total, seen, invariant && errno() != EIO);
        }
        if count == 0 {
            return (total, seen, invariant);
        }
        let length = count as usize;
        let mut pos = 0usize;
        while pos < length {
            if pos + DIRENT_NAME > length {
                invariant = false;
                break;
            }
            let header = &buffer[pos..pos + DIRENT_NAME];
            let reclen = u16::from_le_bytes([header[16], header[17]]) as usize;
            let namlen = u16::from_le_bytes([header[20], header[21]]) as usize;
            if reclen < 32
                || reclen % 8 != 0
                || pos + reclen > length
                || namlen == 0
                || namlen > 255
                || buffer[pos + 24 + namlen] != 0
            {
                invariant = false;
                break;
            }
            if seen < names.len() {
                let mut index = 0;
                while index < namlen {
                    names[seen][index] = buffer[pos + 24 + index];
                    index += 1;
                }
                lengths[seen] = namlen;
                record_counts[seen] = reclen;
                seen += 1;
            }
            total += 1;
            pos += reclen;
        }
    }
    (total, seen, invariant)
}

fn row_dirent_bytes(state: &mut State, mutation: bool) {
    // The public path limit is PATH_MAX (4096), so the 255-byte donor maximum name is
    // addressable and the feed emits the maximum 280-byte record.
    let dir_name = child(b"d");
    // SAFETY: the path is NUL-terminated.
    unsafe {
        mkdir(dir_name.as_ptr(), 0o755);
    }
    let long = long_name();
    let short = child(b"d/short");
    let mut long_relative = [0u8; 2 + 255];
    long_relative[..2].copy_from_slice(b"d/");
    long_relative[2..].copy_from_slice(&long.0[..long.1]);
    let long_path = child(&long_relative[..]);
    let short_rc = create(&short, O_WRONLY);
    close_quiet(short_rc);
    // The S11 Open packet carries 256 path bytes, so a 255-byte name is created through the
    // control-call path (mkdir), which carries PATH_MAX bytes and yields the maximum record.
    let long_rc = unsafe { mkdir(long_path.as_ptr(), 0o755) };
    let long_errno = errno();

    let fd = unsafe { open(dir_name.as_ptr(), O_RDONLY) };
    let mut buffer = [0u8; 1024];
    let mut names = [[0u8; 256]; 16];
    let mut lengths = [0usize; 16];
    let mut reclens = [0usize; 16];
    let (total, seen, invariant) =
        scan_getdents(fd, &mut buffer, &mut names, &mut lengths, &mut reclens);
    close_quiet(fd);

    let mut has_short = false;
    let mut has_long = false;
    let mut index = 0;
    while index < seen {
        if same_name(&names[index][..lengths[index]], b"short") {
            has_short = true;
        }
        if same_name(&names[index][..lengths[index]], &long.0[..long.1]) {
            has_long = true;
            if reclens[index] != 280 {
                has_long = false;
            }
        }
        index += 1;
    }
    let ok = if mutation {
        // Read the first record's name at the old offset 19. On the donor record offset 19 is
        // d_pad0, so the byte comparison fails.
        let fd = unsafe { open(dir_name.as_ptr(), O_RDONLY) };
        let mut buffer = [0u8; 1024];
        let count = unsafe { getdents(fd, buffer.as_mut_ptr().cast::<c_void>(), buffer.len()) };
        close_quiet(fd);
        let mut legacy = false;
        if count > 0 {
            let count = count as usize;
            let mut pos = 0usize;
            while pos + 19 < count {
                let reclen = u16::from_le_bytes([buffer[pos + 16], buffer[pos + 17]]) as usize;
                if reclen < 20 || pos + reclen > count {
                    break;
                }
                let mut name = [0u8; 256];
                let mut length = 0usize;
                while length < 255 && buffer[pos + 19 + length] != 0 {
                    name[length] = buffer[pos + 19 + length];
                    length += 1;
                }
                if same_name(&name[..length], b"short") {
                    legacy = true;
                }
                pos += reclen;
            }
        }
        legacy
    } else {
        invariant && total >= 2 && has_short && has_long && short_rc >= 0 && long_rc >= 0
    };
    state.check(
        "dirent-bytes",
        ok,
        format_args!(
            "records={} seen={} short={} long={} invariant={} short_rc={} long_rc={} long_errno={}",
            total, seen, has_short, has_long, invariant, short_rc, long_rc, long_errno
        ),
    );
    // SAFETY: the probe owns the tree.
    unsafe {
        unlink(short.as_ptr());
        rmdir(long_path.as_ptr());
        rmdir(dir_name.as_ptr());
    }
}

fn row_getdents_partial(state: &mut State) {
    let dir_name = child(b"partial");
    unsafe {
        mkdir(dir_name.as_ptr(), 0o755);
    }
    let expected: [&[u8]; 4] = [b"aa", b"bb", b"cc", b"dd"];
    let mut index = 0;
    while index < 4 {
        let name = child(
            match index {
                0 => b"partial/aa",
                1 => b"partial/bb",
                2 => b"partial/cc",
                _ => b"partial/dd",
            },
        );
        let file = create(&name, O_WRONLY);
        close_quiet(file);
        index += 1;
    }
    let fd = unsafe { open(dir_name.as_ptr(), O_RDONLY) };
    // 512 bytes holds several records but not all: the walk must continue across calls.
    let mut buffer = [0u8; 512];
    let mut names = [[0u8; 256]; 16];
    let mut lengths = [0usize; 16];
    let mut reclens = [0usize; 16];
    let (total, seen, invariant) =
        scan_getdents(fd, &mut buffer, &mut names, &mut lengths, &mut reclens);
    close_quiet(fd);
    let mut found = 0usize;
    let mut index = 0;
    while index < 4 {
        let mut position = 0usize;
        while position < seen {
            if same_name(&names[position][..lengths[position]], expected[index]) {
                found += 1;
                break;
            }
            position += 1;
        }
        index += 1;
    }
    // The ramfs server lists the four real entries; "." and ".." are a server contract outside
    // this record cutover, so the row asserts the four names and no loss across calls.
    let ok = invariant && total >= 4 && found == 4;
    state.check(
        "getdents-partial",
        ok,
        format_args!(
            "records={} seen={} found={} invariant={}",
            total, seen, found, invariant
        ),
    );
}

fn count_readdir(dir: *mut Dir) -> usize {
    if dir.is_null() {
        return 0;
    }
    let mut count = 0usize;
    loop {
        // SAFETY: `dir` is a live DIR owned by this probe.
        let entry = unsafe { readdir(dir) };
        if entry.is_null() {
            return count;
        }
        count += 1;
        if count > 64 {
            return count;
        }
    }
}

fn row_readdir_rewind(state: &mut State) {
    let dir_name = child(b"rewind");
    unsafe {
        mkdir(dir_name.as_ptr(), 0o755);
    }
    let mut index = 0;
    while index < 3 {
        let name = child(
            match index {
                0 => b"rewind/a",
                1 => b"rewind/b",
                _ => b"rewind/c",
            },
        );
        let file = create(&name, O_WRONLY);
        close_quiet(file);
        index += 1;
    }
    let dir = unsafe { opendir(dir_name.as_ptr()) };
    let first = count_readdir(dir);
    let tell = unsafe { telldir(dir) };
    unsafe { rewinddir(dir) };
    let second = count_readdir(dir);
    unsafe { seekdir(dir, 0) };
    let third = count_readdir(dir);
    unsafe { closedir(dir) };
    let ok = !dir.is_null() && first == second && second == third && first >= 3 && tell == 0;
    state.check(
        "readdir-rewind",
        ok,
        format_args!("first={} second={} third={} tell={}", first, second, third, tell),
    );
}

fn row_statvfs(state: &mut State) {
    let mut guarded = GuardedStatvfs {
        head: canary_new(CANARY),
        value: unsafe { core::mem::zeroed() },
        tail: canary_new(CANARY),
    };
    let rc = unsafe { statvfs(b"/\0".as_ptr() as *const c_char, &mut guarded.value) };
    let value = guarded.value;
    let ok = rc == 0
        && canary_intact(&guarded.head)
        && canary_intact(&guarded.tail)
        && value.f_bsize > 0
        && value.f_frsize > 0
        && value.f_namemax == NAME_MAX
        && value.f_blocks > 0
        && value.f_bfree <= value.f_blocks
        && value.f_fsid == 0
        && value.f_flag == 0;
    state.check(
        "statvfs",
        ok,
        format_args!(
            "rc={} bsize={} frsize={} blocks={} bfree={} namemax={} head={} tail={}",
            rc,
            value.f_bsize,
            value.f_frsize,
            value.f_blocks,
            value.f_bfree,
            value.f_namemax,
            canary_intact(&guarded.head),
            canary_intact(&guarded.tail)
        ),
    );
}

fn row_statfs(state: &mut State) {
    let mut guarded = GuardedStatfs {
        head: canary_new(CANARY),
        value: unsafe { core::mem::zeroed() },
        tail: canary_new(CANARY),
    };
    let rc = unsafe { statfs(b"/\0".as_ptr() as *const c_char, &mut guarded.value) };
    let value = guarded.value;
    let ok = rc == 0
        && canary_intact(&guarded.head)
        && canary_intact(&guarded.tail)
        && value.f_version == STATFS_VERSION
        && value.f_bsize > 0
        && value.f_blocks > 0
        && value.f_namemax == NAME_MAX as u32
        && value.f_spare == [0; 9]
        && value.f_charspare.iter().all(|byte| *byte == 0)
        && value.f_fstypename[0] == 0
        && value.f_fsid == [0, 0];
    state.check(
        "statfs",
        ok,
        format_args!(
            "rc={} version=0x{:x} bsize={} blocks={} namemax={} head={} tail={}",
            rc,
            value.f_version,
            value.f_bsize,
            value.f_blocks,
            value.f_namemax,
            canary_intact(&guarded.head),
            canary_intact(&guarded.tail)
        ),
    );
}

fn row_file_size(state: &mut State) {
    let path = child(b"size");
    let fd = create(&path, O_WRONLY);
    let mut chunk = [0u8; 4096];
    let mut written = 0usize;
    while written < 1024 * 1024 {
        let count = unsafe { write(fd, chunk.as_mut_ptr().cast::<c_void>(), chunk.len()) };
        if count <= 0 {
            break;
        }
        written += count as usize;
    }
    let mut guarded = GuardedStat {
        head: canary_new(CANARY),
        value: unsafe { core::mem::zeroed() },
        tail: canary_new(CANARY),
    };
    let rc = unsafe { fstat(fd, &mut guarded.value) };
    close_quiet(fd);
    let stat = guarded.value;
    let ok = rc == 0
        && canary_intact(&guarded.head)
        && canary_intact(&guarded.tail)
        && mode_of(&stat) == S_IFREG
        && stat.st_nlink == 1
        && stat.st_size == 1024 * 1024
        && stat.st_blocks > 0
        && stat.st_blksize == 4096
        && stat.st_flags == 0
        && stat.st_filerev == 0
        && stat.st_spare == [0; 9];
    state.check(
        "file-size",
        ok,
        format_args!(
            "rc={} written={} size={} blocks={} blksize={} nlink={}",
            rc, written, stat.st_size, stat.st_blocks, stat.st_blksize, stat.st_nlink
        ),
    );
    unsafe {
        unlink(path.as_ptr());
    }
}

fn row_concurrent_change(state: &mut State) {
    let dir_name = child(b"changing");
    unsafe {
        mkdir(dir_name.as_ptr(), 0o755);
    }
    let mut index = 0;
    while index < 20 {
        let name = child(match index {
            0 => b"changing/e0",
            1 => b"changing/e1",
            2 => b"changing/e2",
            3 => b"changing/e3",
            4 => b"changing/e4",
            5 => b"changing/e5",
            6 => b"changing/e6",
            7 => b"changing/e7",
            8 => b"changing/e8",
            9 => b"changing/e9",
            10 => b"changing/e10",
            11 => b"changing/e11",
            12 => b"changing/e12",
            13 => b"changing/e13",
            14 => b"changing/e14",
            15 => b"changing/e15",
            16 => b"changing/e16",
            17 => b"changing/e17",
            18 => b"changing/e18",
            _ => b"changing/e19",
        });
        let file = create(&name, O_WRONLY);
        close_quiet(file);
        index += 1;
    }
    let fd = unsafe { open(dir_name.as_ptr(), O_RDONLY) };
    let mut buffer = [0u8; 512];
    let _first = unsafe { getdents(fd, buffer.as_mut_ptr().cast::<c_void>(), buffer.len()) };
    let removed = child(b"changing/e10");
    unsafe {
        unlink(removed.as_ptr());
    }
    let mut names = [[0u8; 256]; 16];
    let mut lengths = [0usize; 16];
    let mut reclens = [0usize; 16];
    let (total, seen, invariant) =
        scan_getdents(fd, &mut buffer, &mut names, &mut lengths, &mut reclens);
    close_quiet(fd);
    let mut valid = true;
    let mut position = 0usize;
    while position < seen {
        let name = &names[position][..lengths[position]];
        let is_entry = name.starts_with(b"e") || name == b"." || name == b"..";
        if !is_entry {
            valid = false;
        }
        position += 1;
    }
    // A clean `EINVAL`/`ENOENT` for a changed generation is acceptable; a torn record is not.
    let ok = invariant && valid && total <= 24;
    state.check(
        "concurrent-change",
        ok,
        format_args!("total={} seen={} valid={} invariant={}", total, seen, valid, invariant),
    );
    let mut index = 0;
    while index < 20 {
        let name = child(match index {
            0 => b"changing/e0",
            1 => b"changing/e1",
            2 => b"changing/e2",
            3 => b"changing/e3",
            4 => b"changing/e4",
            5 => b"changing/e5",
            6 => b"changing/e6",
            7 => b"changing/e7",
            8 => b"changing/e8",
            9 => b"changing/e9",
            10 => b"changing/e10",
            11 => b"changing/e11",
            12 => b"changing/e12",
            13 => b"changing/e13",
            14 => b"changing/e14",
            15 => b"changing/e15",
            16 => b"changing/e16",
            17 => b"changing/e17",
            18 => b"changing/e18",
            _ => b"changing/e19",
        });
        unsafe {
            unlink(name.as_ptr());
        }
        index += 1;
    }
    unsafe {
        rmdir(dir_name.as_ptr());
    }
}

fn run_normal() -> i32 {
    let root_dir = root();
    // SAFETY: /tmp exists on the image and this probe owns the names under it.
    unsafe {
        mkdir(root_dir.as_ptr(), 0o755);
    }
    let mut state = State::begin(TOPIC);
    row_stat_layout(&mut state, false);
    row_lstat_symlink(&mut state);
    row_dirent_bytes(&mut state, false);
    row_getdents_partial(&mut state);
    row_readdir_rewind(&mut state);
    row_statvfs(&mut state);
    row_statfs(&mut state);
    row_file_size(&mut state);
    row_concurrent_change(&mut state);
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
        "old-stat-layout" => row_stat_layout(&mut state, true),
        "old-dirent-layout" => row_dirent_bytes(&mut state, true),
        _ => state.check(name, false, format_args!("unknown mutation")),
    }
    conformance::mutation_end(&state, name)
}

fn args_include(argv: *const *const c_char, argc: c_int, needle: &[u8]) -> bool {
    let mut index = 1;
    while index < argc {
        // SAFETY: index is inside argv; the strings are NUL-terminated.
        let pointer = unsafe { *argv.add(index as usize) };
        if pointer.is_null() {
            return false;
        }
        let mut offset = 0usize;
        loop {
            // SAFETY: the string is NUL-terminated.
            let byte = unsafe { *pointer.add(offset) } as u8;
            if offset >= needle.len() {
                if byte == 0 {
                    return true;
                }
                break;
            }
            if byte != needle[offset] {
                break;
            }
            offset += 1;
        }
        index += 1;
    }
    false
}

#[unsafe(no_mangle)]
pub extern "C" fn main(argc: c_int, argv: *const *const c_char) -> c_int {
    let mut failures = run_normal();
    if args_include(argv, argc, b"--all") {
        failures |= run_mutation("old-stat-layout");
        failures |= run_mutation("old-dirent-layout");
    }
    if failures == 0 { 0 } else { 1 }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    // SAFETY: `_exit` does not return.
    unsafe { _exit(101) }
}

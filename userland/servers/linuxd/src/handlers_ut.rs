// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Handler tests: every B20 syscall, the error paths and the dispatcher counters.

use core::mem::offset_of;

use raven_abi::errno::Errno;
use raven_abi::types::Handle;
use raven_linux_abi::structs::x86_64::{Stat as AbiStat, Statx as AbiStatx};
use raven_linux_dispatch::SyscallArgs;

use super::*;

fn process() -> LinuxProcess {
    LinuxProcess::new(7, Handle(9), 0x4000, 0x5000, 0x8000, 0x9000)
}

fn call(nr: usize, a: [usize; 6]) -> SyscallArgs {
    SyscallArgs { nr, a }
}

fn dispatch(process: &mut LinuxProcess, nr: usize, a: [usize; 6]) -> isize {
    let mut dispatcher = dispatcher();
    dispatcher.dispatch(process, &call(nr, a))
}

fn set_path(process: &mut LinuxProcess, path: &[u8]) {
    process.user_buffer.clear();
    process.user_buffer.extend_from_slice(path);
    process.user_buffer.push(0);
}

fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    let mut word = [0u8; 4];
    word.copy_from_slice(&bytes[offset..offset + 4]);
    u32::from_le_bytes(word)
}

fn u64_at(bytes: &[u8], offset: usize) -> u64 {
    let mut word = [0u8; 8];
    word.copy_from_slice(&bytes[offset..offset + 8]);
    u64::from_le_bytes(word)
}

fn stat_bytes(process: &mut LinuxProcess, nr: usize, args: [usize; 6], address: u64) -> Vec<u8> {
    assert_eq!(dispatch(process, nr, args), 0);
    let (queued, bytes) = process.pending_write.take().expect("a stat copy-out");
    assert_eq!(queued, address);
    bytes
}

fn proc_file_len(process: &LinuxProcess, path: &[u8]) -> u64 {
    match procfs::lookup(process, path) {
        Ok(Resource::File(bytes)) => bytes.len() as u64,
        other => panic!("{path:?}: {other:?}"),
    }
}

#[test]
fn write_copies_the_prefetched_buffer() {
    let mut process = process();
    process
        .user_buffer
        .extend_from_slice(b"hello from linuxd\n");
    let written = dispatch(&mut process, 1, [1, 0x1000, 18, 0, 0, 0]);
    assert_eq!(written, 18);
    assert_eq!(process.output, b"hello from linuxd\n");
}

#[test]
fn write_to_a_bad_descriptor_is_ebadf() {
    let mut process = process();
    process.user_buffer.extend_from_slice(b"x");
    assert_eq!(
        dispatch(&mut process, 1, [9, 0x1000, 1, 0, 0, 0]),
        -(Errno::EBADF.0 as isize)
    );
    assert!(process.output.is_empty());
}

#[test]
fn write_stops_at_the_prefetched_length() {
    let mut process = process();
    process.user_buffer.extend_from_slice(b"abc");
    assert_eq!(dispatch(&mut process, 1, [1, 0, 1, 0, 0, 0]), 1);
    assert_eq!(process.output, b"a");
}

#[test]
fn exit_group_records_the_status() {
    let mut process = process();
    assert_eq!(dispatch(&mut process, 231, [3, 0, 0, 0, 0, 0]), 0);
    assert_eq!(process.exit, Some(3));
}

#[test]
fn brk_reads_and_moves_within_the_arena() {
    let mut process = process();
    assert_eq!(dispatch(&mut process, 12, [0, 0, 0, 0, 0, 0]), 0x4000);
    assert_eq!(dispatch(&mut process, 12, [0x4800, 0, 0, 0, 0, 0]), 0x4800);
    assert_eq!(process.brk, 0x4800);
    // Outside the arena the call fails by returning the unchanged break.
    assert_eq!(dispatch(&mut process, 12, [0x9000, 0, 0, 0, 0, 0]), 0x4800);
    assert_eq!(process.brk, 0x4800);
}

#[test]
fn anonymous_mmap_hands_out_the_arena_in_pages() {
    let mut process = process();
    let first = dispatch(&mut process, 9, [0, 100, 3, 0x22, usize::MAX, 0]);
    assert_eq!(first, 0x8000);
    assert_eq!(process.mmap_next, 0x9000);
    // The 4 KiB arena is exhausted; the call fails without moving the cursor.
    assert_eq!(
        dispatch(&mut process, 9, [0, 4096, 3, 0x22, usize::MAX, 0]),
        -(Errno::ENOMEM.0 as isize)
    );
    assert_eq!(process.mmap_next, 0x9000);
}

#[test]
fn file_backed_and_executable_mmap_are_refused() {
    let mut process = process();
    assert_eq!(
        dispatch(&mut process, 9, [0, 4096, 3, 0, 3, 0]),
        -(Errno::ENOSYS.0 as isize)
    );
    assert_eq!(
        dispatch(&mut process, 9, [0, 4096, 5, 0x22, usize::MAX, 0]),
        -(Errno::ENOSYS.0 as isize)
    );
    assert_eq!(process.mmap_next, 0x8000);
}

#[test]
fn getpid_returns_the_table_pid() {
    let mut process = process();
    assert_eq!(dispatch(&mut process, 39, [0; 6]), 7);
}

#[test]
fn uname_queues_a_six_field_record() {
    let mut process = process();
    assert_eq!(dispatch(&mut process, 63, [0x2000, 0, 0, 0, 0, 0]), 0);
    let (address, bytes) = process
        .pending_write
        .take()
        .expect("uname queues a copy-out");
    assert_eq!(address, 0x2000);
    assert_eq!(bytes.len(), 6 * 65);
    assert_eq!(&bytes[..6], b"Linux\0");
    assert_eq!(&bytes[65..74], b"ravenbsd\0");
}

#[test]
fn rt_sigprocmask_checks_the_set_size() {
    let mut process = process();
    assert_eq!(dispatch(&mut process, 14, [0, 0, 0, 8, 0, 0]), 0);
    assert_eq!(
        dispatch(&mut process, 14, [0, 0, 0, 4, 0, 0]),
        -(Errno::EINVAL.0 as isize)
    );
}

#[test]
fn arch_prctl_queues_the_tls_base_for_the_trap_loop() {
    let mut process = process();
    assert_eq!(dispatch(&mut process, 158, [0x1002, 0x7000, 0, 0, 0, 0]), 0);
    assert_eq!(process.pending_tls, Some(0x7000));
    assert_eq!(process.fs_base, 0);
    assert_eq!(
        dispatch(&mut process, 158, [0x9999, 0x7000, 0, 0, 0, 0]),
        -(Errno::EINVAL.0 as isize)
    );
}

#[test]
fn arch_prctl_get_fs_copies_the_stored_base() {
    let mut process = process();
    process.fs_base = 0x7fff_0000;
    assert_eq!(dispatch(&mut process, 158, [0x1003, 0x2000, 0, 0, 0, 0]), 0);
    let (address, bytes) = process.pending_write.take().expect("a copy-out");
    assert_eq!(address, 0x2000);
    assert_eq!(bytes, 0x7fff_0000u64.to_le_bytes());
}

#[test]
fn an_unknown_number_is_enosys_and_counted() {
    let mut process = process();
    let mut dispatcher = dispatcher();
    assert_eq!(
        dispatcher.dispatch(&mut process, &call(218, [0; 6])),
        -(Errno::ENOSYS.0 as isize)
    );
    let hits = dispatcher.unimplemented_hits();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].0, 218);
    assert_eq!(hits[0].1, Some("set_tid_address"));
    assert_eq!(hits[0].2, 1);
}

#[test]
fn the_stats_name_every_implemented_handler() {
    let mut process = process();
    let mut dispatcher = dispatcher();
    let mut names = dispatcher.implemented_names();
    names.sort_unstable();
    assert_eq!(
        names,
        [
            "access",
            "arch_prctl",
            "brk",
            "close",
            "exit_group",
            "fchmodat",
            "fchownat",
            "fstat",
            "getdents64",
            "getpid",
            "lseek",
            "lstat",
            "mmap",
            "newfstatat",
            "open",
            "openat",
            "read",
            "readlink",
            "readlinkat",
            "rt_sigprocmask",
            "stat",
            "statx",
            "truncate",
            "uname",
            "utimensat",
            "write"
        ]
    );
    let _ = dispatcher.dispatch(&mut process, &call(1, [1, 0, 0, 0, 0, 0]));
    let _ = dispatcher.dispatch(&mut process, &call(1, [1, 0, 0, 0, 0, 0]));
    let stats = dispatcher.stats();
    let write = stats.iter().find(|(name, _, _)| *name == "write");
    assert_eq!(write.map(|(_, calls, _)| *calls), Some(2));
}

#[test]
fn openat_and_read_serve_a_proc_file() {
    let mut process = process();
    set_path(&mut process, b"/proc/self/status");
    let fd = dispatch(&mut process, 257, [AT_FDCWD as usize, 0, 0, 0, 0, 0]);
    assert_eq!(fd, 3);
    let read = dispatch(&mut process, 0, [3, 0x1000, 4096, 0, 0, 0]);
    assert!(read > 0);
    let (address, bytes) = process.pending_write.take().unwrap();
    assert_eq!(address, 0x1000);
    assert!(bytes.starts_with(b"Name:\t"));
    assert_eq!(dispatch(&mut process, 3, [3, 0, 0, 0, 0, 0]), 0);
    assert!(process.file(3).is_none());
}

#[test]
fn open_legacy_and_readlinkat_serve_a_symlink() {
    let mut process = process();
    process.set_identity(b"/hello", vec![b"/hello".to_vec()]);
    set_path(&mut process, b"/proc/self/exe");
    let read = dispatch(&mut process, 267, [AT_FDCWD as usize, 0, 0x3000, 64, 0, 0]);
    assert_eq!(read, 6);
    let (address, bytes) = process.pending_write.take().unwrap();
    assert_eq!(address, 0x3000);
    assert_eq!(bytes, b"/hello");

    set_path(&mut process, b"/proc/cpuinfo");
    let fd = dispatch(&mut process, 2, [0, 0, 0, 0, 0, 0]);
    assert_eq!(fd, 3);
    let read = dispatch(&mut process, 0, [3, 0x4000, 64, 0, 0, 0]);
    assert_eq!(read, 64);
    assert!(
        process
            .pending_write
            .take()
            .unwrap()
            .1
            .starts_with(b"processor\t")
    );
}

#[test]
fn a_directory_getdents64_lists_its_entries() {
    let mut process = process();
    set_path(&mut process, b"/proc/self/fd");
    let fd = dispatch(
        &mut process,
        257,
        [AT_FDCWD as usize, 0, O_DIRECTORY as usize, 0, 0, 0],
    );
    assert_eq!(fd, 3);
    let read = dispatch(&mut process, 217, [3, 0x5000, 4096, 0, 0, 0]);
    assert!(read > 0);
    let (_, bytes) = process.pending_write.take().unwrap();
    assert!(bytes.windows(2).any(|window| window == b"0\0"));
    assert!(bytes.windows(2).any(|window| window == b"1\0"));
    assert!(bytes.windows(2).any(|window| window == b"2\0"));
    assert_eq!(
        dispatch(&mut process, 0, [3, 0x5000, 16, 0, 0, 0]),
        -(Errno::EISDIR.0 as isize)
    );
}

#[test]
fn lseek_moves_the_read_cursor() {
    let mut process = process();
    set_path(&mut process, b"/proc/self/exe");
    process.set_identity(b"/hello", vec![b"/hello".to_vec()]);
    let _ = dispatch(&mut process, 2, [0, O_RDONLY as usize, 0, 0, 0, 0]);
    // Opening a symlink is ELOOP; use a regular file instead.
    set_path(&mut process, b"/proc/cpuinfo");
    let fd = dispatch(&mut process, 2, [0, 0, 0, 0, 0, 0]);
    assert_eq!(fd, 3);
    assert_eq!(
        dispatch(&mut process, 8, [3, 6, SEEK_SET as usize, 0, 0, 0]),
        6
    );
    let read = dispatch(&mut process, 0, [3, 0x6000, 8, 0, 0, 0]);
    assert_eq!(read, 8);
    let (_, bytes) = process.pending_write.take().unwrap();
    assert_eq!(bytes, b"sor\t: 0\n");
}

#[test]
fn open_errors_are_reported_without_state() {
    let mut process = process();
    set_path(&mut process, b"/proc/nope");
    assert_eq!(
        dispatch(&mut process, 257, [AT_FDCWD as usize, 0, 0, 0, 0, 0]),
        -(Errno::ENOENT.0 as isize)
    );
    set_path(&mut process, b"/proc/self/status");
    assert_eq!(
        dispatch(&mut process, 257, [AT_FDCWD as usize, 0, 1, 0, 0, 0]),
        -(Errno::EACCES.0 as isize)
    );
    set_path(&mut process, b"/proc/self/exe");
    assert_eq!(
        dispatch(&mut process, 257, [AT_FDCWD as usize, 0, 0, 0, 0, 0]),
        -(Errno::ELOOP.0 as isize)
    );
    assert_eq!(
        dispatch(&mut process, 3, [3, 0, 0, 0, 0, 0]),
        -(Errno::EBADF.0 as isize)
    );
}

#[test]
fn stat_queues_the_linux_layout() {
    let mut process = process();
    let size = proc_file_len(&process, b"/proc/self/status");
    set_path(&mut process, b"/proc/self/status");
    let bytes = stat_bytes(&mut process, 4, [0, 0x2000, 0, 0, 0, 0], 0x2000);
    assert_eq!(bytes.len(), stat_layout::STAT_SIZE);
    assert_eq!(u32_at(&bytes, offset_of!(AbiStat, st_mode)), 0o100444);
    assert_eq!(u64_at(&bytes, offset_of!(AbiStat, st_nlink)), 1);
    assert_eq!(u64_at(&bytes, offset_of!(AbiStat, st_size)), size);
    assert_eq!(
        u64_at(&bytes, offset_of!(AbiStat, st_blksize)),
        procfs::BLKSIZE
    );
    assert_eq!(
        u64_at(&bytes, offset_of!(AbiStat, st_blocks)),
        size.div_ceil(512)
    );
    assert!(u64_at(&bytes, offset_of!(AbiStat, st_ino)) != 0);
    assert_eq!(u32_at(&bytes, offset_of!(AbiStat, st_uid)), 0);
    assert_eq!(u32_at(&bytes, offset_of!(AbiStat, st_gid)), 0);
    assert_eq!(u64_at(&bytes, offset_of!(AbiStat, st_mtime)), 0);
}

#[test]
fn stat_follows_symlinks_and_accepts_relative_paths() {
    let mut process = process();
    process.set_identity(b"/hello", vec![b"/hello".to_vec()]);

    set_path(&mut process, b"/proc/self/exe");
    let link = stat_bytes(&mut process, 6, [0, 0x2100, 0, 0, 0, 0], 0x2100);
    assert_eq!(u32_at(&link, offset_of!(AbiStat, st_mode)), 0o120777);
    assert_eq!(u64_at(&link, offset_of!(AbiStat, st_size)), 6);

    set_path(&mut process, b"/proc/self/exe");
    assert_eq!(
        dispatch(&mut process, 4, [0, 0x2200, 0, 0, 0, 0]),
        -(Errno::ENOENT.0 as isize)
    );
    assert!(process.pending_write.is_none());

    set_path(&mut process, b"proc/self/status");
    let relative = stat_bytes(&mut process, 4, [0, 0x2300, 0, 0, 0, 0], 0x2300);
    assert_eq!(u32_at(&relative, offset_of!(AbiStat, st_mode)), 0o100444);

    set_path(&mut process, b"/proc/nope");
    assert_eq!(
        dispatch(&mut process, 4, [0, 0x2400, 0, 0, 0, 0]),
        -(Errno::ENOENT.0 as isize)
    );
}

#[test]
fn fstat_serves_standard_and_open_descriptors() {
    let mut process = process();
    let stdin = stat_bytes(&mut process, 5, [0, 0x3000, 0, 0, 0, 0], 0x3000);
    assert_eq!(u32_at(&stdin, offset_of!(AbiStat, st_mode)), 0o020666);
    assert_eq!(u64_at(&stdin, offset_of!(AbiStat, st_size)), 0);
    let stdout = stat_bytes(&mut process, 5, [1, 0x3100, 0, 0, 0, 0], 0x3100);
    assert_eq!(u32_at(&stdout, offset_of!(AbiStat, st_mode)), 0o020666);

    set_path(&mut process, b"/proc/cpuinfo");
    let fd = dispatch(&mut process, 2, [0, 0, 0, 0, 0, 0]);
    assert_eq!(fd, 3);
    let size = proc_file_len(&process, b"/proc/cpuinfo");
    let opened = stat_bytes(&mut process, 5, [fd as usize, 0x3200, 0, 0, 0, 0], 0x3200);
    assert_eq!(u32_at(&opened, offset_of!(AbiStat, st_mode)), 0o100444);
    assert_eq!(u64_at(&opened, offset_of!(AbiStat, st_size)), size);

    assert_eq!(
        dispatch(&mut process, 5, [99, 0x3300, 0, 0, 0, 0]),
        -(Errno::EBADF.0 as isize)
    );
}

#[test]
fn newfstatat_handles_dirfds_flags_and_empty_paths() {
    let mut process = process();
    set_path(&mut process, b"/proc/self/status");
    let absolute = stat_bytes(
        &mut process,
        262,
        [AT_FDCWD as usize, 0, 0x4000, 0, 0, 0],
        0x4000,
    );
    assert_eq!(u32_at(&absolute, offset_of!(AbiStat, st_mode)), 0o100444);

    set_path(&mut process, b"/proc/self");
    let dirfd = dispatch(
        &mut process,
        257,
        [AT_FDCWD as usize, 0, O_DIRECTORY as usize, 0, 0, 0],
    );
    assert_eq!(dirfd, 3);
    set_path(&mut process, b"status");
    let relative = stat_bytes(
        &mut process,
        262,
        [dirfd as usize, 0, 0x4100, 0, 0, 0],
        0x4100,
    );
    assert_eq!(u32_at(&relative, offset_of!(AbiStat, st_mode)), 0o100444);
    set_path(&mut process, b".");
    let child_dir = stat_bytes(
        &mut process,
        262,
        [dirfd as usize, 0, 0x4200, 0, 0, 0],
        0x4200,
    );
    assert_eq!(u32_at(&child_dir, offset_of!(AbiStat, st_mode)), 0o040555);

    process.set_identity(b"/hello", vec![b"/hello".to_vec()]);
    set_path(&mut process, b"/proc/self/exe");
    let link = stat_bytes(
        &mut process,
        262,
        [
            AT_FDCWD as usize,
            0,
            0x4300,
            AT_SYMLINK_NOFOLLOW as usize,
            0,
            0,
        ],
        0x4300,
    );
    assert_eq!(u32_at(&link, offset_of!(AbiStat, st_mode)), 0o120777);

    set_path(&mut process, b"");
    let empty = stat_bytes(
        &mut process,
        262,
        [1, 0, 0x4400, AT_EMPTY_PATH as usize, 0, 0],
        0x4400,
    );
    assert_eq!(u32_at(&empty, offset_of!(AbiStat, st_mode)), 0o020666);

    set_path(&mut process, b"");
    assert_eq!(
        dispatch(&mut process, 262, [AT_FDCWD as usize, 0, 0x4500, 0, 0, 0]),
        -(Errno::ENOENT.0 as isize)
    );
    set_path(&mut process, b"status");
    assert_eq!(
        dispatch(&mut process, 262, [42, 0, 0x4600, 0, 0, 0]),
        -(Errno::EBADF.0 as isize)
    );
    assert_eq!(
        dispatch(
            &mut process,
            262,
            [AT_FDCWD as usize, 0, 0x4700, 0x8000, 0, 0]
        ),
        -(Errno::EINVAL.0 as isize)
    );
}

#[test]
fn statx_queues_the_basic_layout() {
    let mut process = process();
    let size = proc_file_len(&process, b"/proc/self/status");
    set_path(&mut process, b"/proc/self/status");
    let bytes = stat_bytes(
        &mut process,
        332,
        [AT_FDCWD as usize, 0, 0, 0x7ff, 0x5000, 0],
        0x5000,
    );
    assert_eq!(bytes.len(), stat_layout::STATX_SIZE);
    assert_eq!(
        u32_at(&bytes, offset_of!(AbiStatx, stx_mask)),
        stat_layout::STATX_BASIC_STATS
    );
    assert_eq!(
        u32_at(&bytes, offset_of!(AbiStatx, stx_blksize)),
        procfs::BLKSIZE as u32
    );
    assert_eq!(u32_at(&bytes, offset_of!(AbiStatx, stx_nlink)), 1);
    assert_eq!(u32_at(&bytes, offset_of!(AbiStatx, stx_mode)), 0o100444);
    assert_eq!(u64_at(&bytes, offset_of!(AbiStatx, stx_size)), size);
    assert_eq!(
        u64_at(&bytes, offset_of!(AbiStatx, stx_blocks)),
        size.div_ceil(512)
    );

    set_path(&mut process, b"");
    let empty = stat_bytes(
        &mut process,
        332,
        [0, 0, AT_EMPTY_PATH as usize, 0x7ff, 0x5100, 0],
        0x5100,
    );
    assert_eq!(u32_at(&empty, offset_of!(AbiStatx, stx_mode)), 0o020666);

    set_path(&mut process, b"/proc/self/status");
    assert_eq!(
        dispatch(
            &mut process,
            332,
            [AT_FDCWD as usize, 0, 0x8000, 0x7ff, 0x5200, 0]
        ),
        -(Errno::EINVAL.0 as isize)
    );
}

#[test]
fn access_uses_the_synthetic_mode_bits() {
    let mut process = process();
    set_path(&mut process, b"/proc/self/status");
    assert_eq!(dispatch(&mut process, 21, [0, 0, 0, 0, 0, 0]), 0);
    assert_eq!(dispatch(&mut process, 21, [0, 4, 0, 0, 0, 0]), 0);
    assert_eq!(
        dispatch(&mut process, 21, [0, 2, 0, 0, 0, 0]),
        -(Errno::EACCES.0 as isize)
    );
    assert_eq!(
        dispatch(&mut process, 21, [0, 1, 0, 0, 0, 0]),
        -(Errno::EACCES.0 as isize)
    );
    set_path(&mut process, b"/proc/self");
    assert_eq!(dispatch(&mut process, 21, [0, 1, 0, 0, 0, 0]), 0);
    assert_eq!(
        dispatch(&mut process, 21, [0, 2, 0, 0, 0, 0]),
        -(Errno::EACCES.0 as isize)
    );
    set_path(&mut process, b"/proc/nope");
    assert_eq!(
        dispatch(&mut process, 21, [0, 0, 0, 0, 0, 0]),
        -(Errno::ENOENT.0 as isize)
    );
    set_path(&mut process, b"/proc/self/status");
    assert_eq!(
        dispatch(&mut process, 21, [0, 0o10, 0, 0, 0, 0]),
        -(Errno::EINVAL.0 as isize)
    );
}

#[test]
fn metadata_mutators_report_a_read_only_tree() {
    let mut process = process();

    set_path(&mut process, b"/proc/self/status");
    assert_eq!(
        dispatch(&mut process, 268, [AT_FDCWD as usize, 0, 0o444, 0, 0, 0]),
        -(Errno::EROFS.0 as isize)
    );
    set_path(&mut process, b"/proc/nope");
    assert_eq!(
        dispatch(&mut process, 268, [AT_FDCWD as usize, 0, 0o444, 0, 0, 0]),
        -(Errno::ENOENT.0 as isize)
    );
    set_path(&mut process, b"/proc/self/status");
    assert_eq!(
        dispatch(
            &mut process,
            268,
            [AT_FDCWD as usize, 0, 0o444, 0x8000, 0, 0]
        ),
        -(Errno::EINVAL.0 as isize)
    );

    set_path(&mut process, b"/proc/self/status");
    assert_eq!(
        dispatch(&mut process, 260, [AT_FDCWD as usize, 0, 0, 0, 0, 0]),
        -(Errno::EROFS.0 as isize)
    );
    set_path(&mut process, b"/proc/self/status");
    assert_eq!(
        dispatch(&mut process, 280, [AT_FDCWD as usize, 0, 0, 0, 0, 0]),
        -(Errno::EROFS.0 as isize)
    );

    set_path(&mut process, b"/proc/cpuinfo");
    assert_eq!(
        dispatch(&mut process, 76, [0, 0, 0, 0, 0, 0]),
        -(Errno::EROFS.0 as isize)
    );
    set_path(&mut process, b"/proc/self");
    assert_eq!(
        dispatch(&mut process, 76, [0, 0, 0, 0, 0, 0]),
        -(Errno::EISDIR.0 as isize)
    );
    set_path(&mut process, b"/proc/nope");
    assert_eq!(
        dispatch(&mut process, 76, [0, 0, 0, 0, 0, 0]),
        -(Errno::ENOENT.0 as isize)
    );
    set_path(&mut process, b"/proc/cpuinfo");
    assert_eq!(
        dispatch(&mut process, 76, [0, usize::MAX, 0, 0, 0, 0]),
        -(Errno::EINVAL.0 as isize)
    );
}

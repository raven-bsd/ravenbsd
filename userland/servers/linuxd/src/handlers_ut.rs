// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Handler tests: every B20 syscall, the error paths and the dispatcher counters.

use raven_abi::errno::Errno;
use raven_abi::types::Handle;
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
            "arch_prctl",
            "brk",
            "exit_group",
            "getpid",
            "mmap",
            "rt_sigprocmask",
            "uname",
            "write"
        ]
    );
    let _ = dispatcher.dispatch(&mut process, &call(1, [1, 0, 0, 0, 0, 0]));
    let _ = dispatcher.dispatch(&mut process, &call(1, [1, 0, 0, 0, 0, 0]));
    let stats = dispatcher.stats();
    let write = stats.iter().find(|(name, _, _)| *name == "write");
    assert_eq!(write.map(|(_, calls, _)| *calls), Some(2));
}

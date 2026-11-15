// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Handler tests: every B20 syscall, the error paths and the dispatcher counters.

use raven_abi::errno::Errno;
use raven_abi::types::Handle;
use raven_linux_dispatch::SyscallArgs;

use super::*;

fn process() -> LinuxProcess {
    LinuxProcess::new(7, 700, Handle(9), 0x4000, 0x5000, 0x8000, 0x9000)
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
        dispatcher.dispatch(&mut process, &call(110, [0; 6])),
        -(Errno::ENOSYS.0 as isize)
    );
    let hits = dispatcher.unimplemented_hits();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].0, 110);
    assert_eq!(hits[0].1, Some("getppid"));
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
            "clone",
            "close",
            "execve",
            "exit_group",
            "getdents64",
            "getpid",
            "gettid",
            "lseek",
            "mmap",
            "open",
            "openat",
            "prctl",
            "read",
            "readlink",
            "readlinkat",
            "rt_sigprocmask",
            "set_robust_list",
            "set_tid_address",
            "uname",
            "wait4",
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
fn gettid_returns_the_kernel_thread_id() {
    let mut process = process();
    assert_eq!(dispatch(&mut process, 186, [0; 6]), 700);
}

#[test]
fn set_tid_address_records_the_word_and_returns_the_tid() {
    let mut process = process();
    assert_eq!(dispatch(&mut process, 218, [0x2345, 0, 0, 0, 0, 0]), 700);
    assert_eq!(process.clear_tid, 0x2345);
    assert_eq!(dispatch(&mut process, 218, [0, 0, 0, 0, 0, 0]), 700);
    assert_eq!(process.clear_tid, 0);
}

#[test]
fn set_robust_list_checks_the_head_size() {
    let mut process = process();
    assert_eq!(dispatch(&mut process, 273, [0x2800, 24, 0, 0, 0, 0]), 0);
    assert_eq!(process.robust_list, 0x2800);
    assert_eq!(process.robust_list_len, 24);
    assert_eq!(
        dispatch(&mut process, 273, [0x2900, 16, 0, 0, 0, 0]),
        -(Errno::EINVAL.0 as isize)
    );
    assert_eq!(process.robust_list, 0x2800);
}

#[test]
fn prctl_sets_and_gets_the_task_name() {
    let mut process = process();
    process.user_buffer.extend_from_slice(b"clone-fixture\0zz");
    assert_eq!(
        dispatch(
            &mut process,
            157,
            [PR_SET_NAME as usize, 0x3000, 0, 0, 0, 0]
        ),
        0
    );
    assert_eq!(process.comm, b"clone-fixture");
    assert_eq!(
        dispatch(
            &mut process,
            157,
            [PR_GET_NAME as usize, 0x3100, 0, 0, 0, 0]
        ),
        0
    );
    let (address, bytes) = process.pending_write.take().expect("a name copy-out");
    assert_eq!(address, 0x3100);
    assert_eq!(bytes.len(), 16);
    assert_eq!(&bytes[..13], b"clone-fixture");
    assert_eq!(&bytes[13..], &[0, 0, 0]);
}

#[test]
fn prctl_switches_the_process_attributes() {
    let mut process = process();
    assert_eq!(
        dispatch(
            &mut process,
            157,
            [PR_SET_PDEATHSIG as usize, 9, 0, 0, 0, 0]
        ),
        0
    );
    assert_eq!(process.pdeathsig, 9);
    assert_eq!(
        dispatch(
            &mut process,
            157,
            [PR_GET_PDEATHSIG as usize, 0x3200, 0, 0, 0, 0]
        ),
        0
    );
    assert_eq!(
        process.pending_write.take().expect("a signal copy-out").1,
        9u32.to_le_bytes()
    );
    assert_eq!(
        dispatch(&mut process, 157, [PR_GET_DUMPABLE as usize, 0, 0, 0, 0, 0]),
        1
    );
    assert_eq!(
        dispatch(&mut process, 157, [PR_SET_DUMPABLE as usize, 0, 0, 0, 0, 0]),
        0
    );
    assert_eq!(
        dispatch(&mut process, 157, [PR_GET_DUMPABLE as usize, 0, 0, 0, 0, 0]),
        0
    );
    assert_eq!(
        dispatch(
            &mut process,
            157,
            [PR_GET_NO_NEW_PRIVS as usize, 0, 0, 0, 0, 0]
        ),
        0
    );
    assert_eq!(
        dispatch(
            &mut process,
            157,
            [PR_SET_NO_NEW_PRIVS as usize, 1, 0, 0, 0, 0]
        ),
        0
    );
    assert_eq!(
        dispatch(
            &mut process,
            157,
            [PR_GET_NO_NEW_PRIVS as usize, 0, 0, 0, 0, 0]
        ),
        1
    );
    assert_eq!(
        dispatch(
            &mut process,
            157,
            [PR_SET_THP_DISABLE as usize, 1, 0, 0, 0, 0]
        ),
        0
    );
    assert_eq!(
        dispatch(
            &mut process,
            157,
            [PR_GET_THP_DISABLE as usize, 0, 0, 0, 0, 0]
        ),
        1
    );
    assert_eq!(
        dispatch(
            &mut process,
            157,
            [PR_SET_CHILD_SUBREAPER as usize, 1, 0, 0, 0, 0]
        ),
        0
    );
    assert_eq!(
        dispatch(
            &mut process,
            157,
            [PR_GET_CHILD_SUBREAPER as usize, 0x3300, 0, 0, 0, 0]
        ),
        0
    );
    assert_eq!(
        process
            .pending_write
            .take()
            .expect("a subreaper copy-out")
            .1,
        1u32.to_le_bytes()
    );
}

#[test]
fn prctl_rejects_unknown_options_and_bad_values() {
    let mut process = process();
    assert_eq!(
        dispatch(&mut process, 157, [0x1234, 0, 0, 0, 0, 0]),
        -(Errno::EINVAL.0 as isize)
    );
    assert_eq!(
        dispatch(
            &mut process,
            157,
            [PR_SET_NO_NEW_PRIVS as usize, 0, 0, 0, 0, 0]
        ),
        -(Errno::EINVAL.0 as isize)
    );
    assert!(!process.no_new_privs);
}

#[test]
fn arch_prctl_rejects_the_gs_operations() {
    let mut process = process();
    assert_eq!(
        dispatch(
            &mut process,
            158,
            [ARCH_SET_GS as usize, 0x7000, 0, 0, 0, 0]
        ),
        -(Errno::ENOSYS.0 as isize)
    );
    assert_eq!(
        dispatch(
            &mut process,
            158,
            [ARCH_GET_GS as usize, 0x7000, 0, 0, 0, 0]
        ),
        -(Errno::ENOSYS.0 as isize)
    );
}

#[test]
fn clone_validates_flags_and_reports_the_missing_native_frame() {
    let mut process = process();
    let thread_flags = clone_flags::CLONE_VM
        | clone_flags::CLONE_FS
        | clone_flags::CLONE_FILES
        | clone_flags::CLONE_SIGHAND
        | clone_flags::CLONE_THREAD
        | clone_flags::CLONE_SYSVSEM
        | clone_flags::CLONE_SETTLS
        | clone_flags::CLONE_PARENT_SETTID
        | clone_flags::CLONE_CHILD_CLEARTID;
    assert_eq!(
        dispatch(
            &mut process,
            56,
            [thread_flags as usize, 0x7000, 0, 0, 0x7100, 0]
        ),
        -(Errno::ENOSYS.0 as isize)
    );
    assert_eq!(
        dispatch(&mut process, 56, [0, 0, 0, 0, 0, 0]),
        -(Errno::ENOSYS.0 as isize)
    );
    // `clone(SIGCHLD, ...)` is the fork-like form: the low byte is a signal, not a flag.
    assert_eq!(
        dispatch(&mut process, 56, [17, 0, 0, 0, 0, 0]),
        -(Errno::ENOSYS.0 as isize)
    );
    assert_eq!(
        dispatch(&mut process, 56, [65, 0, 0, 0, 0, 0]),
        -(Errno::EINVAL.0 as isize)
    );
    assert_eq!(
        dispatch(
            &mut process,
            56,
            [clone_flags::CLONE_THREAD as usize, 0, 0, 0, 0, 0]
        ),
        -(Errno::EINVAL.0 as isize)
    );
    assert_eq!(
        dispatch(
            &mut process,
            56,
            [clone_flags::CLONE_THREAD as usize, 0x7004, 0, 0, 0, 0]
        ),
        -(Errno::EINVAL.0 as isize)
    );
    assert_eq!(
        dispatch(&mut process, 56, [0xdead_beef, 0, 0, 0, 0, 0]),
        -(Errno::EINVAL.0 as isize)
    );
}

#[test]
fn execve_queues_the_path_and_argument_arrays() {
    let mut process = process();
    set_path(&mut process, b"/exec");
    assert_eq!(
        dispatch(&mut process, 59, [0x1000, 0x2000, 0x3000, 0, 0, 0]),
        0
    );
    let request = process.pending_exec.take().expect("a queued exec");
    assert_eq!(request.path, b"/exec");
    assert_eq!(request.argv, 0x2000);
    assert_eq!(request.envp, 0x3000);

    set_path(&mut process, b"relative");
    assert_eq!(
        dispatch(&mut process, 59, [0x1000, 0, 0, 0, 0, 0]),
        -(Errno::ENOENT.0 as isize)
    );
    process.user_buffer.clear();
    process.user_buffer.push(0);
    assert_eq!(
        dispatch(&mut process, 59, [0x1000, 0, 0, 0, 0, 0]),
        -(Errno::ENOENT.0 as isize)
    );
    process.user_buffer.clear();
    process.user_buffer.resize(128, b'a');
    assert_eq!(
        dispatch(&mut process, 59, [0x1000, 0, 0, 0, 0, 0]),
        -(Errno::ENAMETOOLONG.0 as isize)
    );
    assert!(process.pending_exec.is_none());
}

#[test]
fn wait4_validates_options_and_reports_no_children() {
    let mut process = process();
    assert_eq!(
        dispatch(&mut process, 61, [usize::MAX, 0, 0, 0, 0, 0]),
        -(Errno::ECHILD.0 as isize)
    );
    assert_eq!(
        dispatch(&mut process, 61, [usize::MAX, 0, WNOHANG as usize, 0, 0, 0]),
        -(Errno::ECHILD.0 as isize)
    );
    assert_eq!(
        dispatch(&mut process, 61, [usize::MAX, 0, 2, 0, 0, 0]),
        -(Errno::EINVAL.0 as isize)
    );
}

#[test]
fn exit_group_queues_the_clear_tid_word() {
    let mut process = process();
    process.clear_tid = 0x3400;
    assert_eq!(dispatch(&mut process, 231, [3, 0, 0, 0, 0, 0]), 0);
    assert_eq!(process.exit, Some(3));
    let (address, bytes) = process.pending_write.take().expect("a clear-tid write");
    assert_eq!(address, 0x3400);
    assert_eq!(bytes, 0u32.to_le_bytes());
}

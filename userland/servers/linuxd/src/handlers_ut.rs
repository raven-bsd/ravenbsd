// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Handler tests: every B20 syscall, the error paths and the dispatcher counters.

use raven_abi::types::Handle;
use raven_linux_abi::flags::clone as clone_flags;
use raven_linux_dispatch::SyscallArgs;
use raven_proto_linux::LinuxFrame;

use crate::files::O_CLOEXEC;
use crate::files::O_RDWR;

use super::*;

fn process() -> LinuxProcess {
    LinuxProcess::new(7, 7, Handle(1), Handle(9), 0x4000, 0x5000, 0x8000, 0x9000)
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
        -(raven_linux_abi::errno::EBADF as isize)
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
        -(raven_linux_abi::errno::ENOMEM as isize)
    );
    assert_eq!(process.mmap_next, 0x9000);
}

#[test]
fn file_backed_and_executable_mmap_are_refused() {
    let mut process = process();
    assert_eq!(
        dispatch(&mut process, 9, [0, 4096, 3, 0, 3, 0]),
        -(raven_linux_abi::errno::ENOSYS as isize)
    );
    assert_eq!(
        dispatch(&mut process, 9, [0, 4096, 5, 0x22, usize::MAX, 0]),
        -(raven_linux_abi::errno::ENOSYS as isize)
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
        -(raven_linux_abi::errno::EINVAL as isize)
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
        -(raven_linux_abi::errno::EINVAL as isize)
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
fn process_identity_handlers_record_the_linux_state() {
    let mut process = process();
    assert_eq!(dispatch(&mut process, 186, [0; 6]), 7);
    assert_eq!(dispatch(&mut process, 218, [0x3000, 0, 0, 0, 0, 0]), 7);
    assert_eq!(process.clear_tid, 0x3000);
    assert_eq!(dispatch(&mut process, 273, [0x4000, 24, 0, 0, 0, 0]), 0);
    assert_eq!(process.robust_list, 0x4000);
    assert_eq!(process.robust_list_len, 24);
    assert_eq!(
        dispatch(&mut process, 273, [0x4000, 16, 0, 0, 0, 0]),
        -(raven_linux_abi::errno::EINVAL as isize)
    );
    process.user_buffer.extend_from_slice(b"worker\0");
    assert_eq!(dispatch(&mut process, 157, [15, 0x5000, 0, 0, 0, 0]), 0);
    assert_eq!(process.comm(), b"worker");
    assert_eq!(dispatch(&mut process, 157, [16, 0x5000, 0, 0, 0, 0]), 0);
    let (address, bytes) = process.pending_write.take().expect("the name copy-out");
    assert_eq!(address, 0x5000);
    assert_eq!(bytes, b"worker\0\0\0\0\0\0\0\0\0\0");
}

fn clone_frame() -> LinuxFrame {
    let mut gprs = [0u64; raven_proto_linux::FRAME_GPRS];
    gprs[0] = 56;
    gprs[1] = 0x1234;
    LinuxFrame {
        rip: 0x40_1234,
        rsp: 0x7fff_0000,
        rflags: 0x202,
        fs_base: 0x7000,
        gprs,
    }
}

#[test]
fn clone_queues_the_frame_and_arguments_for_the_trap_loop() {
    let mut process = process();
    process.frame = Some(clone_frame());
    let flags = clone_flags::CLONE_VM | clone_flags::CLONE_THREAD | clone_flags::CLONE_SIGHAND;
    assert_eq!(
        dispatch(&mut process, 56, [flags as usize, 0x9000, 0, 0, 0x7000, 0]),
        0
    );
    let request = process.pending_clone.take().expect("a queued clone");
    assert_eq!(request.flags, flags);
    assert_eq!(request.child_stack, 0x9000);
    assert_eq!(request.tls, 0x7000);
    assert_eq!(request.frame.rip, 0x40_1234);
    assert_eq!(request.frame.rsp, 0x7fff_0000);
}

#[test]
fn clone_without_a_frame_or_with_bad_flags_is_rejected() {
    let mut process = process();
    assert_eq!(
        dispatch(&mut process, 56, [17, 0, 0, 0, 0, 0]),
        -(raven_linux_abi::errno::ENOSYS as isize)
    );
    process.frame = Some(clone_frame());
    assert_eq!(
        dispatch(&mut process, 56, [0xdead_beef, 0, 0, 0, 0, 0]),
        -(raven_linux_abi::errno::EINVAL as isize)
    );
    assert!(process.pending_clone.is_none());
}

#[test]
fn an_unknown_number_is_enosys_and_counted() {
    let mut process = process();
    let mut dispatcher = dispatcher();
    assert_eq!(
        dispatcher.dispatch(&mut process, &call(60, [0; 6])),
        -(raven_linux_abi::errno::ENOSYS as isize)
    );
    let hits = dispatcher.unimplemented_hits();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].0, 60);
    assert_eq!(hits[0].1, Some("exit"));
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
            "dup3",
            "exit_group",
            "fcntl",
            "getdents64",
            "getpid",
            "gettid",
            "lseek",
            "mmap",
            "open",
            "openat",
            "prctl",
            "pread64",
            "pwrite64",
            "read",
            "readlink",
            "readlinkat",
            "rt_sigprocmask",
            "set_robust_list",
            "set_tid_address",
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
fn openat_queues_a_remote_file_with_a_file_scheme() {
    let mut process = process();
    set_path(&mut process, b"/tmp/b23.txt");
    let flags = O_RDWR | crate::files::O_CREAT;
    assert_eq!(
        dispatch(
            &mut process,
            257,
            [AT_FDCWD as usize, 0, flags as usize, 0, 0, 0]
        ),
        0
    );
    assert_eq!(
        process.pending_file.take(),
        Some(FileOp::Open {
            scheme: b"file:/tmp/b23.txt".to_vec(),
            path: b"/tmp/b23.txt".to_vec(),
            flags,
        })
    );
}

#[test]
fn remote_descriptors_queue_read_write_seek_and_close() {
    let mut process = process();
    let fd = process.open_remote(b"/tmp/b23.txt".to_vec(), Handle(5), O_RDWR);
    assert_eq!(fd, 3);
    process.user_buffer.extend_from_slice(b"abc");
    assert_eq!(
        dispatch(&mut process, 1, [fd as usize, 0x1000, 3, 0, 0, 0]),
        3
    );
    assert_eq!(
        process.pending_file.take(),
        Some(FileOp::Write {
            fd,
            data: b"abc".to_vec(),
        })
    );
    assert_eq!(
        dispatch(&mut process, 0, [fd as usize, 0x2000, 16, 0, 0, 0]),
        16
    );
    assert_eq!(
        process.pending_file.take(),
        Some(FileOp::Read {
            fd,
            address: 0x2000,
            len: 16,
        })
    );
    assert_eq!(
        dispatch(&mut process, 17, [fd as usize, 0x3000, 8, 4, 0, 0]),
        8
    );
    assert_eq!(
        process.pending_file.take(),
        Some(FileOp::Pread {
            fd,
            address: 0x3000,
            len: 8,
            offset: 4,
        })
    );
    process.user_buffer.clear();
    process.user_buffer.extend_from_slice(b"xy");
    assert_eq!(
        dispatch(&mut process, 18, [fd as usize, 0x4000, 2, 9, 0, 0]),
        2
    );
    assert_eq!(
        process.pending_file.take(),
        Some(FileOp::Pwrite {
            fd,
            data: b"xy".to_vec(),
            offset: 9,
        })
    );
    assert_eq!(
        dispatch(
            &mut process,
            8,
            [fd as usize, 6, SEEK_SET as usize, 0, 0, 0]
        ),
        0
    );
    assert_eq!(
        process.pending_file.take(),
        Some(FileOp::Seek {
            fd,
            offset: 6,
            whence: SEEK_SET,
        })
    );
    assert_eq!(dispatch(&mut process, 3, [fd as usize, 0, 0, 0, 0, 0]), 0);
    assert_eq!(process.pending_file.take(), Some(FileOp::Close { fd }));
}

#[test]
fn fcntl_and_dup3_queue_descriptor_operations() {
    let mut process = process();
    let fd = process.open_remote(b"/tmp/b23.txt".to_vec(), Handle(5), O_RDWR);
    assert_eq!(
        dispatch(
            &mut process,
            72,
            [fd as usize, F_GETFL as usize, 0, 0, 0, 0]
        ),
        O_RDWR as isize
    );
    assert_eq!(
        dispatch(
            &mut process,
            72,
            [fd as usize, F_SETFD as usize, 1, 0, 0, 0]
        ),
        0
    );
    assert_eq!(
        dispatch(
            &mut process,
            72,
            [fd as usize, F_GETFD as usize, 0, 0, 0, 0]
        ),
        FD_CLOEXEC as isize
    );
    assert_eq!(
        dispatch(
            &mut process,
            72,
            [fd as usize, F_DUPFD as usize, 10, 0, 0, 0]
        ),
        0
    );
    assert_eq!(
        process.pending_file.take(),
        Some(FileOp::FcntlDup {
            fd,
            min: 10,
            cloexec: false,
        })
    );
    assert_eq!(dispatch(&mut process, 292, [fd as usize, 9, 0, 0, 0, 0]), 0);
    assert_eq!(
        process.pending_file.take(),
        Some(FileOp::Dup3 {
            oldfd: fd,
            newfd: 9,
            cloexec: false,
        })
    );
    assert_eq!(
        dispatch(&mut process, 292, [fd as usize, fd as usize, 0, 0, 0, 0]),
        -(raven_linux_abi::errno::EINVAL as isize)
    );
}

#[test]
fn open_remote_records_cloexec_as_a_descriptor_flag() {
    let mut process = process();
    let fd = process.open_remote(b"/tmp/b23.txt".to_vec(), Handle(5), O_RDWR | O_CLOEXEC);
    let file = process.file(fd).expect("the remote descriptor");
    assert_eq!(file.fd_flags, FD_CLOEXEC);
    assert_eq!(file.flags, O_RDWR);
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
        -(raven_linux_abi::errno::EISDIR as isize)
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
        -(raven_linux_abi::errno::ENOENT as isize)
    );
    set_path(&mut process, b"/proc/self/status");
    assert_eq!(
        dispatch(&mut process, 257, [AT_FDCWD as usize, 0, 1, 0, 0, 0]),
        -(raven_linux_abi::errno::EROFS as isize)
    );
    set_path(&mut process, b"/proc/self/exe");
    assert_eq!(
        dispatch(&mut process, 257, [AT_FDCWD as usize, 0, 0, 0, 0, 0]),
        -(raven_linux_abi::errno::ELOOP as isize)
    );
    assert_eq!(
        dispatch(&mut process, 3, [3, 0, 0, 0, 0, 0]),
        -(raven_linux_abi::errno::EBADF as isize)
    );
}

#[test]
fn boundary_errors_are_linux_numbers_not_native_numbers() {
    // The native domain is FreeBSD-valued (RAVEN-ABI contract); the Linux process must observe
    // Linux numbers. These two rows differ between the domains, so an unchanged passthrough of
    // the native value is caught here, not merely by a source-text scan.
    assert_eq!(raven_abi::errno::Errno::ENAMETOOLONG.0, 63);
    assert_eq!(
        crate::linux_error(raven_abi::errno::Errno::ENAMETOOLONG),
        -36
    );
    assert_eq!(raven_abi::errno::Errno::ENOSYS.0, 78);
    assert_eq!(crate::linux_error(raven_abi::errno::Errno::ENOSYS), -38);
    assert_eq!(raven_abi::errno::Errno::EAGAIN.0, 35);
    assert_eq!(crate::linux_error(raven_abi::errno::Errno::EAGAIN), -11);
}

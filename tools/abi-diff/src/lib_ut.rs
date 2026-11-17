// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the ABI parser and diff.

use super::*;

const SRC: &str = r#"
    /// `exit(code: i32) -> !` terminates the calling process.
    EXIT = 0x01;
    /// `yield() -> 0` gives up the CPU.
    YIELD = 0x02;
    /// `clock_get(clock_id: u32, out: *mut Timespec) -> 0`. See [`crate::types::clock`].
    CLOCK_GET = 0x04;

    /// `trace_set(id: u64) -> 0`.
    ///
    /// Sets the calling thread's current trace id.
    pub const TRACE_SET: usize = 0x0B;

    /// `record_lock(file: Handle, cmd: u32, lock: *mut RecordLock) -> 0`.
    pub const RECORD_LOCK: usize = 0x0C;

    /// `THREAD_CREATE` flag: the sixth argument points to a [`ThreadCreateEx`].
    pub const THREAD_CREATE_EX: usize = 1;

    /// Extended `THREAD_CREATE` payload (PG-2).
    #[repr(C)]
    #[derive(Clone, Copy, Debug)]
    pub struct ThreadCreateEx {
        pub tp: usize,
        pub tid_out: usize,
        pub clear_tid_addr: usize,
    }

    const _: () = {
        assert!(core::mem::size_of::<ThreadCreateEx>() == 24);
        assert!(core::mem::align_of::<ThreadCreateEx>() == 8);
    };
"#;

#[test]
fn parses_syscalls_and_raw_consts() {
    let syscalls = parse_syscalls(SRC);
    assert_eq!(syscalls.len(), 5);
    assert_eq!(
        syscalls[0],
        Syscall {
            number: 0x01,
            name: "EXIT".into(),
            argc: 1,
            ret: "!".into(),
        }
    );
    assert_eq!(syscalls[1].name, "YIELD");
    assert_eq!(syscalls[1].argc, 0);
    assert_eq!(syscalls[2].argc, 2);
    assert_eq!(syscalls[3].name, "TRACE_SET");
    assert_eq!(syscalls[3].number, 0x0B);
    assert_eq!(syscalls[4].name, "RECORD_LOCK");
    assert_eq!(syscalls[4].number, 0x0C);
}

#[test]
fn ignores_non_syscall_constants() {
    // THREAD_CREATE_EX is a flag (decimal, not hex) and must not be a syscall.
    let syscalls = parse_syscalls(SRC);
    assert!(!syscalls.iter().any(|s| s.name == "THREAD_CREATE_EX"));
}

#[test]
fn parses_struct_layout() {
    let structs = parse_structs(SRC);
    assert_eq!(structs.len(), 1);
    assert_eq!(structs[0].name, "ThreadCreateEx");
    assert_eq!(structs[0].size, 24);
    assert_eq!(structs[0].align, 8);
}

#[test]
fn signature_return_strips_whitespace() {
    let (argc, ret) = parse_signature("`recv() -> (Handle, packet)`");
    assert_eq!(argc, 0);
    assert_eq!(ret, "(Handle,packet)");
}

#[test]
fn render_is_deterministic_and_sorted() {
    let abi = Abi {
        syscalls: vec![
            Syscall {
                number: 0x04,
                name: "CLOCK_GET".into(),
                argc: 2,
                ret: "0".into(),
            },
            Syscall {
                number: 0x01,
                name: "EXIT".into(),
                argc: 1,
                ret: "!".into(),
            },
        ],
        structs: vec![Struct {
            name: "ThreadCreateEx".into(),
            size: 24,
            align: 8,
        }],
    };
    let rendered = render(&abi);
    assert_eq!(
        rendered,
        "SYSCALL 0x01 EXIT argc=1 ret=!\nSYSCALL 0x04 CLOCK_GET argc=2 ret=0\nSTRUCT ThreadCreateEx size=24 align=8\n"
    );
}

#[test]
fn diff_detects_add_remove_and_change() {
    let base = "SYSCALL 0x01 EXIT argc=1 ret=!\nSTRUCT Timespec size=16 align=8\n";
    let head = "SYSCALL 0x01 EXIT argc=1 ret=!\nSYSCALL 0x02 YIELD argc=0 ret=0\nSTRUCT Timespec size=24 align=8\n";
    let report = diff(base, head);
    assert_eq!(report.lines.len(), 2); // ADDED YIELD + CHANGED Timespec
    assert!(report.breaking);
}

#[test]
fn diff_addition_only_is_not_breaking() {
    let base = "SYSCALL 0x01 EXIT argc=1 ret=!\n";
    let head = "SYSCALL 0x01 EXIT argc=1 ret=!\nSYSCALL 0x02 YIELD argc=0 ret=0\n";
    let report = diff(base, head);
    assert!(!report.breaking);
}

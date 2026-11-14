// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `signalfd` layout and the `PG-7` contract.

use super::*;
use crate::seams::Handle;
use crate::testutil::{FakeFdTable, FakeSignal};

const SIGNALFD_H: &str = include_str!("../../../include/sys/signalfd.h");

fn field_size(kind: &str) -> Option<usize> {
    match kind {
        "uint32_t" | "int32_t" => Some(4),
        "uint64_t" => Some(8),
        "uint16_t" => Some(2),
        "uint8_t" => Some(1),
        _ => None,
    }
}

fn align_up(value: usize, align: usize) -> usize {
    if align == 0 {
        return value;
    }
    value.div_ceil(align).saturating_mul(align)
}

#[test]
fn header_layout_matches_the_rust_struct() {
    let start = SIGNALFD_H.find("struct signalfd_siginfo {").unwrap();
    let body = &SIGNALFD_H[start..];
    let mut offset = 0usize;
    let mut seen = 0usize;
    for line in body.lines().skip(1) {
        let line = line.trim();
        if line.starts_with('}') {
            break;
        }
        if line.is_empty() || line.starts_with("/*") || line.starts_with('*') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(kind) = parts.next() else { continue };
        let Some(name_field) = parts.next() else {
            continue;
        };
        let name = name_field.trim_end_matches(';');
        if name.starts_with("__pad[") {
            let rust = core::mem::offset_of!(SignalfdSiginfo, ssi_pad);
            assert_eq!(rust, offset, "trailing padding");
            seen = seen.saturating_add(1);
            break;
        }
        let Some(size) = field_size(kind) else {
            panic!("unknown header field type {kind}");
        };
        offset = align_up(offset, size);
        let rust = match name {
            "ssi_signo" => core::mem::offset_of!(SignalfdSiginfo, ssi_signo),
            "ssi_errno" => core::mem::offset_of!(SignalfdSiginfo, ssi_errno),
            "ssi_code" => core::mem::offset_of!(SignalfdSiginfo, ssi_code),
            "ssi_pid" => core::mem::offset_of!(SignalfdSiginfo, ssi_pid),
            "ssi_uid" => core::mem::offset_of!(SignalfdSiginfo, ssi_uid),
            "ssi_fd" => core::mem::offset_of!(SignalfdSiginfo, ssi_fd),
            "ssi_tid" => core::mem::offset_of!(SignalfdSiginfo, ssi_tid),
            "ssi_band" => core::mem::offset_of!(SignalfdSiginfo, ssi_band),
            "ssi_overrun" => core::mem::offset_of!(SignalfdSiginfo, ssi_overrun),
            "ssi_trapno" => core::mem::offset_of!(SignalfdSiginfo, ssi_trapno),
            "ssi_status" => core::mem::offset_of!(SignalfdSiginfo, ssi_status),
            "ssi_int" => core::mem::offset_of!(SignalfdSiginfo, ssi_int),
            "ssi_ptr" => core::mem::offset_of!(SignalfdSiginfo, ssi_ptr),
            "ssi_utime" => core::mem::offset_of!(SignalfdSiginfo, ssi_utime),
            "ssi_stime" => core::mem::offset_of!(SignalfdSiginfo, ssi_stime),
            "ssi_addr" => core::mem::offset_of!(SignalfdSiginfo, ssi_addr),
            "ssi_addr_lsb" => core::mem::offset_of!(SignalfdSiginfo, ssi_addr_lsb),
            "__pad2" => core::mem::offset_of!(SignalfdSiginfo, ssi_pad2),
            "ssi_syscall" => core::mem::offset_of!(SignalfdSiginfo, ssi_syscall),
            "ssi_call_addr" => core::mem::offset_of!(SignalfdSiginfo, ssi_call_addr),
            "ssi_arch" => core::mem::offset_of!(SignalfdSiginfo, ssi_arch),
            other => panic!("unknown header field {other}"),
        };
        assert_eq!(rust, offset, "{name}");
        offset = offset.saturating_add(size);
        seen = seen.saturating_add(1);
    }
    assert!(seen >= 20, "parsed only {seen} fields");
}

#[test]
fn struct_is_128_bytes() {
    assert_eq!(core::mem::size_of::<SignalfdSiginfo>(), 128);
}

#[test]
fn bad_flag_is_einval() {
    let mut table = FakeFdTable::new();
    let mut signal = FakeSignal::new();
    let mask = [0u64; 2];
    assert_eq!(
        signalfd(&mut table, &mut signal, -1, &mask, 0x80),
        Err(Errno::EINVAL)
    );
}

#[test]
fn sigkill_mask_is_einval() {
    let mut table = FakeFdTable::new();
    let mut signal = FakeSignal::new();
    let mut mask = [0u64; 2];
    mask[0] = 1u64 << (SIGKILL - 1);
    assert_eq!(
        signalfd(&mut table, &mut signal, -1, &mask, 0),
        Err(Errno::EINVAL)
    );
}

#[test]
fn sigstop_mask_is_einval() {
    let mut table = FakeFdTable::new();
    let mut signal = FakeSignal::new();
    let mut mask = [0u64; 2];
    mask[0] = 1u64 << (SIGSTOP - 1);
    assert_eq!(
        signalfd(&mut table, &mut signal, -1, &mask, 0),
        Err(Errno::EINVAL)
    );
}

#[test]
fn create_needs_pg7() {
    let mut table = FakeFdTable::new();
    let mut signal = FakeSignal::new();
    let mask = [2u64, 0];
    assert_eq!(
        signalfd(&mut table, &mut signal, -1, &mask, 0),
        Err(Errno::ENOSYS)
    );
}

#[test]
fn create_installs_when_available() {
    let mut table = FakeFdTable::new();
    let mut signal = FakeSignal::new();
    signal.available = true;
    let mask = [2u64, 0];
    let fd = signalfd(&mut table, &mut signal, -1, &mask, SFD_CLOEXEC).unwrap();
    assert_eq!(table.kind_of(fd), Ok(FdKind::SignalFd));
    assert!(table.cloexec(fd));
    assert_eq!(signal.masks, alloc::vec![mask]);
}

#[test]
fn update_returns_the_same_fd() {
    let mut table = FakeFdTable::new();
    let mut signal = FakeSignal::new();
    signal.available = true;
    let mask = [1u64, 0];
    let fd = signalfd(&mut table, &mut signal, -1, &mask, 0).unwrap();
    let updated = signalfd(&mut table, &mut signal, fd, &[4u64, 0], 0).unwrap();
    assert_eq!(updated, fd);
    assert_eq!(signal.masks.len(), 2);
}

#[test]
fn update_non_signalfd_is_einval() {
    let mut table = FakeFdTable::new();
    table.insert(9, Handle(709), FdKind::Pipe);
    let mut signal = FakeSignal::new();
    signal.available = true;
    assert_eq!(
        signalfd(&mut table, &mut signal, 9, &[1u64, 0], 0),
        Err(Errno::EINVAL)
    );
}

#[test]
fn update_closed_fd_is_ebadf() {
    let mut table = FakeFdTable::new();
    let mut signal = FakeSignal::new();
    signal.available = true;
    assert_eq!(
        signalfd(&mut table, &mut signal, 33, &[1u64, 0], 0),
        Err(Errno::EBADF)
    );
}

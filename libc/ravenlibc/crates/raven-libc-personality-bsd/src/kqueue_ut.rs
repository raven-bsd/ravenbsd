// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `kqueue` constants-from-header, filter table and flag semantics (ticket C66 7(d)).

use super::*;
use crate::seams::Handle;
use crate::testutil::{FakeEvq, FakeFdTable, FakeSrc, header_define};

const EVENT_H: &str = include_str!("../../../include/sys/event.h");

fn align_up(value: usize, align: usize) -> usize {
    if align == 0 {
        return value;
    }
    value.div_ceil(align).saturating_mul(align)
}

fn type_size(kind: &str) -> Option<usize> {
    match kind {
        "uintptr_t" | "int64_t" | "uint64_t" | "void *" | "void*" => Some(8),
        "short" | "unsigned short" => Some(2),
        "unsigned" | "unsigned int" | "int" => Some(4),
        _ => None,
    }
}

#[test]
fn struct_layout_matches_the_header() {
    let start = EVENT_H.find("struct kevent {").unwrap();
    let body = &EVENT_H[start..];
    let mut offset = 0usize;
    for line in body.lines().skip(1) {
        let line = line.trim();
        if line.starts_with('}') {
            break;
        }
        if line.is_empty() {
            continue;
        }
        let line = line.trim_end_matches(';');
        let Some(raw_name) = line.split_whitespace().last() else {
            continue;
        };
        let name = raw_name.trim_start_matches('*');
        if name.starts_with("ext[") {
            assert_eq!(offset, core::mem::offset_of!(Kevent, ext));
            assert_eq!(core::mem::size_of::<Kevent>(), offset + 32);
            break;
        }
        let Some(position) = line.rfind(name) else {
            continue;
        };
        let kind = line[..position].trim();
        let Some(size) = type_size(kind) else {
            panic!("unknown header type `{kind}` for `{name}`");
        };
        offset = align_up(offset, size);
        let rust = match name {
            "ident" => core::mem::offset_of!(Kevent, ident),
            "filter" => core::mem::offset_of!(Kevent, filter),
            "flags" => core::mem::offset_of!(Kevent, flags),
            "fflags" => core::mem::offset_of!(Kevent, fflags),
            "data" => core::mem::offset_of!(Kevent, data),
            "udata" => core::mem::offset_of!(Kevent, udata),
            other => panic!("unknown field {other}"),
        };
        assert_eq!(rust, offset, "{name}");
        offset = offset.saturating_add(size);
    }
    assert_eq!(core::mem::size_of::<Kevent>(), 64);
}

#[test]
fn filter_numbers_match_the_header() {
    for (name, value) in [
        ("EVFILT_READ", EVFILT_READ),
        ("EVFILT_WRITE", EVFILT_WRITE),
        ("EVFILT_AIO", EVFILT_AIO),
        ("EVFILT_VNODE", EVFILT_VNODE),
        ("EVFILT_PROC", EVFILT_PROC),
        ("EVFILT_SIGNAL", EVFILT_SIGNAL),
        ("EVFILT_TIMER", EVFILT_TIMER),
        ("EVFILT_PROCDESC", EVFILT_PROCDESC),
        ("EVFILT_FS", EVFILT_FS),
        ("EVFILT_LIO", EVFILT_LIO),
        ("EVFILT_USER", EVFILT_USER),
        ("EVFILT_SENDFILE", EVFILT_SENDFILE),
        ("EVFILT_EMPTY", EVFILT_EMPTY),
        ("EVFILT_JAIL", EVFILT_JAIL),
        ("EVFILT_JAILDESC", EVFILT_JAILDESC),
    ] {
        assert_eq!(
            header_define(EVENT_H, name),
            Some(i64::from(value)),
            "{name}"
        );
    }
    assert_eq!(
        header_define(EVENT_H, "EVFILT_SYSCOUNT"),
        Some(i64::from(EVFILT_SYSCOUNT))
    );
}

#[test]
fn action_flags_match_the_header() {
    for (name, value) in [
        ("EV_ADD", EV_ADD),
        ("EV_DELETE", EV_DELETE),
        ("EV_ENABLE", EV_ENABLE),
        ("EV_DISABLE", EV_DISABLE),
        ("EV_ONESHOT", EV_ONESHOT),
        ("EV_CLEAR", EV_CLEAR),
        ("EV_RECEIPT", EV_RECEIPT),
        ("EV_DISPATCH", EV_DISPATCH),
        ("EV_FORCEONESHOT", EV_FORCEONESHOT),
        ("EV_KEEPUDATA", EV_KEEPUDATA),
        ("EV_EOF", EV_EOF),
        ("EV_ERROR", EV_ERROR),
    ] {
        assert_eq!(
            header_define(EVENT_H, name),
            Some(i64::from(value)),
            "{name}"
        );
    }
}

#[test]
fn note_flags_match_the_header() {
    for (name, value) in [
        ("NOTE_EXIT", NOTE_EXIT),
        ("NOTE_FORK", NOTE_FORK),
        ("NOTE_EXEC", NOTE_EXEC),
        ("NOTE_TRIGGER", NOTE_TRIGGER),
        ("NOTE_FFNOP", NOTE_FFNOP),
        ("NOTE_FFAND", NOTE_FFAND),
        ("NOTE_FFOR", NOTE_FFOR),
        ("NOTE_FFCOPY", NOTE_FFCOPY),
        ("NOTE_SECONDS", NOTE_SECONDS),
        ("NOTE_MSECONDS", NOTE_MSECONDS),
        ("NOTE_USECONDS", NOTE_USECONDS),
        ("NOTE_NSECONDS", NOTE_NSECONDS),
        ("NOTE_ABSTIME", NOTE_ABSTIME),
    ] {
        assert_eq!(
            header_define(EVENT_H, name),
            Some(i64::from(value)),
            "{name}"
        );
    }
}

fn setup() -> (FakeEvq, FakeFdTable, FakeSrc, Kqueue, i32) {
    let mut evq = FakeEvq::new();
    let mut table = FakeFdTable::new();
    let source = FakeSrc::new();
    let kqueue = Kqueue::new();
    let kqfd = kqueue1(&mut evq, &mut table, 0).unwrap();
    (evq, table, source, kqueue, kqfd)
}

fn change(ident: usize, filter: i16, flags: u16, fflags: u32, data: i64) -> Kevent {
    Kevent {
        ident,
        filter,
        flags,
        fflags,
        data,
        udata: 0,
        ext: [0; 4],
    }
}

fn token_of(kqueue: &Kqueue, kqfd: i32, key: (usize, i16)) -> u64 {
    kqueue
        .registrations(kqfd)
        .unwrap()
        .tokens
        .iter()
        .find(|(_, value)| **value == key)
        .map(|(token, _)| *token)
        .unwrap()
}

#[test]
fn kqueue_installs_a_kqueue_fd() {
    let (_, table, _, _, kqfd) = setup();
    assert_eq!(table.kind_of(kqfd), Ok(FdKind::Kqueue));
}

#[test]
fn kqueue1_rejects_unknown_flags() {
    let mut evq = FakeEvq::new();
    let mut table = FakeFdTable::new();
    assert_eq!(kqueue1(&mut evq, &mut table, 0x80), Err(Errno::EINVAL));
}

#[test]
fn kqueue_create_failure_propagates() {
    let mut evq = FakeEvq::new();
    let mut table = FakeFdTable::new();
    table.install_error = Some(Errno::EMFILE);
    assert_eq!(kqueue(&mut evq, &mut table, false), Err(Errno::EMFILE));
}

#[test]
fn add_read_registers_readable_and_hup() {
    let (mut evq, mut table, mut source, mut kqueue, kqfd) = setup();
    table.insert(10, Handle(710), FdKind::Pipe);
    let mut events = [Kevent::default(); 1];
    let count = kevent(
        &mut evq,
        &table,
        &mut source,
        &mut kqueue,
        kqfd,
        &[change(10, EVFILT_READ, EV_ADD, 0, 0)],
        &mut events,
        None,
    )
    .unwrap();
    assert_eq!(count, 0);
    assert!(
        kqueue
            .registrations(kqfd)
            .unwrap()
            .regs
            .contains_key(&(10, EVFILT_READ))
    );
    let token = token_of(&kqueue, kqfd, (10, EVFILT_READ));
    assert_eq!(
        evq.last_mask(EvqCtl::Add, token).unwrap() & (EV_READABLE | EV_HUP),
        EV_READABLE | EV_HUP
    );
}

#[test]
fn add_write_registers_writable() {
    let (mut evq, mut table, mut source, mut kqueue, kqfd) = setup();
    table.insert(10, Handle(710), FdKind::Pipe);
    let mut events = [Kevent::default(); 1];
    kevent(
        &mut evq,
        &table,
        &mut source,
        &mut kqueue,
        kqfd,
        &[change(10, EVFILT_WRITE, EV_ADD, 0, 0)],
        &mut events,
        None,
    )
    .unwrap();
    let token = token_of(&kqueue, kqfd, (10, EVFILT_WRITE));
    assert_eq!(
        evq.last_mask(EvqCtl::Add, token).unwrap() & EV_WRITABLE,
        EV_WRITABLE
    );
}

#[test]
fn add_regular_file_is_eopnotsupp() {
    let (mut evq, mut table, mut source, mut kqueue, kqfd) = setup();
    table.insert(10, Handle(710), FdKind::File);
    let mut events = [Kevent::default(); 1];
    let count = kevent(
        &mut evq,
        &table,
        &mut source,
        &mut kqueue,
        kqfd,
        &[change(10, EVFILT_READ, EV_ADD, 0, 0)],
        &mut events,
        None,
    )
    .unwrap();
    assert_eq!(count, 1);
    assert_eq!(events[0].flags, EV_ERROR);
    assert_eq!(events[0].data, i64::from(Errno::EOPNOTSUPP.0));
}

#[test]
fn add_timer_arms_the_source() {
    let (mut evq, table, mut source, mut kqueue, kqfd) = setup();
    let mut events = [Kevent::default(); 1];
    kevent(
        &mut evq,
        &table,
        &mut source,
        &mut kqueue,
        kqfd,
        &[change(1, EVFILT_TIMER, EV_ADD, 0, 10)],
        &mut events,
        None,
    )
    .unwrap();
    assert_eq!(
        source.timer_sets.last(),
        Some(&(Handle(901), 10_000_000, 10_000_000))
    );
}

#[test]
fn timer_units_are_converted() {
    let (mut evq, table, mut source, mut kqueue, kqfd) = setup();
    let mut events = [Kevent::default(); 1];
    kevent(
        &mut evq,
        &table,
        &mut source,
        &mut kqueue,
        kqfd,
        &[change(1, EVFILT_TIMER, EV_ADD, NOTE_SECONDS, 2)],
        &mut events,
        None,
    )
    .unwrap();
    assert_eq!(source.timer_sets.last().unwrap().1, 2_000_000_000);
}

#[test]
fn timer_abstime_is_eopnotsupp() {
    let (mut evq, table, mut source, mut kqueue, kqfd) = setup();
    let mut events = [Kevent::default(); 1];
    kevent(
        &mut evq,
        &table,
        &mut source,
        &mut kqueue,
        kqfd,
        &[change(1, EVFILT_TIMER, EV_ADD, NOTE_ABSTIME, 2)],
        &mut events,
        None,
    )
    .unwrap();
    assert_eq!(events[0].flags, EV_ERROR);
    assert_eq!(events[0].data, i64::from(Errno::EOPNOTSUPP.0));
}

#[test]
fn timer_oneshot_has_no_interval() {
    let (mut evq, table, mut source, mut kqueue, kqfd) = setup();
    let mut events = [Kevent::default(); 1];
    kevent(
        &mut evq,
        &table,
        &mut source,
        &mut kqueue,
        kqfd,
        &[change(1, EVFILT_TIMER, EV_ADD | EV_ONESHOT, 0, 5)],
        &mut events,
        None,
    )
    .unwrap();
    assert_eq!(source.timer_sets.last().unwrap().2, 0);
}

#[test]
fn user_filter_creates_and_triggers() {
    let (mut evq, table, mut source, mut kqueue, kqfd) = setup();
    let mut events = [Kevent::default(); 1];
    kevent(
        &mut evq,
        &table,
        &mut source,
        &mut kqueue,
        kqfd,
        &[change(1, EVFILT_USER, EV_ADD, NOTE_TRIGGER, 0)],
        &mut events,
        None,
    )
    .unwrap();
    assert_eq!(source.events, 1);
    assert_eq!(source.signals.len(), 1);
}

#[test]
fn proc_filter_needs_pg44() {
    let (mut evq, table, mut source, mut kqueue, kqfd) = setup();
    let mut events = [Kevent::default(); 1];
    kevent(
        &mut evq,
        &table,
        &mut source,
        &mut kqueue,
        kqfd,
        &[change(1, EVFILT_PROC, EV_ADD, NOTE_EXIT, 0)],
        &mut events,
        None,
    )
    .unwrap();
    assert_eq!(events[0].flags, EV_ERROR);
    assert_eq!(events[0].data, i64::from(Errno::ENOSYS.0));
}

#[test]
fn proc_filter_without_note_exit_is_enotsup() {
    let (mut evq, table, mut source, mut kqueue, kqfd) = setup();
    source.process_available = true;
    let mut events = [Kevent::default(); 1];
    kevent(
        &mut evq,
        &table,
        &mut source,
        &mut kqueue,
        kqfd,
        &[change(1, EVFILT_PROC, EV_ADD, NOTE_FORK, 0)],
        &mut events,
        None,
    )
    .unwrap();
    assert_eq!(events[0].data, i64::from(Errno::EOPNOTSUPP.0));
}

#[test]
fn signal_filter_needs_pg7() {
    let (mut evq, table, mut source, mut kqueue, kqfd) = setup();
    let mut events = [Kevent::default(); 1];
    kevent(
        &mut evq,
        &table,
        &mut source,
        &mut kqueue,
        kqfd,
        &[change(2, EVFILT_SIGNAL, EV_ADD, 0, 0)],
        &mut events,
        None,
    )
    .unwrap();
    assert_eq!(events[0].data, i64::from(Errno::ENOSYS.0));
}

#[test]
fn vnode_filter_needs_pg38() {
    let (mut evq, table, mut source, mut kqueue, kqfd) = setup();
    let mut events = [Kevent::default(); 1];
    kevent(
        &mut evq,
        &table,
        &mut source,
        &mut kqueue,
        kqfd,
        &[change(1, EVFILT_VNODE, EV_ADD, NOTE_EXIT, 0)],
        &mut events,
        None,
    )
    .unwrap();
    assert_eq!(events[0].data, i64::from(Errno::ENOSYS.0));
}

#[test]
fn unknown_filter_is_einval() {
    let (mut evq, table, mut source, mut kqueue, kqfd) = setup();
    let mut events = [Kevent::default(); 1];
    kevent(
        &mut evq,
        &table,
        &mut source,
        &mut kqueue,
        kqfd,
        &[change(1, 99, EV_ADD, 0, 0)],
        &mut events,
        None,
    )
    .unwrap();
    assert_eq!(events[0].data, i64::from(Errno::EINVAL.0));
}

#[test]
fn undefined_flag_bits_are_einval() {
    let (mut evq, mut table, mut source, mut kqueue, kqfd) = setup();
    table.insert(10, Handle(710), FdKind::Pipe);
    let mut events = [Kevent::default(); 1];
    kevent(
        &mut evq,
        &table,
        &mut source,
        &mut kqueue,
        kqfd,
        &[change(10, EVFILT_READ, EV_ADD | 0x0400, 0, 0)],
        &mut events,
        None,
    )
    .unwrap();
    assert_eq!(events[0].data, i64::from(Errno::EINVAL.0));
}

#[test]
fn system_flag_bits_are_masked() {
    let (mut evq, mut table, mut source, mut kqueue, kqfd) = setup();
    table.insert(10, Handle(710), FdKind::Pipe);
    let mut events = [Kevent::default(); 1];
    let count = kevent(
        &mut evq,
        &table,
        &mut source,
        &mut kqueue,
        kqfd,
        &[change(10, EVFILT_READ, EV_ADD | EV_EOF, 0, 0)],
        &mut events,
        None,
    )
    .unwrap();
    assert_eq!(count, 0);
    assert!(
        kqueue
            .registrations(kqfd)
            .unwrap()
            .regs
            .contains_key(&(10, EVFILT_READ))
    );
}

#[test]
fn delete_unknown_is_enoent() {
    let (mut evq, table, mut source, mut kqueue, kqfd) = setup();
    let mut events = [Kevent::default(); 1];
    kevent(
        &mut evq,
        &table,
        &mut source,
        &mut kqueue,
        kqfd,
        &[change(10, EVFILT_READ, EV_DELETE, 0, 0)],
        &mut events,
        None,
    )
    .unwrap();
    assert_eq!(events[0].data, i64::from(Errno::ENOENT.0));
}

#[test]
fn delete_removes_and_notifies_the_queue() {
    let (mut evq, mut table, mut source, mut kqueue, kqfd) = setup();
    table.insert(10, Handle(710), FdKind::Pipe);
    let mut events = [Kevent::default(); 1];
    kevent(
        &mut evq,
        &table,
        &mut source,
        &mut kqueue,
        kqfd,
        &[change(10, EVFILT_READ, EV_ADD, 0, 0)],
        &mut events,
        None,
    )
    .unwrap();
    kevent(
        &mut evq,
        &table,
        &mut source,
        &mut kqueue,
        kqfd,
        &[change(10, EVFILT_READ, EV_DELETE, 0, 0)],
        &mut events,
        None,
    )
    .unwrap();
    assert!(kqueue.is_empty(kqfd));
    assert!(evq.ops.iter().any(|(op, _, _, _)| *op == EvqCtl::Del));
}

#[test]
fn receipt_echoes_success() {
    let (mut evq, mut table, mut source, mut kqueue, kqfd) = setup();
    table.insert(10, Handle(710), FdKind::Pipe);
    let mut events = [Kevent::default(); 1];
    let count = kevent(
        &mut evq,
        &table,
        &mut source,
        &mut kqueue,
        kqfd,
        &[change(10, EVFILT_READ, EV_ADD | EV_RECEIPT, 0, 0)],
        &mut events,
        None,
    )
    .unwrap();
    assert_eq!(count, 1);
    assert_eq!(events[0].flags, EV_ERROR);
    assert_eq!(events[0].data, 0);
}

#[test]
fn error_without_room_returns_the_change_error() {
    let (mut evq, table, mut source, mut kqueue, kqfd) = setup();
    assert_eq!(
        kevent(
            &mut evq,
            &table,
            &mut source,
            &mut kqueue,
            kqfd,
            &[change(10, EVFILT_READ, EV_DELETE, 0, 0)],
            &mut [],
            None,
        ),
        Err(Errno::ENOENT)
    );
}

#[test]
fn wait_delivers_read_event() {
    let (mut evq, mut table, mut source, mut kqueue, kqfd) = setup();
    table.insert(10, Handle(710), FdKind::Pipe);
    let mut events = [Kevent::default(); 1];
    kevent(
        &mut evq,
        &table,
        &mut source,
        &mut kqueue,
        kqfd,
        &[change(10, EVFILT_READ, EV_ADD, 0, 0)],
        &mut events,
        None,
    )
    .unwrap();
    let token = token_of(&kqueue, kqfd, (10, EVFILT_READ));
    evq.push(token, EV_READABLE);
    let count = kevent(
        &mut evq,
        &table,
        &mut source,
        &mut kqueue,
        kqfd,
        &[],
        &mut events,
        None,
    )
    .unwrap();
    assert_eq!(count, 1);
    assert_eq!(events[0].ident, 10);
    assert_eq!(events[0].filter, EVFILT_READ);
    assert_eq!(events[0].data, 1);
}

#[test]
fn hup_sets_eof() {
    let (mut evq, mut table, mut source, mut kqueue, kqfd) = setup();
    table.insert(10, Handle(710), FdKind::Pipe);
    let mut events = [Kevent::default(); 1];
    kevent(
        &mut evq,
        &table,
        &mut source,
        &mut kqueue,
        kqfd,
        &[change(10, EVFILT_READ, EV_ADD, 0, 0)],
        &mut events,
        None,
    )
    .unwrap();
    let token = token_of(&kqueue, kqfd, (10, EVFILT_READ));
    evq.push(token, EV_HUP);
    kevent(
        &mut evq,
        &table,
        &mut source,
        &mut kqueue,
        kqfd,
        &[],
        &mut events,
        None,
    )
    .unwrap();
    assert_eq!(events[0].flags & EV_EOF, EV_EOF);
}

#[test]
fn clear_registers_edge() {
    let (mut evq, mut table, mut source, mut kqueue, kqfd) = setup();
    table.insert(10, Handle(710), FdKind::Pipe);
    let mut events = [Kevent::default(); 1];
    kevent(
        &mut evq,
        &table,
        &mut source,
        &mut kqueue,
        kqfd,
        &[change(10, EVFILT_READ, EV_ADD | EV_CLEAR, 0, 0)],
        &mut events,
        None,
    )
    .unwrap();
    let token = token_of(&kqueue, kqfd, (10, EVFILT_READ));
    assert_eq!(
        evq.last_mask(EvqCtl::Add, token).unwrap() & EV_EDGE,
        EV_EDGE
    );
}

#[test]
fn oneshot_deletes_after_delivery() {
    let (mut evq, mut table, mut source, mut kqueue, kqfd) = setup();
    table.insert(10, Handle(710), FdKind::Pipe);
    let mut events = [Kevent::default(); 1];
    kevent(
        &mut evq,
        &table,
        &mut source,
        &mut kqueue,
        kqfd,
        &[change(10, EVFILT_READ, EV_ADD | EV_ONESHOT, 0, 0)],
        &mut events,
        None,
    )
    .unwrap();
    let token = token_of(&kqueue, kqfd, (10, EVFILT_READ));
    evq.push(token, EV_READABLE);
    kevent(
        &mut evq,
        &table,
        &mut source,
        &mut kqueue,
        kqfd,
        &[],
        &mut events,
        None,
    )
    .unwrap();
    assert!(kqueue.is_empty(kqfd));
    assert!(evq.ops.iter().rev().any(|(op, _, _, _)| *op == EvqCtl::Del));
}

#[test]
fn dispatch_disables_after_delivery() {
    let (mut evq, mut table, mut source, mut kqueue, kqfd) = setup();
    table.insert(10, Handle(710), FdKind::Pipe);
    let mut events = [Kevent::default(); 1];
    kevent(
        &mut evq,
        &table,
        &mut source,
        &mut kqueue,
        kqfd,
        &[change(10, EVFILT_READ, EV_ADD | EV_DISPATCH, 0, 0)],
        &mut events,
        None,
    )
    .unwrap();
    let token = token_of(&kqueue, kqfd, (10, EVFILT_READ));
    evq.push(token, EV_READABLE);
    kevent(
        &mut evq,
        &table,
        &mut source,
        &mut kqueue,
        kqfd,
        &[],
        &mut events,
        None,
    )
    .unwrap();
    let reg = kqueue
        .registrations(kqfd)
        .unwrap()
        .regs
        .get(&(10, EVFILT_READ))
        .copied()
        .unwrap();
    assert!(reg.disabled);
}

#[test]
fn stale_fd_event_is_dropped() {
    let (mut evq, mut table, mut source, mut kqueue, kqfd) = setup();
    table.insert(10, Handle(710), FdKind::Pipe);
    let mut events = [Kevent::default(); 1];
    kevent(
        &mut evq,
        &table,
        &mut source,
        &mut kqueue,
        kqfd,
        &[change(10, EVFILT_READ, EV_ADD, 0, 0)],
        &mut events,
        None,
    )
    .unwrap();
    let token = token_of(&kqueue, kqfd, (10, EVFILT_READ));
    table.close_fd(10).unwrap();
    table.insert(10, Handle(711), FdKind::Pipe);
    evq.push(token, EV_READABLE);
    let count = kevent(
        &mut evq,
        &table,
        &mut source,
        &mut kqueue,
        kqfd,
        &[],
        &mut events,
        None,
    )
    .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn registration_cap_is_enomem() {
    let (mut evq, mut table, mut source, mut kqueue, kqfd) = setup();
    table.insert(10, Handle(710), FdKind::Pipe);
    let instance = kqueue.instances.entry(kqfd).or_default();
    for index in 0..MAX_REGISTRATIONS {
        instance.regs.insert(
            (index.saturating_add(1000), EVFILT_READ),
            Reg {
                token: index as u64,
                flags: EV_ADD,
                fflags: 0,
                data: 0,
                udata: 0,
                handle: Handle(1),
                fd: None,
                epoch: 0,
                mask: 0,
                disabled: false,
            },
        );
    }
    let mut events = [Kevent::default(); 1];
    kevent(
        &mut evq,
        &table,
        &mut source,
        &mut kqueue,
        kqfd,
        &[change(10, EVFILT_READ, EV_ADD, 0, 0)],
        &mut events,
        None,
    )
    .unwrap();
    assert_eq!(events[0].data, i64::from(Errno::ENOMEM.0));
}

#[test]
fn empty_changes_wait_for_the_timeout() {
    let (mut evq, table, mut source, mut kqueue, kqfd) = setup();
    let mut events = [Kevent::default(); 1];
    let count = kevent(
        &mut evq,
        &table,
        &mut source,
        &mut kqueue,
        kqfd,
        &[],
        &mut events,
        None,
    )
    .unwrap();
    assert_eq!(count, 0);
    assert_eq!(evq.last_timeout_ns, Some(0));
}

#[test]
fn zero_timespec_is_a_poll() {
    let (mut evq, table, mut source, mut kqueue, kqfd) = setup();
    let mut events = [Kevent::default(); 1];
    kevent(
        &mut evq,
        &table,
        &mut source,
        &mut kqueue,
        kqfd,
        &[],
        &mut events,
        Some(Timespec {
            tv_sec: 0,
            tv_nsec: 0,
        }),
    )
    .unwrap();
    assert_eq!(evq.last_timeout_ns, Some(1));
}

#[test]
fn one_second_timeout_is_a_billion_nanoseconds() {
    let (mut evq, table, mut source, mut kqueue, kqfd) = setup();
    let mut events = [Kevent::default(); 1];
    kevent(
        &mut evq,
        &table,
        &mut source,
        &mut kqueue,
        kqfd,
        &[],
        &mut events,
        Some(Timespec {
            tv_sec: 1,
            tv_nsec: 0,
        }),
    )
    .unwrap();
    assert_eq!(evq.last_timeout_ns, Some(1_000_000_000));
}

#[test]
fn invalid_timeout_is_einval() {
    let (mut evq, table, mut source, mut kqueue, kqfd) = setup();
    let mut events = [Kevent::default(); 1];
    for value in [
        Timespec {
            tv_sec: -1,
            tv_nsec: 0,
        },
        Timespec {
            tv_sec: 0,
            tv_nsec: -1,
        },
        Timespec {
            tv_sec: 0,
            tv_nsec: 1_000_000_000,
        },
    ] {
        assert_eq!(
            kevent(
                &mut evq,
                &table,
                &mut source,
                &mut kqueue,
                kqfd,
                &[],
                &mut events,
                Some(value)
            ),
            Err(Errno::EINVAL)
        );
    }
}

#[test]
fn wrong_fd_kind_is_einval() {
    let (mut evq, table, mut source, mut kqueue, _) = setup();
    assert_eq!(
        kevent(
            &mut evq,
            &table,
            &mut source,
            &mut kqueue,
            99,
            &[],
            &mut [],
            None
        ),
        Err(Errno::EBADF)
    );
}

#[test]
fn kqueue_fd_is_not_inherited_semantics_are_not_faked() {
    // The child's fd table has no entry; the personality reports EBADF, never a fake queue.
    let mut evq = FakeEvq::new();
    let table = FakeFdTable::new();
    let mut source = FakeSrc::new();
    let mut kqueue = Kqueue::new();
    assert_eq!(
        kevent(
            &mut evq,
            &table,
            &mut source,
            &mut kqueue,
            3,
            &[],
            &mut [],
            None
        ),
        Err(Errno::EBADF)
    );
}

// ---------------------------------------------------------------------------------------------
// Pinned FreeBSD `sys/kern/kern_event.c` `kqueue_register`/`kqueue_kevent` contract rows (S743).

fn add_read(evq: &mut FakeEvq, table: &FakeFdTable, source: &mut FakeSrc, kqueue: &mut Kqueue, kqfd: i32) {
    let mut events = [Kevent::default(); 2];
    let count = kevent(
        evq,
        table,
        source,
        kqueue,
        kqfd,
        &[change(10, EVFILT_READ, EV_ADD, 0, 0)],
        &mut events,
        None,
    )
    .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn read_and_write_on_one_fd_register_together() {
    let (mut evq, mut table, mut source, mut kqueue, kqfd) = setup();
    table.insert(10, Handle(710), FdKind::Pipe);
    let mut events = [Kevent::default(); 2];
    let count = kevent(
        &mut evq,
        &table,
        &mut source,
        &mut kqueue,
        kqfd,
        &[
            change(10, EVFILT_READ, EV_ADD, 0, 0),
            change(10, EVFILT_WRITE, EV_ADD, 0, 0),
        ],
        &mut events,
        None,
    )
    .unwrap();
    assert_eq!(count, 0);
    let instance = kqueue.registrations(kqfd).unwrap();
    assert!(instance.regs.contains_key(&(10, EVFILT_READ)));
    assert!(instance.regs.contains_key(&(10, EVFILT_WRITE)));
    assert_eq!(
        evq.ops
            .iter()
            .filter(|(op, handle, _, _)| *op == EvqCtl::Add && handle.0 == 710)
            .count(),
        1
    );
    let cookie = evq.cookie_of(Handle(710)).unwrap();
    let mask = evq.last_mask(EvqCtl::Add, cookie).unwrap();
    assert_eq!(
        mask & (EV_READABLE | EV_WRITABLE | EV_HUP),
        EV_READABLE | EV_WRITABLE | EV_HUP
    );
}

#[test]
fn read_and_write_delivery_is_per_filter() {
    let (mut evq, mut table, mut source, mut kqueue, kqfd) = setup();
    table.insert(10, Handle(710), FdKind::Pipe);
    let mut events = [Kevent::default(); 2];
    kevent(
        &mut evq,
        &table,
        &mut source,
        &mut kqueue,
        kqfd,
        &[
            change(10, EVFILT_READ, EV_ADD, 0, 0),
            change(10, EVFILT_WRITE, EV_ADD, 0, 0),
        ],
        &mut events,
        None,
    )
    .unwrap();
    let cookie = evq.cookie_of(Handle(710)).unwrap();
    evq.push(cookie, EV_READABLE | EV_WRITABLE);
    let count = kevent(
        &mut evq,
        &table,
        &mut source,
        &mut kqueue,
        kqfd,
        &[],
        &mut events,
        None,
    )
    .unwrap();
    assert_eq!(count, 2);
    assert_eq!(events[0].filter, EVFILT_READ);
    assert_eq!(events[1].filter, EVFILT_WRITE);
}

#[test]
fn readd_updates_udata_and_flags() {
    let (mut evq, mut table, mut source, mut kqueue, kqfd) = setup();
    table.insert(10, Handle(710), FdKind::Pipe);
    add_read(&mut evq, &table, &mut source, &mut kqueue, kqfd);
    let mut events = [Kevent::default(); 2];
    let mut updated = change(10, EVFILT_READ, EV_ADD, 0, 0);
    updated.udata = 9;
    let count = kevent(
        &mut evq,
        &table,
        &mut source,
        &mut kqueue,
        kqfd,
        &[updated],
        &mut events,
        None,
    )
    .unwrap();
    assert_eq!(count, 0);
    assert_eq!(
        kqueue
            .registrations(kqfd)
            .unwrap()
            .regs
            .get(&(10, EVFILT_READ))
            .unwrap()
            .udata,
        9
    );
}

#[test]
fn modify_without_keepudata_replaces_udata() {
    let (mut evq, mut table, mut source, mut kqueue, kqfd) = setup();
    table.insert(10, Handle(710), FdKind::Pipe);
    let mut first = change(10, EVFILT_READ, EV_ADD, 0, 0);
    first.udata = 7;
    let mut events = [Kevent::default(); 2];
    kevent(
        &mut evq,
        &table,
        &mut source,
        &mut kqueue,
        kqfd,
        &[first],
        &mut events,
        None,
    )
    .unwrap();
    let mut second = change(10, EVFILT_READ, 0, 0, 0);
    second.udata = 8;
    kevent(
        &mut evq,
        &table,
        &mut source,
        &mut kqueue,
        kqfd,
        &[second],
        &mut events,
        None,
    )
    .unwrap();
    assert_eq!(
        kqueue
            .registrations(kqfd)
            .unwrap()
            .regs
            .get(&(10, EVFILT_READ))
            .unwrap()
            .udata,
        8
    );
}

#[test]
fn modify_with_keepudata_keeps_udata() {
    let (mut evq, mut table, mut source, mut kqueue, kqfd) = setup();
    table.insert(10, Handle(710), FdKind::Pipe);
    let mut first = change(10, EVFILT_READ, EV_ADD, 0, 0);
    first.udata = 7;
    let mut events = [Kevent::default(); 2];
    kevent(
        &mut evq,
        &table,
        &mut source,
        &mut kqueue,
        kqfd,
        &[first],
        &mut events,
        None,
    )
    .unwrap();
    let mut second = change(10, EVFILT_READ, EV_KEEPUDATA, 0, 0);
    second.udata = 8;
    kevent(
        &mut evq,
        &table,
        &mut source,
        &mut kqueue,
        kqfd,
        &[second],
        &mut events,
        None,
    )
    .unwrap();
    assert_eq!(
        kqueue
            .registrations(kqfd)
            .unwrap()
            .regs
            .get(&(10, EVFILT_READ))
            .unwrap()
            .udata,
        7
    );
}

#[test]
fn keepudata_with_add_is_einval() {
    let (mut evq, mut table, mut source, mut kqueue, kqfd) = setup();
    table.insert(10, Handle(710), FdKind::Pipe);
    let mut events = [Kevent::default(); 1];
    let count = kevent(
        &mut evq,
        &table,
        &mut source,
        &mut kqueue,
        kqfd,
        &[change(10, EVFILT_READ, EV_ADD | EV_KEEPUDATA, 0, 0)],
        &mut events,
        None,
    )
    .unwrap();
    assert_eq!(count, 1);
    assert_eq!(events[0].data, i64::from(Errno::EINVAL.0));
}

#[test]
fn enable_and_disable_together_is_einval() {
    let (mut evq, mut table, mut source, mut kqueue, kqfd) = setup();
    table.insert(10, Handle(710), FdKind::Pipe);
    let mut events = [Kevent::default(); 1];
    let count = kevent(
        &mut evq,
        &table,
        &mut source,
        &mut kqueue,
        kqfd,
        &[change(10, EVFILT_READ, EV_ADD | EV_ENABLE | EV_DISABLE, 0, 0)],
        &mut events,
        None,
    )
    .unwrap();
    assert_eq!(count, 1);
    assert_eq!(events[0].data, i64::from(Errno::EINVAL.0));
}

#[test]
fn enable_without_add_on_missing_is_enoent() {
    let (mut evq, mut table, mut source, mut kqueue, kqfd) = setup();
    table.insert(10, Handle(710), FdKind::Pipe);
    let mut events = [Kevent::default(); 1];
    let count = kevent(
        &mut evq,
        &table,
        &mut source,
        &mut kqueue,
        kqfd,
        &[change(10, EVFILT_READ, EV_ENABLE, 0, 0)],
        &mut events,
        None,
    )
    .unwrap();
    assert_eq!(count, 1);
    assert_eq!(events[0].data, i64::from(Errno::ENOENT.0));
}

#[test]
fn disable_removes_the_watch_and_enable_restores_it() {
    let (mut evq, mut table, mut source, mut kqueue, kqfd) = setup();
    table.insert(10, Handle(710), FdKind::Pipe);
    add_read(&mut evq, &table, &mut source, &mut kqueue, kqfd);
    let mut events = [Kevent::default(); 1];
    let count = kevent(
        &mut evq,
        &table,
        &mut source,
        &mut kqueue,
        kqfd,
        &[change(10, EVFILT_READ, EV_DISABLE, 0, 0)],
        &mut events,
        None,
    )
    .unwrap();
    assert_eq!(count, 0);
    assert_eq!(evq.cookie_of(Handle(710)), None);
    let count = kevent(
        &mut evq,
        &table,
        &mut source,
        &mut kqueue,
        kqfd,
        &[change(10, EVFILT_READ, EV_ENABLE, 0, 0)],
        &mut events,
        None,
    )
    .unwrap();
    assert_eq!(count, 0);
    let cookie = evq.cookie_of(Handle(710)).unwrap();
    evq.push(cookie, EV_READABLE);
    let count = kevent(
        &mut evq,
        &table,
        &mut source,
        &mut kqueue,
        kqfd,
        &[],
        &mut events,
        None,
    )
    .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn timer_zero_period_is_one_unit() {
    let (mut evq, table, mut source, mut kqueue, kqfd) = setup();
    let mut events = [Kevent::default(); 1];
    let count = kevent(
        &mut evq,
        &table,
        &mut source,
        &mut kqueue,
        kqfd,
        &[change(1, EVFILT_TIMER, EV_ADD, 0, 0)],
        &mut events,
        None,
    )
    .unwrap();
    assert_eq!(count, 0);
    assert_eq!(
        source.timer_sets.last(),
        Some(&(Handle(901), 1_000_000, 1_000_000))
    );
}

#[test]
fn timer_negative_data_is_einval() {
    let (mut evq, table, mut source, mut kqueue, kqfd) = setup();
    let mut events = [Kevent::default(); 1];
    let count = kevent(
        &mut evq,
        &table,
        &mut source,
        &mut kqueue,
        kqfd,
        &[change(1, EVFILT_TIMER, EV_ADD, 0, -1)],
        &mut events,
        None,
    )
    .unwrap();
    assert_eq!(count, 1);
    assert_eq!(events[0].data, i64::from(Errno::EINVAL.0));
}

#[test]
fn timer_unsupported_fflags_is_einval() {
    let (mut evq, table, mut source, mut kqueue, kqfd) = setup();
    let mut events = [Kevent::default(); 1];
    let count = kevent(
        &mut evq,
        &table,
        &mut source,
        &mut kqueue,
        kqfd,
        &[change(1, EVFILT_TIMER, EV_ADD, 0x100, 5)],
        &mut events,
        None,
    )
    .unwrap();
    assert_eq!(count, 1);
    assert_eq!(events[0].data, i64::from(Errno::EINVAL.0));
}

#[test]
fn read_lowat_is_eopnotsupp() {
    let (mut evq, mut table, mut source, mut kqueue, kqfd) = setup();
    table.insert(10, Handle(710), FdKind::Pipe);
    let mut events = [Kevent::default(); 1];
    let count = kevent(
        &mut evq,
        &table,
        &mut source,
        &mut kqueue,
        kqfd,
        &[change(10, EVFILT_READ, EV_ADD, NOTE_LOWAT, 0)],
        &mut events,
        None,
    )
    .unwrap();
    assert_eq!(count, 1);
    assert_eq!(events[0].data, i64::from(Errno::EOPNOTSUPP.0));
}

#[test]
fn user_ff_control_is_eopnotsupp() {
    let (mut evq, table, mut source, mut kqueue, kqfd) = setup();
    let mut events = [Kevent::default(); 1];
    let count = kevent(
        &mut evq,
        &table,
        &mut source,
        &mut kqueue,
        kqfd,
        &[change(1, EVFILT_USER, EV_ADD, NOTE_FFOR, 0)],
        &mut events,
        None,
    )
    .unwrap();
    assert_eq!(count, 1);
    assert_eq!(events[0].data, i64::from(Errno::EOPNOTSUPP.0));
}

#[test]
fn proc_extra_fflags_is_eopnotsupp() {
    let (mut evq, table, mut source, mut kqueue, kqfd) = setup();
    source.process_available = true;
    let mut events = [Kevent::default(); 1];
    let count = kevent(
        &mut evq,
        &table,
        &mut source,
        &mut kqueue,
        kqfd,
        &[change(1, EVFILT_PROC, EV_ADD, NOTE_EXIT | NOTE_FORK, 0)],
        &mut events,
        None,
    )
    .unwrap();
    assert_eq!(count, 1);
    assert_eq!(events[0].data, i64::from(Errno::EOPNOTSUPP.0));
}

#[test]
fn receipt_returns_without_waiting() {
    let (mut evq, mut table, mut source, mut kqueue, kqfd) = setup();
    table.insert(10, Handle(710), FdKind::Pipe);
    let mut events = [Kevent::default(); 2];
    let count = kevent(
        &mut evq,
        &table,
        &mut source,
        &mut kqueue,
        kqfd,
        &[change(10, EVFILT_READ, EV_ADD | EV_RECEIPT, 0, 0)],
        &mut events,
        None,
    )
    .unwrap();
    assert_eq!(count, 1);
    assert_eq!(events[0].flags, EV_ERROR);
    assert_eq!(events[0].data, 0);
    assert_eq!(evq.last_timeout_ns, None);
}

#[test]
fn change_error_returns_without_waiting() {
    let (mut evq, table, mut source, mut kqueue, kqfd) = setup();
    let mut events = [Kevent::default(); 2];
    let count = kevent(
        &mut evq,
        &table,
        &mut source,
        &mut kqueue,
        kqfd,
        &[change(10, EVFILT_READ, EV_DELETE, 0, 0)],
        &mut events,
        None,
    )
    .unwrap();
    assert_eq!(count, 1);
    assert_eq!(events[0].data, i64::from(Errno::ENOENT.0));
    assert_eq!(evq.last_timeout_ns, None);
}

#[test]
fn filter_zero_is_skipped() {
    let (mut evq, table, mut source, mut kqueue, kqfd) = setup();
    let mut events = [Kevent::default(); 1];
    let count = kevent(
        &mut evq,
        &table,
        &mut source,
        &mut kqueue,
        kqfd,
        &[change(1, 0, EV_ADD, 0, 0)],
        &mut events,
        None,
    )
    .unwrap();
    assert_eq!(count, 0);
    assert!(kqueue.is_empty(kqfd));
}

#[test]
fn stale_fd_delete_is_enoent() {
    let (mut evq, mut table, mut source, mut kqueue, kqfd) = setup();
    table.insert(10, Handle(710), FdKind::Pipe);
    add_read(&mut evq, &table, &mut source, &mut kqueue, kqfd);
    table.close_fd(10).unwrap();
    table.insert(10, Handle(711), FdKind::Pipe);
    let mut events = [Kevent::default(); 1];
    let count = kevent(
        &mut evq,
        &table,
        &mut source,
        &mut kqueue,
        kqfd,
        &[change(10, EVFILT_READ, EV_DELETE, 0, 0)],
        &mut events,
        None,
    )
    .unwrap();
    assert_eq!(count, 1);
    assert_eq!(events[0].data, i64::from(Errno::ENOENT.0));
}

#[test]
fn delete_closes_the_timer_source() {
    let (mut evq, table, mut source, mut kqueue, kqfd) = setup();
    let mut events = [Kevent::default(); 2];
    kevent(
        &mut evq,
        &table,
        &mut source,
        &mut kqueue,
        kqfd,
        &[change(1, EVFILT_TIMER, EV_ADD, 0, 5)],
        &mut events,
        None,
    )
    .unwrap();
    kevent(
        &mut evq,
        &table,
        &mut source,
        &mut kqueue,
        kqfd,
        &[change(1, EVFILT_TIMER, EV_DELETE, 0, 0)],
        &mut events,
        None,
    )
    .unwrap();
    assert_eq!(source.closed, [Handle(901)]);
}

#[test]
fn oneshot_timer_closes_its_source_after_delivery() {
    let (mut evq, table, mut source, mut kqueue, kqfd) = setup();
    let mut events = [Kevent::default(); 1];
    kevent(
        &mut evq,
        &table,
        &mut source,
        &mut kqueue,
        kqfd,
        &[change(1, EVFILT_TIMER, EV_ADD | EV_ONESHOT, 0, 5)],
        &mut events,
        None,
    )
    .unwrap();
    let cookie = evq.cookie_of(Handle(901)).unwrap();
    evq.push(cookie, EV_SIGNALED);
    kevent(
        &mut evq,
        &table,
        &mut source,
        &mut kqueue,
        kqfd,
        &[],
        &mut events,
        None,
    )
    .unwrap();
    assert_eq!(source.closed, [Handle(901)]);
}

#[test]
fn readd_of_timer_rearms_the_same_source() {
    let (mut evq, table, mut source, mut kqueue, kqfd) = setup();
    let mut events = [Kevent::default(); 2];
    kevent(
        &mut evq,
        &table,
        &mut source,
        &mut kqueue,
        kqfd,
        &[change(1, EVFILT_TIMER, EV_ADD, 0, 5)],
        &mut events,
        None,
    )
    .unwrap();
    kevent(
        &mut evq,
        &table,
        &mut source,
        &mut kqueue,
        kqfd,
        &[change(1, EVFILT_TIMER, EV_ADD, 0, 9)],
        &mut events,
        None,
    )
    .unwrap();
    assert_eq!(source.timers, 1);
    assert_eq!(source.timer_sets.last(), Some(&(Handle(901), 9_000_000, 9_000_000)));
}

#[test]
fn eintr_is_propagated() {
    let (mut evq, table, mut source, mut kqueue, kqfd) = setup();
    evq.wait_error = Some(Errno::EINTR);
    let mut events = [Kevent::default(); 1];
    assert_eq!(
        kevent(
            &mut evq,
            &table,
            &mut source,
            &mut kqueue,
            kqfd,
            &[],
            &mut events,
            Some(Timespec {
                tv_sec: 5,
                tv_nsec: 0,
            })
        ),
        Err(Errno::EINTR)
    );
}

#[test]
fn kqueue1_accepts_o_nonblock() {
    let mut evq = FakeEvq::new();
    let mut table = FakeFdTable::new();
    assert!(kqueue1(&mut evq, &mut table, raven_libc_abi::fcntl::O_NONBLOCK).is_ok());
    let fd = kqueue1(&mut evq, &mut table, raven_libc_abi::fcntl::O_NONBLOCK).unwrap();
    assert!(!table.cloexec(fd));
}

#[test]
fn known_filter_boundary_is_explicit() {
    let (mut evq, table, mut source, mut kqueue, kqfd) = setup();
    let mut events = [Kevent::default(); 1];
    let count = kevent(
        &mut evq,
        &table,
        &mut source,
        &mut kqueue,
        kqfd,
        &[change(1, -16, EV_ADD, 0, 0)],
        &mut events,
        None,
    )
    .unwrap();
    assert_eq!(count, 1);
    assert_eq!(events[0].data, i64::from(Errno::EINVAL.0));
}

#[test]
fn proc_note_exit_delivers_eof() {
    let (mut evq, table, mut source, mut kqueue, kqfd) = setup();
    source.process_available = true;
    let mut events = [Kevent::default(); 1];
    let count = kevent(
        &mut evq,
        &table,
        &mut source,
        &mut kqueue,
        kqfd,
        &[change(42, EVFILT_PROC, EV_ADD, NOTE_EXIT, 0)],
        &mut events,
        None,
    )
    .unwrap();
    assert_eq!(count, 0);
    let cookie = evq.cookie_of(Handle(1012)).unwrap();
    evq.push(cookie, EV_HUP);
    let count = kevent(
        &mut evq,
        &table,
        &mut source,
        &mut kqueue,
        kqfd,
        &[],
        &mut events,
        None,
    )
    .unwrap();
    assert_eq!(count, 1);
    assert_eq!(events[0].ident, 42);
    assert_eq!(events[0].flags & EV_EOF, EV_EOF);
    assert!(source.closed.is_empty());
}

#[test]
fn user_trigger_delivers_registered_fflags() {
    let (mut evq, table, mut source, mut kqueue, kqfd) = setup();
    let mut events = [Kevent::default(); 1];
    let count = kevent(
        &mut evq,
        &table,
        &mut source,
        &mut kqueue,
        kqfd,
        &[change(1, EVFILT_USER, EV_ADD, NOTE_TRIGGER, 0)],
        &mut events,
        None,
    )
    .unwrap();
    assert_eq!(count, 0);
    let cookie = evq.cookie_of(Handle(951)).unwrap();
    evq.push(cookie, EV_SIGNALED);
    let count = kevent(
        &mut evq,
        &table,
        &mut source,
        &mut kqueue,
        kqfd,
        &[],
        &mut events,
        None,
    )
    .unwrap();
    assert_eq!(count, 1);
    assert_eq!(events[0].fflags, NOTE_TRIGGER);
    assert_eq!(events[0].data, 0);
}

#[test]
fn add_disabled_creates_no_watch_until_enabled() {
    let (mut evq, mut table, mut source, mut kqueue, kqfd) = setup();
    table.insert(10, Handle(710), FdKind::Pipe);
    let mut events = [Kevent::default(); 2];
    let count = kevent(
        &mut evq,
        &table,
        &mut source,
        &mut kqueue,
        kqfd,
        &[change(10, EVFILT_READ, EV_ADD | EV_DISABLE, 0, 0)],
        &mut events,
        None,
    )
    .unwrap();
    assert_eq!(count, 0);
    assert_eq!(evq.cookie_of(Handle(710)), None);
    let count = kevent(
        &mut evq,
        &table,
        &mut source,
        &mut kqueue,
        kqfd,
        &[change(10, EVFILT_READ, EV_ENABLE, 0, 0)],
        &mut events,
        None,
    )
    .unwrap();
    assert_eq!(count, 0);
    assert!(evq.cookie_of(Handle(710)).is_some());
}


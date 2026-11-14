// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `epoll` table and lifecycle tests (ticket C66 section 7(a), 5.H family `epoll`).

use super::*;
use crate::seams::Handle;
use crate::testutil::{FakeEvq, FakeFdTable, header_define};

const EPOLL_H: &str = include_str!("../../../include/sys/epoll.h");

fn setup() -> (FakeEvq, FakeFdTable, EpollTable, i32) {
    let mut evq = FakeEvq::new();
    let mut table = FakeFdTable::new();
    let epfd = create1(&mut evq, &mut table, 0).unwrap();
    (evq, table, EpollTable::new(), epfd)
}

fn add_pipe(table: &mut FakeFdTable, fd: i32) {
    table.insert(fd, Handle(700u32.saturating_add(fd as u32)), FdKind::Pipe);
}

#[test]
fn epoll_event_is_twelve_bytes_on_x86_64() {
    #[cfg(target_arch = "x86_64")]
    assert_eq!(core::mem::size_of::<EpollEvent>(), 12);
    #[cfg(not(target_arch = "x86_64"))]
    assert_eq!(core::mem::size_of::<EpollEvent>(), 16);
}

#[test]
fn header_epoll_bits_match() {
    for (name, value) in [
        ("EPOLLIN", EPOLLIN),
        ("EPOLLPRI", EPOLLPRI),
        ("EPOLLOUT", EPOLLOUT),
        ("EPOLLERR", EPOLLERR),
        ("EPOLLHUP", EPOLLHUP),
        ("EPOLLNVAL", EPOLLNVAL),
        ("EPOLLRDNORM", EPOLLRDNORM),
        ("EPOLLRDBAND", EPOLLRDBAND),
        ("EPOLLWRNORM", EPOLLWRNORM),
        ("EPOLLWRBAND", EPOLLWRBAND),
        ("EPOLLMSG", EPOLLMSG),
        ("EPOLLRDHUP", EPOLLRDHUP),
        ("EPOLLEXCLUSIVE", EPOLLEXCLUSIVE),
        ("EPOLLWAKEUP", EPOLLWAKEUP),
        ("EPOLLONESHOT", EPOLLONESHOT),
        ("EPOLLET", EPOLLET),
        ("EPOLL_CTL_ADD", EPOLL_CTL_ADD as u32),
        ("EPOLL_CTL_DEL", EPOLL_CTL_DEL as u32),
        ("EPOLL_CTL_MOD", EPOLL_CTL_MOD as u32),
    ] {
        assert_eq!(
            header_define(EPOLL_H, name),
            Some(i64::from(value)),
            "{name}"
        );
    }
}

macro_rules! in_row {
    ($name:ident, $bit:expr, $evq:expr) => {
        #[test]
        fn $name() {
            let mask = translate_in($bit);
            assert_eq!(mask & $evq, $evq);
        }
    };
}

in_row!(in_in_readable, EPOLLIN, EV_READABLE);
in_row!(in_pri_readable, EPOLLPRI, EV_READABLE);
in_row!(in_rdnorm_readable, EPOLLRDNORM, EV_READABLE);
in_row!(in_rdband_readable, EPOLLRDBAND, EV_READABLE);
in_row!(in_out_writable, EPOLLOUT, EV_WRITABLE);
in_row!(in_wrnorm_writable, EPOLLWRNORM, EV_WRITABLE);
in_row!(in_wrband_writable, EPOLLWRBAND, EV_WRITABLE);
in_row!(in_always_hup, 0, EV_HUP);
in_row!(in_hup_hup, EPOLLHUP, EV_HUP);
in_row!(in_err_hup, EPOLLERR, EV_HUP);
in_row!(in_rdhup_hup, EPOLLRDHUP, EV_HUP);
in_row!(in_et_edge, EPOLLET, EV_EDGE);
in_row!(
    in_in_and_out_both,
    EPOLLIN | EPOLLOUT,
    EV_READABLE | EV_WRITABLE
);

#[test]
fn in_ignored_bits_add_nothing_but_hup() {
    assert_eq!(
        translate_in(EPOLLEXCLUSIVE | EPOLLWAKEUP | EPOLLMSG | EPOLLNVAL),
        EV_HUP
    );
}

macro_rules! out_row {
    ($name:ident, $delivered:expr, $registered:expr, $expected:expr) => {
        #[test]
        fn $name() {
            assert_eq!(translate_out($delivered, $registered), $expected);
        }
    };
}

out_row!(out_read_in, EV_READABLE, EPOLLIN, EPOLLIN);
out_row!(
    out_read_rdnorm,
    EV_READABLE,
    EPOLLRDNORM,
    EPOLLIN | EPOLLRDNORM
);
out_row!(out_read_pri, EV_READABLE, EPOLLPRI, EPOLLIN);
out_row!(out_read_not_requested, EV_READABLE, EPOLLOUT, 0);
out_row!(out_write_out, EV_WRITABLE, EPOLLOUT, EPOLLOUT);
out_row!(
    out_write_wrnorm,
    EV_WRITABLE,
    EPOLLWRNORM,
    EPOLLOUT | EPOLLWRNORM
);
out_row!(out_hup_always_reported, EV_HUP, EPOLLIN, EPOLLHUP);
out_row!(out_hup_rdhup, EV_HUP, EPOLLRDHUP, EPOLLHUP | EPOLLRDHUP);
out_row!(
    out_err_never_invented,
    EV_READABLE,
    EPOLLIN | EPOLLERR,
    EPOLLIN
);
out_row!(
    out_edge_adds_nothing,
    EV_READABLE | EV_EDGE,
    EPOLLIN | EPOLLET,
    EPOLLIN
);
out_row!(out_signal_not_reported, EV_SIGNALED, EPOLLIN, 0);

#[test]
fn cookie_roundtrip() {
    let value = cookie(42, 0xdead_beef);
    assert_eq!(cookie_fd(value), 42);
    assert_eq!(cookie_epoch(value), 0xdead_beef);
}

#[test]
fn create_size_zero_is_einval() {
    let (mut evq, mut table) = (FakeEvq::new(), FakeFdTable::new());
    assert_eq!(create(&mut evq, &mut table, 0), Err(Errno::EINVAL));
}

#[test]
fn create_positive_size_installs() {
    let (mut evq, mut table) = (FakeEvq::new(), FakeFdTable::new());
    let fd = create(&mut evq, &mut table, 16).unwrap();
    assert_eq!(table.kind_of(fd), Ok(FdKind::Epoll));
}

#[test]
fn create1_rejects_unknown_flags() {
    let (mut evq, mut table) = (FakeEvq::new(), FakeFdTable::new());
    assert_eq!(create1(&mut evq, &mut table, 0x4000), Err(Errno::EINVAL));
}

#[test]
fn create1_cloexec_is_stored() {
    let (mut evq, mut table) = (FakeEvq::new(), FakeFdTable::new());
    let fd = create1(&mut evq, &mut table, EPOLL_CLOEXEC).unwrap();
    assert!(table.cloexec(fd));
}

#[test]
fn install_failure_propagates() {
    let (mut evq, mut table) = (FakeEvq::new(), FakeFdTable::new());
    table.install_error = Some(Errno::EMFILE);
    assert_eq!(create1(&mut evq, &mut table, 0), Err(Errno::EMFILE));
}

#[test]
fn add_unknown_epfd_is_ebadf() {
    let (mut evq, mut table, mut ep) = (FakeEvq::new(), FakeFdTable::new(), EpollTable::new());
    add_pipe(&mut table, 10);
    let event = EpollEvent {
        events: EPOLLIN,
        data: 0,
    };
    assert_eq!(
        ctl(
            &mut evq,
            &mut table,
            &mut ep,
            99,
            EPOLL_CTL_ADD,
            10,
            Some(&event)
        ),
        Err(Errno::EBADF)
    );
}

#[test]
fn add_non_epoll_is_einval() {
    let (mut evq, mut table, mut ep, epfd) = setup();
    add_pipe(&mut table, 10);
    table.insert(11, Handle(711), FdKind::Pipe);
    let event = EpollEvent {
        events: EPOLLIN,
        data: 0,
    };
    assert_eq!(
        ctl(
            &mut evq,
            &mut table,
            &mut ep,
            11,
            EPOLL_CTL_ADD,
            10,
            Some(&event)
        ),
        Err(Errno::EINVAL)
    );
    assert_eq!(table.kind_of(epfd), Ok(FdKind::Epoll));
}

#[test]
fn add_epoll_fd_itself_is_einval() {
    let (mut evq, mut table, mut ep, epfd) = setup();
    let event = EpollEvent {
        events: EPOLLIN,
        data: 0,
    };
    assert_eq!(
        ctl(
            &mut evq,
            &mut table,
            &mut ep,
            epfd,
            EPOLL_CTL_ADD,
            epfd,
            Some(&event)
        ),
        Err(Errno::EINVAL)
    );
}

#[test]
fn add_regular_file_is_eperm() {
    let (mut evq, mut table, mut ep, epfd) = setup();
    table.insert(10, Handle(710), FdKind::File);
    let event = EpollEvent {
        events: EPOLLIN,
        data: 0,
    };
    assert_eq!(
        ctl(
            &mut evq,
            &mut table,
            &mut ep,
            epfd,
            EPOLL_CTL_ADD,
            10,
            Some(&event)
        ),
        Err(Errno::EPERM)
    );
}

#[test]
fn add_same_fd_twice_is_eexist() {
    let (mut evq, mut table, mut ep, epfd) = setup();
    add_pipe(&mut table, 10);
    let event = EpollEvent {
        events: EPOLLIN,
        data: 7,
    };
    assert_eq!(
        ctl(
            &mut evq,
            &mut table,
            &mut ep,
            epfd,
            EPOLL_CTL_ADD,
            10,
            Some(&event)
        ),
        Ok(())
    );
    assert_eq!(
        ctl(
            &mut evq,
            &mut table,
            &mut ep,
            epfd,
            EPOLL_CTL_ADD,
            10,
            Some(&event)
        ),
        Err(Errno::EEXIST)
    );
}

#[test]
fn add_null_event_is_efault() {
    let (mut evq, mut table, mut ep, epfd) = setup();
    add_pipe(&mut table, 10);
    assert_eq!(
        ctl(&mut evq, &mut table, &mut ep, epfd, EPOLL_CTL_ADD, 10, None),
        Err(Errno::EFAULT)
    );
}

#[test]
fn add_uses_epoch_cookie_and_hup_mask() {
    let (mut evq, mut table, mut ep, epfd) = setup();
    add_pipe(&mut table, 10);
    let epoch = table.epoch(10);
    let event = EpollEvent {
        events: EPOLLIN | EPOLLET,
        data: 9,
    };
    ctl(
        &mut evq,
        &mut table,
        &mut ep,
        epfd,
        EPOLL_CTL_ADD,
        10,
        Some(&event),
    )
    .unwrap();
    let (op, _, mask, cookie_value) = *evq.ops.last().unwrap();
    assert_eq!(op, EvqCtl::Add);
    assert_eq!(
        mask & (EV_READABLE | EV_HUP | EV_EDGE),
        EV_READABLE | EV_HUP | EV_EDGE
    );
    assert_eq!(cookie_value, cookie(10, epoch));
}

#[test]
fn mod_unknown_fd_is_enoent() {
    let (mut evq, mut table, mut ep, epfd) = setup();
    add_pipe(&mut table, 10);
    let event = EpollEvent {
        events: EPOLLIN,
        data: 0,
    };
    assert_eq!(
        ctl(
            &mut evq,
            &mut table,
            &mut ep,
            epfd,
            EPOLL_CTL_MOD,
            10,
            Some(&event)
        ),
        Err(Errno::ENOENT)
    );
}

#[test]
fn mod_rejects_exclusive() {
    let (mut evq, mut table, mut ep, epfd) = setup();
    add_pipe(&mut table, 10);
    let event = EpollEvent {
        events: EPOLLIN,
        data: 0,
    };
    ctl(
        &mut evq,
        &mut table,
        &mut ep,
        epfd,
        EPOLL_CTL_ADD,
        10,
        Some(&event),
    )
    .unwrap();
    let exclusive = EpollEvent {
        events: EPOLLIN | EPOLLEXCLUSIVE,
        data: 0,
    };
    assert_eq!(
        ctl(
            &mut evq,
            &mut table,
            &mut ep,
            epfd,
            EPOLL_CTL_MOD,
            10,
            Some(&exclusive)
        ),
        Err(Errno::EINVAL)
    );
}

#[test]
fn mod_updates_events_and_data() {
    let (mut evq, mut table, mut ep, epfd) = setup();
    add_pipe(&mut table, 10);
    let event = EpollEvent {
        events: EPOLLIN,
        data: 1,
    };
    ctl(
        &mut evq,
        &mut table,
        &mut ep,
        epfd,
        EPOLL_CTL_ADD,
        10,
        Some(&event),
    )
    .unwrap();
    let changed = EpollEvent {
        events: EPOLLOUT | EPOLLONESHOT,
        data: 2,
    };
    ctl(
        &mut evq,
        &mut table,
        &mut ep,
        epfd,
        EPOLL_CTL_MOD,
        10,
        Some(&changed),
    )
    .unwrap();
    let reg = ep.registrations(epfd).unwrap().get(&10).copied().unwrap();
    assert_eq!(reg.events, EPOLLOUT | EPOLLONESHOT);
    assert_eq!(reg.data, 2);
}

#[test]
fn del_unknown_is_enoent() {
    let (mut evq, mut table, mut ep, epfd) = setup();
    add_pipe(&mut table, 10);
    assert_eq!(
        ctl(&mut evq, &mut table, &mut ep, epfd, EPOLL_CTL_DEL, 10, None),
        Err(Errno::ENOENT)
    );
}

#[test]
fn del_removes_the_registration() {
    let (mut evq, mut table, mut ep, epfd) = setup();
    add_pipe(&mut table, 10);
    let event = EpollEvent {
        events: EPOLLIN,
        data: 0,
    };
    ctl(
        &mut evq,
        &mut table,
        &mut ep,
        epfd,
        EPOLL_CTL_ADD,
        10,
        Some(&event),
    )
    .unwrap();
    ctl(&mut evq, &mut table, &mut ep, epfd, EPOLL_CTL_DEL, 10, None).unwrap();
    assert!(ep.is_empty(epfd));
    assert_eq!(evq.ops.last().unwrap().0, EvqCtl::Del);
}

#[test]
fn bad_operation_is_einval() {
    let (mut evq, mut table, mut ep, epfd) = setup();
    add_pipe(&mut table, 10);
    assert_eq!(
        ctl(&mut evq, &mut table, &mut ep, epfd, 99, 10, None),
        Err(Errno::EINVAL)
    );
}

#[test]
fn registration_cap_is_enospc() {
    let (mut evq, mut table, mut ep, epfd) = setup();
    let regs = ep.instances.entry(epfd).or_default();
    for fd in 0..MAX_REGISTRATIONS {
        regs.insert(
            i32::try_from(fd).unwrap(),
            Reg {
                events: EPOLLIN,
                data: 0,
                epoch: 1,
                disabled: false,
            },
        );
    }
    add_pipe(&mut table, 5000);
    let event = EpollEvent {
        events: EPOLLIN,
        data: 0,
    };
    assert_eq!(
        ctl(
            &mut evq,
            &mut table,
            &mut ep,
            epfd,
            EPOLL_CTL_ADD,
            5000,
            Some(&event)
        ),
        Err(Errno::ENOSPC)
    );
}

#[test]
fn timeout_conversion() {
    assert_eq!(timeout_ns(-1), 0);
    assert_eq!(timeout_ns(0), 1);
    assert_eq!(timeout_ns(10), 10_000_000);
    assert_eq!(timeout_ns(i32::MAX), 2_147_483_647_000_000);
}

#[test]
fn wait_empty_output_is_einval() {
    let (mut evq, table, mut ep, epfd) = setup();
    assert_eq!(
        wait(&mut evq, &table, &mut ep, epfd, &mut [], 0, None),
        Err(Errno::EINVAL)
    );
}

#[test]
fn wait_with_mask_is_enosys() {
    let (mut evq, table, mut ep, epfd) = setup();
    let mut out = [EpollEvent { events: 0, data: 0 }; 1];
    assert_eq!(
        wait(&mut evq, &table, &mut ep, epfd, &mut out, 0, Some(&[0, 0])),
        Err(Errno::ENOSYS)
    );
}

#[test]
fn wait_delivers_matching_event() {
    let (mut evq, mut table, mut ep, epfd) = setup();
    add_pipe(&mut table, 10);
    let event = EpollEvent {
        events: EPOLLIN,
        data: 0xabc,
    };
    ctl(
        &mut evq,
        &mut table,
        &mut ep,
        epfd,
        EPOLL_CTL_ADD,
        10,
        Some(&event),
    )
    .unwrap();
    let epoch = table.epoch(10);
    evq.push(cookie(10, epoch), EV_READABLE);
    let mut out = [EpollEvent { events: 0, data: 0 }; 2];
    let count = wait(&mut evq, &table, &mut ep, epfd, &mut out, 0, None).unwrap();
    assert_eq!(count, 1);
    let delivered_events = out[0].events;
    let delivered_data = out[0].data;
    assert_eq!(delivered_events, EPOLLIN);
    assert_eq!(delivered_data, 0xabc);
}

#[test]
fn wait_drops_stale_epoch() {
    let (mut evq, mut table, mut ep, epfd) = setup();
    add_pipe(&mut table, 10);
    let event = EpollEvent {
        events: EPOLLIN,
        data: 1,
    };
    ctl(
        &mut evq,
        &mut table,
        &mut ep,
        epfd,
        EPOLL_CTL_ADD,
        10,
        Some(&event),
    )
    .unwrap();
    let old_epoch = table.epoch(10);
    table.close_fd(10).unwrap();
    add_pipe(&mut table, 10);
    assert_ne!(table.epoch(10), old_epoch);
    evq.push(cookie(10, old_epoch), EV_READABLE);
    let mut out = [EpollEvent { events: 0, data: 0 }; 1];
    assert_eq!(
        wait(&mut evq, &table, &mut ep, epfd, &mut out, 0, None),
        Ok(0)
    );
}

#[test]
fn oneshot_disables_after_first_delivery() {
    let (mut evq, mut table, mut ep, epfd) = setup();
    add_pipe(&mut table, 10);
    let epoch = table.epoch(10);
    let event = EpollEvent {
        events: EPOLLIN | EPOLLONESHOT,
        data: 1,
    };
    ctl(
        &mut evq,
        &mut table,
        &mut ep,
        epfd,
        EPOLL_CTL_ADD,
        10,
        Some(&event),
    )
    .unwrap();
    evq.push(cookie(10, epoch), EV_READABLE);
    let mut out = [EpollEvent { events: 0, data: 0 }; 1];
    assert_eq!(
        wait(&mut evq, &table, &mut ep, epfd, &mut out, 0, None),
        Ok(1)
    );
    evq.push(cookie(10, epoch), EV_READABLE);
    assert_eq!(
        wait(&mut evq, &table, &mut ep, epfd, &mut out, 0, None),
        Ok(0)
    );
    let disarm = evq
        .ops
        .iter()
        .rev()
        .find(|(op, _, mask, _)| *op == EvqCtl::Mod && *mask == 0);
    assert!(disarm.is_some());
}

#[test]
fn mod_rearms_oneshot() {
    let (mut evq, mut table, mut ep, epfd) = setup();
    add_pipe(&mut table, 10);
    let epoch = table.epoch(10);
    let event = EpollEvent {
        events: EPOLLIN | EPOLLONESHOT,
        data: 1,
    };
    ctl(
        &mut evq,
        &mut table,
        &mut ep,
        epfd,
        EPOLL_CTL_ADD,
        10,
        Some(&event),
    )
    .unwrap();
    evq.push(cookie(10, epoch), EV_READABLE);
    let mut out = [EpollEvent { events: 0, data: 0 }; 1];
    assert_eq!(
        wait(&mut evq, &table, &mut ep, epfd, &mut out, 0, None),
        Ok(1)
    );
    ctl(
        &mut evq,
        &mut table,
        &mut ep,
        epfd,
        EPOLL_CTL_MOD,
        10,
        Some(&event),
    )
    .unwrap();
    evq.push(cookie(10, epoch), EV_READABLE);
    assert_eq!(
        wait(&mut evq, &table, &mut ep, epfd, &mut out, 0, None),
        Ok(1)
    );
}

#[test]
fn wait_reports_hup_for_every_registration() {
    let (mut evq, mut table, mut ep, epfd) = setup();
    add_pipe(&mut table, 10);
    let epoch = table.epoch(10);
    let event = EpollEvent {
        events: EPOLLIN,
        data: 0,
    };
    ctl(
        &mut evq,
        &mut table,
        &mut ep,
        epfd,
        EPOLL_CTL_ADD,
        10,
        Some(&event),
    )
    .unwrap();
    evq.push(cookie(10, epoch), EV_HUP);
    let mut out = [EpollEvent { events: 0, data: 0 }; 1];
    assert_eq!(
        wait(&mut evq, &table, &mut ep, epfd, &mut out, 0, None),
        Ok(1)
    );
    let delivered = out[0].events;
    assert_eq!(delivered, EPOLLHUP);
}

#[test]
fn wait_timeout_is_forwarded() {
    let (mut evq, table, mut ep, epfd) = setup();
    let mut out = [EpollEvent { events: 0, data: 0 }; 1];
    assert_eq!(
        wait(&mut evq, &table, &mut ep, epfd, &mut out, 250, None),
        Ok(0)
    );
    assert_eq!(evq.last_timeout_ns, Some(250_000_000));
}

#[test]
fn wait_forever_is_zero_ns() {
    let (mut evq, table, mut ep, epfd) = setup();
    let mut out = [EpollEvent { events: 0, data: 0 }; 1];
    assert_eq!(
        wait(&mut evq, &table, &mut ep, epfd, &mut out, -1, None),
        Ok(0)
    );
    assert_eq!(evq.last_timeout_ns, Some(0));
}

#[test]
fn wait_poll_is_one_ns() {
    let (mut evq, table, mut ep, epfd) = setup();
    let mut out = [EpollEvent { events: 0, data: 0 }; 1];
    assert_eq!(
        wait(&mut evq, &table, &mut ep, epfd, &mut out, 0, None),
        Ok(0)
    );
    assert_eq!(evq.last_timeout_ns, Some(1));
}

#[test]
fn wait_passes_eintr_through() {
    let (mut evq, table, mut ep, epfd) = setup();
    evq.wait_error = Some(Errno::EINTR);
    let mut out = [EpollEvent { events: 0, data: 0 }; 1];
    assert_eq!(
        wait(&mut evq, &table, &mut ep, epfd, &mut out, 0, None),
        Err(Errno::EINTR)
    );
}

#[test]
fn pwait_with_null_mask_waits() {
    let (mut evq, table, mut ep, epfd) = setup();
    let mut out = [EpollEvent { events: 0, data: 0 }; 1];
    assert_eq!(
        pwait(&mut evq, &table, &mut ep, epfd, &mut out, 0, None),
        Ok(0)
    );
}

#[test]
fn pwait2_converts_and_rejects_negative() {
    let (mut evq, table, mut ep, epfd) = setup();
    let mut out = [EpollEvent { events: 0, data: 0 }; 1];
    assert_eq!(
        pwait2(&mut evq, &table, &mut ep, epfd, &mut out, Some(-1), None),
        Err(Errno::EINVAL)
    );
    assert_eq!(
        pwait2(
            &mut evq,
            &table,
            &mut ep,
            epfd,
            &mut out,
            Some(1_500_000),
            None
        ),
        Ok(0)
    );
    assert_eq!(evq.last_timeout_ns, Some(2_000_000));
}

#[test]
fn wait_drops_unregistered_cookie() {
    let (mut evq, table, mut ep, epfd) = setup();
    evq.push(cookie(77, 1), EV_READABLE);
    let mut out = [EpollEvent { events: 0, data: 0 }; 1];
    assert_eq!(
        wait(&mut evq, &table, &mut ep, epfd, &mut out, 0, None),
        Ok(0)
    );
}

#[test]
fn wait_reports_readable_only_when_requested() {
    let (mut evq, mut table, mut ep, epfd) = setup();
    add_pipe(&mut table, 10);
    let epoch = table.epoch(10);
    let event = EpollEvent {
        events: EPOLLOUT,
        data: 0,
    };
    ctl(
        &mut evq,
        &mut table,
        &mut ep,
        epfd,
        EPOLL_CTL_ADD,
        10,
        Some(&event),
    )
    .unwrap();
    evq.push(cookie(10, epoch), EV_READABLE);
    let mut out = [EpollEvent { events: 0, data: 0 }; 1];
    assert_eq!(
        wait(&mut evq, &table, &mut ep, epfd, &mut out, 0, None),
        Ok(0)
    );
}

#[test]
fn all_four_read_bits_are_readable() {
    for bit in [EPOLLIN, EPOLLPRI, EPOLLRDNORM, EPOLLRDBAND] {
        assert_eq!(translate_in(bit) & EV_READABLE, EV_READABLE);
    }
}

#[test]
fn all_three_write_bits_are_writable() {
    for bit in [EPOLLOUT, EPOLLWRNORM, EPOLLWRBAND] {
        assert_eq!(translate_in(bit) & EV_WRITABLE, EV_WRITABLE);
    }
}

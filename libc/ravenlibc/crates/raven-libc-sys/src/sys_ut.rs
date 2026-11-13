// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for [`crate::sys`].

#![allow(clippy::indexing_slicing)]

use super::*;

use host::collections::BTreeSet;

#[test]
fn every_syscall_variant_has_a_wrapper_name() {
    assert_eq!(WRAPPER_COUNT, Syscall::ALL.len());
    let mut names = BTreeSet::new();
    for call in Syscall::ALL {
        let name = wrapper_name(*call);
        assert!(!name.is_empty(), "{call:?} has no typed wrapper");
        assert!(names.insert(name), "{call:?} reuses wrapper name {name}");
    }
    assert_eq!(names.len(), Syscall::ALL.len());
}

#[test]
fn the_variant_list_matches_the_pinned_abi() {
    let expected = [
        Syscall::EXIT,
        Syscall::YIELD,
        Syscall::DEBUG_WRITE,
        Syscall::CLOCK_GET,
        Syscall::NANOSLEEP,
        Syscall::UNAME,
        Syscall::DEBUG_READ,
        Syscall::THREAD_CREATE,
        Syscall::THREAD_EXIT,
        Syscall::FUTEX,
        Syscall::PROC_CREATE,
        Syscall::PROC_WAIT,
        Syscall::PROC_TERMINATE,
        Syscall::PROC_NAMESPACE,
        Syscall::MAP,
        Syscall::UNMAP,
        Syscall::PROTECT,
        Syscall::MEMOBJ_CREATE,
        Syscall::PAGER_SUPPLY,
        Syscall::HANDLE_CLOSE,
        Syscall::HANDLE_DUP,
        Syscall::HANDLE_TRANSFER,
        Syscall::HANDLE_REVOKE,
        Syscall::EVQ_CREATE,
        Syscall::EVQ_CTL,
        Syscall::EVQ_WAIT,
        Syscall::TIMER_CREATE,
        Syscall::EVENT_CREATE,
        Syscall::EVENT_SIGNAL,
        Syscall::TIMER_SET,
        Syscall::SCHEME_REGISTER,
        Syscall::OPEN,
        Syscall::READ,
        Syscall::WRITE,
        Syscall::CLOSE,
        Syscall::DUP,
        Syscall::FSTAT,
        Syscall::CALL,
        Syscall::RECV,
        Syscall::SEND,
        Syscall::MMIO_CREATE,
        Syscall::BOOT_BLOB,
        Syscall::DMA_ALLOC,
        Syscall::DMA_INFO,
        Syscall::IRQ_CREATE,
        Syscall::IRQ_WAIT,
        Syscall::IRQ_ACK,
    ];
    assert_eq!(Syscall::ALL, expected);
}

#[test]
fn raven_number_window_is_exact() {
    assert!(!crate::is_raven_syscall_number(0));
    assert!(crate::is_raven_syscall_number(0x01));
    assert!(crate::is_raven_syscall_number(0x100));
    assert!(!crate::is_raven_syscall_number(0x101));
    assert!(!crate::is_raven_syscall_number(0xdead));
}

#[test]
fn decode_window_is_the_from_ret_rule() {
    assert_eq!(crate::errno::ret(0), Ok(0));
    assert_eq!(crate::errno::ret(-4095), Err(Errno(4095)));
    assert_eq!(crate::errno::ret(-4096), Ok(0xffff_ffff_ffff_f000));
    assert_eq!(crate::errno::ret(1), Ok(1));
}

#[test]
fn a_high_map_address_is_success() {
    let address = 0xffff_0000_0000_0000u64 as isize;
    assert_eq!(crate::errno::ret(address), Ok(0xffff_0000_0000_0000));
}

#[test]
fn handle_conversion_keeps_the_maximum_value() {
    let value = u32::MAX as usize;
    assert_eq!(handle_from_ret(value as isize), Ok(Handle(u32::MAX)));
    assert_eq!(handle_from_ret(value as isize + 1), Err(Errno::EINVAL));
}

#[test]
fn handle_conversion_maps_errno() {
    assert_eq!(handle_from_ret(Errno::EPERM.to_ret()), Err(Errno::EPERM));
    assert_eq!(handle_from_ret(7), Ok(Handle(7)));
}

#[test]
fn transfer_rejects_none_and_empty_rights() {
    assert_eq!(
        handle_transfer(Handle::NONE, Handle(1), Rights::READ),
        Err(Errno::EINVAL)
    );
    assert_eq!(
        handle_transfer(Handle(1), Handle::NONE, Rights::READ),
        Err(Errno::EINVAL)
    );
    assert_eq!(
        handle_transfer(Handle(1), Handle(2), Rights::NONE),
        Err(Errno::EINVAL)
    );
}

#[test]
fn evq_event_layout_is_the_abi_layout() {
    assert_eq!(core::mem::size_of::<EvqEvent>(), 16);
    assert_eq!(core::mem::align_of::<EvqEvent>(), 8);
    assert_eq!(core::mem::size_of::<DmaInfo>(), 24);
    assert_eq!(core::mem::size_of::<IrqInfo>(), 16);
}

#[cfg(feature = "host-fake")]
#[test]
fn wrappers_roundtrip_through_the_fake_kernel() {
    let _guard = crate::fake::test_lock();
    crate::fake::reset();
    let ts = clock_get(raven_abi::types::clock::MONOTONIC).unwrap();
    assert!(ts.tv_sec >= 0);
    let before = crate::fake::now_ns();
    nanosleep(5_000).unwrap();
    assert_eq!(crate::fake::now_ns(), before + 5_000);
    let record = uname().unwrap();
    assert_eq!(
        raven_abi::uname::Uname::field_str(&record.sysname),
        Some("RavenBSD")
    );
}

#[cfg(feature = "host-fake")]
#[test]
fn map_wrapper_returns_a_usable_fake_address() {
    let _guard = crate::fake::test_lock();
    crate::fake::reset();
    let addr = map(Handle::SELF_PROC, Handle::NONE, 0, 0, 4096, 3, 0).unwrap();
    assert!(addr >= crate::fake::mem::FAKE_BASE);
    assert_eq!(crate::fake::mapped_bytes(), 4096);
    unmap(Handle::SELF_PROC, addr, 4096).unwrap();
    assert_eq!(crate::fake::mapped_bytes(), 0);
}

#[cfg(feature = "host-fake")]
#[test]
fn scheme_wrappers_route_to_a_registered_server() {
    use host::sync::Arc;

    let _guard = crate::fake::test_lock();
    crate::fake::reset();
    crate::fake::register_scheme(
        b"test",
        Arc::new(crate::fake::scheme::MemScheme::new(&[(
            b"/hello", b"world",
        )])),
    );
    let handle = open(b"test:/hello", 0).unwrap();
    let mut buffer = [0u8; 16];
    assert_eq!(read(handle, &mut buffer).unwrap(), 5);
    assert_eq!(&buffer[..5], b"world");
    close(handle).unwrap();
    assert_eq!(open(b"nope:/hello", 0), Err(Errno::ENOENT));
}

#[cfg(feature = "host-fake")]
#[test]
fn process_wrappers_follow_terminate_then_wait() {
    let _guard = crate::fake::test_lock();
    crate::fake::reset();
    let child = proc_create(0).unwrap();
    proc_terminate(child, 7).unwrap();
    assert_eq!(proc_wait(child), Ok(7));
}

#[cfg(feature = "host-fake")]
#[test]
fn event_and_timer_wrappers_reach_a_queue() {
    let _guard = crate::fake::test_lock();
    crate::fake::reset();
    let event = event_create(0).unwrap();
    let queue = evq_create(0).unwrap();
    evq_ctl(queue, 1, event, 8, 0xfeed).unwrap();
    event_signal(event, 1).unwrap();
    let mut out = [EvqEvent::default(); 4];
    let count = evq_wait(queue, &mut out, 0).unwrap();
    assert_eq!(count, 1);
    assert_eq!(out[0].cookie, 0xfeed);
    assert_eq!(out[0].events, 8);

    let timer = timer_create(raven_abi::types::clock::MONOTONIC, 0).unwrap();
    timer_set(timer, 1000, 0).unwrap();
    crate::fake::advance_ns(2000);
    evq_ctl(queue, 1, timer, 8, 0xbeef).unwrap();
    let count = evq_wait(queue, &mut out, 0).unwrap();
    assert_eq!(count, 1);
    assert_eq!(out[0].cookie, 0xbeef);
}

#[cfg(feature = "host-fake")]
#[test]
fn handle_wrappers_enforce_rights() {
    let _guard = crate::fake::test_lock();
    crate::fake::reset();
    let event = event_create(0).unwrap();
    let read_only = handle_dup(event, Rights::READ).unwrap();
    assert_eq!(event_signal(read_only, 1), Err(Errno::EACCES));
    handle_revoke(event, Rights::SIGNAL).unwrap();
    assert_eq!(event_signal(event, 1), Err(Errno::EACCES));
    handle_close(event).unwrap();
    assert_eq!(handle_close(event), Err(Errno::EBADF));
}

#[cfg(feature = "host-fake")]
#[test]
fn futex_wrappers_report_wake_counts() {
    let _guard = crate::fake::test_lock();
    crate::fake::reset();
    crate::fake::set_futex_blocking(false);
    let word = AtomicU32::new(3);
    assert_eq!(futex_wait(&word, 3, 0), Ok(()));
    assert_eq!(futex_wait(&word, 4, 0), Err(Errno::EAGAIN));
    assert_eq!(futex_wake(&word, 1), Ok(0));
}

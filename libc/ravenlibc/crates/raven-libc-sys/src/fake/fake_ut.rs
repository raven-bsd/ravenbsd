// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the fake kernel.

#![allow(clippy::indexing_slicing)]

use super::*;

use core::sync::atomic::AtomicU32;
use host::sync::Arc;

use crate::sys;

fn fresh() -> host::sync::MutexGuard<'static, ()> {
    let guard = super::test_lock();
    reset();
    guard
}

fn mem_scheme() -> Arc<scheme::MemScheme> {
    Arc::new(scheme::MemScheme::new(&[
        (b"/hello", b"world"),
        (b"/empty", b""),
    ]))
}

#[test]
fn self_process_is_handle_zero() {
    let _guard = fresh();
    let kernel = lock();
    let kernel = kernel.as_ref().unwrap();
    let entry = kernel.handles.entry(Handle::SELF_PROC).unwrap();
    assert!(matches!(entry.object, Object::SelfProcess));
    assert!(entry.rights.contains(Rights::ADMIN));
}

#[test]
fn dup_cannot_grow_rights() {
    let _guard = fresh();
    let event = sys::event_create(0).unwrap();
    let read_only = sys::handle_dup(event, Rights::READ).unwrap();
    assert_eq!(sys::event_signal(read_only, 1), Err(Errno::EACCES));
    let signaller = sys::handle_dup(event, Rights::SIGNAL).unwrap();
    sys::event_signal(signaller, 1).unwrap();
}

#[test]
fn dup_requires_the_dup_right() {
    let _guard = fresh();
    let event = sys::event_create(0).unwrap();
    let stripped = sys::handle_dup(event, Rights::READ).unwrap();
    assert_eq!(sys::handle_dup(stripped, Rights::READ), Err(Errno::EACCES));
}

#[test]
fn close_twice_is_ebadf() {
    let _guard = fresh();
    let event = sys::event_create(0).unwrap();
    sys::handle_close(event).unwrap();
    assert_eq!(sys::handle_close(event), Err(Errno::EBADF));
}

#[test]
fn transfer_requires_the_transfer_right() {
    let _guard = fresh();
    let child = sys::proc_create(0).unwrap();
    let event = sys::event_create(0).unwrap();
    let no_transfer = sys::handle_dup(event, Rights::READ).unwrap();
    assert_eq!(
        sys::handle_transfer(child, no_transfer, Rights::READ),
        Err(Errno::EPERM)
    );
}

#[test]
fn transfer_to_a_child_returns_a_child_handle() {
    let _guard = fresh();
    let child = sys::proc_create(0).unwrap();
    let event = sys::event_create(0).unwrap();
    let transferred = sys::handle_transfer(child, event, Rights::READ).unwrap();
    let kernel = lock();
    let kernel = kernel.as_ref().unwrap();
    assert!(kernel.children[0].handles.entry(transferred).is_ok());
}

#[test]
fn revoke_is_idempotent_and_removes_rights() {
    let _guard = fresh();
    let event = sys::event_create(0).unwrap();
    sys::handle_revoke(event, Rights::SIGNAL).unwrap();
    assert_eq!(sys::event_signal(event, 1), Err(Errno::EACCES));
    sys::handle_revoke(event, Rights::SIGNAL).unwrap();
    assert_eq!(sys::event_signal(event, 1), Err(Errno::EACCES));
}

#[test]
fn closing_one_duplicate_keeps_the_other_alive() {
    let _guard = fresh();
    let event = sys::event_create(0).unwrap();
    let duplicate = sys::handle_dup(event, Rights::ALL).unwrap();
    sys::handle_close(event).unwrap();
    sys::event_signal(duplicate, 1).unwrap();
    sys::handle_close(duplicate).unwrap();
    assert_eq!(sys::handle_close(duplicate), Err(Errno::EBADF));
}

#[test]
fn anonymous_map_is_zeroed_and_accounted() {
    let _guard = fresh();
    let addr = sys::map(Handle::SELF_PROC, Handle::NONE, 0, 0, 8192, 3, 0).unwrap();
    assert_eq!(mapped_bytes(), 8192);
    assert!(peek(addr, 8192).unwrap().iter().all(|&byte| byte == 0));
    sys::unmap(Handle::SELF_PROC, addr, 8192).unwrap();
    assert_eq!(mapped_bytes(), 0);
}

#[test]
fn fixed_map_overlap_is_eexist() {
    let _guard = fresh();
    let addr = mem::FAKE_BASE + 0x10_0000;
    sys::map(
        Handle::SELF_PROC,
        Handle::NONE,
        0,
        addr,
        8192,
        3,
        raven_abi::types::map::FIXED as u32,
    )
    .unwrap();
    assert_eq!(
        sys::map(
            Handle::SELF_PROC,
            Handle::NONE,
            0,
            addr,
            4096,
            3,
            raven_abi::types::map::FIXED as u32,
        ),
        Err(Errno::EEXIST)
    );
}

#[test]
fn map_validates_length_and_protection() {
    let _guard = fresh();
    assert_eq!(
        sys::map(Handle::SELF_PROC, Handle::NONE, 0, 0, 100, 3, 0),
        Err(Errno::EINVAL)
    );
    assert_eq!(
        sys::map(
            Handle::SELF_PROC,
            Handle::NONE,
            0,
            0,
            4096,
            (raven_abi::types::prot::READ
                | raven_abi::types::prot::WRITE
                | raven_abi::types::prot::EXEC) as u32,
            0
        ),
        Err(Errno::EINVAL)
    );
}

#[test]
fn partial_unmap_frees_only_the_range() {
    let _guard = fresh();
    let addr = sys::map(Handle::SELF_PROC, Handle::NONE, 0, 0, 3 * 4096, 3, 0).unwrap();
    sys::unmap(Handle::SELF_PROC, addr + 4096, 4096).unwrap();
    assert_eq!(mapped_bytes(), 2 * 4096);
    assert!(peek(addr + 4096, 4096).is_none());
    sys::unmap(Handle::SELF_PROC, addr, 4096).unwrap();
    sys::unmap(Handle::SELF_PROC, addr + 2 * 4096, 4096).unwrap();
    assert_eq!(mapped_bytes(), 0);
}

#[test]
fn protect_changes_the_page_bits() {
    let _guard = fresh();
    let addr = sys::map(Handle::SELF_PROC, Handle::NONE, 0, 0, 3 * 4096, 3, 0).unwrap();
    sys::protect(Handle::SELF_PROC, addr + 4096, 4096, 1).unwrap();
    assert_eq!(protection(addr), Some(3));
    assert_eq!(protection(addr + 4096), Some(1));
    assert_eq!(protection(addr + 2 * 4096), Some(3));
}

#[test]
fn memobj_create_validates_length_and_flags() {
    let _guard = fresh();
    assert_eq!(sys::memobj_create(0, 0), Err(Errno::EINVAL));
    assert_eq!(sys::memobj_create(100, 0), Err(Errno::EINVAL));
    assert_eq!(sys::memobj_create(4096, 2), Err(Errno::EINVAL));
    let handle = sys::memobj_create(4096, 0).unwrap();
    sys::handle_close(handle).unwrap();
}

#[test]
fn pager_supply_requires_a_pager_object() {
    let _guard = fresh();
    let object = sys::memobj_create(4096, 0).unwrap();
    assert_eq!(sys::pager_supply(object, 0, b"abc"), Err(Errno::EINVAL));
}

#[test]
fn pager_supply_is_visible_in_a_mapping() {
    let _guard = fresh();
    let object = sys::memobj_create(4096, raven_abi::types::memobj::PAGER).unwrap();
    sys::pager_supply(object, 0, b"abc").unwrap();
    let addr = sys::map(Handle::SELF_PROC, object, 0, 0, 4096, 1, 0).unwrap();
    let bytes = peek(addr, 4096).unwrap();
    assert_eq!(&bytes[..3], b"abc");
}

#[test]
fn map_hint_is_honoured_when_free() {
    let _guard = fresh();
    let hint = mem::FAKE_BASE + 0x20_0000;
    let addr = sys::map(Handle::SELF_PROC, Handle::NONE, 0, hint, 4096, 3, 0).unwrap();
    assert_eq!(addr, hint);
}

#[test]
fn clock_get_returns_both_clocks() {
    let _guard = fresh();
    let monotonic = sys::clock_get(raven_abi::types::clock::MONOTONIC).unwrap();
    let realtime = sys::clock_get(raven_abi::types::clock::REALTIME).unwrap();
    assert!(realtime.tv_sec > monotonic.tv_sec);
}

#[test]
fn clock_get_rejects_unknown_ids() {
    let _guard = fresh();
    assert_eq!(sys::clock_get(99), Err(Errno::EINVAL));
}

#[test]
fn nanosleep_advances_the_virtual_clock() {
    let _guard = fresh();
    let before = now_ns();
    sys::nanosleep(123_456).unwrap();
    assert_eq!(now_ns(), before + 123_456);
}

#[test]
fn uname_fills_the_identification_record() {
    let _guard = fresh();
    let record = sys::uname().unwrap();
    assert_eq!(
        raven_abi::uname::Uname::field_str(&record.sysname),
        Some("RavenBSD")
    );
    assert_eq!(
        raven_abi::uname::Uname::field_str(&record.nodename),
        Some("raven-fake")
    );
}

#[test]
fn debug_write_captures_and_limits_output() {
    let _guard = fresh();
    assert_eq!(sys::debug_write(b"hello").unwrap(), 5);
    assert_eq!(debug_output(), b"hello");
    let too_long = host::vec![b'x'; 4097];
    assert_eq!(sys::debug_write(&too_long), Err(Errno::EINVAL));
}

#[test]
fn debug_read_with_a_timeout_advances_the_clock() {
    let _guard = fresh();
    let before = now_ns();
    let mut buffer = [0u8; 8];
    assert_eq!(sys::debug_read(&mut buffer, 5000).unwrap(), 0);
    assert_eq!(now_ns(), before + 5000);
    assert_eq!(sys::debug_read(&mut buffer, 0), Err(Errno::ENOSYS));
}

#[test]
fn open_of_an_unregistered_scheme_is_enoent() {
    let _guard = fresh();
    assert_eq!(sys::open(b"nope:/x", 0), Err(Errno::ENOENT));
}

#[test]
fn scheme_read_and_write_route_to_the_handler() {
    let _guard = fresh();
    register_scheme(b"test", mem_scheme());
    let handle = sys::open(b"test:/hello", 2).unwrap();
    let mut buffer = [0u8; 8];
    assert_eq!(sys::read(handle, &mut buffer).unwrap(), 5);
    assert_eq!(&buffer[..5], b"world");
    assert_eq!(sys::write(handle, b"!!").unwrap(), 2);
    sys::close(handle).unwrap();
    let handle = sys::open(b"test:/hello", 0).unwrap();
    let mut buffer = [0u8; 8];
    assert_eq!(sys::read(handle, &mut buffer).unwrap(), 5);
    assert_eq!(&buffer[..5], b"world");
}

#[test]
fn write_only_file_rejects_read_and_fstat() {
    let _guard = fresh();
    register_scheme(b"test", mem_scheme());
    let handle = sys::open(b"test:/hello", 1).unwrap();
    let mut buffer = [0u8; 8];
    assert_eq!(sys::read(handle, &mut buffer), Err(Errno::EACCES));
    let mut stat = [0u8; 64];
    assert_eq!(sys::fstat(handle, &mut stat), Err(Errno::EACCES));
}

#[test]
fn read_only_file_rejects_write() {
    let _guard = fresh();
    register_scheme(b"test", mem_scheme());
    let handle = sys::open(b"test:/hello", 0).unwrap();
    assert_eq!(sys::write(handle, b"x"), Err(Errno::EACCES));
}

#[test]
fn fstat_writes_the_size_record() {
    let _guard = fresh();
    register_scheme(b"test", mem_scheme());
    let handle = sys::open(b"test:/hello", 0).unwrap();
    let mut stat = [0u8; 64];
    sys::fstat(handle, &mut stat).unwrap();
    let size = u64::from_le_bytes(stat[8..16].try_into().unwrap());
    assert_eq!(size, 5);
}

#[test]
fn call_uses_the_server_rpc() {
    let _guard = fresh();
    register_scheme(b"test", mem_scheme());
    let handle = sys::open(b"test:/hello", 0).unwrap();
    let mut response = [0u8; 8];
    let count = sys::call(handle, b"size", &mut response).unwrap();
    assert_eq!(&response[..count], b"5");
    assert_eq!(
        sys::call(handle, b"other", &mut response),
        Err(Errno::EINVAL)
    );
}

#[test]
fn scheme_dup_intersects_rights() {
    let _guard = fresh();
    register_scheme(b"test", mem_scheme());
    let handle = sys::open(b"test:/hello", 0).unwrap();
    let duplicate = sys::dup(handle, Rights::READ.bits()).unwrap();
    assert_eq!(sys::write(duplicate, b"x"), Err(Errno::EACCES));
    sys::close(duplicate).unwrap();
    sys::close(handle).unwrap();
}

#[test]
fn scheme_split_rejects_paths_without_a_colon() {
    assert_eq!(scheme::SchemeWorld::split(b"noscheme"), Err(Errno::EINVAL));
    assert_eq!(scheme::SchemeWorld::split(b":x"), Err(Errno::EINVAL));
    assert_eq!(
        scheme::SchemeWorld::split(b"a:b"),
        Ok((&b"a"[..], &b"b"[..]))
    );
}

#[test]
fn scheme_register_validates_the_name() {
    let _guard = fresh();
    assert_eq!(sys::scheme_register(b"bad name"), Err(Errno::EINVAL));
    assert_eq!(sys::scheme_register(b"good-name"), Ok(()));
}

#[test]
fn proc_create_rejects_flags() {
    let _guard = fresh();
    assert_eq!(sys::proc_create(1), Err(Errno::EINVAL));
}

#[test]
fn proc_wait_reaps_an_unstarted_process() {
    let _guard = fresh();
    let child = sys::proc_create(0).unwrap();
    assert_eq!(sys::proc_wait(child), Ok(0));
    assert_eq!(sys::proc_terminate(child, 1), Err(Errno::ESRCH));
}

#[test]
fn proc_terminate_stores_the_exit_code() {
    let _guard = fresh();
    let child = sys::proc_create(0).unwrap();
    sys::proc_terminate(child, 9).unwrap();
    assert_eq!(sys::proc_wait(child), Ok(9));
}

#[test]
fn proc_terminate_needs_signal() {
    let _guard = fresh();
    let child = sys::proc_create(0).unwrap();
    let read_only = sys::handle_dup(child, Rights::READ).unwrap();
    assert_eq!(sys::proc_terminate(read_only, 1), Err(Errno::EPERM));
}

#[test]
fn namespace_add_remove_and_clear() {
    let _guard = fresh();
    sys::proc_namespace(Handle::SELF_PROC, raven_abi::types::namespace::ADD, b"test").unwrap();
    sys::proc_namespace(
        Handle::SELF_PROC,
        raven_abi::types::namespace::ADD,
        b"other",
    )
    .unwrap();
    sys::proc_namespace(
        Handle::SELF_PROC,
        raven_abi::types::namespace::REMOVE,
        b"test",
    )
    .unwrap();
    sys::proc_namespace(Handle::SELF_PROC, raven_abi::types::namespace::CLEAR, b"").unwrap();
}

#[test]
fn thread_create_returns_a_handle() {
    let _guard = fresh();
    let thread = sys::thread_create(Handle::SELF_PROC, 0x1000, 0x2000, 0, 0).unwrap();
    assert!(thread.0 > 0);
    sys::handle_close(thread).unwrap();
}

#[test]
fn evq_create_rejects_flags() {
    let _guard = fresh();
    assert_eq!(sys::evq_create(1), Err(Errno::EINVAL));
}

#[test]
fn evq_ctl_validates_the_event_mask() {
    let _guard = fresh();
    let event = sys::event_create(0).unwrap();
    let queue = sys::evq_create(0).unwrap();
    assert_eq!(sys::evq_ctl(queue, 1, event, 0x10, 0), Err(Errno::EINVAL));
}

#[test]
fn evq_wait_timeout_advances_the_clock() {
    let _guard = fresh();
    let queue = sys::evq_create(0).unwrap();
    let mut out = [crate::sys::EvqEvent::default(); 1];
    let before = now_ns();
    assert_eq!(sys::evq_wait(queue, &mut out, 7000), Ok(0));
    assert_eq!(now_ns(), before + 7000);
}

#[test]
fn evq_wait_forever_is_enosys() {
    let _guard = fresh();
    let queue = sys::evq_create(0).unwrap();
    let mut out = [crate::sys::EvqEvent::default(); 1];
    assert_eq!(sys::evq_wait(queue, &mut out, 0), Err(Errno::ENOSYS));
}

#[test]
fn timer_create_validates_the_clock() {
    let _guard = fresh();
    assert_eq!(
        sys::timer_create(raven_abi::types::clock::REALTIME, 0),
        Err(Errno::EINVAL)
    );
    assert_eq!(
        sys::timer_create(raven_abi::types::clock::MONOTONIC, 1),
        Err(Errno::EINVAL)
    );
}

#[test]
fn event_signal_is_consumed_once() {
    let _guard = fresh();
    let event = sys::event_create(0).unwrap();
    let queue = sys::evq_create(0).unwrap();
    sys::evq_ctl(queue, 1, event, 8, 1).unwrap();
    sys::event_signal(event, 1).unwrap();
    let mut out = [crate::sys::EvqEvent::default(); 2];
    assert_eq!(sys::evq_wait(queue, &mut out, 0), Ok(1));
    assert_eq!(sys::evq_wait(queue, &mut out, 0), Err(Errno::ENOSYS));
}

#[test]
fn futex_wait_checks_the_word() {
    let _guard = fresh();
    set_futex_blocking(false);
    let word = AtomicU32::new(0);
    assert_eq!(sys::futex_wait(&word, 0, 0), Ok(()));
    assert_eq!(sys::futex_wait(&word, 1, 0), Err(Errno::EAGAIN));
}

#[test]
fn futex_wake_returns_the_exact_count() {
    let _guard = fresh();
    static WORD: AtomicU32 = AtomicU32::new(0);
    let address = &WORD as *const AtomicU32 as usize;
    set_futex_blocking(true);
    let threads: host::vec::Vec<_> = (0..3)
        .map(|_| {
            host::thread::spawn(|| {
                let _ = sys::futex_wait(&WORD, 0, 0);
            })
        })
        .collect();
    let deadline = host::time::Instant::now() + host::time::Duration::from_secs(2);
    while futex_waiters(address) < 3 && host::time::Instant::now() < deadline {
        host::thread::yield_now();
    }
    assert_eq!(futex_waiters(address), 3);
    assert_eq!(sys::futex_wake(&WORD, 2), Ok(2));
    while futex_waiters(address) > 1 && host::time::Instant::now() < deadline {
        host::thread::yield_now();
    }
    assert_eq!(futex_waiters(address), 1);
    assert_eq!(sys::futex_wake(&WORD, 1), Ok(1));
    for thread in threads {
        thread.join().unwrap();
    }
    set_futex_blocking(false);
}

#[test]
fn futex_timeout_is_etimedout() {
    let _guard = fresh();
    set_futex_blocking(true);
    let word = AtomicU32::new(0);
    assert_eq!(sys::futex_wait(&word, 0, 1_000_000), Err(Errno::ETIMEDOUT));
    set_futex_blocking(false);
}

#[test]
fn unknown_syscall_is_enosys() {
    let _guard = fresh();
    // SAFETY: dispatch validates the number and the fake never executes an instruction.
    let result = unsafe { dispatch(0xdead, [0; 6]) };
    assert_eq!(result, Errno::ENOSYS.to_ret());
}

#[test]
fn driver_calls_are_enosys() {
    let _guard = fresh();
    assert_eq!(
        sys::mmio_create(Handle::ROOT_DEVICE, 0, 4096, 0),
        Err(Errno::ENOSYS)
    );
    assert_eq!(sys::boot_blob(Handle::ROOT_DEVICE, 1), Err(Errno::ENOSYS));
    assert_eq!(
        sys::dma_alloc(Handle::ROOT_DEVICE, 4096, 4096, 32, 0),
        Err(Errno::ENOSYS)
    );
    assert_eq!(
        sys::irq_create(Handle::ROOT_DEVICE, 1, 1, None),
        Err(Errno::ENOSYS)
    );
}

#[test]
fn the_same_script_has_the_same_trace_hash() {
    fn script() {
        let _ = sys::clock_get(raven_abi::types::clock::MONOTONIC).unwrap();
        let addr = sys::map(Handle::SELF_PROC, Handle::NONE, 0, 0, 8192, 3, 0).unwrap();
        sys::protect(Handle::SELF_PROC, addr, 8192, 1).unwrap();
        sys::unmap(Handle::SELF_PROC, addr, 8192).unwrap();
        sys::nanosleep(1000).unwrap();
    }

    let _guard = super::test_lock();
    reset();
    script();
    let first = trace_hash();
    reset();
    script();
    assert_eq!(trace_hash(), first);
}

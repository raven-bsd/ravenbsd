// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use core::sync::atomic::Ordering;

#[test]
fn the_last_thread_calls_the_process_exit_hook() {
    let _serial = crate::test_support::serial();
    let kernel = crate::test_support::MockKernel::new();
    crate::libc_state::LIBC.reset(kernel.tcb());
    crate::thread::list::reset_for_fork_child();
    let pthread = crate::thread::current_pthread_mut(&kernel);
    let address = pthread as *mut raven_libc_tls::Pthread as usize;
    raw::set_links(pthread, address, address);
    raw::install_exit_hook(|code| std::panic::panic_any(code));
    let result = std::panic::catch_unwind(|| exit(&kernel, 7));
    assert!(result.is_err());
    raw::install_exit_hook(|_| -> ! {
        loop {
            std::thread::yield_now();
        }
    });
}

#[test]
fn a_joinable_exit_stores_the_result_and_clears_the_tid() {
    let _serial = crate::test_support::serial();
    let kernel = crate::test_support::MockKernel::new();
    let other = crate::test_support::MockKernel::new();
    crate::libc_state::LIBC.reset(kernel.tcb());
    crate::libc_state::LIBC.add_thread();
    crate::thread::list::reset_for_fork_child();
    let self_pthread = crate::thread::current_pthread_mut(&kernel);
    let self_address = self_pthread as *mut raven_libc_tls::Pthread as usize;
    let peer = crate::thread::current_pthread_mut(&other);
    let peer_address = peer as *mut raven_libc_tls::Pthread as usize;
    raw::set_links(peer, self_address, self_address);
    raw::set_links(self_pthread, peer_address, peer_address);
    raw::install_exit_hook(|code| std::panic::panic_any(code));
    let result = std::panic::catch_unwind(|| exit(&kernel, 0x55));
    assert!(result.is_err());
    assert_eq!(self_pthread.result, 0x55);
    assert_eq!(self_pthread.tid.load(Ordering::Acquire), 0);
    assert_eq!(self_pthread.exit_word.load(Ordering::Acquire), 1);
    assert_eq!(crate::libc_state::LIBC.threads(), 1);
    raw::install_exit_hook(|_| -> ! {
        loop {
            std::thread::yield_now();
        }
    });
}

#[test]
fn thrd_exit_uses_the_same_tail() {
    let _serial = crate::test_support::serial();
    let kernel = crate::test_support::MockKernel::new();
    crate::libc_state::LIBC.reset(kernel.tcb());
    crate::thread::list::reset_for_fork_child();
    let pthread = crate::thread::current_pthread_mut(&kernel);
    let address = pthread as *mut raven_libc_tls::Pthread as usize;
    raw::set_links(pthread, address, address);
    raw::install_exit_hook(|code| std::panic::panic_any(code));
    let result = std::panic::catch_unwind(|| thrd_exit(&kernel, 3));
    assert!(result.is_err());
    raw::install_exit_hook(|_| -> ! {
        loop {
            std::thread::yield_now();
        }
    });
}

#[test]
fn a_robust_mutex_is_marked_owner_died_on_exit_without_pg5() {
    let _serial = crate::test_support::serial();
    let kernel = crate::test_support::MockKernel::with_caps(
        crate::kernel::KernelCaps::full().with(crate::kernel::CapFlag::Robust, false),
    );
    crate::libc_state::LIBC.reset(kernel.tcb());
    crate::libc_state::LIBC.add_thread();
    crate::thread::list::reset_for_fork_child();
    let peer = crate::test_support::MockKernel::new();
    let mut attr = crate::mutex::attr_init();
    crate::mutex::attr_setrobust(&mut attr, 1).unwrap();
    let mut mutex = crate::mutex::Mutex::new();
    crate::mutex::init(&mut mutex, Some(&attr));
    assert_eq!(crate::mutex::lock(&kernel, &mutex), Ok(()));
    let pthread = crate::thread::current_pthread_mut(&kernel);
    let address = pthread as *mut raven_libc_tls::Pthread as usize;
    let peer_pthread = crate::thread::current_pthread_mut(&peer);
    let peer_address = peer_pthread as *mut raven_libc_tls::Pthread as usize;
    raw::set_links(peer_pthread, address, address);
    raw::set_links(pthread, peer_address, peer_address);
    raw::install_exit_hook(|code| std::panic::panic_any(code));
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| exit(&kernel, 0)));
    assert!(result.is_err());
    assert_eq!(
        mutex.lock_value() & crate::limits::OWNER_DIED,
        crate::limits::OWNER_DIED
    );
    raw::install_exit_hook(|_| -> ! {
        loop {
            std::thread::yield_now();
        }
    });
}

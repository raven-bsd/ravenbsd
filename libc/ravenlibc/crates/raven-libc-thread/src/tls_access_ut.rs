// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn a_null_dtv_returns_zero() {
    let kernel = crate::test_support::MockKernel::new();
    assert_eq!(tls_get_addr(&kernel, 1, 0), 0);
}

#[test]
fn the_module_count_is_zero_before_the_dtv_is_installed() {
    let kernel = crate::test_support::MockKernel::new();
    assert_eq!(module_count(&kernel), 0);
}

#[test]
fn a_dtv_word_lookup_goes_through_the_pointer() {
    #[repr(C)]
    struct Buffer {
        head: raven_libc_tls::Dtv,
        words: [core::sync::atomic::AtomicUsize; 2],
    }
    let buffer = Buffer {
        head: raven_libc_tls::Dtv {
            count: core::sync::atomic::AtomicUsize::new(1),
        },
        words: [
            core::sync::atomic::AtomicUsize::new(0x2000),
            core::sync::atomic::AtomicUsize::new(0),
        ],
    };
    let kernel = crate::test_support::MockKernel::new();
    let pthread = crate::thread::current_pthread(&kernel);
    crate::raw::install_dtv_pointer(
        pthread,
        &buffer.head as *const _ as *mut raven_libc_tls::Dtv,
    );
    assert_eq!(tls_get_addr(&kernel, 1, 0x40), 0x2040);
    assert_eq!(module_count(&kernel), 1);
    crate::raw::install_dtv_pointer(pthread, core::ptr::null_mut());
}

#[test]
fn an_out_of_range_module_returns_zero() {
    #[repr(C)]
    struct Buffer {
        head: raven_libc_tls::Dtv,
        words: [core::sync::atomic::AtomicUsize; 1],
    }
    let buffer = Buffer {
        head: raven_libc_tls::Dtv {
            count: core::sync::atomic::AtomicUsize::new(1),
        },
        words: [core::sync::atomic::AtomicUsize::new(0x3000)],
    };
    let kernel = crate::test_support::MockKernel::new();
    let pthread = crate::thread::current_pthread(&kernel);
    crate::raw::install_dtv_pointer(
        pthread,
        &buffer.head as *const _ as *mut raven_libc_tls::Dtv,
    );
    assert_eq!(tls_get_addr(&kernel, 2, 0), 0);
    assert_eq!(tls_get_addr(&kernel, 0, 0), 0);
    crate::raw::install_dtv_pointer(pthread, core::ptr::null_mut());
}

#[test]
fn reset_with_no_modules_is_harmless() {
    let kernel = crate::test_support::MockKernel::new();
    reset_tls(&kernel);
}

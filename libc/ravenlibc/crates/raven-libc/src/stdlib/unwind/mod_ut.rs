// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn a_backtrace_reports_the_end_of_the_stack() {
    assert_eq!(
        backtrace_impl(None, core::ptr::null_mut()),
        URC_END_OF_STACK
    );
}

#[test]
fn the_inspectors_have_neutral_values() {
    let context = core::ptr::null_mut();
    assert_eq!(get_ip_impl(context), 0);
    assert_eq!(get_cfa_impl(context), 0);
    assert_eq!(get_gr_impl(context, 0), 0);
    assert_eq!(get_region_start_impl(context), 0);
    assert_eq!(get_text_rel_base_impl(context), 0);
    assert_eq!(get_data_rel_base_impl(context), 0);
    assert!(get_language_specific_data_impl(context).is_null());
    assert!(find_enclosing_function_impl(core::ptr::null_mut()).is_null());
}

#[test]
fn get_ip_info_zeroes_its_output() {
    let mut before = -1;
    // SAFETY: `before` is a valid writable `c_int` for the call.
    let ip = unsafe { get_ip_info_impl(core::ptr::null_mut(), &mut before) };
    assert_eq!(ip, 0);
    assert_eq!(before, 0);
}

#[test]
fn setters_and_delete_are_no_ops() {
    set_gr_impl(core::ptr::null_mut(), 0, 1);
    set_ip_impl(core::ptr::null_mut(), 1);
    delete_exception_impl(core::ptr::null_mut());
}

#[test]
fn raising_reports_the_fatal_phase_codes() {
    assert_eq!(
        raise_exception_impl(core::ptr::null_mut()),
        URC_FATAL_PHASE1_ERROR
    );
    assert_eq!(
        forced_unwind_impl(core::ptr::null_mut(), None, core::ptr::null_mut()),
        URC_FATAL_PHASE2_ERROR
    );
}

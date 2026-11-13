// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `stdlib-openbsd` oracle: conversion, sorting/searching, getopt_long, getsubopt and a64l.

#![allow(missing_docs)] // extern declarations mirror the donor prototypes
use core::ffi::{c_char, c_int, c_long, c_longlong, c_ulong, c_void};

/// The C `struct option` layout.
#[repr(C)]
pub struct COption {
    pub name: *const c_char,
    pub has_arg: c_int,
    pub flag: *mut c_int,
    pub val: c_int,
}

pub type Cmp = Option<unsafe extern "C" fn(*const c_void, *const c_void) -> c_int>;

#[cfg_attr(
    feature = "oracle-stdlib-openbsd",
    link(name = "oracle_stdlib_openbsd", kind = "dylib")
)]
unsafe extern "C" {
    pub fn oracle_strtol(nptr: *const c_char, endptr: *mut *mut c_char, base: c_int) -> c_long;
    pub fn oracle_strtoul(nptr: *const c_char, endptr: *mut *mut c_char, base: c_int) -> c_ulong;
    pub fn oracle_strtonum(
        nptr: *const c_char,
        minval: c_longlong,
        maxval: c_longlong,
        errstrp: *mut *const c_char,
    ) -> c_longlong;
    pub fn oracle_qsort(base: *mut c_void, n: usize, es: usize, cmp: Cmp);
    pub fn oracle_heapsort(base: *mut c_void, n: usize, es: usize, cmp: Cmp) -> c_int;
    pub fn oracle_mergesort(base: *mut c_void, n: usize, es: usize, cmp: Cmp) -> c_int;
    pub fn oracle_bsearch(
        key: *const c_void,
        base: *const c_void,
        n: usize,
        es: usize,
        cmp: Cmp,
    ) -> *mut c_void;
    pub fn oracle_getopt(argc: c_int, argv: *const *mut c_char, optstring: *const c_char) -> c_int;
    pub fn oracle_getopt_long(
        argc: c_int,
        argv: *const *mut c_char,
        optstring: *const c_char,
        longopts: *const COption,
        idx: *mut c_int,
    ) -> c_int;
    pub fn oracle_getopt_long_only(
        argc: c_int,
        argv: *const *mut c_char,
        optstring: *const c_char,
        longopts: *const COption,
        idx: *mut c_int,
    ) -> c_int;
    pub fn oracle_getsubopt(
        optionp: *mut *mut c_char,
        tokens: *const *mut c_char,
        valuep: *mut *mut c_char,
    ) -> c_int;
    pub fn oracle_a64l(s: *const c_char) -> c_long;
    pub fn oracle_l64a(value: c_long) -> *mut c_char;
    pub static mut oracle_optarg: *mut c_char;
    pub static mut oracle_optind: c_int;
    pub static mut oracle_opterr: c_int;
    pub static mut oracle_optopt: c_int;
    pub static mut oracle_optreset: c_int;
    pub static mut oracle_suboptarg: *mut c_char;
}

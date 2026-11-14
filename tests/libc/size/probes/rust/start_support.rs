// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// START-FINDING support for the size probes.
//
// `C61` ships `__raven_libc_start`, which calls `__raven_init_main_thread`.
// The owning ticket (C64 thread init) has not shipped that definition on this branch, so
// `libc.a` alone cannot link a program. This test-owned object supplies the one missing
// definition; it is compiled once per probe directory and is NOT part of `libc.a`.
// The other START-FINDING symbols (`__cxa_atexit`, the four program-name aliases and
// `__stack_chk_guard`) are now defined by `libc.a`, so defining them here would duplicate a
// strong symbol and make every static probe skip. When C64 lands, delete this file and the
// `start_support.o` argument.
#![no_std]

#[unsafe(no_mangle)]
pub extern "C" fn __raven_init_main_thread(_auxv: *const usize, _canary: usize) {}

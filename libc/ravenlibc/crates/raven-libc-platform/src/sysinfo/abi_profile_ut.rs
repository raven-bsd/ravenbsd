// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S746 red ABI-profile rows for the remaining native records and pthread constants.
//!
//! Expected values are the FreeBSD 133344405 donor definitions:
//! `sys/sys/utsname.h` (`SYS_NMLN` 256, five fields, no `domainname`),
//! `include/pwd.h` (eleven-field `struct passwd`), `include/grp.h`,
//! `include/pthread.h` and `sys/sys/_pthreadtypes.h`. These rows are red until the
//! S746 fix lands.

use core::mem::{align_of, size_of};

use raven_libc_abi::pthread::{
    PTHREAD_CANCEL_ASYNCHRONOUS, PTHREAD_EXPLICIT_SCHED, PTHREAD_INHERIT_SCHED,
    PTHREAD_MUTEX_DEFAULT, PTHREAD_SCOPE_PROCESS, PTHREAD_SCOPE_SYSTEM,
};
use raven_libc_abi::structs::{Group, Passwd, Utsname};
use raven_libc_abi::types::{pthread_key_t, pthread_spinlock_t};

#[test]
fn utsname_is_the_freebsd_five_by_256_record() {
    assert_eq!(size_of::<Utsname>(), 5 * 256);
    assert_eq!(align_of::<Utsname>(), 1);
}

#[test]
fn passwd_is_the_freebsd_eleven_field_record() {
    assert_eq!(size_of::<Passwd>(), 80);
}

#[test]
fn group_is_the_freebsd_four_field_record() {
    assert_eq!(size_of::<Group>(), 32);
}

#[test]
fn pthread_scope_constants_are_the_freebsd_values() {
    assert_eq!(PTHREAD_SCOPE_SYSTEM, 2);
    assert_eq!(PTHREAD_SCOPE_PROCESS, 0);
}

#[test]
fn pthread_scheduling_constants_are_the_freebsd_values() {
    assert_eq!(PTHREAD_INHERIT_SCHED, 4);
    assert_eq!(PTHREAD_EXPLICIT_SCHED, 0);
}

#[test]
fn pthread_mutex_default_and_cancel_type_are_the_freebsd_values() {
    assert_eq!(PTHREAD_MUTEX_DEFAULT, 1);
    assert_eq!(PTHREAD_CANCEL_ASYNCHRONOUS, 2);
}

#[test]
fn pthread_key_and_spinlock_are_the_freebsd_types() {
    assert_eq!(size_of::<pthread_key_t>(), 4);
    assert_eq!(size_of::<pthread_spinlock_t>(), 8);
}

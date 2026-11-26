// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Compile-time probe for the Raven native ABI records.
//!
//! These records must stay identical to the S746 native model in
//! `toolchain/vendor/libc/src/unix/ravenbsd/mod.rs` and the FreeBSD
//! `133344405` headers it was taken from: a five-field 256-byte `utsname`
//! (`sys/sys/utsname.h`), a signed `int` `pthread_key_t` and the pointer-sized
//! opaque `pthread_spinlock_t` (`sys/sys/_pthreadtypes.h`).
//!
//! The array-length equalities below are the probe. When a record drifts, the
//! Raven build fails and names the observed size: `expected an array with a
//! fixed size of 1280 elements, found one with 390 elements` is the stale
//! six-65-byte `utsname`, and `... 8 elements, found one with 4` is the stale
//! four-byte spinlock.

use crate::prelude::*;

use super::{
    pthread_key_t, pthread_spinlock_t, utsname, PTHREAD_MUTEX_DEFAULT, PTHREAD_MUTEX_ERRORCHECK,
    PTHREAD_MUTEX_NORMAL, PTHREAD_MUTEX_RECURSIVE, UTSLENGTH, _SC_2_FORT_DEV, _SC_2_FORT_RUN,
    _SC_SEM_NSEMS_MAX, _SC_SEM_VALUE_MAX,
};

// `SYS_NMLN` is 256: five fields of 256 bytes, 1280 bytes, no `domainname`.
const _: [(); 256] = [(); UTSLENGTH];
const _: [(); 1280] = [(); size_of::<utsname>()];
const _: [(); 0] = [(); core::mem::offset_of!(utsname, sysname)];
const _: [(); 256] = [(); core::mem::offset_of!(utsname, nodename)];
const _: [(); 512] = [(); core::mem::offset_of!(utsname, release)];
const _: [(); 768] = [(); core::mem::offset_of!(utsname, version)];
const _: [(); 1024] = [(); core::mem::offset_of!(utsname, machine)];

// Naming every field and nothing else pins the record to the donor's five
// fields: a restored `domainname`, or any renamed/missing field, fails here.
const _UTSNAME_FIELD_SET: utsname = utsname {
    sysname: [0; UTSLENGTH],
    nodename: [0; UTSLENGTH],
    release: [0; UTSLENGTH],
    version: [0; UTSLENGTH],
    machine: [0; UTSLENGTH],
};

// `pthread_key_t` is the signed `int` of `sys/sys/_pthreadtypes.h`.
const _: [(); 4] = [(); size_of::<pthread_key_t>()];
const _KEY_IS_SIGNED: pthread_key_t = -1;

// `pthread_spinlock_t` is `struct pthread_spinlock *`, so it is pointer-sized
// and pointer-aligned; the FFI allocates and owns the opaque object.
const _: [(); size_of::<*mut c_void>()] = [(); size_of::<pthread_spinlock_t>()];
const _: [(); align_of::<*mut c_void>()] = [(); align_of::<pthread_spinlock_t>()];

// The FreeBSD `include/pthread.h` 133344405 mutex type values.
const _: [(); 1] = [(); PTHREAD_MUTEX_ERRORCHECK as usize];
const _: [(); 2] = [(); PTHREAD_MUTEX_RECURSIVE as usize];
const _: [(); 3] = [(); PTHREAD_MUTEX_NORMAL as usize];
const _: [(); 1] = [(); PTHREAD_MUTEX_DEFAULT as usize];

// The FreeBSD `include/unistd.h` 133344405 `_SC_SEM_*` rows. `_SC_2_FORT_*`
// keep their distinct FreeBSD values so the SEM rows can occupy 49/50 in the
// single `SysconfVar` discriminant space.
const _: [(); 49] = [(); _SC_SEM_NSEMS_MAX as usize];
const _: [(); 50] = [(); _SC_SEM_VALUE_MAX as usize];
const _: [(); 21] = [(); _SC_2_FORT_DEV as usize];
const _: [(); 22] = [(); _SC_2_FORT_RUN as usize];

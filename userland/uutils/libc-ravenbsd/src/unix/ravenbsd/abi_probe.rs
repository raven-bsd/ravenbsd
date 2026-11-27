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
    pthread_key_t, pthread_spinlock_t, termios, utsname, _POSIX_VDISABLE, _SC_2_FORT_DEV,
    _SC_2_FORT_RUN, _SC_SEM_NSEMS_MAX, _SC_SEM_VALUE_MAX, CSIZE, ECHO, ECHOKE, ICANON, ISIG, IXOFF,
    IXON, NCCS, NOFLSH, PTHREAD_MUTEX_DEFAULT, PTHREAD_MUTEX_ERRORCHECK, PTHREAD_MUTEX_NORMAL,
    PTHREAD_MUTEX_RECURSIVE, TABDLY, UTSLENGTH, VDISCARD, VDSUSP, VEOF, VEOL, VEOL2, VERASE,
    VERASE2, VINTR, VKILL, VLNEXT, VMIN, VQUIT, VREPRINT, VSTART, VSTATUS, VSTOP, VSUSP, VTIME,
    VWERASE,
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

// `struct termios` is the pinned FreeBSD 133344405 `sys/sys/_termios.h` record:
// 44 bytes, alignment 4, `NCCS` 20, no `c_line`, speeds at 36/40.
const _: [(); 44] = [(); size_of::<termios>()];
const _: [(); 4] = [(); align_of::<termios>()];
const _: [(); 0] = [(); core::mem::offset_of!(termios, c_iflag)];
const _: [(); 4] = [(); core::mem::offset_of!(termios, c_oflag)];
const _: [(); 8] = [(); core::mem::offset_of!(termios, c_cflag)];
const _: [(); 12] = [(); core::mem::offset_of!(termios, c_lflag)];
const _: [(); 16] = [(); core::mem::offset_of!(termios, c_cc)];
const _: [(); 36] = [(); core::mem::offset_of!(termios, c_ispeed)];
const _: [(); 40] = [(); core::mem::offset_of!(termios, c_ospeed)];
const _: [(); 20] = [(); NCCS];

// Naming every field and nothing else pins the record to the donor's seven
// fields: a restored `c_line` (or `__c_ispeed`/`__c_ospeed`) fails here.
const _TERMIOS_FIELD_SET: termios = termios {
    c_iflag: 0,
    c_oflag: 0,
    c_cflag: 0,
    c_lflag: 0,
    c_cc: [0; NCCS],
    c_ispeed: 0,
    c_ospeed: 0,
};

// The FreeBSD `c_cc` indices: index 7 is VERASE2, not VSWTC.
const _: [(); 0] = [(); VEOF];
const _: [(); 1] = [(); VEOL];
const _: [(); 2] = [(); VEOL2];
const _: [(); 3] = [(); VERASE];
const _: [(); 4] = [(); VWERASE];
const _: [(); 5] = [(); VKILL];
const _: [(); 6] = [(); VREPRINT];
const _: [(); 7] = [(); VERASE2];
const _: [(); 8] = [(); VINTR];
const _: [(); 9] = [(); VQUIT];
const _: [(); 10] = [(); VSUSP];
const _: [(); 11] = [(); VDSUSP];
const _: [(); 12] = [(); VSTART];
const _: [(); 13] = [(); VSTOP];
const _: [(); 14] = [(); VLNEXT];
const _: [(); 15] = [(); VDISCARD];
const _: [(); 16] = [(); VMIN];
const _: [(); 17] = [(); VTIME];
const _: [(); 18] = [(); VSTATUS];
const _: [(); 0xff] = [(); _POSIX_VDISABLE as usize];

// The FreeBSD numeric speed and flag values (the former copy carried the
// musl/Linux octal encodings).
const _: [(); 9600] = [(); super::B9600 as usize];
const _: [(); 57600] = [(); super::B57600 as usize];
const _: [(); 19200] = [(); super::EXTA as usize];
const _: [(); 0x200] = [(); IXON as usize];
const _: [(); 0x400] = [(); IXOFF as usize];
const _: [(); 0x80] = [(); ISIG as usize];
const _: [(); 0x100] = [(); ICANON as usize];
const _: [(); 0x8] = [(); ECHO as usize];
const _: [(); 0x1] = [(); ECHOKE as usize];
const _: [(); 0x4] = [(); TABDLY as usize];
const _: [(); 0x300] = [(); CSIZE as usize];
const _: [(); 0x8000_0000] = [(); NOFLSH as usize];

// The FreeBSD errno, signal and clock surfaces the rustix `bsd` paths consume.
const _: [(); 35] = [(); super::EAGAIN as usize];
const _: [(); 11] = [(); super::EDEADLK as usize];
const _: [(); 72] = [(); super::EBADRPC as usize];
const _: [(); 80] = [(); super::EAUTH as usize];
const _: [(); 7] = [(); super::SIGEMT as usize];
const _: [(); 29] = [(); super::SIGINFO as usize];
const _: [(); 30] = [(); super::SIGUSR1 as usize];
const _: [(); 20] = [(); super::SIGCHLD as usize];
const _: [(); 5] = [(); super::CLOCK_UPTIME as usize];
const _: [(); 15] = [(); super::CLOCK_PROCESS_CPUTIME_ID as usize];
const _: [(); 4] = [(); super::CLOCK_MONOTONIC as usize];

// The FreeBSD `dirent` record: `d_fileno` first, `d_name` at 24, 280 bytes.
const _: [(); 280] = [(); size_of::<super::dirent>()];
const _: [(); 0] = [(); core::mem::offset_of!(super::dirent, d_fileno)];
const _: [(); 24] = [(); core::mem::offset_of!(super::dirent, d_name)];

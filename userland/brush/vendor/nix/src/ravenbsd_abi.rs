// SPDX-License-Identifier: MIT
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Compile-time ABI checks for the RavenBSD nix platform selection (S765).
//!
//! RavenBSD uses the FreeBSD-shaped native ABI. The nix platform detection must
//! select the freebsd-compatible signal and record paths, not the Linux ones. These
//! assertions are compiled into every RavenBSD consumer, so a wrong selection or a
//! target-libc layout drift fails the build instead of surfacing at the shell.

#[cfg(linux_android)]
compile_error!("ravenbsd must not select the linux_android nix path");

#[cfg(not(freebsd))]
compile_error!("ravenbsd must select the freebsd-compatible nix path");

use crate::sys::signal::Signal;

const _: () = {
    // FreeBSD-shaped utsname: five 256-byte fields, 1280 bytes, no domainname.
    assert!(core::mem::size_of::<libc::utsname>() == 1280);
    assert!(core::mem::offset_of!(libc::utsname, sysname) == 0);
    assert!(core::mem::offset_of!(libc::utsname, nodename) == 256);
    assert!(core::mem::offset_of!(libc::utsname, release) == 512);
    assert!(core::mem::offset_of!(libc::utsname, version) == 768);
    assert!(core::mem::offset_of!(libc::utsname, machine) == 1024);

    // FreeBSD-shaped passwd: 80 bytes, pw_change 24, pw_class 32, pw_expire 64,
    // pw_fields 72. A Linux-shaped initializer cannot compile against this layout.
    assert!(core::mem::size_of::<libc::passwd>() == 80);
    assert!(core::mem::offset_of!(libc::passwd, pw_name) == 0);
    assert!(core::mem::offset_of!(libc::passwd, pw_passwd) == 8);
    assert!(core::mem::offset_of!(libc::passwd, pw_uid) == 16);
    assert!(core::mem::offset_of!(libc::passwd, pw_gid) == 20);
    assert!(core::mem::offset_of!(libc::passwd, pw_change) == 24);
    assert!(core::mem::offset_of!(libc::passwd, pw_class) == 32);
    assert!(core::mem::offset_of!(libc::passwd, pw_gecos) == 40);
    assert!(core::mem::offset_of!(libc::passwd, pw_dir) == 48);
    assert!(core::mem::offset_of!(libc::passwd, pw_shell) == 56);
    assert!(core::mem::offset_of!(libc::passwd, pw_expire) == 64);
    assert!(core::mem::offset_of!(libc::passwd, pw_fields) == 72);

    // The selected signal enum carries the FreeBSD signals, at the libc values.
    assert!(Signal::SIGEMT as i32 == libc::SIGEMT);
    assert!(Signal::SIGINFO as i32 == libc::SIGINFO);
    assert!(Signal::SIGURG as i32 == libc::SIGURG);
    assert!(Signal::SIGIO as i32 == libc::SIGIO);
    assert!(Signal::SIGSYS as i32 == libc::SIGSYS);
    assert!(Signal::SIGUSR1 as i32 == libc::SIGUSR1);
    assert!(Signal::SIGUSR2 as i32 == libc::SIGUSR2);
};

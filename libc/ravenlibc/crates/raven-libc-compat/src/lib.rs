// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! ravenlibc BSD/GNU compatibility extras (`architecture/raven-libc/donor-map/15` 3.B).
//!
//! The crate owns the OS-neutral BSD "extras" that no area crate owns: `vis`/`unvis`,
//! `stringlist`, `humanize_number`/`expand_number`/`fmt_scaled`/`scan_scaled`, `fparseln`,
//! `getbsize`, `getcap`, `getttyent`, `getusershell`, `setmode`/`getmode`, `pidfile`,
//! `readpassphrase`, the BSD `uuid` API, `crypt()` (`$1$`, `$2a$`, `$2b$`, `$5$`, `$6$`),
//! the `openpty`/`login_tty`/`forkpty` logic, `mntent`, `utmpx` stubs, `backtrace` and the
//! `devname` stub. The C-ABI shell (`raven-libc/src/compat/**`) owns the exported symbols and
//! the C structs; this crate is pure logic over slices and the seams in [`seams`].
//!
//! There is no clock, no environment, no filesystem and no randomness outside the seams. Every
//! allocating function documents its cap. The crate is class P: `#![forbid(unsafe_code)]`.
//!
//! Sources (licence class read per file; `RD-new` substitutions are listed in the C65 report):
//! `netbsd:lib/libc/gen/{vis,unvis}.c` BSD-3 (the FreeBSD `contrib/libc-vis` checkout is absent),
//! `freebsd:lib/libc/gen/stringlist.c` BSD-2, `freebsd:lib/libutil/{humanize_number,
//! expand_number,fparseln,pty,login_tty}.c` BSD-2/BSD-3, `netbsd:crypto/external/bsd/openssh/dist/
//! fmt_scaled.c` BSD-3, `openbsd:lib/libc/gen/{getbsize,getcap,getttyent,getusershell,
//! readpassphrase,setmode}.c` BSD-3/ISC, `netbsd:lib/libc/uuid/*.c` BSD-2,
//! `netbsd:lib/libcrypt/{bcrypt,blowfish}.c` BSD-3, `musl:src/crypt/{crypt_md5,crypt_sha256,
//! crypt_sha512}.c` MIT via `musl/COPYRIGHT` (the FreeBSD `lib/libcrypt` checkout is absent).

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented
)]

extern crate alloc;

pub mod crypt;
pub mod devname;
pub mod execinfo;
pub mod fparseln;
pub mod getbsize;
pub mod getcap;
pub mod humanize;
pub mod mntent;
pub mod pidfile;
pub mod pty;
pub mod readpassphrase;
pub mod seams;
pub mod setmode;
pub mod stringlist;
pub mod ttyent;
pub mod usershell;
pub mod utmpx;
pub mod uuid;
pub mod vis;
pub mod zero;

#[cfg(test)]
pub mod testutil;

pub use pty::{forkpty, login_tty, openpty};
pub use seams::{
    Entropy, ForkResult, LineSource, PidfileOps, PtyOps, ReadFile, Termios, TtyOps, Winsize,
};

// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! BSD personality exports (`personality_bsd`).
//!
//! One file per symbol; the body is a pointer check and one call into
//! `raven-libc-personality-bsd`. `pthread_*_np` is alias-table only on this branch because
//! `raven-libc-thread` (C57) is not merged (ticket C66 4.4 and the C66 brief).

pub(crate) mod __errno;
pub(crate) mod __error;
pub(crate) mod close_range;
pub(crate) mod closefrom;
pub(crate) mod elf_aux_info;
pub(crate) mod getfsstat;
pub(crate) mod getmntinfo;
pub(crate) mod kevent;
pub(crate) mod kqueue;
pub(crate) mod kqueue1;
pub(crate) mod pledge;
pub(crate) mod setproctitle;
pub(crate) mod setproctitle_fast;
pub(crate) mod sysctl;
pub(crate) mod sysctlbyname;
pub(crate) mod sysctlgetmibinfo;
pub(crate) mod sysctlnametomib;
pub(crate) mod unveil;

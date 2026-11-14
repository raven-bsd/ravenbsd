// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! Linux personality exports (`personality_linux`).
//!
//! One file per symbol (ticket C66 5.J); the body is a pointer check and one call into
//! `raven-libc-personality-linux`. `personality_common` owns the real seam implementations and
//! the process-global tables.

pub(crate) mod alias64;
pub(crate) mod copy_file_range;
pub(crate) mod epoll_create;
pub(crate) mod epoll_create1;
pub(crate) mod epoll_ctl;
pub(crate) mod epoll_pwait;
pub(crate) mod epoll_pwait2;
pub(crate) mod epoll_wait;
pub(crate) mod error;
pub(crate) mod error_at_line;
pub(crate) mod error_globals;
pub(crate) mod error_message_count;
pub(crate) mod error_one_per_line;
pub(crate) mod error_print_progname;
pub(crate) mod eventfd;
pub(crate) mod eventfd_read;
pub(crate) mod eventfd_write;
pub(crate) mod get_avphys_pages;
pub(crate) mod get_nprocs;
pub(crate) mod get_nprocs_conf;
pub(crate) mod get_phys_pages;
pub(crate) mod getauxval;
pub(crate) mod inotify_add_watch;
pub(crate) mod inotify_init;
pub(crate) mod inotify_init1;
pub(crate) mod inotify_rm_watch;
pub(crate) mod personality;
pub(crate) mod prctl;
pub(crate) mod sendfile;
pub(crate) mod signalfd;
pub(crate) mod sysinfo;
pub(crate) mod timerfd_create;
pub(crate) mod timerfd_gettime;
pub(crate) mod timerfd_settime;

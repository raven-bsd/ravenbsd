// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The user-database family of the C-ABI shell (C64 4.8, rows P23/X46/X47/B07): `pwd.h`,
//! `grp.h`, `shadow.h` and the `/etc/shells` cursor. Lookups go through C51's `FilesSource`
//! over [`source::ShellFiles`]; packing goes through [`crate::ffi::tcbbuf`]. Every `_r` form
//! returns `0` plus a NULL result for "not found" and `ERANGE` plus NULL for a short buffer,
//! never a partial record. `/etc/master.passwd` iteration and the `FILE`-based forms are
//! deferred with C51's missing shadow source and C60b's `FILE` layer.

pub(crate) mod cursor;
pub(crate) mod lookup;
pub(crate) mod source;

pub(crate) mod endpwent;
pub(crate) mod getpwent;
pub(crate) mod getpwent_r;
pub(crate) mod getpwnam;
pub(crate) mod getpwnam_r;
pub(crate) mod getpwuid;
pub(crate) mod getpwuid_r;
pub(crate) mod setpassent;
pub(crate) mod setpwent;
pub(crate) mod uid_from_user;
pub(crate) mod user_from_uid;

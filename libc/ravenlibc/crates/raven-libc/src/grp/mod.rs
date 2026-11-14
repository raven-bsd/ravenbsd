// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The group-database family (C64 4.8, rows P11/X46): the `getgr*` lookups and enumerator over
//! C51's `FilesSource`, `getgrouplist`, the pwcache pair and `initgroups` (exported from the
//! process directory because it also changes credentials). The `_r` contract is the same as the
//! passwd side: `0` plus NULL for "not found", `ERANGE` plus NULL for a short buffer.

pub(crate) mod endgrent;
pub(crate) mod getgrent;
pub(crate) mod getgrent_r;
pub(crate) mod getgrgid;
pub(crate) mod getgrgid_r;
pub(crate) mod getgrnam;
pub(crate) mod getgrnam_r;
pub(crate) mod getgrouplist;
pub(crate) mod gid_from_group;
pub(crate) mod group_from_gid;
pub(crate) mod setgrent;
pub(crate) mod setgroupent;

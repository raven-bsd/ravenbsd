// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The uutils `libc::statfs` record, included from its one definition so the host layout test
//! and the target probe cannot drift from the type the utilities allocate.

#[path = "../../../../userland/uutils/libc-ravenbsd/src/unix/ravenbsd/statfs_record.rs"]
pub mod record;

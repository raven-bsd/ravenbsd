// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Error numbers used by the pledge and Capsicum models.

/// A positive C `errno` value.
pub type Errno = i32;

pub use raven_libc_abi::errno::{E2BIG, EACCES, EINVAL, ENAMETOOLONG, ENOENT, EPERM};

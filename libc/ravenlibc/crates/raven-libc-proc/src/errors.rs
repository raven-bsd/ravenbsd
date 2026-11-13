// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Error numbers used by the process models.
//!
//! The C library's `errno` is a positive Linux number. The constants come from the generated
//! `raven-libc-abi` crate so the models and the headers cannot drift.

#![allow(unused_imports)] // the complete set is part of the crate's public vocabulary

/// A positive C `errno` value.
pub type Errno = i32;

pub use raven_libc_abi::errno::{
    E2BIG, EACCES, EAGAIN, EBADF, ECHILD, EEXIST, EFAULT, EINTR, EINVAL, EIO, EISDIR, ELOOP,
    EMFILE, ENAMETOOLONG, ENODEV, ENOENT, ENOEXEC, ENOMEM, ENOSPC, ENOSYS, ENOTDIR, EOPNOTSUPP,
    EOVERFLOW, EPERM, EPIPE, EPROTO, ERANGE, ESRCH, ETIMEDOUT, ETXTBSY, EXDEV,
};

// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The `scanf` glue: `vfscanf`/`vfwscanf` over the C43 engine and [`FileSource`].
//!
//! The engine consumes through [`crate::glue_io::FileSource`], which uses `getc_unlocked` and
//! `ungetc` (wide: `fgetwc_unlocked`/`ungetwc`) and counts the consumed units; the glue holds
//! the FILE lock for the whole call. The `ScanResult` tells the shell the assignment count,
//! the consumed count and whether the C return is `EOF`.

use raven_libc_fmt::{FloatScan, FmtEnv, FmtErr, ScanOptions, ScanResult, vscan};

use crate::file::{F_ERR, File};
use crate::glue_io::FileSource;
use crate::lock::FileGuard;
use crate::traits::Env;

/// `vfscanf`.
///
/// # Errors
/// The engine error; a format failure is part of [`ScanResult`], not an error.
pub fn vfscanf_file<E: Env, A: FmtEnv, F: FloatScan>(
    file: &File<E>,
    fmt: &[u8],
    args: &mut A,
    opts: &ScanOptions<'_>,
    floats: &F,
) -> Result<ScanResult, FmtErr> {
    let _guard = FileGuard::new(file);
    if file.st().mode == 1 {
        file.st_mut().flags |= F_ERR;
        return Err(FmtErr::Invalid);
    }
    let mut source = FileSource::new(file);
    vscan(&mut source, fmt, args, opts, floats)
}

/// `vfwscanf`.
///
/// # Errors
/// The engine error; a format failure is part of [`ScanResult`], not an error.
pub fn vfwscanf_file<E: Env, A: FmtEnv, F: FloatScan>(
    file: &File<E>,
    fmt: &[u32],
    args: &mut A,
    opts: &ScanOptions<'_>,
    floats: &F,
) -> Result<ScanResult, FmtErr> {
    let _guard = FileGuard::new(file);
    if file.st().mode == -1 {
        file.st_mut().flags |= F_ERR;
        return Err(FmtErr::Invalid);
    }
    if file.st().mode == 0 {
        let _ = crate::wide::fwide(file, 1);
    }
    let mut source = FileSource::new(file);
    vscan(&mut source, fmt, args, opts, floats)
}

#[cfg(test)]
#[path = "scanf_glue_ut.rs"]
mod tests;

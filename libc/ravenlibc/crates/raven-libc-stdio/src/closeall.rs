// SPDX-License-Identifier: BSD-2-Clause
// Copyright (C) 2006 Daniel M. Eischen.  All rights reserved.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from freebsd:lib/libc/stdio/fcloseall.c @ 133344405f532bd54ff3629a77d4c4e062f01fdb
// (file revision tag: none; SPDX line 2, Copyright (C) 2006 Daniel M. Eischen)
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions
// are met:
// 1. Redistributions of source code must retain the above copyright
//    notice, this list of conditions and the following disclaimer.
// 2. Redistributions in binary form must reproduce the above copyright
//    notice, this list of conditions and the following disclaimer in the
//    documentation and/or other materials provided with the distribution.
//
// THIS SOFTWARE IS PROVIDED BY AUTHOR AND CONTRIBUTORS ``AS IS'' AND
// ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED.  IN NO EVENT SHALL AUTHOR OR CONTRIBUTORS BE LIABLE
// FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
// DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
// OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
// HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
// LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY
// OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
// SUCH DAMAGE.
//
//! `fcloseall`: close every non-standard FILE (the donor's `_fwalk(fclose)` rewritten over
//! the intrusive open-file list; the three standard streams are left open).

use alloc::vec::Vec;

use crate::R;
use crate::file::{F_PERM, File};
use crate::state::State;
use crate::traits::Env;

/// `fcloseall`: close every non-standard FILE.
///
/// # Errors
/// The first close error, after every FILE was attempted.
pub fn fcloseall<E: Env>(state: &'static State<E>) -> R<()> {
    let mut files: Vec<&'static File<E>> = Vec::new();
    {
        let guard = crate::ofl::OflGuard::acquire(state);
        let mut cursor = guard.head();
        while let Some(file) = cursor {
            cursor = file.st().next;
            if file.st().flags & F_PERM == 0 {
                files.push(file);
            }
        }
    }
    let mut first_error = None;
    for file in files {
        if let Err(error) = crate::close::fclose(file)
            && first_error.is_none()
        {
            first_error = Some(error);
        }
    }
    match first_error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

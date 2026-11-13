// SPDX-License-Identifier: BSD-2-Clause
//
// Copyright 1996, 1997, 1998, 1999, 2000 John D. Polstra.
// Copyright 2003 Alexander Kabaev <kan@FreeBSD.ORG>.
// Copyright 2009-2013 Konstantin Belousov <kib@FreeBSD.ORG>.
// Copyright 2012 John Marino <draco@marino.st>.
// Copyright 2014-2017 The FreeBSD Foundation
// All rights reserved.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from freebsd:libexec/rtld-elf/rtld.c @ 133344405f532bd54ff3629a77d4c4e062f01fdb
// (file revision tag: 133344405f532bd54ff3629a77d4c4e062f01fdb)
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
// THIS SOFTWARE IS PROVIDED BY THE AUTHOR AND CONTRIBUTORS ``AS IS'' AND
// ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED.  IN NO EVENT SHALL THE AUTHOR OR CONTRIBUTORS BE LIABLE
// FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
// DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
// OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
// HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
// LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY
// OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
// SUCH DAMAGE.
//
//! Library search paths: name expansion into an ordered candidate list.
//!
//! The loader tries each candidate with `OPEN`. The order is the donor's (13 3.13): a name with
//! a slash is used as is; otherwise `RPATH` (when no `RUNPATH`), the main program's `RPATH`,
//! `LD_LIBRARY_PATH`, the object's `RUNPATH`, then the default directories unless `NODEFLIB`.
//! The pre-conforming `LD_LIBRARY_PATH_RPATH` order puts `LD_LIBRARY_PATH` first. There is no
//! hints file, no libmap and no file-descriptor variable.

use alloc::vec::Vec;

use crate::errors::SearchError;
use crate::limits::{MAX_PATH_LEN, MAX_SEARCH_CANDIDATES, MAX_SEARCH_PATH_LEN};
use crate::scope::ObjId;

/// The context one expansion needs.
pub struct SearchCtx<'a> {
    /// Secure-exec is off (`AT_SECURE == 0`).
    pub trust: bool,
    /// `LD_LIBRARY_PATH`.
    pub ld_library_path: Option<&'a [u8]>,
    /// `LD_LIBRARY_PATH_RPATH`: the pre-conforming order.
    pub library_path_rpath: bool,
    /// The main program's `DT_RPATH`.
    pub main_rpath: Option<&'a [u8]>,
    /// `/lib`, `/usr/lib`, ...
    pub default_dirs: &'a [&'a [u8]],
    /// The origin (directory) of an object.
    pub origin_of: &'a dyn Fn(ObjId) -> Option<Vec<u8>>,
    /// `$PLATFORM` (`AT_PLATFORM`, for example `x86_64`).
    pub platform: &'a [u8],
}

/// The referencing object's search attributes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RefObj<'a> {
    /// The object id.
    pub id: ObjId,
    /// `DT_RPATH` (already expanded at digest time).
    pub rpath: Option<&'a [u8]>,
    /// `DT_RUNPATH`.
    pub runpath: Option<&'a [u8]>,
    /// `DF_1_NODEFLIB`.
    pub nodeflib: bool,
    /// The object is the main program.
    pub is_main: bool,
}

/// The last component of a path.
pub fn basename(path: &[u8]) -> &[u8] {
    match path.iter().rposition(|byte| *byte == b'/') {
        Some(index) => path.get(index.saturating_add(1)..).unwrap_or(&[]),
        None => path,
    }
}

fn substitute(input: &[u8], keyword: &[u8], value: &[u8]) -> Vec<u8> {
    let mut output = Vec::new();
    let mut index = 0usize;
    while index < input.len() {
        let rest = input.get(index..).unwrap_or(&[]);
        if rest.starts_with(keyword) {
            output.extend_from_slice(value);
            index = index.saturating_add(keyword.len());
        } else {
            if let Some(byte) = input.get(index) {
                output.push(*byte);
            }
            index = index.saturating_add(1);
        }
    }
    output
}

/// Expand `$ORIGIN`, `${ORIGIN}`, `$PLATFORM` and `${PLATFORM}`.
///
/// In secure mode the string is returned unchanged (A-ld-13); an unresolved `$ORIGIN` leaves the
/// token in place and the caller skips the candidate.
pub fn origin_subst(
    ctx: &SearchCtx<'_>,
    refobj: Option<ObjId>,
    input: &[u8],
) -> Result<Vec<u8>, SearchError> {
    if !ctx.trust {
        return Ok(input.to_vec());
    }
    let origin = refobj.and_then(|id| (ctx.origin_of)(id));
    let mut text = input.to_vec();
    if let Some(origin) = origin {
        text = substitute(&text, b"${ORIGIN}", &origin);
        text = substitute(&text, b"$ORIGIN", &origin);
    }
    text = substitute(&text, b"${PLATFORM}", ctx.platform);
    text = substitute(&text, b"$PLATFORM", ctx.platform);
    Ok(text)
}

fn candidates_from(
    ctx: &SearchCtx<'_>,
    path: &[u8],
    name: &[u8],
    refobj: Option<ObjId>,
    output: &mut Vec<Vec<u8>>,
) -> Result<(), SearchError> {
    if path.len() > MAX_SEARCH_PATH_LEN {
        return Err(SearchError::PathTooLong);
    }
    let mut index = 0usize;
    while index < path.len() {
        let rest = path.get(index..).unwrap_or(&[]);
        let length = rest
            .iter()
            .position(|byte| *byte == b':' || *byte == b';')
            .unwrap_or(rest.len());
        let element = rest.get(..length).unwrap_or(&[]);
        index = index.saturating_add(length);
        while path
            .get(index)
            .is_some_and(|byte| *byte == b':' || *byte == b';')
        {
            index = index.saturating_add(1);
        }
        if element.is_empty() {
            continue;
        }
        if !ctx.trust && element.first() != Some(&b'/') {
            continue;
        }
        let expanded = origin_subst(ctx, refobj, element)?;
        if expanded.contains(&b'$') {
            continue;
        }
        let mut candidate = expanded.to_vec();
        candidate.push(b'/');
        candidate.extend_from_slice(name);
        if candidate.len() > MAX_PATH_LEN {
            continue;
        }
        if output.len() >= MAX_SEARCH_CANDIDATES {
            return Err(SearchError::TooManyCandidates);
        }
        output.push(candidate);
    }
    Ok(())
}

/// The ordered candidate paths for `name`.
pub fn expand_search(
    ctx: &SearchCtx<'_>,
    name: &[u8],
    refobj: Option<RefObj<'_>>,
) -> Result<Vec<Vec<u8>>, SearchError> {
    if name.contains(&b'/') {
        if !ctx.trust && name.first() != Some(&b'/') {
            return Err(SearchError::NotAbsolute);
        }
        return Ok(alloc::vec![origin_subst(
            ctx,
            refobj.map(|object| object.id),
            name
        )?]);
    }
    let mut output = Vec::new();
    if let Some(object) = refobj
        && object.rpath.is_some()
        && ctx.library_path_rpath
    {
        if let Some(path) = ctx.ld_library_path
            && ctx.trust
        {
            candidates_from(ctx, path, name, Some(object.id), &mut output)?;
        }
        if let Some(path) = object.rpath {
            candidates_from(ctx, path, name, Some(object.id), &mut output)?;
        }
        for directory in ctx.default_dirs {
            candidates_from(ctx, directory, name, Some(object.id), &mut output)?;
        }
        return Ok(output);
    }

    if let Some(object) = refobj
        && object.runpath.is_none()
        && let Some(path) = object.rpath
    {
        candidates_from(ctx, path, name, Some(object.id), &mut output)?;
    }
    if let Some(object) = refobj
        && object.runpath.is_none()
        && !object.is_main
        && let Some(path) = ctx.main_rpath
    {
        candidates_from(ctx, path, name, Some(object.id), &mut output)?;
    }
    if let Some(path) = ctx.ld_library_path
        && ctx.trust
    {
        candidates_from(ctx, path, name, refobj.map(|object| object.id), &mut output)?;
    }
    if let Some(object) = refobj
        && let Some(path) = object.runpath
    {
        candidates_from(ctx, path, name, Some(object.id), &mut output)?;
    }
    if !refobj.is_some_and(|object| object.nodeflib) {
        for directory in ctx.default_dirs {
            candidates_from(
                ctx,
                directory,
                name,
                refobj.map(|object| object.id),
                &mut output,
            )?;
        }
    }
    Ok(output)
}

#[cfg(test)]
#[path = "search_ut.rs"]
mod tests;

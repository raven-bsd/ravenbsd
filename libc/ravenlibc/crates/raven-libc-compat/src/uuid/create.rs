// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// Copyright (c) 2002 Marcel Moolenaar
// Copyright (c) 2002 Hiten Mahesh Pandya
// All rights reserved.
//
// RavenBSD: CLEANROOM version of freebsd:lib/libc/uuid/uuid_create.c @ 133344405f532bd54ff3629a77d4c4e062f01fdb
// (file revision tag: $NetBSD: uuid_create.c,v 1.1 2004/09/13 21:44:54 thorpej Exp $)
// The donor calls uuidgen(2); Raven has no such system call, so this is a version-4 UUID
// seeded from the Entropy seam (ticket 5.B, RD-new; A-hard-12/13, PG-52).
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
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `uuid_create` (version 4) and `uuid_create_nil`.

use crate::seams::Entropy;

use super::{Uuid, dec_be};

/// `uuid_create`: a random (version 4) UUID.
///
/// The 16 bytes come from `entropy`; byte 6 carries version 4 and byte 8 the `10xx` variant.
pub fn create<E: Entropy>(entropy: &mut E) -> Uuid {
    let mut bytes = [0u8; 16];
    entropy.fill(&mut bytes);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    dec_be(&bytes)
}

/// `uuid_create_nil`: the nil UUID.
#[must_use]
pub fn create_nil() -> Uuid {
    Uuid::default()
}

#[cfg(test)]
#[path = "create_ut.rs"]
mod tests;

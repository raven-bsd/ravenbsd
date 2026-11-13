// SPDX-License-Identifier: BSD-2-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// Copyright (c) 2002 Marcel Moolenaar
// All rights reserved.
//
// RavenBSD: transpiled from netbsd:lib/libc/uuid/uuid_compare.c @ 36eddebcc36cb0775aa2962005b9c05419a6ff01
// (file revision tag: $NetBSD: uuid_compare.c,v 1.2 2008/04/23 07:52:32 plunky Exp $)
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

//! `uuid_compare`: the donor's field order, `-1`/`0`/`1`.

use super::Uuid;

/// `uuid_compare`.
#[must_use]
pub fn compare(a: &Uuid, b: &Uuid) -> i32 {
    if a.time_low != b.time_low {
        return if a.time_low < b.time_low { -1 } else { 1 };
    }
    if a.time_mid != b.time_mid {
        return if a.time_mid < b.time_mid { -1 } else { 1 };
    }
    if a.time_hi_and_version != b.time_hi_and_version {
        return if a.time_hi_and_version < b.time_hi_and_version {
            -1
        } else {
            1
        };
    }
    if a.clock_seq_hi_and_reserved != b.clock_seq_hi_and_reserved {
        return if a.clock_seq_hi_and_reserved < b.clock_seq_hi_and_reserved {
            -1
        } else {
            1
        };
    }
    if a.clock_seq_low != b.clock_seq_low {
        return if a.clock_seq_low < b.clock_seq_low {
            -1
        } else {
            1
        };
    }
    if a.node < b.node {
        return -1;
    }
    if a.node > b.node {
        return 1;
    }
    0
}

#[cfg(test)]
#[path = "compare_ut.rs"]
mod tests;

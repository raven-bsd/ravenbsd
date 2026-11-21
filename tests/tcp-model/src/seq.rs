// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// Copyright (c) 1982, 1986, 1993, 1995
//	The Regents of the University of California.  All rights reserved.
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions
// are met:
// 1. Redistributions of source code must retain the above copyright
//    notice, this list of conditions and the following disclaimer.
// 2. Redistributions in binary form must reproduce the above copyright
//    notice, this list of conditions and the following disclaimer in the
//    documentation and/or other materials provided with the distribution.
// 3. Neither the name of the University nor the names of its contributors
//    may be used to endorse or promote products derived from this software
//    without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE REGENTS AND CONTRIBUTORS ``AS IS'' AND
// ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED.  IN NO EVENT SHALL THE REGENTS OR CONTRIBUTORS BE LIABLE
// FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
// DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
// OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
// HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
// LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY
// OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
// SUCH DAMAGE.

//! Model-only port of FreeBSD `sys/netinet/tcp_seq.h` at
//! `133344405f532bd54ff3629a77d4c4e062f01fdb` (BSD-3-Clause).
//!
//! FreeBSD's `SEQ_*`, `tcp_sendseqinit` and `tcp_rcvseqinit` are ported literally. The
//! sequence windows are 32-bit modular arithmetic; `seq_distance` is a first-party helper for
//! byte counts and is not part of the donor file.

/// `SEQ_LT(a,b)`.
#[must_use]
pub const fn seq_lt(a: u32, b: u32) -> bool {
    (a.wrapping_sub(b) as i32) < 0
}

/// `SEQ_LEQ(a,b)`.
#[must_use]
pub const fn seq_le(a: u32, b: u32) -> bool {
    (a.wrapping_sub(b) as i32) <= 0
}

/// `SEQ_GT(a,b)`.
#[must_use]
pub const fn seq_gt(a: u32, b: u32) -> bool {
    (a.wrapping_sub(b) as i32) > 0
}

/// `SEQ_GEQ(a,b)`.
#[must_use]
pub const fn seq_ge(a: u32, b: u32) -> bool {
    (a.wrapping_sub(b) as i32) >= 0
}

/// `SEQ_MIN(a,b)`.
#[must_use]
pub const fn seq_min(a: u32, b: u32) -> u32 {
    if seq_lt(a, b) { a } else { b }
}

/// `SEQ_MAX(a,b)`.
#[must_use]
pub const fn seq_max(a: u32, b: u32) -> u32 {
    if seq_gt(a, b) { a } else { b }
}

/// `SEQ_SUB(a,b)`.
#[must_use]
pub const fn seq_sub(a: u32, b: u32) -> i32 {
    a.wrapping_sub(b) as i32
}

/// Unsigned distance from `a` forward to `b` (first-party helper).
#[must_use]
pub const fn seq_distance(a: u32, b: u32) -> u32 {
    b.wrapping_sub(a)
}

/// The fields initialised by FreeBSD `tcp_sendseqinit`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SendSeq {
    /// Oldest unacknowledged sequence.
    pub snd_una: u32,
    /// Next sequence to send.
    pub snd_nxt: u32,
    /// Highest sequence ever sent.
    pub snd_max: u32,
    /// Urgent pointer sequence.
    pub snd_up: u32,
    /// Recovery point.
    pub snd_recover: u32,
}

/// `tcp_sendseqinit`: every send-side field starts at `iss`.
#[must_use]
pub const fn tcp_sendseqinit(iss: u32) -> SendSeq {
    SendSeq {
        snd_una: iss,
        snd_nxt: iss,
        snd_max: iss,
        snd_up: iss,
        snd_recover: iss,
    }
}

/// `tcp_rcvseqinit`: return `(rcv_adv, rcv_nxt)`.
#[must_use]
pub const fn tcp_rcvseqinit(irs: u32) -> (u32, u32) {
    let next = irs.wrapping_add(1);
    (next, next)
}

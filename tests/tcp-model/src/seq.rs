// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Model-only literal port of the NetBSD TCP sequence arithmetic.
//!
//! Upstream: `sys/netinet/tcp_seq.h,v 1.17 2014/07/25 17:53:59 ryo Exp $`.
//!
//! Copyright (c) 1982, 1986, 1993, 1995
//!     The Regents of the University of California.  All rights reserved.
//!
//! Redistribution and use in source and binary forms, with or without modification, are
//! permitted provided that the following conditions are met:
//! 1. Redistributions of source code must retain the above copyright notice, this list of
//!    conditions and the following disclaimer.
//! 2. Redistributions in binary form must reproduce the above copyright notice, this list of
//!    conditions and the following disclaimer in the documentation and/or other materials
//!    provided with the distribution.
//! 3. Neither the name of the University nor the names of its contributors may be used to
//!    endorse or promote products derived from this software without specific prior written
//!    permission.
//!
//! THIS SOFTWARE IS PROVIDED BY THE REGENTS AND CONTRIBUTORS ``AS IS'' AND ANY EXPRESS OR
//! IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY
//! AND FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED.  IN NO EVENT SHALL THE REGENTS OR
//! CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
//! CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//! SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY
//! THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR
//! OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
//! POSSIBILITY OF SUCH DAMAGE.
//!
//! RavenBSD: model-only port used as the test reference. No product code is changed.

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

/// `SEQ_SUB(a,b)` in the NetBSD `long` representation.
#[must_use]
pub const fn seq_sub(a: u32, b: u32) -> i64 {
    a.wrapping_sub(b) as i64
}

/// Unsigned distance from `a` forward to `b` (the `b - a` byte count used by windows).
#[must_use]
pub const fn seq_distance(a: u32, b: u32) -> u32 {
    b.wrapping_sub(a)
}

/// The fields initialised by NetBSD `tcp_sendseqinit`.
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
    /// Highest SACKed sequence.
    pub snd_high: u32,
    /// Forward-most SACKed sequence.
    pub snd_fack: u32,
}

/// `tcp_sendseqinit`: every send-side high-water field starts at `iss`.
#[must_use]
pub const fn tcp_sendseqinit(iss: u32) -> SendSeq {
    SendSeq {
        snd_una: iss,
        snd_nxt: iss,
        snd_max: iss,
        snd_up: iss,
        snd_recover: iss,
        snd_high: iss,
        snd_fack: iss,
    }
}

/// `tcp_rcvseqinit`: return `(rcv_adv, rcv_nxt)`.
#[must_use]
pub const fn tcp_rcvseqinit(irs: u32) -> (u32, u32) {
    let next = irs.wrapping_add(1);
    (next, next)
}

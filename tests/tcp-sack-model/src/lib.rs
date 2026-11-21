// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-only SACK/DSACK reference for K268.
//!
//! `SackedSet` is a naive set of individual byte offsets. It is deliberately
//! slower and simpler than the product scoreboard; the randomized tests compare
//! the merged ranges, gaps, pipe and lost ranges against it. The adapter maps
//! absolute TCP sequence numbers to the scoreboard's `snd_una`-relative offsets.
//!
//! Donor: DragonFly `sys/netinet/tcp_sack.c` @
//! `94448f622a06cde90ba42d688e9cab2448ad9ab3` (3-clause BSD, behaviour only)
//! and RFC 6675. No donor code is copied.

#![forbid(unsafe_code)]

pub mod byte_set;
pub mod rng;

pub use byte_set::SackedSet;
pub use rng::Rng;

use raven_tcpip::SackScoreboard;

/// Map an absolute sequence number to an offset from `base`, preserving wrap.
#[must_use]
pub fn relative_offset(sequence: u32, base: u32) -> i64 {
    i64::from(sequence.wrapping_sub(base) as i32)
}

/// DragonFly `tcp_sack_ndsack_blocks`: 0 none, 1 DSACK, 2 duplicate out-of-order.
#[must_use]
pub fn classify_dsack(blocks: &[(i64, i64)], snd_una: i64) -> u8 {
    if blocks.is_empty() {
        return 0;
    }
    if blocks[0].0 < snd_una {
        return 1;
    }
    if blocks.len() > 1 && blocks[0].0 >= blocks[1].0 && blocks[0].1 <= blocks[1].1 {
        return 2;
    }
    0
}

/// Insert an absolute TCP range into a scoreboard relative to `snd_una`.
pub fn insert_absolute(board: &mut SackScoreboard, start: u32, end: u32, snd_una: u32) {
    board.insert_block(
        relative_offset(start, snd_una),
        relative_offset(end, snd_una),
    );
}

/// Build a scoreboard with the reference ranges already installed.
#[must_use]
pub fn board_from_reference(
    reference: &SackedSet,
    high_rxt: u32,
    high_ack: u32,
    mss: u32,
    dup_threshold: u32,
    range_cap: usize,
) -> SackScoreboard {
    let mut board = SackScoreboard::new(
        reference.high_data,
        high_rxt,
        high_ack,
        mss,
        dup_threshold,
        range_cap,
    );
    board.sacked = reference.runs();
    board.update_lostseq(0);
    board
}

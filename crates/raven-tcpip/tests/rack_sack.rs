// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Public-surface checks for the K336 SACK/DSACK/reneging group.

use raven_tcpip::{RACK_ACKED, RackSack};

#[test]
fn public_rack_sack_exposes_the_donor_group() {
    let mut sack = RackSack::new();
    sack.stack.tp.snd_una = 0;
    sack.stack.tp.snd_max = 100;
    sack.insert_segment(0, 100, 0, 1);

    assert_eq!(sack.rack_proc_sack_blk(0, 100, 100, 1_460), 100);
    assert_eq!(sack.sacked_bytes(), 100);
    assert_eq!(sack.stack.sacked, 100);
    assert_eq!(sack.sendmap[&0].flags & RACK_ACKED, RACK_ACKED);

    assert!(!sack.rack_note_dsack(0, 100));
    assert_eq!(sack.stack.num_dsacks, 1);
    assert_eq!(sack.dsack_persist, 16);
}

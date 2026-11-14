// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn constants_match_the_generated_table() {
    assert_eq!(number(EINVAL_ERR), 22);
    assert_eq!(number(EAGAIN_ERR), 11);
    assert_eq!(number(ENOMEM_ERR), 12);
    assert_eq!(number(ENOSYS_ERR), 38);
    assert_eq!(number(EPERM_ERR), 1);
    assert_eq!(number(EINTR_ERR), 4);
    assert_eq!(number(ETIMEDOUT_ERR), 110);
}

#[test]
fn errno_round_trips() {
    assert_eq!(errno(9).0, 9);
    assert!(EINVAL_ERR.is_linux());
}

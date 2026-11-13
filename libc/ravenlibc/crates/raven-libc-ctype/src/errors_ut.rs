// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
use super::*;

#[test]
fn ilseq_is_the_only_error_and_matches_the_abi_constant() {
    assert_eq!(MbError::Ilseq, MbError::Ilseq);
    assert_eq!(raven_libc_abi::errno::EILSEQ, 84);
}

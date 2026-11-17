// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the `eventfd` write value.

use super::eventfd_value;
use raven_libc_platform::Errno;

#[test]
fn an_eight_byte_little_endian_value_is_accepted() {
    assert_eq!(eventfd_value(&1u64.to_le_bytes()), Ok(1));
    assert_eq!(
        eventfd_value(&0x0102_0304_0506_0708u64.to_le_bytes()),
        Ok(0x0102_0304_0506_0708)
    );
}

#[test]
fn any_other_length_is_einval() {
    assert_eq!(eventfd_value(&[]), Err(Errno::EINVAL));
    assert_eq!(eventfd_value(&[1, 2, 3, 4]), Err(Errno::EINVAL));
    assert_eq!(eventfd_value(&[0; 9]), Err(Errno::EINVAL));
}

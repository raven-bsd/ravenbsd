// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pure user-pointer validation.
//!
//! This crate performs no dereference and no copy: it only checks that an address and length lie
//! inside a caller-supplied user range, that the length is representable, and that typed pointers
//! meet their alignment. The kernel performs the actual fault-safe copy in assembly and uses the
//! same predicates before touching user memory.

#![no_std]
#![forbid(unsafe_code)]

use core::mem::{align_of, size_of};

use raven_abi::errno::Errno;
#[cfg(target_arch = "aarch64")]
use raven_abi::layout::aarch64::{USER_END, USER_MIN};
#[cfg(target_arch = "x86_64")]
use raven_abi::layout::x86_64::{USER_END, USER_MIN};

/// A validated byte range in user space.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UserSlice {
    address: u64,
    length: u64,
}

impl UserSlice {
    /// Validate a byte range against the architecture user range.
    pub fn new(address: u64, length: u64) -> Result<Self, Errno> {
        Self::new_in(address, length, USER_MIN, USER_END)
    }

    /// Validate a byte range against an explicit half-open range (tests and non-x86 users).
    pub fn new_in(address: u64, length: u64, start: u64, end: u64) -> Result<Self, Errno> {
        validate_range(address, length, start, end)?;
        Ok(Self { address, length })
    }

    /// First address.
    pub const fn address(self) -> u64 {
        self.address
    }

    /// Length in bytes.
    pub const fn length(self) -> u64 {
        self.length
    }

    /// True for a zero-length range.
    pub const fn is_empty(self) -> bool {
        self.length == 0
    }
}

/// A validated pointer to one `T` in user space.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UserPtr<T> {
    address: u64,
    phantom: core::marker::PhantomData<T>,
}

impl<T> UserPtr<T> {
    /// Validate a typed user pointer against the architecture user range.
    pub fn new(address: u64) -> Result<Self, Errno> {
        Self::new_in(address, USER_MIN, USER_END)
    }

    /// Validate a typed user pointer against an explicit range.
    pub fn new_in(address: u64, start: u64, end: u64) -> Result<Self, Errno> {
        if !address.is_multiple_of(align_of::<T>() as u64) {
            return Err(Errno::EFAULT);
        }
        let length = u64::try_from(size_of::<T>()).map_err(|_| Errno::EFAULT)?;
        validate_range(address, length, start, end)?;
        Ok(Self {
            address,
            phantom: core::marker::PhantomData,
        })
    }

    /// The pointer value.
    pub const fn address(self) -> u64 {
        self.address
    }
}

/// Validate `address..address + length` against `start..end`.
///
/// A zero-length range only needs an address inside `start..=end`; a non-zero range must end at or
/// before `end`. Every failure is `EFAULT`, including overflow and lengths above `isize::MAX`.
pub fn validate_range(address: u64, length: u64, start: u64, end: u64) -> Result<(), Errno> {
    if start >= end || length > isize::MAX as u64 {
        return Err(Errno::EFAULT);
    }
    if address < start || address > end {
        return Err(Errno::EFAULT);
    }
    let Some(last) = address.checked_add(length) else {
        return Err(Errno::EFAULT);
    };
    if last > end {
        return Err(Errno::EFAULT);
    }
    if length != 0 && address >= end {
        return Err(Errno::EFAULT);
    }
    Ok(())
}

/// True when `address` is inside the architecture user range.
pub fn is_user_address(address: u64) -> bool {
    (USER_MIN..USER_END).contains(&address)
}

#[cfg(test)]
mod tests {
    use super::*;

    const START: u64 = 0x1000;
    const END: u64 = 0x4000;

    #[test]
    fn normal_range() {
        assert!(UserSlice::new_in(0x2000, 0x100, START, END).is_ok());
    }

    #[test]
    fn null_is_rejected() {
        assert_eq!(UserSlice::new_in(0, 1, START, END), Err(Errno::EFAULT));
    }

    #[test]
    fn below_start_is_rejected() {
        assert_eq!(
            UserSlice::new_in(START - 1, 1, START, END),
            Err(Errno::EFAULT)
        );
    }

    #[test]
    fn kernel_half_is_rejected() {
        assert_eq!(
            UserSlice::new_in(0xFFFF_8000_0000_0000, 1, START, END),
            Err(Errno::EFAULT)
        );
    }

    #[test]
    fn end_overflow_is_rejected() {
        assert_eq!(
            UserSlice::new_in(END - 1, 2, START, END),
            Err(Errno::EFAULT)
        );
    }

    #[test]
    fn exact_end_with_zero_length_is_allowed() {
        assert!(UserSlice::new_in(END, 0, START, END).is_ok());
    }

    #[test]
    fn exact_end_with_data_is_rejected() {
        assert_eq!(UserSlice::new_in(END, 1, START, END), Err(Errno::EFAULT));
    }

    #[test]
    fn zero_length_inside_range_is_allowed() {
        assert!(UserSlice::new_in(START, 0, START, END).is_ok());
    }

    #[test]
    fn zero_length_below_range_is_rejected() {
        assert_eq!(
            UserSlice::new_in(START - 1, 0, START, END),
            Err(Errno::EFAULT)
        );
    }

    #[test]
    fn length_above_isize_max_is_rejected() {
        assert_eq!(
            UserSlice::new_in(START, (isize::MAX as u64) + 1, START, END),
            Err(Errno::EFAULT)
        );
    }

    #[test]
    fn max_length_is_rejected_without_overflow() {
        assert_eq!(
            UserSlice::new_in(START, u64::MAX, START, END),
            Err(Errno::EFAULT)
        );
    }

    #[test]
    fn exact_boundary_is_accepted() {
        assert!(UserSlice::new_in(START, END - START, START, END).is_ok());
    }

    #[test]
    fn typed_pointer_checks_size() {
        let pointer = UserPtr::<u64>::new_in(START, START, END).expect("aligned pointer");
        assert_eq!(pointer.address(), START);
    }

    #[test]
    fn typed_pointer_rejects_misalignment() {
        assert_eq!(
            UserPtr::<u32>::new_in(START + 1, START, END),
            Err(Errno::EFAULT)
        );
    }

    #[test]
    fn typed_pointer_needs_full_object() {
        assert_eq!(
            UserPtr::<u64>::new_in(END - 4, START, END),
            Err(Errno::EFAULT)
        );
    }

    #[test]
    fn user_address_helper() {
        assert!(is_user_address(USER_MIN));
        assert!(is_user_address(USER_END - 1));
        assert!(!is_user_address(0));
        assert!(!is_user_address(USER_END));
        assert!(!is_user_address(0xFFFF_8000_0000_0000));
    }

    #[test]
    fn invalid_range_is_rejected() {
        assert_eq!(UserSlice::new_in(0x2000, 1, END, START), Err(Errno::EFAULT));
    }
}

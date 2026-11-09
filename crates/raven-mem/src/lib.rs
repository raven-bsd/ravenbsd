// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `mem*` routines for bare-metal RavenBSD targets.
//!
//! Prebuilt `core` does not provide the `memcpy` family on `-none` targets, and the compiler is
//! allowed to emit calls to them at any time. The crate is therefore `#![no_builtins]` so that its
//! own loops are never turned back into calls to the symbols it defines (which would recurse
//! forever). The C ABI wrappers are exported only with the `export-symbols` feature, which the
//! kernel enables; host tests can then still link against libc's symbols.

#![cfg_attr(not(test), no_std)]
#![no_builtins]

use core::mem::size_of;

/// Copies `n` bytes from `src` to `dst`.
///
/// # Safety
///
/// `src` must be valid for reads of `n` bytes and `dst` must be valid for writes of `n` bytes.
/// The two regions must not overlap; use [`copy`] for overlapping regions.
#[inline]
pub unsafe fn copy_nonoverlapping(dst: *mut u8, src: *const u8, n: usize) {
    #[cfg(target_arch = "x86_64")]
    if n >= 32 {
        // SAFETY: the caller guarantees that `src` and `dst` are valid for `n` bytes and do not
        // overlap. `rep movsb` copies exactly `n` bytes from `rsi` to `rdi`; the direction flag is
        // clear by the SysV ABI, and the instruction changes neither the stack nor the flags. All
        // registers it modifies are declared below.
        unsafe {
            core::arch::asm!(
                "rep movsb",
                inout("rcx") n => _,
                inout("rdi") dst => _,
                inout("rsi") src => _,
                options(nostack, preserves_flags),
            );
        }
        return;
    }

    // SAFETY: the caller guarantees the same non-overlapping, valid-for-`n`-bytes contract.
    unsafe { copy_words_forward(dst, src, n) };
}

/// Copies `n` bytes from `src` to `dst`, tolerating overlap.
///
/// # Safety
///
/// `src` must be valid for reads of `n` bytes and `dst` must be valid for writes of `n` bytes.
/// The regions may overlap; the copy direction is chosen from the pointer order.
#[inline]
pub unsafe fn copy(dst: *mut u8, src: *const u8, n: usize) {
    if n == 0 || dst == src.cast_mut() {
        return;
    }
    if (dst as usize) < (src as usize) {
        // SAFETY: the caller guarantees validity for `n` bytes. Copying in ascending order is
        // safe for overlap when the destination starts below the source.
        unsafe { copy_words_forward(dst, src, n) };
    } else {
        // SAFETY: the caller guarantees validity for `n` bytes. Copying in descending order is
        // safe for overlap when the destination starts above the source.
        unsafe { copy_words_backward(dst, src, n) };
    }
}

/// Writes `byte` to the first `n` bytes of `dst`.
///
/// # Safety
///
/// `dst` must be valid for writes of `n` bytes.
#[inline]
pub unsafe fn fill(dst: *mut u8, byte: u8, n: usize) {
    #[cfg(target_arch = "x86_64")]
    if n >= 32 {
        // SAFETY: the caller guarantees that `dst` is valid for `n` bytes. `rep stosb` writes
        // exactly `n` copies of `al` to `rdi`; the direction flag is clear by the SysV ABI, and
        // the instruction changes neither the stack nor the flags. All registers it modifies are
        // declared below.
        unsafe {
            core::arch::asm!(
                "rep stosb",
                inout("rcx") n => _,
                inout("rdi") dst => _,
                in("al") byte,
                options(nostack, preserves_flags),
            );
        }
        return;
    }

    let word = size_of::<usize>();
    let pattern = usize::from_ne_bytes([byte; size_of::<usize>()]);
    let mut offset = 0;
    while n - offset >= word {
        // SAFETY: `n - offset >= word`, and the caller guarantees that `dst` is valid for `n`
        // bytes, so the word at `dst + offset` is writable.
        unsafe { core::ptr::write_unaligned(dst.add(offset).cast::<usize>(), pattern) };
        offset += word;
    }
    while offset < n {
        // SAFETY: `offset < n`, and the caller guarantees that `dst` is valid for `n` bytes.
        unsafe { *dst.add(offset) = byte };
        offset += 1;
    }
}

/// Compares the first `n` bytes of `a` and `b`.
///
/// Returns the difference `a[i] - b[i]` of the first mismatching byte, or zero when the first `n`
/// bytes are equal. The sign therefore matches lexicographic slice comparison.
///
/// # Safety
///
/// `a` and `b` must be valid for reads of `n` bytes.
#[inline]
pub unsafe fn compare(a: *const u8, b: *const u8, n: usize) -> i32 {
    let mut offset = 0;
    while offset < n {
        // SAFETY: `offset < n`, and the caller guarantees that both pointers are valid for `n`
        // bytes.
        let (left, right) = unsafe { (*a.add(offset), *b.add(offset)) };
        if left != right {
            return i32::from(left) - i32::from(right);
        }
        offset += 1;
    }
    0
}

/// Copies `n` bytes in ascending address order, `usize`-sized chunks first and a byte tail last.
///
/// # Safety
///
/// `src` must be valid for reads of `n` bytes and `dst` must be valid for writes of `n` bytes.
/// When the regions overlap, `dst` must start below `src`.
unsafe fn copy_words_forward(dst: *mut u8, src: *const u8, n: usize) {
    let word = size_of::<usize>();
    let mut offset = 0;
    while n - offset >= word {
        // SAFETY: the word ending at `offset + word` lies inside both `n`-byte regions, and the
        // ascending direction guarantees that reading the source happens before that word of the
        // destination can overlap an unread source byte.
        unsafe {
            let value = core::ptr::read_unaligned(src.add(offset).cast::<usize>());
            core::ptr::write_unaligned(dst.add(offset).cast::<usize>(), value);
        }
        offset += word;
    }
    while offset < n {
        // SAFETY: `offset < n` and both pointers are valid for `n` bytes; ascending order is safe
        // for the overlap direction required by the caller.
        unsafe { *dst.add(offset) = *src.add(offset) };
        offset += 1;
    }
}

/// Copies `n` bytes in descending address order, `usize`-sized chunks first and a byte tail last.
///
/// # Safety
///
/// `src` must be valid for reads of `n` bytes and `dst` must be valid for writes of `n` bytes.
/// When the regions overlap, `dst` must start above `src`.
unsafe fn copy_words_backward(dst: *mut u8, src: *const u8, n: usize) {
    let word = size_of::<usize>();
    let mut offset = n;
    while offset >= word {
        offset -= word;
        // SAFETY: the word at `offset` lies inside both `n`-byte regions, and the descending
        // direction guarantees that reading the source happens before that word of the
        // destination can overlap an unread source byte.
        unsafe {
            let value = core::ptr::read_unaligned(src.add(offset).cast::<usize>());
            core::ptr::write_unaligned(dst.add(offset).cast::<usize>(), value);
        }
    }
    while offset > 0 {
        offset -= 1;
        // SAFETY: `offset < n` and both pointers are valid for `n` bytes; descending order is
        // safe for the overlap direction required by the caller.
        unsafe { *dst.add(offset) = *src.add(offset) };
    }
}

#[cfg(feature = "export-symbols")]
mod exports {
    use super::{compare, copy, copy_nonoverlapping, fill};

    /// C ABI `memcpy`: copies `n` non-overlapping bytes and returns `dst`.
    ///
    /// # Safety
    ///
    /// `src` must be valid for reads of `n` bytes and `dst` must be valid for writes of `n` bytes;
    /// the regions must not overlap.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn memcpy(dst: *mut u8, src: *const u8, n: usize) -> *mut u8 {
        // SAFETY: forwarded from this function's contract.
        unsafe { copy_nonoverlapping(dst, src, n) };
        dst
    }

    /// C ABI `memmove`: copies `n` possibly overlapping bytes and returns `dst`.
    ///
    /// # Safety
    ///
    /// `src` must be valid for reads of `n` bytes and `dst` must be valid for writes of `n` bytes.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn memmove(dst: *mut u8, src: *const u8, n: usize) -> *mut u8 {
        // SAFETY: forwarded from this function's contract.
        unsafe { copy(dst, src, n) };
        dst
    }

    /// C ABI `memset`: writes `n` copies of `c` to `dst` and returns `dst`.
    ///
    /// # Safety
    ///
    /// `dst` must be valid for writes of `n` bytes.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn memset(dst: *mut u8, c: i32, n: usize) -> *mut u8 {
        // SAFETY: forwarded from this function's contract.
        unsafe { fill(dst, c as u8, n) };
        dst
    }

    /// C ABI `memcmp`: compares the first `n` bytes of `a` and `b`.
    ///
    /// # Safety
    ///
    /// `a` and `b` must be valid for reads of `n` bytes.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn memcmp(a: *const u8, b: *const u8, n: usize) -> i32 {
        // SAFETY: forwarded from this function's contract.
        unsafe { compare(a, b, n) }
    }

    /// C ABI `bcmp`: compares the first `n` bytes of `a` and `b`.
    ///
    /// # Safety
    ///
    /// `a` and `b` must be valid for reads of `n` bytes.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn bcmp(a: *const u8, b: *const u8, n: usize) -> i32 {
        // SAFETY: forwarded from this function's contract.
        unsafe { compare(a, b, n) }
    }
}

#[cfg(test)]
mod tests {
    use super::{compare, copy, copy_nonoverlapping, fill};
    use core::cmp::Ordering;

    /// Bytes placed before and after every destination under test.
    const GUARD: usize = 16;
    /// Sentinel value for untouched destination and guard bytes.
    const GUARD_BYTE: u8 = 0xCD;

    fn pattern(len: usize) -> Vec<u8> {
        (0..len)
            .map(|i| (i as u8).wrapping_mul(37).wrapping_add(11))
            .collect()
    }

    fn sign(ordering: Ordering) -> i32 {
        match ordering {
            Ordering::Less => -1,
            Ordering::Equal => 0,
            Ordering::Greater => 1,
        }
    }

    fn assert_guards(dst: &[u8], start: usize, len: usize) {
        assert!(
            dst[..start].iter().all(|&b| b == GUARD_BYTE),
            "destination guard before the range was modified"
        );
        assert!(
            dst[start + len..].iter().all(|&b| b == GUARD_BYTE),
            "destination guard after the range was modified"
        );
    }

    #[test]
    fn copy_nonoverlapping_all_lengths_and_misalignments() {
        for len in 0..=300 {
            for src_misalignment in 0..8 {
                for dst_misalignment in 0..8 {
                    let source = pattern(len + src_misalignment);
                    let mut destination = vec![GUARD_BYTE; GUARD + dst_misalignment + len + GUARD];
                    let start = GUARD + dst_misalignment;

                    // SAFETY: both ranges are in bounds, and the source and destination are
                    // different allocations, so they cannot overlap.
                    unsafe {
                        copy_nonoverlapping(
                            destination.as_mut_ptr().add(start),
                            source.as_ptr().add(src_misalignment),
                            len,
                        );
                    }

                    assert_eq!(
                        &destination[start..start + len],
                        &source[src_misalignment..src_misalignment + len],
                        "len={len} src_misalignment={src_misalignment} \
                         dst_misalignment={dst_misalignment}"
                    );
                    assert_guards(&destination, start, len);
                }
            }
        }
    }

    #[test]
    fn copy_handles_overlap_in_both_directions() {
        for len in 0..=300 {
            let offsets = [(0usize, 24usize), (24, 0), (24, 24)];
            for (dst_offset, src_offset) in offsets {
                let mut buffer = pattern(len + 48);
                let mut expected = buffer.clone();

                expected.copy_within(src_offset..src_offset + len, dst_offset);

                // SAFETY: both ranges are inside `buffer`; `copy` is the overlap-safe operation
                // under test and the pointer is valid for writes of `len` bytes at `dst_offset`.
                unsafe {
                    let base = buffer.as_mut_ptr();
                    copy(base.add(dst_offset), base.add(src_offset), len);
                }

                assert_eq!(
                    buffer, expected,
                    "len={len} dst_offset={dst_offset} src_offset={src_offset}"
                );
            }
        }
    }

    #[test]
    fn fill_all_lengths_misalignments_and_bytes() {
        for len in 0..=300 {
            for misalignment in 0..8 {
                for byte in [0u8, 0xAA, 0xFF] {
                    let mut destination = vec![GUARD_BYTE; GUARD + misalignment + len + GUARD];
                    let start = GUARD + misalignment;

                    // SAFETY: the range is in bounds of `destination`.
                    unsafe { fill(destination.as_mut_ptr().add(start), byte, len) };

                    assert!(
                        destination[start..start + len].iter().all(|&b| b == byte),
                        "len={len} misalignment={misalignment} byte={byte:#04x}"
                    );
                    assert_guards(&destination, start, len);
                }
            }
        }
    }

    #[test]
    fn compare_equal_and_empty() {
        let bytes = [0u8, 1, 2, 3];
        // SAFETY: both pointers are valid for the length of the array.
        let equal = unsafe { compare(bytes.as_ptr(), bytes.as_ptr(), bytes.len()) };
        assert_eq!(equal, 0);

        let empty: [u8; 0] = [];
        // SAFETY: a length of zero requires no accessible memory.
        let empty_equal = unsafe { compare(empty.as_ptr(), empty.as_ptr(), 0) };
        assert_eq!(empty_equal, 0);
    }

    #[test]
    fn compare_sign_follows_first_difference() {
        let base = [0x40u8, 0x80, 0x00, 0xFF, 0x7F, 0x01, 0xAA, 0x55];
        for index in 0..base.len() {
            let left = base;
            let mut right = base;
            right[index] = right[index].wrapping_add(1);

            // SAFETY: both arrays are valid for `base.len()` bytes.
            let result = unsafe { compare(left.as_ptr(), right.as_ptr(), base.len()) };

            assert_eq!(sign(left.cmp(&right)), result.signum(), "index={index}");
            assert_eq!(
                result,
                i32::from(left[index]) - i32::from(right[index]),
                "index={index}"
            );

            // SAFETY: both arrays are valid for `base.len()` bytes.
            let reversed = unsafe { compare(right.as_ptr(), left.as_ptr(), base.len()) };
            assert_eq!(sign(right.cmp(&left)), reversed.signum(), "index={index}");
        }
    }

    fn xorshift(state: &mut u64) -> u64 {
        let mut value = *state;
        value ^= value << 13;
        value ^= value >> 7;
        value ^= value << 17;
        *state = value;
        value
    }

    #[test]
    fn compare_sign_matches_slice_cmp_for_random_pairs() {
        let mut state = 0x1234_5678_9ABC_DEF0u64;
        for case in 0..1000 {
            let len = (xorshift(&mut state) % 301) as usize;
            let mut left = vec![0u8; len];
            let mut right = vec![0u8; len];
            for byte in &mut left {
                *byte = xorshift(&mut state) as u8;
            }
            for byte in &mut right {
                *byte = xorshift(&mut state) as u8;
            }

            // SAFETY: both vectors are valid for their common length.
            let result = unsafe { compare(left.as_ptr(), right.as_ptr(), len) };
            assert_eq!(
                result.signum(),
                sign(left.cmp(&right)),
                "case={case} len={len}"
            );
        }
    }
}

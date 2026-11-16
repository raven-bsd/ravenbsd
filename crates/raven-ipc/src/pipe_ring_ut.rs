// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the pipe ring.

use crate::pipe_ring::Ring;

fn fill(ring: &mut Ring, bytes: &[u8]) -> usize {
    let (first, second) = ring.write_parts();
    let first_len = first.len().min(bytes.len());
    first[..first_len].copy_from_slice(&bytes[..first_len]);
    let rest = bytes.get(first_len..).unwrap_or(&[]);
    let second_len = second.len().min(rest.len());
    second[..second_len].copy_from_slice(&rest[..second_len]);
    let written = first_len + second_len;
    ring.produce(written);
    written
}

fn drain(ring: &mut Ring, out: &mut [u8]) -> usize {
    let (first, second) = ring.read_parts();
    let first_len = first.len().min(out.len());
    out[..first_len].copy_from_slice(&first[..first_len]);
    let rest = out.get_mut(first_len..).unwrap_or(&mut []);
    let second_len = second.len().min(rest.len());
    rest[..second_len].copy_from_slice(&second[..second_len]);
    let read = first_len + second_len;
    ring.consume(read);
    read
}

#[test]
fn zero_capacity_is_rejected() {
    assert!(Ring::new(0).is_none());
}

#[test]
fn standard_pipe_ring_is_64_kib() {
    let ring = Ring::pipe().expect("64 KiB allocation");
    assert_eq!(ring.capacity(), 64 * 1024);
    assert!(ring.is_empty());
    assert_eq!(ring.free(), ring.capacity());
}

#[test]
fn write_then_read_round_trips() {
    let mut ring = Ring::new(8).expect("ring");
    assert_eq!(fill(&mut ring, b"abc"), 3);
    assert_eq!(ring.len(), 3);
    let mut out = [0u8; 8];
    assert_eq!(drain(&mut ring, &mut out), 3);
    assert_eq!(&out[..3], b"abc");
    assert!(ring.is_empty());
}

#[test]
fn reads_wrap_in_fifo_order() {
    let mut ring = Ring::new(8).expect("ring");
    assert_eq!(fill(&mut ring, b"abcdef"), 6);
    let mut out = [0u8; 4];
    assert_eq!(drain(&mut ring, &mut out), 4);
    assert_eq!(&out, b"abcd");
    // Four bytes in, four bytes out: the tail is now at byte 2, so "ghij" wraps.
    assert_eq!(fill(&mut ring, b"ghij"), 4);
    let (first, second) = ring.read_parts();
    assert_eq!(first, b"efgh");
    assert_eq!(second, b"ij");
    let mut out = [0u8; 6];
    assert_eq!(drain(&mut ring, &mut out), 6);
    assert_eq!(&out, b"efghij");
}

#[test]
fn free_space_reports_both_segments() {
    let mut ring = Ring::new(8).expect("ring");
    assert_eq!(fill(&mut ring, b"abcde"), 5);
    let (first, second) = ring.write_parts();
    assert_eq!(first.len() + second.len(), 3);
    let mut out = [0u8; 5];
    assert_eq!(drain(&mut ring, &mut out), 5);
    let (first, second) = ring.write_parts();
    assert_eq!(first.len() + second.len(), 8);
}

#[test]
fn counts_clamp_at_the_boundaries() {
    let mut ring = Ring::new(4).expect("ring");
    ring.produce(10);
    assert_eq!(ring.len(), 4);
    assert_eq!(ring.free(), 0);
    ring.consume(10);
    assert!(ring.is_empty());
    assert_eq!(ring.free(), 4);
    ring.produce(0);
    ring.consume(0);
    assert!(ring.is_empty());
}

#[test]
fn a_full_ring_has_no_write_space() {
    let mut ring = Ring::new(4).expect("ring");
    assert_eq!(fill(&mut ring, b"wxyz"), 4);
    let (first, second) = ring.write_parts();
    assert!(first.is_empty());
    assert!(second.is_empty());
    assert_eq!(fill(&mut ring, b"!"), 0);
}

#[test]
fn zero_length_read_and_write_are_no_ops() {
    let mut ring = Ring::new(4).expect("ring");
    assert_eq!(fill(&mut ring, b""), 0);
    assert_eq!(drain(&mut ring, &mut []), 0);
    assert!(ring.is_empty());
}

// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn counting_sink_counts_writes_and_pads() {
    let mut sink = CountingSink::<u8>::new();
    sink.write(b"abcd").unwrap();
    sink.pad(b'z', 70).unwrap();
    assert_eq!(sink.count(), 74);
}

#[test]
fn slice_sink_stores_what_fits_and_counts_all() {
    let mut buf = [0u8; 4];
    let mut sink = SliceSink::new(&mut buf);
    sink.write(b"abcdef").unwrap();
    assert_eq!(sink.len(), 6);
    assert_eq!(sink.stored(), 4);
    assert_eq!(buf, *b"abcd");
}

#[test]
fn slice_sink_pads_through_write() {
    let mut buf = [0u8; 40];
    let mut sink = SliceSink::new(&mut buf);
    sink.pad(b'-', 40).unwrap();
    assert_eq!(sink.stored(), 40);
    assert!(buf.iter().all(|&b| b == b'-'));
}

#[test]
fn bounded_sink_counts_past_its_cap() {
    let mut buf = [0u8; 8];
    let mut sink = BoundedSink::new(&mut buf, 3);
    sink.write(b"012345").unwrap();
    assert_eq!(sink.len(), 6);
    assert_eq!(sink.stored(), 3);
    assert_eq!(&buf[..3], b"012");
    assert_eq!(&buf[3..], &[0u8; 5]);
}

#[test]
fn bounded_sink_pad_counts_every_unit() {
    let mut buf = [0u8; 4];
    let mut sink = BoundedSink::new(&mut buf, 2);
    sink.pad(b'q', 70).unwrap();
    assert_eq!(sink.len(), 70);
    assert_eq!(sink.stored(), 2);
    assert_eq!(&buf[..2], b"qq");
}

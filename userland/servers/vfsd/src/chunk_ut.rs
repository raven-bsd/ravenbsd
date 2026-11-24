// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the vfsd chunk loop.

use super::*;

const THREE_MIB: usize = 3_019_776;

fn copy_with_chunk(chunk: usize) -> (usize, usize, Vec<u8>) {
    let source: Vec<u8> = (0..THREE_MIB).map(|index| (index % 251) as u8).collect();
    let mut out = vec![0u8; source.len()];
    let mut scratch = vec![0u8; chunk];
    let mut calls = 0usize;
    let copied = copy_chunks(&mut out, &mut scratch, |done, buffer| {
        calls += 1;
        let count = buffer.len().min(source.len() - done);
        buffer[..count].copy_from_slice(&source[done..done + count]);
        Ok::<usize, ()>(count)
    })
    .expect("copy");
    (copied, calls, out)
}

#[test]
fn sixty_four_kib_reads_use_forty_seven_calls() {
    let (copied, calls, out) = copy_with_chunk(64 * 1024);
    assert_eq!(copied, THREE_MIB);
    assert_eq!(calls, THREE_MIB.div_ceil(64 * 1024));
    assert_eq!(out[0], 0);
    assert_eq!(out[THREE_MIB - 1], ((THREE_MIB - 1) % 251) as u8);
}

#[test]
fn four_kib_reads_are_the_old_storm() {
    let (copied, calls, _) = copy_with_chunk(4096);
    assert_eq!(copied, THREE_MIB);
    assert_eq!(calls, THREE_MIB.div_ceil(4096));
    assert!(calls > 700);
}

#[test]
fn a_short_read_is_the_end_of_the_file() {
    let source = b"short";
    let mut out = [0u8; 32];
    let mut scratch = [0u8; 16];
    let copied = copy_chunks(&mut out, &mut scratch, |done, buffer| {
        let count = buffer.len().min(source.len() - done);
        buffer[..count].copy_from_slice(&source[done..done + count]);
        Ok::<usize, ()>(count)
    })
    .expect("copy");
    assert_eq!(copied, source.len());
    assert_eq!(&out[..source.len()], source);
    assert_eq!(out[source.len()], 0);
}

#[test]
fn an_overrun_is_rejected() {
    let mut out = [0u8; 8];
    let mut scratch = [0u8; 8];
    let error = copy_chunks(&mut out, &mut scratch, |_, buffer| {
        Ok::<usize, ()>(buffer.len() + 1)
    });
    assert_eq!(error, Err(ChunkError::Overrun));
}

#[test]
fn an_empty_copy_makes_no_call() {
    let mut out = [];
    let mut scratch = [0u8; 8];
    let copied = copy_chunks(&mut out, &mut scratch, |_, _| -> Result<usize, ()> {
        panic!("a zero-length copy must not call the reader")
    })
    .expect("copy");
    assert_eq!(copied, 0);
}

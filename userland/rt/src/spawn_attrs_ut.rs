// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the spawn-attribute block, stack rewrite and auxiliary-vector keys.

use super::*;

fn push_word(bytes: &mut Vec<u8>, value: usize) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

/// A minimal C8 word array: argv[0] = 0x1000, no environment, one auxv pair, AT_NULL.
fn stack_image() -> Vec<u8> {
    let mut bytes = Vec::new();
    push_word(&mut bytes, 1);
    push_word(&mut bytes, 0x1000);
    push_word(&mut bytes, 0);
    push_word(&mut bytes, 0);
    push_word(&mut bytes, 6);
    push_word(&mut bytes, 4096);
    push_word(&mut bytes, 0);
    push_word(&mut bytes, 0);
    bytes.extend_from_slice(b"strings stay here");
    bytes
}

fn read_word(bytes: &[u8], offset: usize) -> usize {
    usize::from_le_bytes(
        bytes[offset..offset + core::mem::size_of::<usize>()]
            .try_into()
            .expect("word"),
    )
}

#[test]
fn fdmap_block_has_the_documented_layout() {
    let block = encode_fdmap(&[(7, 0x0102_0304), (0, 5)]);
    assert_eq!(&block[0..4], b"RFDM");
    assert_eq!(u16::from_le_bytes([block[4], block[5]]), 1);
    assert_eq!(u16::from_le_bytes([block[6], block[7]]), 2);
    assert_eq!(block.len(), 8 + 2 * 18);
    assert_eq!(i32::from_le_bytes(block[8..12].try_into().unwrap()), 7);
    assert_eq!(
        u32::from_le_bytes(block[12..16].try_into().unwrap()),
        0x0102_0304
    );
    assert_eq!(block[16], 255);
    assert_eq!(block[17], 0);
    assert_eq!(u32::from_le_bytes(block[18..22].try_into().unwrap()), 2);
    assert_eq!(
        u32::from_le_bytes(block[22..26].try_into().unwrap()),
        u32::MAX
    );
    assert_eq!(i32::from_le_bytes(block[26..30].try_into().unwrap()), 0);
    assert_eq!(u32::from_le_bytes(block[30..34].try_into().unwrap()), 5);
}

#[test]
fn an_empty_attribute_set_produces_no_page() {
    let empty = [0u8; 128];
    let page = AttributePage::build(
        &SpawnAttributes {
            fdmap: &[],
            sigmask: &empty,
            sigdefault: &empty,
            ignored: &empty,
        },
        &[],
    );
    assert!(page.is_empty());
    assert!(page.auxv(0x1000).is_empty());
}

#[test]
fn the_attribute_page_offsets_and_auxv_addresses_agree() {
    let empty = [0u8; 128];
    let mut mask = empty;
    mask[1] = 1 << 1;
    let fdmap = [(7, 9)];
    let page = AttributePage::build(
        &SpawnAttributes {
            fdmap: &fdmap,
            sigmask: &mask,
            sigdefault: &empty,
            ignored: &empty,
        },
        &fdmap,
    );
    let (fd_offset, count) = page.fdmap.expect("fdmap");
    assert_eq!(fd_offset, 0);
    assert_eq!(count, 1);
    let mask_offset = page.sigmask.expect("sigmask");
    assert!(mask_offset >= 8 + 18);
    assert_eq!(&page.bytes[mask_offset..mask_offset + 128], &mask);
    assert_eq!(page.sigdefault.expect("sigdefault"), mask_offset + 128);
    assert_eq!(page.ignored.expect("ignored"), mask_offset + 256);

    let pairs = page.auxv(0x4000);
    assert_eq!(pairs[0], (AT_RAVEN_FDMAP, 0x4000));
    assert_eq!(pairs[1], (AT_RAVEN_FDMAP_N, 1));
    assert_eq!(pairs[2], (AT_RAVEN_SIGMASK, 0x4000 + mask_offset));
    assert_eq!(pairs[3], (AT_RAVEN_SIGDEFAULT, 0x4000 + mask_offset + 128));
    assert_eq!(pairs[4], (AT_RAVEN_SIGIGNORE, 0x4000 + mask_offset + 256));
    assert_eq!(pairs[5], (0x7006, raven_abi::ABI_VERSION as usize));
}

#[test]
fn augmenting_a_stack_inserts_pairs_before_at_null() {
    let image = stack_image();
    let pairs = [(AT_RAVEN_FDMAP, 0x2000), (AT_RAVEN_FDMAP_N, 3)];
    let (rewritten, shift) = augment_stack(&image, &pairs).expect("rewrite");
    assert_eq!(shift, 4 * core::mem::size_of::<usize>());
    // argc and argv[0] are unchanged.
    assert_eq!(read_word(&rewritten, 0), 1);
    assert_eq!(read_word(&rewritten, core::mem::size_of::<usize>()), 0x1000);
    // The old terminator moved down and the new pairs sit before it.
    let word = core::mem::size_of::<usize>();
    let auxv = 4 * word;
    assert_eq!(read_word(&rewritten, auxv), 6);
    assert_eq!(read_word(&rewritten, auxv + word), 4096);
    assert_eq!(read_word(&rewritten, auxv + 2 * word), AT_RAVEN_FDMAP);
    assert_eq!(read_word(&rewritten, auxv + 3 * word), 0x2000);
    assert_eq!(read_word(&rewritten, auxv + 4 * word), AT_RAVEN_FDMAP_N);
    assert_eq!(read_word(&rewritten, auxv + 5 * word), 3);
    assert_eq!(read_word(&rewritten, auxv + 6 * word), 0);
    assert_eq!(read_word(&rewritten, auxv + 7 * word), 0);
    assert_eq!(rewritten.len(), auxv + 8 * word);
}

#[test]
fn an_unterminated_stack_is_rejected() {
    let mut image = stack_image();
    image.truncate(4 * core::mem::size_of::<usize>());
    assert_eq!(
        augment_stack(&image, &[(AT_RAVEN_FDMAP, 1)]),
        Err(SpawnError::TooLarge)
    );
}

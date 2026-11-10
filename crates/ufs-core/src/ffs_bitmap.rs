// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// Ported from NetBSD sys/ufs/ffs/ffs_subr.c and sys/ufs/ffs/ffs_tables.c
// (revision 36eddebcc36cb0775aa2962005b9c05419a6ff01). The original notice follows.
//
// Copyright (c) 1982, 1986, 1993
//	The Regents of the University of California.  All rights reserved.
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions
// are met:
// 1. Redistributions of source code must retain the above copyright
//    notice, this list of conditions and the following disclaimer.
// 2. Redistributions in binary form must reproduce the above copyright
//    notice, this list of conditions and the following disclaimer in the
//    documentation and/or other materials provided with the distribution.
// 3. Neither the name of the University nor the names of its contributors
//    may be used to endorse or promote products derived from this software
//    without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE REGENTS AND CONTRIBUTORS ``AS IS'' AND
// ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED.  IN NO EVENT SHALL THE REGENTS OR CONTRIBUTORS BE LIABLE
// FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
// DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
// OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
// HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
// LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY
// OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
// SUCH DAMAGE.

//! Free-map bit operations and fragment accounting.

use crate::error::UfsError;

pub(crate) const NBBY: usize = 8;

const AROUND: [i32; 9] = [0x3, 0x7, 0xf, 0x1f, 0x3f, 0x7f, 0xff, 0x1ff, 0x3ff];
const INSIDE: [i32; 9] = [0x0, 0x2, 0x6, 0xe, 0x1e, 0x3e, 0x7e, 0xfe, 0x1fe];

const FRAGTBL124: [u8; 256] = [
    0x00, 0x16, 0x16, 0x2a, 0x16, 0x16, 0x26, 0x4e, 0x16, 0x16, 0x16, 0x3e, 0x2a, 0x3e, 0x4e, 0x8a,
    0x16, 0x16, 0x16, 0x3e, 0x16, 0x16, 0x36, 0x5e, 0x16, 0x16, 0x16, 0x3e, 0x3e, 0x3e, 0x5e, 0x9e,
    0x16, 0x16, 0x16, 0x3e, 0x16, 0x16, 0x36, 0x5e, 0x16, 0x16, 0x16, 0x3e, 0x3e, 0x3e, 0x5e, 0x9e,
    0x2a, 0x3e, 0x3e, 0x2a, 0x3e, 0x3e, 0x2e, 0x6e, 0x3e, 0x3e, 0x3e, 0x3e, 0x2a, 0x3e, 0x6e, 0xaa,
    0x16, 0x16, 0x16, 0x3e, 0x16, 0x16, 0x36, 0x5e, 0x16, 0x16, 0x16, 0x3e, 0x3e, 0x3e, 0x5e, 0x9e,
    0x16, 0x16, 0x16, 0x3e, 0x16, 0x16, 0x36, 0x5e, 0x16, 0x16, 0x16, 0x3e, 0x3e, 0x3e, 0x5e, 0x9e,
    0x26, 0x36, 0x36, 0x2e, 0x36, 0x36, 0x26, 0x6e, 0x36, 0x36, 0x36, 0x3e, 0x2e, 0x3e, 0x6e, 0xae,
    0x4e, 0x5e, 0x5e, 0x6e, 0x5e, 0x5e, 0x6e, 0x4e, 0x5e, 0x5e, 0x5e, 0x7e, 0x6e, 0x7e, 0x4e, 0xce,
    0x16, 0x16, 0x16, 0x3e, 0x16, 0x16, 0x36, 0x5e, 0x16, 0x16, 0x16, 0x3e, 0x3e, 0x3e, 0x5e, 0x9e,
    0x16, 0x16, 0x16, 0x3e, 0x16, 0x16, 0x36, 0x5e, 0x16, 0x16, 0x16, 0x3e, 0x3e, 0x3e, 0x5e, 0x9e,
    0x16, 0x16, 0x16, 0x3e, 0x16, 0x16, 0x36, 0x5e, 0x16, 0x16, 0x16, 0x3e, 0x3e, 0x3e, 0x5e, 0x9e,
    0x3e, 0x3e, 0x3e, 0x3e, 0x3e, 0x3e, 0x3e, 0x7e, 0x3e, 0x3e, 0x3e, 0x3e, 0x3e, 0x3e, 0x7e, 0xbe,
    0x2a, 0x3e, 0x3e, 0x2a, 0x3e, 0x3e, 0x2e, 0x6e, 0x3e, 0x3e, 0x3e, 0x3e, 0x2a, 0x3e, 0x6e, 0xaa,
    0x3e, 0x3e, 0x3e, 0x3e, 0x3e, 0x3e, 0x3e, 0x7e, 0x3e, 0x3e, 0x3e, 0x3e, 0x3e, 0x3e, 0x7e, 0xbe,
    0x4e, 0x5e, 0x5e, 0x6e, 0x5e, 0x5e, 0x6e, 0x4e, 0x5e, 0x5e, 0x5e, 0x7e, 0x6e, 0x7e, 0x4e, 0xce,
    0x8a, 0x9e, 0x9e, 0xaa, 0x9e, 0x9e, 0xae, 0xce, 0x9e, 0x9e, 0x9e, 0xbe, 0xaa, 0xbe, 0xce, 0x8a,
];

const FRAGTBL8: [u8; 256] = [
    0x00, 0x01, 0x01, 0x02, 0x01, 0x01, 0x02, 0x04, 0x01, 0x01, 0x01, 0x03, 0x02, 0x03, 0x04, 0x08,
    0x01, 0x01, 0x01, 0x03, 0x01, 0x01, 0x03, 0x05, 0x02, 0x03, 0x03, 0x02, 0x04, 0x05, 0x08, 0x10,
    0x01, 0x01, 0x01, 0x03, 0x01, 0x01, 0x03, 0x05, 0x01, 0x01, 0x01, 0x03, 0x03, 0x03, 0x05, 0x09,
    0x02, 0x03, 0x03, 0x02, 0x03, 0x03, 0x02, 0x06, 0x04, 0x05, 0x05, 0x06, 0x08, 0x09, 0x10, 0x20,
    0x01, 0x01, 0x01, 0x03, 0x01, 0x01, 0x03, 0x05, 0x01, 0x01, 0x01, 0x03, 0x03, 0x03, 0x05, 0x09,
    0x01, 0x01, 0x01, 0x03, 0x01, 0x01, 0x03, 0x05, 0x03, 0x03, 0x03, 0x03, 0x05, 0x05, 0x09, 0x11,
    0x02, 0x03, 0x03, 0x02, 0x03, 0x03, 0x02, 0x06, 0x03, 0x03, 0x03, 0x03, 0x02, 0x03, 0x06, 0x0a,
    0x04, 0x05, 0x05, 0x06, 0x05, 0x05, 0x06, 0x04, 0x08, 0x09, 0x09, 0x0a, 0x10, 0x11, 0x20, 0x40,
    0x01, 0x01, 0x01, 0x03, 0x01, 0x01, 0x03, 0x05, 0x01, 0x01, 0x01, 0x03, 0x03, 0x03, 0x05, 0x09,
    0x01, 0x01, 0x01, 0x03, 0x01, 0x01, 0x03, 0x05, 0x03, 0x03, 0x03, 0x03, 0x05, 0x05, 0x09, 0x11,
    0x01, 0x01, 0x01, 0x03, 0x01, 0x01, 0x03, 0x05, 0x01, 0x01, 0x01, 0x03, 0x03, 0x03, 0x05, 0x09,
    0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x07, 0x05, 0x05, 0x05, 0x07, 0x09, 0x09, 0x11, 0x21,
    0x02, 0x03, 0x03, 0x02, 0x03, 0x03, 0x02, 0x06, 0x03, 0x03, 0x03, 0x03, 0x02, 0x03, 0x06, 0x0a,
    0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x07, 0x02, 0x03, 0x03, 0x02, 0x06, 0x07, 0x0a, 0x12,
    0x04, 0x05, 0x05, 0x06, 0x05, 0x05, 0x06, 0x04, 0x05, 0x05, 0x05, 0x07, 0x06, 0x07, 0x04, 0x0c,
    0x08, 0x09, 0x09, 0x0a, 0x09, 0x09, 0x0a, 0x0c, 0x10, 0x11, 0x11, 0x12, 0x20, 0x21, 0x40, 0x80,
];

pub(crate) fn set_bit(map: &mut [u8], bit: usize) -> Result<(), UfsError> {
    let byte = map
        .get_mut(bit / NBBY)
        .ok_or(UfsError::Corrupt("free map out of range"))?;
    *byte |= 1 << (bit % NBBY);
    Ok(())
}

pub(crate) fn clear_bit(map: &mut [u8], bit: usize) -> Result<(), UfsError> {
    let byte = map
        .get_mut(bit / NBBY)
        .ok_or(UfsError::Corrupt("free map out of range"))?;
    *byte &= !(1 << (bit % NBBY));
    Ok(())
}

pub(crate) fn bit_is_set(map: &[u8], bit: usize) -> bool {
    map.get(bit / NBBY)
        .is_some_and(|byte| byte & (1 << (bit % NBBY)) != 0)
}

/// `ffs_isblock`: all fragments of block `h` are free.
pub(crate) fn is_block(fragshift: u32, map: &[u8], h: usize) -> bool {
    match fragshift {
        3 => map.get(h) == Some(&0xff),
        2 => map
            .get(h >> 1)
            .is_some_and(|byte| byte & (0x0f << ((h & 1) << 2)) == 0x0f << ((h & 1) << 2)),
        1 => map
            .get(h >> 2)
            .is_some_and(|byte| byte & (0x03 << ((h & 3) << 1)) == 0x03 << ((h & 3) << 1)),
        0 => map
            .get(h >> 3)
            .is_some_and(|byte| byte & (0x01 << (h & 7)) == 0x01 << (h & 7)),
        _ => false,
    }
}

/// `ffs_isfreeblock`: all fragments of block `h` are allocated.
pub(crate) fn is_free_block(fragshift: u32, map: &[u8], h: usize) -> bool {
    match fragshift {
        3 => map.get(h) == Some(&0),
        2 => map
            .get(h >> 1)
            .is_some_and(|byte| byte & (0x0f << ((h & 1) << 2)) == 0),
        1 => map
            .get(h >> 2)
            .is_some_and(|byte| byte & (0x03 << ((h & 3) << 1)) == 0),
        0 => map
            .get(h >> 3)
            .is_some_and(|byte| byte & (0x01 << (h & 7)) == 0),
        _ => false,
    }
}

/// `ffs_clrblock`: take a block out of the free map.
pub(crate) fn clear_block(fragshift: u32, map: &mut [u8], h: usize) -> Result<(), UfsError> {
    match fragshift {
        3 => {
            let byte = map
                .get_mut(h)
                .ok_or(UfsError::Corrupt("free map out of range"))?;
            *byte = 0;
        }
        2 => {
            let byte = map
                .get_mut(h >> 1)
                .ok_or(UfsError::Corrupt("free map out of range"))?;
            *byte &= !(0x0f << ((h & 1) << 2));
        }
        1 => {
            let byte = map
                .get_mut(h >> 2)
                .ok_or(UfsError::Corrupt("free map out of range"))?;
            *byte &= !(0x03 << ((h & 3) << 1));
        }
        0 => {
            let byte = map
                .get_mut(h >> 3)
                .ok_or(UfsError::Corrupt("free map out of range"))?;
            *byte &= !(0x01 << (h & 7));
        }
        _ => return Err(UfsError::Corrupt("bad fragment shift")),
    }
    Ok(())
}

/// `ffs_setblock`: put a block back into the free map.
pub(crate) fn set_block(fragshift: u32, map: &mut [u8], h: usize) -> Result<(), UfsError> {
    match fragshift {
        3 => {
            let byte = map
                .get_mut(h)
                .ok_or(UfsError::Corrupt("free map out of range"))?;
            *byte = 0xff;
        }
        2 => {
            let byte = map
                .get_mut(h >> 1)
                .ok_or(UfsError::Corrupt("free map out of range"))?;
            *byte |= 0x0f << ((h & 1) << 2);
        }
        1 => {
            let byte = map
                .get_mut(h >> 2)
                .ok_or(UfsError::Corrupt("free map out of range"))?;
            *byte |= 0x03 << ((h & 3) << 1);
        }
        0 => {
            let byte = map
                .get_mut(h >> 3)
                .ok_or(UfsError::Corrupt("free map out of range"))?;
            *byte |= 0x01 << (h & 7);
        }
        _ => return Err(UfsError::Corrupt("bad fragment shift")),
    }
    Ok(())
}

/// `blkmap`: extract the fragment bits for the block at `loc`.
pub(crate) fn block_map(frag: u32, map: &[u8], loc: usize) -> Result<u8, UfsError> {
    if !matches!(frag, 1 | 2 | 4 | 8) {
        return Err(UfsError::Corrupt("bad fragments per block"));
    }
    let byte = *map
        .get(loc / NBBY)
        .ok_or(UfsError::Corrupt("free map out of range"))?;
    Ok((byte >> (loc % NBBY)) & (0xff >> (NBBY - frag as usize)))
}

/// `ffs_fragacct`: update the fragment-size histogram for one block map.
pub(crate) fn fragacct(
    frag: u32,
    fragmap: u8,
    fraglist: &mut [i32; 8],
    cnt: i32,
) -> Result<(), UfsError> {
    let table = match frag {
        1 | 2 | 4 => &FRAGTBL124,
        8 => &FRAGTBL8,
        _ => return Err(UfsError::Corrupt("unsupported fragment size mapping")),
    };
    let inblk = i32::from(table[fragmap as usize]) << 1;
    let map = i32::from(fragmap) << 1;
    for siz in 1..frag as usize {
        if inblk & (1 << (siz + (frag as usize & (NBBY - 1)))) == 0 {
            continue;
        }
        let mut field = AROUND[siz];
        let mut subfield = INSIDE[siz];
        let mut pos = siz;
        while pos <= frag as usize {
            if map & field == subfield {
                let slot = fraglist
                    .get_mut(siz)
                    .ok_or(UfsError::Corrupt("fragment histogram"))?;
                *slot = slot.wrapping_add(cnt);
                pos += siz;
                field <<= siz;
                subfield <<= siz;
            }
            field <<= 1;
            subfield <<= 1;
            pos += 1;
        }
    }
    Ok(())
}

/// Search the free map for a run of `allocsiz` free fragments.
///
/// This mirrors NetBSD `ffs_mapsearch`: start at the preference (or the group's fragment
/// rotor), scan to the end, then wrap; the returned fragment is relative to the group base.
pub(crate) fn mapsearch(
    frag: u32,
    fpg: u32,
    start_pref: u32,
    rotor: u32,
    map: &[u8],
    allocsiz: u32,
) -> Result<u32, UfsError> {
    if allocsiz == 0 || allocsiz > frag {
        return Err(UfsError::Corrupt("bad allocation size"));
    }
    let bytes = (fpg as usize).div_ceil(NBBY);
    let start_byte = if start_pref != 0 {
        (start_pref as usize % fpg.max(1) as usize) / NBBY
    } else {
        (rotor as usize) / NBBY
    };
    if start_byte >= bytes {
        return Err(UfsError::Corrupt("free map rotor"));
    }
    for pass in 0..2u8 {
        let (begin, end) = if pass == 0 {
            (start_byte, bytes)
        } else {
            (0, start_byte)
        };
        for byte_index in begin..end {
            let mut block = 0u32;
            while block < frag {
                let loc = byte_index * NBBY + block as usize;
                if loc + frag as usize > fpg as usize {
                    break;
                }
                let bits = block_map(frag, map, loc)?;
                let shifted = i32::from(bits) << 1;
                let mut field = AROUND[allocsiz as usize];
                let mut subfield = INSIDE[allocsiz as usize];
                for pos in 0..=(frag - allocsiz) as usize {
                    if shifted & field == subfield {
                        return Ok(loc as u32 + pos as u32);
                    }
                    field <<= 1;
                    subfield <<= 1;
                }
                block += frag;
            }
        }
        if pass == 0 && start_byte == 0 {
            break;
        }
    }
    Err(UfsError::NoSpace)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_and_clear_bits() {
        let mut map = [0u8; 4];
        set_bit(&mut map, 0).expect("set0");
        set_bit(&mut map, 9).expect("set9");
        assert!(bit_is_set(&map, 0));
        assert!(bit_is_set(&map, 9));
        clear_bit(&mut map, 0).expect("clear0");
        assert!(!bit_is_set(&map, 0));
        assert!(bit_is_set(&map, 9));
    }

    #[test]
    fn set_bit_out_of_range_is_error() {
        let mut map = [0u8; 1];
        assert!(set_bit(&mut map, 8).is_err());
        assert!(clear_bit(&mut map, 8).is_err());
    }

    #[test]
    fn block_operations_frag8() {
        let mut map = [0u8; 2];
        assert!(!is_block(3, &map, 0));
        set_block(3, &mut map, 0).expect("set");
        assert!(is_block(3, &map, 0));
        assert!(!is_free_block(3, &map, 0));
        clear_block(3, &mut map, 0).expect("clear");
        assert!(is_free_block(3, &map, 0));
        assert!(!is_block(3, &map, 0));
    }

    #[test]
    fn block_operations_frag4() {
        let mut map = [0u8; 2];
        set_block(2, &mut map, 0).expect("set0");
        assert!(is_block(2, &map, 0));
        assert!(!is_block(2, &map, 1));
        set_block(2, &mut map, 1).expect("set1");
        assert!(is_block(2, &map, 1));
        assert_eq!(map[0], 0xff);
        clear_block(2, &mut map, 0).expect("clear0");
        assert_eq!(map[0], 0xf0);
    }

    #[test]
    fn block_map_extracts_fragments() {
        let map = [0b1010_0101u8, 0xff];
        assert_eq!(block_map(8, &map, 0).expect("frag8"), 0b1010_0101);
        assert_eq!(block_map(4, &map, 0).expect("frag4 low"), 0b0101);
        assert_eq!(block_map(4, &map, 4).expect("frag4 high"), 0b1010);
        assert_eq!(block_map(1, &map, 1).expect("frag1"), 0);
    }

    #[test]
    fn invalid_fragment_size_is_rejected() {
        let map = [0u8; 1];
        assert!(block_map(3, &map, 0).is_err());
        let mut frsum = [0i32; 8];
        assert!(fragacct(3, 0, &mut frsum, 1).is_err());
    }

    #[test]
    fn fragacct_updates_runs() {
        let mut frsum = [0i32; 8];
        fragacct(8, 0b0000_0011, &mut frsum, 1).expect("two frags");
        assert!(frsum.iter().any(|value| *value > 0));
        let total: i32 = frsum.iter().sum();
        assert!(total > 0);
    }

    #[test]
    fn mapsearch_finds_first_block() {
        let map = [0xffu8; 8];
        assert_eq!(mapsearch(8, 64, 0, 0, &map, 8).expect("search"), 0);
        let mut partial = [0u8; 8];
        partial[0] = 0x0f;
        assert_eq!(mapsearch(8, 64, 0, 0, &partial, 4).expect("search4"), 0);
    }

    #[test]
    fn mapsearch_skips_used_block() {
        let mut map = [0xffu8; 8];
        map[0] = 0;
        assert_eq!(mapsearch(8, 64, 0, 0, &map, 8).expect("search"), 8);
    }

    #[test]
    fn mapsearch_wraps_to_zero() {
        let mut map = [0u8; 8];
        map[0] = 0xff;
        assert_eq!(mapsearch(8, 64, 8, 8, &map, 8).expect("wrap"), 0);
    }

    #[test]
    fn mapsearch_without_space_is_nospace() {
        let map = [0u8; 8];
        assert_eq!(mapsearch(8, 64, 0, 0, &map, 8), Err(UfsError::NoSpace));
    }
}

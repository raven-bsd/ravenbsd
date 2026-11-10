// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// Ported from NetBSD sys/ufs/ufs/ufs_bmap.c (revision
// 36eddebcc36cb0775aa2962005b9c05419a6ff01). The original notice follows.
//
// Copyright (c) 1989, 1991, 1993
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

//! Logical-to-physical block mapping with bounded indirect levels.

use crate::dinode::{Inode, UFS_NDADDR};
use crate::error::UfsError;

/// The indirect path for one logical block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BmapPlan {
    /// A block in `di_db`.
    Direct(usize),
    /// `level` 1, 2 or 3; `indices` holds the per-level pointer indices.
    Indirect { level: u8, indices: [u32; 3] },
}

/// Compute which indirect levels and indices address `lbn`.
pub(crate) fn plan(lbn: u64, nindir: u64) -> Result<BmapPlan, UfsError> {
    if lbn < UFS_NDADDR as u64 {
        return Ok(BmapPlan::Direct(lbn as usize));
    }
    let n = nindir;
    let mut bn = lbn - UFS_NDADDR as u64;
    if bn < n {
        return Ok(BmapPlan::Indirect {
            level: 1,
            indices: [bn as u32, 0, 0],
        });
    }
    bn -= n;
    let n2 = n.checked_mul(n).ok_or(UfsError::Invalid)?;
    if bn < n2 {
        return Ok(BmapPlan::Indirect {
            level: 2,
            indices: [(bn / n) as u32, (bn % n) as u32, 0],
        });
    }
    bn -= n2;
    let n3 = n2.checked_mul(n).ok_or(UfsError::Invalid)?;
    if bn < n3 {
        return Ok(BmapPlan::Indirect {
            level: 3,
            indices: [(bn / n2) as u32, ((bn / n) % n) as u32, (bn % n) as u32],
        });
    }
    Err(UfsError::Invalid)
}

/// Follow a plan through the inode pointers, using `read_ptr` for each indirect block.
///
/// `read_ptr(fragment, index)` returns the pointer stored at `index` in the indirect block at
/// `fragment`; the core validates the block range before calling it.
pub(crate) fn follow<F>(
    ip: &Inode,
    plan: BmapPlan,
    read_ptr: &mut F,
) -> Result<Option<u64>, UfsError>
where
    F: FnMut(u64, u32) -> Result<u64, UfsError>,
{
    match plan {
        BmapPlan::Direct(index) => {
            let value = ip
                .db
                .get(index)
                .copied()
                .ok_or(UfsError::Corrupt("direct block index"))?;
            Ok(nonzero(value))
        }
        BmapPlan::Indirect { level, indices } => {
            let root = *ip
                .ib
                .get((level - 1) as usize)
                .ok_or(UfsError::Corrupt("indirect level"))?;
            if root == 0 {
                return Ok(None);
            }
            let first = read_ptr(root, indices[0])?;
            if first == 0 {
                return Ok(None);
            }
            if level == 1 {
                return Ok(Some(first));
            }
            let second = read_ptr(first, indices[1])?;
            if second == 0 {
                return Ok(None);
            }
            if level == 2 {
                return Ok(Some(second));
            }
            let third = read_ptr(second, indices[2])?;
            Ok(nonzero(third))
        }
    }
}

fn nonzero(value: u64) -> Option<u64> {
    if value == 0 { None } else { Some(value) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Timespec;
    use alloc_crate::vec::Vec;
    use std::collections::BTreeMap;

    fn inode(db: [u64; 12], ib: [u64; 3]) -> Inode {
        Inode {
            raw: Vec::new(),
            mode: 0o100644,
            nlink: 1,
            uid: 0,
            gid: 0,
            size: 0,
            blocks: 0,
            atime: Timespec::default(),
            mtime: Timespec::default(),
            ctime: Timespec::default(),
            flags: 0,
            r#gen: 0,
            rdev: 0,
            db,
            ib,
        }
    }

    #[test]
    fn plan_direct_blocks() {
        assert_eq!(plan(0, 4).expect("direct"), BmapPlan::Direct(0));
        assert_eq!(plan(11, 4).expect("direct"), BmapPlan::Direct(11));
    }

    #[test]
    fn plan_single_indirect_bounds() {
        assert_eq!(
            plan(12, 4).expect("single"),
            BmapPlan::Indirect {
                level: 1,
                indices: [0, 0, 0]
            }
        );
        assert_eq!(
            plan(15, 4).expect("single"),
            BmapPlan::Indirect {
                level: 1,
                indices: [3, 0, 0]
            }
        );
    }

    #[test]
    fn plan_double_indirect_bounds() {
        assert_eq!(
            plan(16, 4).expect("double"),
            BmapPlan::Indirect {
                level: 2,
                indices: [0, 0, 0]
            }
        );
        assert_eq!(
            plan(31, 4).expect("double"),
            BmapPlan::Indirect {
                level: 2,
                indices: [3, 3, 0]
            }
        );
    }

    #[test]
    fn plan_triple_indirect_bounds() {
        assert_eq!(
            plan(32, 4).expect("triple"),
            BmapPlan::Indirect {
                level: 3,
                indices: [0, 0, 0]
            }
        );
        assert_eq!(
            plan(95, 4).expect("triple"),
            BmapPlan::Indirect {
                level: 3,
                indices: [3, 3, 3]
            }
        );
    }

    #[test]
    fn plan_beyond_triple_is_invalid() {
        assert_eq!(plan(96, 4), Err(UfsError::Invalid));
    }

    #[test]
    fn follow_direct_and_hole() {
        let mut db = [0u64; 12];
        db[0] = 42;
        let ip = inode(db, [0; 3]);
        let mut read = |_frag: u64, _idx: u32| Ok(0u64);
        assert_eq!(
            follow(&ip, BmapPlan::Direct(0), &mut read).expect("direct"),
            Some(42)
        );
        assert_eq!(
            follow(&ip, BmapPlan::Direct(1), &mut read).expect("hole"),
            None
        );
    }

    #[test]
    fn follow_single_indirect() {
        let mut ib = [0u64; 3];
        ib[0] = 100;
        let ip = inode([0; 12], ib);
        let mut pointers = BTreeMap::new();
        pointers.insert((100u64, 0u32), 200u64);
        let mut read = |frag: u64, idx: u32| Ok(*pointers.get(&(frag, idx)).unwrap_or(&0));
        assert_eq!(
            follow(
                &ip,
                BmapPlan::Indirect {
                    level: 1,
                    indices: [0, 0, 0]
                },
                &mut read
            )
            .expect("single"),
            Some(200)
        );
    }

    #[test]
    fn follow_double_indirect() {
        let mut ib = [0u64; 3];
        ib[1] = 100;
        let ip = inode([0; 12], ib);
        let mut pointers = BTreeMap::new();
        pointers.insert((100u64, 2u32), 200u64);
        pointers.insert((200u64, 1u32), 300u64);
        let mut read = |frag: u64, idx: u32| Ok(*pointers.get(&(frag, idx)).unwrap_or(&0));
        assert_eq!(
            follow(
                &ip,
                BmapPlan::Indirect {
                    level: 2,
                    indices: [2, 1, 0]
                },
                &mut read
            )
            .expect("double"),
            Some(300)
        );
    }

    #[test]
    fn follow_triple_indirect() {
        let mut ib = [0u64; 3];
        ib[2] = 100;
        let ip = inode([0; 12], ib);
        let mut pointers = BTreeMap::new();
        pointers.insert((100u64, 0u32), 200u64);
        pointers.insert((200u64, 0u32), 300u64);
        pointers.insert((300u64, 7u32), 400u64);
        let mut read = |frag: u64, idx: u32| Ok(*pointers.get(&(frag, idx)).unwrap_or(&0));
        assert_eq!(
            follow(
                &ip,
                BmapPlan::Indirect {
                    level: 3,
                    indices: [0, 0, 7]
                },
                &mut read
            )
            .expect("triple"),
            Some(400)
        );
    }

    #[test]
    fn follow_zero_root_is_hole() {
        let ip = inode([0; 12], [0; 3]);
        let mut read = |_frag: u64, _idx: u32| Ok(1u64);
        assert_eq!(
            follow(
                &ip,
                BmapPlan::Indirect {
                    level: 2,
                    indices: [0, 0, 0]
                },
                &mut read
            )
            .expect("hole"),
            None
        );
    }

    #[test]
    fn follow_zero_pointer_is_hole() {
        let mut ib = [0u64; 3];
        ib[0] = 100;
        let ip = inode([0; 12], ib);
        let mut read = |_frag: u64, _idx: u32| Ok(0u64);
        assert_eq!(
            follow(
                &ip,
                BmapPlan::Indirect {
                    level: 1,
                    indices: [0, 0, 0]
                },
                &mut read
            )
            .expect("hole"),
            None
        );
    }

    #[test]
    fn follow_propagates_pointer_errors() {
        let mut ib = [0u64; 3];
        ib[0] = 100;
        let ip = inode([0; 12], ib);
        let mut read = |_frag: u64, _idx: u32| Err(UfsError::Corrupt("test"));
        assert_eq!(
            follow(
                &ip,
                BmapPlan::Indirect {
                    level: 1,
                    indices: [0, 0, 0]
                },
                &mut read
            ),
            Err(UfsError::Corrupt("test"))
        );
    }
}

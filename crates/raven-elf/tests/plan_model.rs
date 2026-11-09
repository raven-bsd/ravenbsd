// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Differential test of `plan_load` against an independent page-by-page model.
//!
//! Written by the lead during the K07 review from CONTRACTS C9 alone: random valid ELF64 files (unaligned
//! `vaddr`, unaligned file tails, zero-`filesz` segments) are planned in both modes and compared with a
//! model that classifies every page of every segment as file, copy-tail or zero.

use raven_elf::{Elf, ElfError, LoadMode, Machine, StepKind, plan_load};

struct Rng(u64);
impl Rng {
    fn n(&mut self) -> u64 {
        let mut v = self.0;
        v ^= v << 13;
        v ^= v >> 7;
        v ^= v << 17;
        self.0 = v;
        v
    }
    fn r(&mut self, m: u64) -> u64 {
        self.n() % m
    }
}
const P: u64 = 4096;
struct S {
    vaddr: u64,
    filesz: u64,
    memsz: u64,
    off: u64,
    flags: u32,
}

fn build(segs: &[S], entry: u64, flen: usize, rng: &mut Rng) -> Vec<u8> {
    let phnum = segs.len() as u16;
    let mut b = vec![0u8; flen];
    for x in b.iter_mut().skip(64) {
        *x = rng.n() as u8;
    }
    b[0..4].copy_from_slice(b"\x7fELF");
    b[4] = 2;
    b[5] = 1;
    b[6] = 1;
    b[16..18].copy_from_slice(&2u16.to_le_bytes());
    b[18..20].copy_from_slice(&62u16.to_le_bytes());
    b[24..32].copy_from_slice(&entry.to_le_bytes());
    b[32..40].copy_from_slice(&64u64.to_le_bytes());
    b[52..54].copy_from_slice(&64u16.to_le_bytes());
    b[54..56].copy_from_slice(&56u16.to_le_bytes());
    b[56..58].copy_from_slice(&phnum.to_le_bytes());
    for (i, s) in segs.iter().enumerate() {
        let o = 64 + i * 56;
        b[o..o + 4].copy_from_slice(&1u32.to_le_bytes());
        b[o + 4..o + 8].copy_from_slice(&s.flags.to_le_bytes());
        b[o + 8..o + 16].copy_from_slice(&s.off.to_le_bytes());
        b[o + 16..o + 24].copy_from_slice(&s.vaddr.to_le_bytes());
        b[o + 24..o + 32].copy_from_slice(&s.vaddr.to_le_bytes());
        b[o + 32..o + 40].copy_from_slice(&s.filesz.to_le_bytes());
        b[o + 40..o + 48].copy_from_slice(&s.memsz.to_le_bytes());
        b[o + 48..o + 56].copy_from_slice(&4096u64.to_le_bytes());
    }
    b
}

#[test]
fn differential() {
    let mut rng = Rng(0x1234_5678_9abc_def1);
    let mut ok_cases = 0;
    let mut strict_err = 0;
    let mut strict_ok = 0;
    for _ in 0..30_000 {
        let n = 1 + rng.r(4) as usize;
        let mut segs = Vec::new();
        let mut page_cursor = 0x400u64 + rng.r(4); // in pages
        let mut have_x = None;
        for _ in 0..n {
            let po = rng.r(P); // page offset of vaddr
            let vaddr = page_cursor * P + po;
            let filesz = if rng.r(5) == 0 { 0 } else { rng.r(3 * P + 1) };
            let memsz = filesz + if rng.r(3) == 0 { 0 } else { rng.r(3 * P + 1) };
            if rng.r(4) == 0 && filesz > 0 {
                /* make file end page aligned sometimes */
                let fe = vaddr + filesz;
                let fe2 = (fe).div_ceil(P) * P;
                let f2 = fe2 - vaddr;
                if f2 >= filesz {
                    /* extend */
                    let memsz2 = memsz.max(f2);
                    segs.push(S {
                        vaddr,
                        filesz: f2,
                        memsz: memsz2,
                        off: 0,
                        flags: 0,
                    });
                    let l = segs.len() - 1;
                    let _ = l;
                    page_cursor += (po + memsz2).div_ceil(P) + rng.r(3);
                    let fl = [4u32, 5, 6][rng.r(3) as usize];
                    segs.last_mut().unwrap().flags = fl;
                    if fl == 5 && memsz2 > 0 && have_x.is_none() {
                        have_x = Some(vaddr);
                    }
                    continue;
                }
            }
            let flags = [4u32, 5, 6][rng.r(3) as usize];
            if flags == 5 && memsz > 0 && have_x.is_none() {
                have_x = Some(vaddr);
            }
            segs.push(S {
                vaddr,
                filesz,
                memsz,
                off: 0,
                flags,
            });
            page_cursor += (po + memsz).div_ceil(P) + rng.r(3);
        }
        let Some(entry) = have_x else { continue };
        // file offsets: congruent to vaddr mod 4096, sequential
        let mut fo = 4096u64;
        for s in segs.iter_mut() {
            let po = s.vaddr % P;
            s.off = fo + po;
            fo = (s.off + s.filesz).div_ceil(P) * P + P * rng.r(2);
        }
        let flen = (fo + P) as usize;
        let bytes = build(&segs, entry, flen, &mut rng);
        let elf = Elf::parse(&bytes, Machine::X86_64, 0..0x0000_7fff_ffff_ffff)
            .expect("valid elf must parse");
        // model
        let mut expect: Vec<(u64, u64, u8, String)> = Vec::new();
        let mut tail_unaligned = false;
        for s in &segs {
            if s.memsz == 0 {
                continue;
            }
            let start = s.vaddr / P * P;
            let end = (s.vaddr + s.memsz).div_ceil(P) * P;
            let fe = s.vaddr + s.filesz;
            let prot = ((s.flags & 4 != 0) as u8)
                | (((s.flags & 2 != 0) as u8) << 1)
                | (((s.flags & 1 != 0) as u8) << 2);
            if s.filesz > 0 && fe % P != 0 {
                tail_unaligned = true;
            }
            let mut pg = start;
            while pg < end {
                let off = s.off - (s.vaddr - start) + (pg - start);
                let k = if s.filesz == 0 {
                    "Z".to_string()
                } else if pg + P <= fe {
                    format!("F{off}")
                } else if pg < fe {
                    format!("C{off}:{}", fe - pg)
                } else {
                    "Z".to_string()
                };
                expect.push((pg, P, prot, k));
                pg += P;
            }
        }
        let got: Vec<(u64, u64, u8, String)> = plan_load(&elf, P, LoadMode::Copy)
            .expect("copy plan")
            .map(|m| {
                (
                    m.vaddr,
                    m.len,
                    m.prot,
                    match m.kind {
                        StepKind::File { offset } => format!("F{offset}"),
                        StepKind::Zero => "Z".into(),
                        StepKind::CopyTail { offset, len } => format!("C{offset}:{len}"),
                    },
                )
            })
            .collect();
        assert_eq!(
            got,
            expect,
            "segs mismatch for {:?}",
            segs.iter()
                .map(|s| (s.vaddr, s.filesz, s.memsz, s.off, s.flags))
                .collect::<Vec<_>>()
        );
        match plan_load(&elf, P, LoadMode::Strict) {
            Ok(it) => {
                assert!(!tail_unaligned, "strict accepted unaligned tail");
                let g: Vec<_> = it.collect();
                assert_eq!(g.len(), expect.len());
                strict_ok += 1;
            }
            Err(e) => {
                assert!(tail_unaligned, "strict rejected aligned: {e:?}");
                assert_eq!(e, ElfError::FileTailNotPageAligned);
                strict_err += 1;
            }
        }
        // segment_data length == filesz
        for sg in elf.segments() {
            assert_eq!(elf.segment_data(&sg).len() as u64, sg.filesz);
        }
        ok_cases += 1;
    }
    println!("cases={ok_cases} strict_ok={strict_ok} strict_err={strict_err}");
    assert!(ok_cases > 10_000 && strict_ok > 500 && strict_err > 500);
}

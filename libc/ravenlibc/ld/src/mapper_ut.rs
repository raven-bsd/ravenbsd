// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use crate::mapper::{FileId, LoaderFile, MapErrKind, Prot};
use raven_libc_elf::{MapStep, SegmentPlan, StepKind};

struct Fake {
    base: u64,
    calls: Vec<u64>,
    sources: Vec<MapSource>,
    fail_first: usize,
}

impl Mapper for Fake {
    fn open(&mut self, _path: &[u8]) -> Result<LoaderFile, MapErrKind> {
        Err(MapErrKind::NoEnt)
    }
    fn read(
        &mut self,
        _file: &LoaderFile,
        _offset: u64,
        _buf: &mut [u8],
    ) -> Result<usize, MapErrKind> {
        Err(MapErrKind::Io)
    }
    fn close(&mut self, _file: LoaderFile) {}
    fn reserve(&mut self, _len: u64) -> Result<u64, MapErrKind> {
        Ok(self.base)
    }
    fn release(&mut self, _base: u64, _len: u64) -> Result<(), MapErrKind> {
        Ok(())
    }
    fn map_fixed(
        &mut self,
        addr: u64,
        _len: u64,
        prot: Prot,
        src: MapSource,
    ) -> Result<(), MapErrKind> {
        if self.fail_first > 0 {
            self.fail_first = self.fail_first.saturating_sub(1);
            return Err(MapErrKind::Exists);
        }
        assert!(!prot.is_wx());
        self.calls.push(addr);
        self.sources.push(src);
        Ok(())
    }
    fn protect(&mut self, _addr: u64, _len: u64, _prot: Prot) -> Result<(), MapErrKind> {
        Ok(())
    }
}

fn project() -> SegmentPlan {
    SegmentPlan {
        reserve_len: 0x2000,
        base_vaddr: 0x1000,
        steps: vec![
            MapStep {
                vaddr: 0x1000,
                len: 0x1000,
                file_off: 0,
                prot: 5,
                kind: StepKind::File,
            },
            MapStep {
                vaddr: 0x2000,
                len: 0x1000,
                file_off: 0,
                prot: 6,
                kind: StepKind::Zero,
            },
        ],
        relro: None,
        tls: None,
        eh_frame: None,
    }
}

#[test]
fn the_mapping_sequence_maps_each_step() {
    let mut fake = Fake {
        base: 0x7f00_0000,
        calls: Vec::new(),
        sources: Vec::new(),
        fail_first: 0,
    };
    let file = LoaderFile {
        handle: 1,
        id: FileId::default(),
    };
    let mapped = map_object(&project(), &file, &mut fake).expect("map");
    assert_eq!(mapped.bias, 0x7f00_0000 - 0x1000);
    assert_eq!(fake.calls.len(), 2);
    assert_eq!(fake.calls[0], 0x7f00_0000);
}

#[test]
fn a_fixed_collision_is_retried() {
    let mut fake = Fake {
        base: 0x7f00_0000,
        calls: Vec::new(),
        sources: Vec::new(),
        fail_first: 3,
    };
    let file = LoaderFile {
        handle: 1,
        id: FileId::default(),
    };
    assert!(map_object(&project(), &file, &mut fake).is_ok());
}

#[test]
fn more_than_eight_collisions_fail() {
    let mut fake = Fake {
        base: 0x7f00_0000,
        calls: Vec::new(),
        sources: Vec::new(),
        fail_first: 100,
    };
    let file = LoaderFile {
        handle: 1,
        id: FileId::default(),
    };
    assert_eq!(
        map_object(&project(), &file, &mut fake),
        Err(MapErrKind::Exists)
    );
}

#[test]
fn a_write_execute_protection_is_refused() {
    let mut plan = project();
    if let Some(step) = plan.steps.first_mut() {
        step.prot = 7;
    }
    let mut fake = Fake {
        base: 0x7f00_0000,
        calls: Vec::new(),
        sources: Vec::new(),
        fail_first: 0,
    };
    let file = LoaderFile {
        handle: 1,
        id: FileId::default(),
    };
    assert_eq!(map_object(&plan, &file, &mut fake), Err(MapErrKind::Inval));
}

#[test]
fn the_protection_union_is_exact() {
    assert_eq!(Prot::READ.union(Prot::WRITE), Prot::RW);
    assert!(!Prot::RW.is_wx());
    assert!(Prot::RW.union(Prot::EXEC).is_wx());
}

#[test]
fn the_page_tail_is_zero_and_not_remapped() {
    let plan = SegmentPlan {
        reserve_len: 0x4000,
        base_vaddr: 0x1000,
        steps: vec![
            MapStep {
                vaddr: 0x1000,
                len: 0x2000,
                file_off: 0,
                prot: 5,
                kind: StepKind::File,
            },
            MapStep {
                vaddr: 0x2800,
                len: 0x800,
                file_off: 0,
                prot: 5,
                kind: StepKind::Tail,
            },
            MapStep {
                vaddr: 0x3000,
                len: 0x1000,
                file_off: 0,
                prot: 6,
                kind: StepKind::Zero,
            },
        ],
        relro: None,
        tls: None,
        eh_frame: None,
    };
    let mut fake = Fake {
        base: 0x7f00_0000,
        calls: Vec::new(),
        sources: Vec::new(),
        fail_first: 0,
    };
    let file = LoaderFile {
        handle: 1,
        id: FileId::default(),
    };
    let _ = map_object(&plan, &file, &mut fake).expect("map");
    assert_eq!(fake.calls.len(), 2, "the Tail step must not be mapped");
    match fake.sources.first() {
        Some(MapSource::CopyOfFile { len, .. }) => assert_eq!(*len, 0x1800),
        other => panic!("unexpected source: {other:?}"),
    }
}

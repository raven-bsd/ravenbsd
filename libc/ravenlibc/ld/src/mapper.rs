// SPDX-License-Identifier: BSD-2-Clause
//
// Copyright 1996, 1997, 1998, 1999, 2000 John D. Polstra.
// Copyright 2003 Alexander Kabaev <kan@FreeBSD.ORG>.
// Copyright 2009-2013 Konstantin Belousov <kib@FreeBSD.ORG>.
// Copyright 2012 John Marino <draco@marino.st>.
// Copyright 2014-2017 The FreeBSD Foundation
// All rights reserved.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from freebsd:libexec/rtld-elf/map_object.c @
// 133344405f532bd54ff3629a77d4c4e062f01fdb
// (file revision tag: 133344405f532bd54ff3629a77d4c4e062f01fdb)
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions
// are met:
// 1. Redistributions of source code must retain the above copyright
//    notice, this list of conditions and the following disclaimer.
// 2. Redistributions in binary form must reproduce the above copyright
//    notice, this list of conditions and the following disclaimer in the
//    documentation and/or other materials provided with the distribution.
//
// THIS SOFTWARE IS PROVIDED BY THE AUTHOR AND CONTRIBUTORS ``AS IS'' AND
// ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED.  IN NO EVENT SHALL THE AUTHOR OR CONTRIBUTORS BE LIABLE
// FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
// DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
// OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
// HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
// LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY
// OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
// SUCH DAMAGE.
//
//! The `Mapper`/`Platform` seam and the Raven mapping sequence (G-5 option A).

use raven_libc_elf::SegmentPlan;

/// A file identity for deduplication.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FileId {
    /// Device number.
    pub dev: u64,
    /// Inode number.
    pub ino: u64,
    /// File size.
    pub size: u64,
}

/// An opened file. Raven uses a handle, not a POSIX fd.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoaderFile {
    /// The platform handle.
    pub handle: u32,
    /// The file identity.
    pub id: FileId,
}

/// Why a mapper operation failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MapErrKind {
    /// `ENOENT`.
    NoEnt,
    /// `EACCES`.
    Access,
    /// `EEXIST` (the fixed address is occupied).
    Exists,
    /// `ENOMEM`.
    NoMem,
    /// `EINVAL`.
    Inval,
    /// Any other I/O error.
    Io,
}

impl MapErrKind {
    /// The C `errno` value for this failure.
    #[must_use]
    pub const fn errno(self) -> i32 {
        match self {
            Self::NoEnt => 2,
            Self::Access => 13,
            Self::Exists => 17,
            Self::NoMem => 12,
            Self::Inval => 22,
            Self::Io => 5,
        }
    }
}

/// Protection bits: 1 = read, 2 = write, 4 = execute.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Prot(pub u8);

impl Prot {
    /// Read.
    pub const READ: Self = Self(1);
    /// Write.
    pub const WRITE: Self = Self(2);
    /// Execute.
    pub const EXEC: Self = Self(4);
    /// Read and write.
    pub const RW: Self = Self(3);
    /// Read and execute.
    pub const RX: Self = Self(5);

    /// Whether both write and execute are set.
    #[must_use]
    pub const fn is_wx(self) -> bool {
        self.0 & 3 == 3 && self.0 & 4 != 0 || self.0 & 6 == 6
    }

    /// The union of two protections.
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

/// Where the bytes of one mapping come from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MapSource {
    /// Anonymous zero pages.
    Anonymous,
    /// A private file mapping at the given file offset.
    File {
        /// The file handle.
        file: u32,
        /// The file offset.
        offset: u64,
    },
    /// Copy the file bytes into anonymous zero pages (the copy-load fallback); `len` is the
    /// file-backed byte count, which may be shorter than the mapping length because the last
    /// page tail is BSS.
    CopyOfFile {
        /// The file handle.
        file: u32,
        /// The file offset.
        offset: u64,
        /// The number of file bytes to copy.
        len: u64,
    },
}

/// File, memory and process operations the loader needs.
pub trait Mapper {
    /// Open a file.
    fn open(&mut self, path: &[u8]) -> Result<LoaderFile, MapErrKind>;
    /// Read at an offset.
    fn read(&mut self, file: &LoaderFile, offset: u64, buf: &mut [u8])
    -> Result<usize, MapErrKind>;
    /// Close a file.
    fn close(&mut self, file: LoaderFile);
    /// Reserve an address range without mapping it.
    fn reserve(&mut self, len: u64) -> Result<u64, MapErrKind>;
    /// Release a reservation.
    fn release(&mut self, base: u64, len: u64) -> Result<(), MapErrKind>;
    /// Map at a fixed address; fails on overlap.
    fn map_fixed(
        &mut self,
        addr: u64,
        len: u64,
        prot: Prot,
        src: MapSource,
    ) -> Result<(), MapErrKind>;
    /// Change the protection of a mapped range.
    fn protect(&mut self, addr: u64, len: u64, prot: Prot) -> Result<(), MapErrKind>;
}

/// The process operations on top of [`Mapper`].
pub trait Platform: Mapper {
    /// Write bytes to the debug console.
    fn debug_write(&mut self, bytes: &[u8]);
    /// Terminate the process.
    fn exit(&mut self, code: i32) -> !;
    /// A monotonic hint; the loader does not use the clock.
    fn now_hint(&self) -> u64 {
        0
    }
}

/// One mapped object.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mapped {
    /// The address returned by `reserve` (the lowest mapped page).
    pub base: u64,
    /// The span in bytes.
    pub span: u64,
    /// The load bias (`base - plan.base_vaddr`).
    pub bias: u64,
}

/// The maximum number of mapping retries on `Exists`/`NoMem`.
pub const MAP_RETRIES: usize = 8;

/// Map one object from `plan`; the file is already open.
///
/// The sequence is reserve, release, then one `map_fixed` per plan step. On a fixed-address
/// collision the mapped steps are released and the sequence is retried at most [`MAP_RETRIES`]
/// times.
///
/// # Errors
///
/// The last [`MapErrKind`] after the retry bound.
pub fn map_object<M: Mapper>(
    plan: &SegmentPlan,
    file: &LoaderFile,
    mapper: &mut M,
) -> Result<Mapped, MapErrKind> {
    let mut last = MapErrKind::NoMem;
    let mut attempt = 0usize;
    while attempt <= MAP_RETRIES {
        let base = match mapper.reserve(plan.reserve_len) {
            Ok(base) => base,
            Err(error) => {
                last = error;
                attempt = attempt.saturating_add(1);
                continue;
            }
        };
        let _ = mapper.release(base, plan.reserve_len);
        let mut mapped: [(u64, u64); 64] = [(0, 0); 64];
        let mut mapped_count = 0usize;
        let mut failed = None;
        for (index, step) in plan.steps.iter().enumerate() {
            // `Tail` describes the zero part of the page the preceding `File` step already
            // mapped; mapping it again would overwrite the page start.
            if step.kind == raven_libc_elf::StepKind::Tail {
                continue;
            }
            // `step.vaddr` may carry the segment's low bits; the mapping address is page-based.
            let relative = (step.vaddr & !0xfff).saturating_sub(plan.base_vaddr & !0xfff);
            let address = base.saturating_add(relative);
            let src = match step.kind {
                raven_libc_elf::StepKind::Zero => MapSource::Anonymous,
                raven_libc_elf::StepKind::File => {
                    let copy_len = plan
                        .steps
                        .get(index.saturating_add(1))
                        .filter(|next| {
                            next.kind == raven_libc_elf::StepKind::Tail
                                && next.vaddr >= step.vaddr
                                && next.vaddr < step.vaddr.saturating_add(step.len)
                        })
                        .map_or(step.len, |next| next.vaddr.saturating_sub(step.vaddr));
                    MapSource::CopyOfFile {
                        file: file.handle,
                        offset: step.file_off,
                        len: copy_len,
                    }
                }
                raven_libc_elf::StepKind::Tail => continue,
            };
            // `MapStep::prot` is the ELF `p_flags` bit layout (X=1, W=2, R=4); `Prot` uses
            // R=1, W=2, X=4.
            let mut bits = 0u8;
            if step.prot & 4 != 0 {
                bits |= 1;
            }
            if step.prot & 2 != 0 {
                bits |= 2;
            }
            if step.prot & 1 != 0 {
                bits |= 4;
            }
            let prot = Prot(bits);
            if prot.is_wx() {
                failed = Some(MapErrKind::Inval);
                break;
            }
            if let Err(error) = mapper.map_fixed(address, step.len, prot, src) {
                failed = Some(error);
                break;
            }
            if let Some(slot) = mapped.get_mut(mapped_count) {
                *slot = (address, step.len);
                mapped_count = mapped_count.saturating_add(1);
            }
        }
        if let Some(error) = failed {
            for (address, len) in mapped.iter().take(mapped_count) {
                let _ = mapper.release(*address, *len);
            }
            last = error;
            attempt = attempt.saturating_add(1);
            continue;
        }
        return Ok(Mapped {
            base,
            span: plan.reserve_len,
            bias: base.saturating_sub(plan.base_vaddr),
        });
    }
    Err(last)
}

#[cfg(test)]
#[path = "mapper_ut.rs"]
mod tests;

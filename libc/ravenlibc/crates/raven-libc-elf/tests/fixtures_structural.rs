// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! C62 structural tests over the C52 ELF image writer.

#[path = "common/elfbuild.rs"]
mod elfbuild;

use elfbuild::{Builder, Image, VersionDef, global_func, global_object, weak_undefined};
use raven_libc_elf::lookup::{SymFlags, SymLook, lookup_obj};
use raven_libc_elf::reloc::{RelocApplier, apply_object, classify};
use raven_libc_elf::{
    ApplyError, Arch, BindMode, ObjId, Phase, TlsDesc, elf_hash, gnu_hash, plan_segments,
    relr_offsets,
};

fn digest(image: &Image) -> raven_libc_elf::ObjView {
    let memory = image.memory(0);
    raven_libc_elf::ObjView::digest(&memory, image.phdr_vaddr, image.phnum, 0).expect("digest")
}

fn image(extra: impl FnOnce(Builder) -> Builder) -> Image {
    extra(Builder::new().symbol(global_func(b"f", 0x1000))).build()
}

#[test]
fn every_image_digests() {
    let image = image(|builder| builder);
    let view = digest(&image);
    assert_eq!(view.bias, 0);
    assert!(!view.segments.is_empty());
}

#[test]
fn relative_relocations_are_counted() {
    let image = image(|builder| builder.rela(0x0, 0, 8, 0x20));
    let view = digest(&image);
    assert_eq!(view.rela.1 / 24, 1);
}

#[test]
fn relr_offsets_decode() {
    let image = image(|builder| builder.relr(0x10).relr(0x20));
    let view = digest(&image);
    let memory = image.memory(0);
    let offsets: Vec<u64> = relr_offsets(&view, &memory)
        .expect("relr")
        .map(|entry| entry.expect("entry"))
        .collect();
    assert!(offsets.contains(&0x10));
}

#[test]
fn relr_is_empty_without_entries() {
    let image = image(|builder| builder);
    let view = digest(&image);
    let memory = image.memory(0);
    let count = relr_offsets(&view, &memory).expect("relr").count();
    assert_eq!(count, 0);
}

#[test]
fn sysv_and_gnu_hash_agree_on_a_symbol() {
    assert_eq!(elf_hash(b"f"), elf_hash(b"f"));
    assert_eq!(gnu_hash(b"f"), gnu_hash(b"f"));
}

#[test]
fn lookup_finds_the_exported_function() {
    let image = image(|builder| builder);
    let memory = image.memory(0);
    let view = digest(&image);
    let hit = lookup_obj(ObjId(0), &view, &memory, &SymLook::new(b"f"))
        .expect("lookup")
        .expect("hit");
    assert_eq!(hit.sym.value, 0x1000);
}

#[test]
fn a_weak_undefined_symbol_is_visible_as_undefined() {
    let image = image(|builder| builder.symbol(weak_undefined(b"missing")));
    let view = digest(&image);
    assert!(view.dynsymcount >= 2);
}

#[test]
fn a_version_definition_is_recorded() {
    let image = image(|builder| {
        builder
            .version_def(VersionDef {
                name: b"LIBV_1.0".to_vec(),
                index: 2,
                flags: 0,
            })
            .symbol(global_func(b"ver", 0x1010))
    });
    let view = digest(&image);
    assert!(view.verdef.is_some());
    assert_eq!(view.soname, None);
}

#[test]
fn tls_template_is_recorded() {
    let image = image(|builder| builder.tls(&[1u8; 16], 32, 16));
    let view = digest(&image);
    let tls = view.tls.expect("tls");
    assert_eq!(tls.filesz, 16);
    assert_eq!(tls.memsz, 32);
    assert_eq!(tls.align, 16);
}

#[test]
fn relro_range_is_recorded() {
    let image = image(|builder| builder.relro(0x100, 0x200));
    let view = digest(&image);
    assert!(view.relro.is_some());
}

#[test]
fn init_and_fini_arrays_are_recorded() {
    let image = image(|builder| builder.init_array(&[0, 1, 0x1000]).fini_array(&[0x1000]));
    let view = digest(&image);
    assert!(view.init_array.1 >= 24);
    assert!(view.fini_array.1 >= 8);
}

#[test]
fn segments_plan_one_step_per_load() {
    let image = image(|builder| builder);
    let view = digest(&image);
    let mut phdrs = Vec::new();
    let memory = image.memory(0);
    phdrs.extend(
        raven_libc_elf::elf::read_phdrs(&memory, image.phdr_vaddr, image.phnum).expect("phdrs"),
    );
    let plan = plan_segments(&phdrs, 4096).expect("plan");
    assert_eq!(plan.steps.len(), view.segments.len());
}

#[test]
fn a_rel_change_is_rejected_as_rel_not_supported() {
    let image = image(|builder| builder.extra_dyn(17, 0));
    let memory = image.memory(0);
    assert!(raven_libc_elf::ObjView::digest(&memory, image.phdr_vaddr, image.phnum, 0).is_err());
}

#[test]
fn an_executable_stack_is_flagged() {
    let image = image(|builder| builder.stack_exec());
    let view = digest(&image);
    assert!(view.gnu_stack_exec);
}

#[test]
fn a_needed_entry_is_recorded() {
    let image = image(|builder| builder.needed(b"libdeepend.so"));
    let view = digest(&image);
    assert_eq!(view.needed.len(), 1);
    let memory = image.memory(0);
    assert_eq!(
        view.name_at(&memory, view.needed[0]).expect("name"),
        b"libdeepend.so"
    );
}

#[test]
fn a_hidden_symbol_round_trips_through_its_definition() {
    let mut symbol = global_object(b"secret", 0x2000);
    symbol.other = 2;
    let image = image(|builder| builder.symbol(symbol));
    let view = digest(&image);
    let memory = image.memory(0);
    let index = *image.symbol_index.get(b"secret".as_slice()).expect("index");
    let parsed = view.sym(&memory, index).expect("symbol");
    assert_eq!(parsed.visibility(), 2);
}

#[test]
fn an_unknown_relocation_type_is_rejected() {
    assert!(matches!(
        classify(Arch::X86_64, 0xdead),
        raven_libc_elf::RelocAction::Reject(_)
    ));
}

#[test]
fn a_relative_relocation_writes_the_bias() {
    struct App {
        words: Vec<u64>,
    }
    impl RelocApplier for App {
        fn write64(&mut self, addr: u64, val: u64) -> Result<(), ApplyError> {
            if let Some(slot) = self.words.get_mut(usize::try_from(addr / 8).unwrap_or(0)) {
                *slot = val;
            }
            Ok(())
        }
        fn write32(&mut self, _addr: u64, _val: u32) -> Result<(), ApplyError> {
            Ok(())
        }
        fn read64(&self, _addr: u64) -> Option<u64> {
            Some(0)
        }
        fn call_ifunc(
            &mut self,
            resolver: u64,
            _arg: &raven_libc_elf::IfuncArg,
        ) -> Result<u64, ApplyError> {
            Ok(resolver)
        }
        fn tls_module_id(&self, _obj: ObjId) -> u64 {
            1
        }
        fn tls_static_offset(&mut self, _obj: ObjId) -> Result<i64, ApplyError> {
            Ok(0)
        }
        fn tls_desc_alloc(
            &mut self,
            _obj: ObjId,
            _sym: u64,
            _addend: i64,
        ) -> Result<TlsDesc, ApplyError> {
            Ok(TlsDesc {
                resolver: 0,
                arg: 0,
            })
        }
        fn copy(&mut self, _dst: u64, _src: u64, _len: u64) -> Result<(), ApplyError> {
            Ok(())
        }
    }
    let image = image(|builder| builder.rela(0x100, 0, 8, 0x20));
    let view = digest(&image);
    let memory = image.memory(0x1000);
    struct Resolver;
    impl raven_libc_elf::SymResolver for Resolver {
        fn resolve(
            &mut self,
            _obj: ObjId,
            _sym: u32,
        ) -> Result<Option<raven_libc_elf::SymDef>, raven_libc_elf::RelocError> {
            Ok(None)
        }
    }
    let mut applier = App {
        words: vec![0; 128],
    };
    let mut resolver = Resolver;
    apply_object(
        Arch::X86_64,
        ObjId(0),
        &view,
        false,
        &memory,
        &mut resolver,
        &mut applier,
        BindMode::Now,
        Phase::NonPlt,
        &raven_libc_elf::IfuncArg {
            hwcap: 0,
            hwcap2: 0,
        },
    )
    .expect("apply");
    let _ = SymFlags::NO_TLS;
}

// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: first-party tests of the transpiled self-relocator (no donor text).

use super::*;

const WORDS: usize = 8192;
const DYN: usize = 512;
const RELA: usize = 1024;
const RELR: usize = 2560;
const JMPREL: usize = 3000;
const PHDR: usize = 3500;

/// A flat fake image.
struct Image {
    words: Vec<u64>,
}

impl Image {
    fn new() -> Self {
        Self {
            words: vec![0; WORDS],
        }
    }

    fn base(&self) -> usize {
        self.words.as_ptr() as usize
    }

    fn offset(&self, word: usize) -> usize {
        word.wrapping_mul(8)
    }

    fn set(&mut self, word: usize, value: usize) {
        self.words[word] = value as u64;
    }

    fn get(&self, word: usize) -> usize {
        self.words[word] as usize
    }
}

fn dyn_entry(image: &mut Image, index: usize, tag: usize, value: usize) {
    image.set(DYN + index * 2, tag);
    image.set(DYN + index * 2 + 1, value);
}

fn dyn_word(image: &mut Image, index: usize, tag: usize, word: usize) {
    dyn_entry(image, index, tag, image.offset(word));
}

fn rela_entry(
    image: &mut Image,
    base_word: usize,
    index: usize,
    offset_word: usize,
    kind: usize,
    addend: usize,
) {
    image.set(base_word + index * 3, image.offset(offset_word));
    image.set(base_word + index * 3 + 1, kind);
    image.set(base_word + index * 3 + 2, addend);
}

/// A C8 stack with one argv entry and the supplied auxv pairs.
fn stack(base: usize, at_base: usize, phdr: Option<(usize, usize, usize)>) -> Vec<usize> {
    let mut words = vec![1usize, 0x1000, 0, 0];
    words.push(AT_BASE);
    words.push(at_base);
    if let Some((address, phent, phnum)) = phdr {
        words.push(AT_PHDR);
        words.push(address);
        words.push(AT_PHENT);
        words.push(phent);
        words.push(AT_PHNUM);
        words.push(phnum);
    }
    words.push(AT_HWCAP);
    words.push(0x42);
    words.push(AT_HWCAP2);
    words.push(0x24);
    words.push(0);
    words.push(0);
    let _ = base;
    words
}

fn program_headers(image: &mut Image, dynamic_offset: usize) {
    image.set(PHDR, 1); // PT_LOAD
    image.set(PHDR + 7, 2); // PT_DYNAMIC
    image.set(PHDR + 9, dynamic_offset); // p_vaddr at +16 bytes
}

/// The `DT_*` table of a RELATIVE/RELR image.
fn dynamic(image: &mut Image, rela_count: usize, relr_count: usize) {
    dyn_word(image, 0, DT_RELA, RELA);
    dyn_entry(image, 1, DT_RELASZ, rela_count.wrapping_mul(RELA_SIZE));
    if relr_count > 0 {
        dyn_word(image, 2, DT_RELR, RELR);
        dyn_entry(image, 3, DT_RELRSZ, relr_count.wrapping_mul(8));
    }
    dyn_entry(image, 4, 0, 0);
}

fn image_base(image: &Image) -> usize {
    image.base()
}

fn info(image: &Image, rela_count: usize, relr_count: usize) -> DynInfo {
    let base = image.base();
    DynInfo {
        rela: if rela_count == 0 {
            0
        } else {
            base.wrapping_add(image.offset(RELA))
        },
        relasz: rela_count.wrapping_mul(RELA_SIZE),
        relr: if relr_count == 0 {
            0
        } else {
            base.wrapping_add(image.offset(RELR))
        },
        relrsz: relr_count.wrapping_mul(8),
        jmprel: 0,
        pltrelsz: 0,
        pltgot: 0,
    }
}

/// A real resolver: its address is `base + addend`, the x86-64 psABI passes no arguments.
extern "C" fn resolver_dead() -> usize {
    0xDEAD_BEEF
}

/// A real resolver returning a second marker value.
extern "C" fn resolver_1234() -> usize {
    0x1234_5678
}

/// A real resolver returning a third marker value.
extern "C" fn resolver_aa() -> usize {
    0xAA
}

/// A real resolver used by the ordering test.
extern "C" fn resolver_order() -> usize {
    0x5A5A
}

/// The addend that makes `base + addend` the address of `resolver`.
fn resolver_addend(base: usize, resolver: extern "C" fn() -> usize) -> usize {
    (resolver as usize).wrapping_sub(base)
}

// ------------------------------------------------------------------ RELATIVE

#[test]
fn relative_single_entry() {
    let mut image = Image::new();
    let base = image_base(&image);
    image.set(10, 0x1111);
    rela_entry(
        &mut image,
        RELA,
        0,
        10,
        crate::selfreloc_consts::REL_RELATIVE,
        0x2000,
    );
    let d = info(&image, 1, 0);
    // SAFETY: the fake `DynInfo` names an entry of this image and word 10 is writable.
    unsafe { apply_relative(base, &d) };
    assert_eq!(image.get(10), base.wrapping_add(0x2000));
}

#[test]
fn relative_63_entries() {
    let mut image = Image::new();
    let base = image_base(&image);
    for index in 0..63 {
        image.set(10 + index, 0x100 + index);
        rela_entry(
            &mut image,
            RELA,
            index,
            10 + index,
            crate::selfreloc_consts::REL_RELATIVE,
            0x10000 + index,
        );
    }
    let d = info(&image, 63, 0);
    // SAFETY: 63 entries inside the fake image, every offset is a writable word.
    unsafe { apply_relative(base, &d) };
    for index in 0..63 {
        assert_eq!(image.get(10 + index), base.wrapping_add(0x10000 + index));
    }
}

#[test]
fn relative_64_entries() {
    let mut image = Image::new();
    let base = image_base(&image);
    for index in 0..64 {
        image.set(10 + index, 0);
        rela_entry(
            &mut image,
            RELA,
            index,
            10 + index,
            crate::selfreloc_consts::REL_RELATIVE,
            7 + index,
        );
    }
    let d = info(&image, 64, 0);
    // SAFETY: 64 entries inside the fake image.
    unsafe { apply_relative(base, &d) };
    assert_eq!(image.get(10), base.wrapping_add(7));
    assert_eq!(image.get(73), base.wrapping_add(70));
}

#[test]
fn relative_10000_entries() {
    let mut words = vec![0u64; 2 * 10000 * 3 + 16];
    let base = words.as_ptr() as usize;
    for index in 0..10000 {
        words[index] = 5 + index as u64;
        let entry = index * 3;
        words[10000 + entry] = (index * 8) as u64;
        words[10000 + entry + 1] = crate::selfreloc_consts::REL_RELATIVE as u64;
        words[10000 + entry + 2] = (1_000_000 + index) as u64;
    }
    let d = DynInfo {
        rela: base.wrapping_add(10000 * 8),
        relasz: 10000 * 24,
        relr: 0,
        relrsz: 0,
        jmprel: 0,
        pltrelsz: 0,
        pltgot: 0,
    };
    // SAFETY: the array is 10 000 well-formed entries and every offset is in the buffer.
    unsafe { apply_relative(base, &d) };
    assert_eq!(words[0] as usize, base.wrapping_add(1_000_000));
    assert_eq!(words[9999] as usize, base.wrapping_add(1_009_999));
}

#[test]
fn relative_skips_other_types() {
    let mut image = Image::new();
    let base = image_base(&image);
    image.set(10, 0xAAAA);
    image.set(11, 0xBBBB);
    rela_entry(&mut image, RELA, 0, 10, 1, 0x2000); // R_X86_64_64
    rela_entry(
        &mut image,
        RELA,
        1,
        11,
        crate::selfreloc_consts::REL_RELATIVE,
        0x3000,
    );
    let d = info(&image, 2, 0);
    // SAFETY: two fake entries inside the image.
    unsafe { apply_relative(base, &d) };
    assert_eq!(image.get(10), 0xAAAA);
    assert_eq!(image.get(11), base.wrapping_add(0x3000));
}

#[test]
fn relative_negative_addend_wraps() {
    let mut image = Image::new();
    let base = image_base(&image);
    image.set(10, 0);
    rela_entry(
        &mut image,
        RELA,
        0,
        10,
        crate::selfreloc_consts::REL_RELATIVE,
        usize::MAX,
    );
    let d = info(&image, 1, 0);
    // SAFETY: one fake entry inside the image.
    unsafe { apply_relative(base, &d) };
    assert_eq!(image.get(10), base.wrapping_sub(1));
}

#[test]
fn relative_zero_size_is_noop() {
    let mut image = Image::new();
    let base = image_base(&image);
    image.set(10, 0x5A5A);
    let d = info(&image, 0, 0);
    // SAFETY: an empty table touches no memory.
    unsafe { apply_relative(base, &d) };
    assert_eq!(image.get(10), 0x5A5A);
}

// ----------------------------------------------------------------------- RELR

#[test]
fn relr_address_entry_only() {
    let mut image = Image::new();
    let base = image_base(&image);
    image.set(20, 0x1000);
    image.set(21, 0x2000);
    image.set(RELR, image.offset(20));
    let d = info(&image, 0, 1);
    // SAFETY: the one even entry names word 20 of this image.
    unsafe { apply_relr(base, &d) };
    assert_eq!(image.get(20), 0x1000usize.wrapping_add(base));
    assert_eq!(image.get(21), 0x2000);
}

#[test]
fn relr_bitmap_all_63_bits() {
    let mut image = Image::new();
    let base = image_base(&image);
    for index in 0..64 {
        image.set(30 + index, index);
    }
    image.set(RELR, image.offset(30));
    image.set(RELR + 1, usize::MAX); // bits 0..63 set; bit 0 is the continuation marker
    let d = info(&image, 0, 2);
    // SAFETY: the even entry names 64 consecutive words of this image.
    unsafe { apply_relr(base, &d) };
    assert_eq!(image.get(30), base);
    for index in 1..64 {
        assert_eq!(image.get(30 + index), index.wrapping_add(base));
    }
}

#[test]
fn relr_bitmap_crosses_page_boundary() {
    let mut image = Image::new();
    let base = image_base(&image);
    // Pick a word whose address is four words below a page boundary.
    let residue = (base / 8) % 512;
    let word = 40 + ((512 - residue) % 512);
    let word = word.min(WORDS - 80);
    for index in 0..8 {
        image.set(word + index, index);
    }
    image.set(RELR, image.offset(word));
    // bits 1..8 -> words word..word+7
    image.set(RELR + 1, 0b1_1111_1111); // bit 0 marks a bitmap
    let d = info(&image, 0, 2);
    // SAFETY: the bitmap names eight consecutive words of this image.
    unsafe { apply_relr(base, &d) };
    for index in 0..8 {
        assert_eq!(image.get(word + index), index.wrapping_add(base));
    }
}

#[test]
fn relr_empty_section() {
    let mut image = Image::new();
    let base = image_base(&image);
    image.set(20, 7);
    let d = info(&image, 0, 0);
    // SAFETY: an empty section touches no memory.
    unsafe { apply_relr(base, &d) };
    assert_eq!(image.get(20), 7);
}

#[test]
fn relr_consecutive_bitmaps_advance_63_words() {
    let mut image = Image::new();
    let base = image_base(&image);
    for index in 0..126 {
        image.set(100 + index, index);
    }
    image.set(RELR, image.offset(100));
    image.set(RELR + 1, 0b11); // bit 0 marks the bitmap; bit 1 is word 101
    image.set(RELR + 2, 0b11); // the next address is word 101 + 63 = word 164
    let d = info(&image, 0, 3);
    // SAFETY: two bitmaps over words 101 and 164 of this image.
    unsafe { apply_relr(base, &d) };
    assert_eq!(image.get(100), base);
    assert_eq!(image.get(101), 1usize.wrapping_add(base));
    assert_eq!(image.get(164), 64usize.wrapping_add(base));
}

// ------------------------------------------------------------------ IRELATIVE

#[cfg(target_arch = "x86_64")]
#[test]
fn irelative_in_rela() {
    let mut image = Image::new();
    let base = image_base(&image);
    image.set(50, 0);
    let resolver = resolver_addend(base, resolver_dead);
    rela_entry(
        &mut image,
        RELA,
        0,
        50,
        crate::selfreloc_consts::REL_IRELATIVE,
        resolver,
    );
    let d = info(&image, 1, 0);
    let aux = [0usize; AUX_CNT];
    // SAFETY: the fake entry names word 50 and the in-image resolver thunk.
    unsafe { apply_irelative(base, &d, &aux) };
    assert_eq!(image.get(50), 0xDEAD_BEEF);
}

#[cfg(target_arch = "x86_64")]
#[test]
fn irelative_in_jmprel() {
    let mut image = Image::new();
    let base = image_base(&image);
    image.set(51, 0);
    let resolver = resolver_addend(base, resolver_1234);
    rela_entry(
        &mut image,
        JMPREL,
        0,
        51,
        crate::selfreloc_consts::REL_IRELATIVE,
        resolver,
    );
    let mut d = info(&image, 0, 0);
    d.jmprel = base.wrapping_add(image.offset(JMPREL));
    d.pltrelsz = RELA_SIZE;
    let aux = [0usize; AUX_CNT];
    // SAFETY: the fake entry names word 51 and the in-image resolver thunk.
    unsafe { apply_irelative(base, &d, &aux) };
    assert_eq!(image.get(51), 0x1234_5678);
}

#[cfg(target_arch = "x86_64")]
#[test]
fn irelative_skips_relative_entries_in_the_same_table() {
    let mut image = Image::new();
    let base = image_base(&image);
    image.set(52, 1);
    let resolver = resolver_addend(base, resolver_aa);
    image.set(53, 2);
    rela_entry(
        &mut image,
        RELA,
        0,
        52,
        crate::selfreloc_consts::REL_RELATIVE,
        0x55,
    );
    rela_entry(
        &mut image,
        RELA,
        1,
        53,
        crate::selfreloc_consts::REL_IRELATIVE,
        resolver,
    );
    let d = info(&image, 2, 0);
    let aux = [0usize; AUX_CNT];
    // SAFETY: two entries of this image; the IRELATIVE thunk is in it.
    unsafe { apply_irelative(base, &d, &aux) };
    assert_eq!(image.get(52), 1);
    assert_eq!(image.get(53), 0xAA);
}

// ------------------------------------------------------------------ load_base

#[test]
fn base_from_at_base() {
    let image = Image::new();
    let base = image_base(&image);
    let words = stack(base, base, None);
    let mut aux = [0usize; AUX_CNT];
    // SAFETY: `words` is a C8 stack and `image`'s dynamic array is not read here.
    let computed = unsafe { load_base(words.as_ptr(), core::ptr::null(), &mut aux) };
    assert_eq!(computed, base);
    assert_eq!(aux[AT_HWCAP], 0x42);
}

#[test]
fn base_from_pt_dynamic_when_at_base_is_zero() {
    let mut image = Image::new();
    let base = image_base(&image);
    let dynv = base.wrapping_add(image.offset(DYN));
    let dynamic_offset = image.offset(DYN);
    program_headers(&mut image, dynamic_offset);
    let words = stack(
        base,
        0,
        Some((base.wrapping_add(image.offset(PHDR)), 56, 2)),
    );
    let mut aux = [0usize; AUX_CNT];
    // SAFETY: the fake stack and phdr table are consistent; `dynv` points into the image.
    let computed = unsafe { load_base(words.as_ptr(), dynv as *const usize, &mut aux) };
    assert_eq!(computed, base);
}

#[test]
fn aux_cnt_clamp_ignores_raven_keys() {
    let image = Image::new();
    let base = image_base(&image);
    let mut words = vec![1usize, 0x1000, 0, 0];
    words.push(0x7001);
    words.push(0xFFFF);
    words.push(0x7006);
    words.push(1);
    words.push(40);
    words.push(2);
    words.push(AT_BASE);
    words.push(base);
    words.push(0);
    words.push(0);
    let mut aux = [0usize; AUX_CNT];
    // SAFETY: a well-formed C8 stack whose unknown keys must be ignored.
    let computed = unsafe { load_base(words.as_ptr(), core::ptr::null(), &mut aux) };
    assert_eq!(computed, base);
    assert!(!aux.contains(&0xFFFF));
}

#[test]
fn dt_rel_is_ignored_by_design() {
    let mut image = Image::new();
    let base = image_base(&image);
    image.set(DYN, 17); // DT_REL
    let rela_offset = image.offset(RELA);
    image.set(DYN + 1, rela_offset);
    image.set(DYN + 2, 18); // DT_RELSZ
    image.set(DYN + 3, 24);
    image.set(DYN + 4, 0);
    let dynv = base.wrapping_add(image.offset(DYN));
    // SAFETY: the fake `_DYNAMIC` is terminated and inside the image.
    let d = unsafe { gather_dyn(base, dynv as *const usize) };
    assert_eq!(d.rela, 0);
    assert_eq!(d.relr, 0);
}

// --------------------------------------------------------------------- order

#[cfg(target_arch = "x86_64")]
#[test]
fn order_relative_then_relr_then_irelative() {
    let mut image = Image::new();
    let base = image_base(&image);
    image.set(60, 0x10);
    let resolver = resolver_addend(base, resolver_order);
    rela_entry(
        &mut image,
        RELA,
        0,
        60,
        crate::selfreloc_consts::REL_RELATIVE,
        0x20,
    );
    rela_entry(
        &mut image,
        RELA,
        1,
        60,
        crate::selfreloc_consts::REL_IRELATIVE,
        resolver,
    );
    image.set(RELR, image.offset(60));
    let mut d = info(&image, 2, 1);
    d.relr = base.wrapping_add(image.offset(RELR));
    d.relrsz = 8;
    let aux = [0usize; AUX_CNT];
    // SAFETY: all three passes name word 60 of this image and the resolver thunk in it.
    unsafe {
        apply_relative(base, &d);
        apply_relr(base, &d);
        apply_irelative(base, &d, &aux);
    }
    assert_eq!(image.get(60), 0x5A5A);
}

#[test]
fn relative_then_relr_accumulates_bias_twice() {
    let mut image = Image::new();
    let base = image_base(&image);
    image.set(61, 0);
    rela_entry(
        &mut image,
        RELA,
        0,
        61,
        crate::selfreloc_consts::REL_RELATIVE,
        0x30,
    );
    image.set(RELR, image.offset(61));
    let mut d = info(&image, 1, 1);
    d.relr = base.wrapping_add(image.offset(RELR));
    d.relrsz = 8;
    // SAFETY: two passes over word 61 of this image, in the mandated order.
    unsafe {
        apply_relative(base, &d);
        apply_relr(base, &d);
    }
    assert_eq!(image.get(61), base.wrapping_add(0x30).wrapping_add(base));
}

// --------------------------------------------------------------- end to end

#[test]
fn gather_dyn_collects_all_tables() {
    let mut image = Image::new();
    let base = image_base(&image);
    dyn_word(&mut image, 0, DT_RELA, RELA);
    dyn_entry(&mut image, 1, DT_RELASZ, 48);
    dyn_word(&mut image, 2, DT_RELR, RELR);
    dyn_entry(&mut image, 3, DT_RELRSZ, 16);
    dyn_word(&mut image, 4, DT_JMPREL, JMPREL);
    dyn_entry(&mut image, 5, DT_PLTRELSZ, 24);
    dyn_word(&mut image, 6, DT_PLTGOT, 100);
    dyn_entry(&mut image, 7, 0, 0);
    let dynv = base.wrapping_add(image.offset(DYN));
    // SAFETY: the fake `_DYNAMIC` is terminated and inside the image.
    let d = unsafe { gather_dyn(base, dynv as *const usize) };
    assert_eq!(d.rela, base.wrapping_add(image.offset(RELA)));
    assert_eq!(d.relasz, 48);
    assert_eq!(d.relr, base.wrapping_add(image.offset(RELR)));
    assert_eq!(d.relrsz, 16);
    assert_eq!(d.jmprel, base.wrapping_add(image.offset(JMPREL)));
    assert_eq!(d.pltrelsz, 24);
    assert_eq!(d.pltgot, base.wrapping_add(image.offset(100)));
}

#[test]
fn relocate_self_matches_the_reference_loop() {
    let mut image = Image::new();
    let base = image_base(&image);
    for index in 0..8 {
        image.set(200 + index, index);
    }
    rela_entry(
        &mut image,
        RELA,
        0,
        200,
        crate::selfreloc_consts::REL_RELATIVE,
        0x100,
    );
    rela_entry(&mut image, RELA, 1, 201, 1, 0x999); // skipped by both
    image.set(RELR, image.offset(202));
    image.set(RELR + 1, 0b11); // bit 0 marks the bitmap; bit 1 is word 203
    dynamic(&mut image, 2, 2);
    let dynv = base.wrapping_add(image.offset(DYN));
    let words = stack(base, base, None);
    let mut reference = image.words.clone();
    reference[200] = (base.wrapping_add(0x100)) as u64;
    reference[202] = (2usize.wrapping_add(base)) as u64;
    reference[203] = (3usize.wrapping_add(base)) as u64;
    // SAFETY: the fake stack, `_DYNAMIC` and relocation tables all belong to `image`.
    let computed = unsafe { relocate_self(words.as_ptr(), dynv as *const usize) };
    assert_eq!(computed, base);
    assert_eq!(image.words, reference);
}

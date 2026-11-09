// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Shared ELF builder and stack decoder for the linux-elf tests.

#![allow(dead_code)]

use linux_elf::{Config, StackImage};
use raven_elf::{Elf, Machine};

pub const ET_EXEC: u16 = 2;
pub const ET_DYN: u16 = 3;
pub const STACK_TOP: u64 = 0x7fff_ffff_f000;

pub struct ElfOptions<'a> {
    pub e_type: u16,
    pub entry: u64,
    pub load_vaddr: u64,
    pub interp: Option<&'a str>,
    pub exec_stack: Option<bool>,
    pub tls: bool,
    pub include_phdr: bool,
}

impl Default for ElfOptions<'_> {
    fn default() -> Self {
        Self {
            e_type: ET_EXEC,
            entry: 0x40_0000,
            load_vaddr: 0x40_0000,
            interp: None,
            exec_stack: None,
            tls: false,
            include_phdr: true,
        }
    }
}

pub fn build_elf(options: &ElfOptions<'_>) -> Vec<u8> {
    let phnum = 2
        + usize::from(options.include_phdr)
        + usize::from(options.interp.is_some())
        + usize::from(options.exec_stack.is_some())
        + usize::from(options.tls);
    let mut phdrs = Vec::new();

    if options.include_phdr {
        phdrs.push(Phdr {
            p_type: 6,
            flags: 4,
            offset: 64,
            vaddr: options.load_vaddr + 64,
            filesz: phnum as u64 * 56,
            memsz: phnum as u64 * 56,
            align: 8,
        });
    }
    if let Some(interp) = options.interp {
        let len = interp.len() as u64 + 1;
        phdrs.push(Phdr {
            p_type: 3,
            flags: 4,
            offset: 0x3000,
            vaddr: 0,
            filesz: len,
            memsz: len,
            align: 1,
        });
    }
    phdrs.push(Phdr {
        p_type: 1,
        flags: 5,
        offset: 0x1000,
        vaddr: options.load_vaddr,
        filesz: 0x1000,
        memsz: 0x1000,
        align: 0x1000,
    });
    phdrs.push(Phdr {
        p_type: 1,
        flags: 6,
        offset: 0x2000,
        vaddr: options.load_vaddr + 0x1000,
        filesz: 0x1000,
        memsz: 0x2000,
        align: 0x1000,
    });
    if let Some(exec) = options.exec_stack {
        phdrs.push(Phdr {
            p_type: 0x6474_e551,
            flags: if exec { 7 } else { 6 },
            offset: 0,
            vaddr: 0,
            filesz: 0,
            memsz: 0,
            align: 16,
        });
    }
    if options.tls {
        phdrs.push(Phdr {
            p_type: 7,
            flags: 4,
            offset: 0x3080,
            vaddr: options.load_vaddr + 0x2000,
            filesz: 16,
            memsz: 32,
            align: 16,
        });
    }
    assert_eq!(phdrs.len(), phnum);

    let mut data = vec![0u8; 0x4000];
    data[0..4].copy_from_slice(b"\x7fELF");
    data[4] = 2;
    data[5] = 1;
    data[6] = 1;
    put16(&mut data, 16, options.e_type);
    put16(&mut data, 18, 62);
    put32(&mut data, 20, 1);
    put64(&mut data, 24, options.entry);
    put64(&mut data, 32, 64);
    put16(&mut data, 52, 64);
    put16(&mut data, 54, 56);
    put16(&mut data, 56, phnum as u16);

    for (index, phdr) in phdrs.iter().enumerate() {
        phdr.write(&mut data, 64 + index * 56);
    }
    if let Some(interp) = options.interp {
        data[0x3000..0x3000 + interp.len()].copy_from_slice(interp.as_bytes());
        data[0x3000 + interp.len()] = 0;
    }
    if options.tls {
        for index in 0..16 {
            data[0x3080 + index] = index as u8;
        }
    }
    data
}

pub fn parse_elf(data: &[u8]) -> Elf<'_> {
    Elf::parse(data, Machine::X86_64, 0..u64::MAX).expect("test ELF must parse")
}

pub fn config() -> Config {
    Config {
        main_pie_base: 0x1000_0000,
        interp_base: 0x2000_0000,
        page: 4096,
        uid: 1000,
        euid: 1000,
        gid: 1000,
        egid: 1000,
        secure: false,
        hwcap: 0x1234,
        platform: "x86_64",
        clktck: 100,
        sysinfo_ehdr: None,
        random: [0x11; 16],
    }
}

struct Phdr {
    p_type: u32,
    flags: u32,
    offset: u64,
    vaddr: u64,
    filesz: u64,
    memsz: u64,
    align: u64,
}

impl Phdr {
    fn write(&self, data: &mut [u8], base: usize) {
        put32(data, base, self.p_type);
        put32(data, base + 4, self.flags);
        put64(data, base + 8, self.offset);
        put64(data, base + 16, self.vaddr);
        put64(data, base + 24, 0);
        put64(data, base + 32, self.filesz);
        put64(data, base + 40, self.memsz);
        put64(data, base + 48, self.align);
    }
}

fn put16(data: &mut [u8], offset: usize, value: u16) {
    data[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn put32(data: &mut [u8], offset: usize, value: u32) {
    data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn put64(data: &mut [u8], offset: usize, value: u64) {
    data[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

pub struct Decoded {
    pub argc: u64,
    pub argv: Vec<Vec<u8>>,
    pub envp: Vec<Vec<u8>>,
    pub auxv: Vec<(u64, u64)>,
    pub argv_pointers: Vec<u64>,
    pub envp_pointers: Vec<u64>,
}

pub fn decode(image: &StackImage, stack_top: u64) -> Decoded {
    assert_eq!(image.sp % 16, 0, "sp is not 16-byte aligned");
    assert_eq!(image.base, image.sp, "base must equal sp");
    assert_eq!(
        image.base + image.bytes.len() as u64,
        stack_top,
        "image must end at stack_top"
    );

    let mut position = 0usize;
    let argc = read_u64(&image.bytes, position);
    position += 8;
    let argc_usize = usize::try_from(argc).unwrap();
    let mut argv_pointers = Vec::with_capacity(argc_usize);
    for _ in 0..argc_usize {
        argv_pointers.push(checked_pointer(image, read_u64(&image.bytes, position)));
        position += 8;
    }
    assert_eq!(read_u64(&image.bytes, position), 0);
    position += 8;

    let mut envp_pointers = Vec::new();
    loop {
        let pointer = read_u64(&image.bytes, position);
        position += 8;
        if pointer == 0 {
            break;
        }
        envp_pointers.push(checked_pointer(image, pointer));
    }

    let mut auxv = Vec::new();
    loop {
        let key = read_u64(&image.bytes, position);
        let value = read_u64(&image.bytes, position + 8);
        position += 16;
        auxv.push((key, value));
        if key == 0 {
            break;
        }
    }

    let argv = argv_pointers
        .iter()
        .map(|pointer| read_cstr(image, *pointer))
        .collect();
    let envp = envp_pointers
        .iter()
        .map(|pointer| read_cstr(image, *pointer))
        .collect();

    for key in [15, 25, 31] {
        if let Some(pointer) = auxv_value(&auxv, key) {
            checked_pointer(image, pointer);
            if key == 25 {
                assert!(
                    pointer + 16 <= image.base + image.bytes.len() as u64,
                    "AT_RANDOM crosses the image end"
                );
            } else {
                let _ = read_cstr(image, pointer);
            }
        }
    }

    Decoded {
        argc,
        argv,
        envp,
        auxv,
        argv_pointers,
        envp_pointers,
    }
}

pub fn auxv_value(auxv: &[(u64, u64)], key: u64) -> Option<u64> {
    auxv.iter()
        .find(|(candidate, _)| *candidate == key)
        .map(|(_, value)| *value)
}

fn checked_pointer(image: &StackImage, pointer: u64) -> u64 {
    assert!(pointer >= image.base, "pointer below the image");
    assert!(
        pointer < image.base + image.bytes.len() as u64,
        "pointer above the image"
    );
    pointer
}

fn read_cstr(image: &StackImage, pointer: u64) -> Vec<u8> {
    let offset = usize::try_from(pointer - image.base).unwrap();
    let rest = &image.bytes[offset..];
    let end = rest.iter().position(|byte| *byte == 0).expect("NUL");
    rest[..end].to_vec()
}

fn read_u64(data: &[u8], offset: usize) -> u64 {
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&data[offset..offset + 8]);
    u64::from_le_bytes(bytes)
}

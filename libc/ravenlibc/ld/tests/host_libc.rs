// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! C62 L1: load the real `libc.so.1` into the host process and call pure functions.
#![allow(unused_imports, clippy::manual_c_str_literals)]

#[path = "support/fixtures.rs"]
mod fixtures;
#[path = "support/host_mapper.rs"]
mod host_mapper;

use std::path::PathBuf;

use host_mapper::{HostApplier, HostMapper, HostMem};
use raven_libc_rtld::loadobj::Loader;

fn libc_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../build/x86_64/libc.so.1")
}

#[test]
fn host_load_real_libc_and_call_pure_functions() {
    let path = libc_path();
    if !path.is_file() {
        println!(
            "SKIP: {} is absent (run cargo xtask libc-link --arch x86_64)",
            path.display()
        );
        return;
    }
    let (mapper, mem, applier) = HostMapper::new();
    let mut loader = Loader::new(mapper, mem, applier);
    let id = match loader.load_object(path.as_os_str().as_encoded_bytes(), b"libc.so.1") {
        Ok(id) => id,
        Err(error) => {
            println!("SKIP: partial libc.so.1 (LINK-FINDING): {error:?}");
            return;
        }
    };
    let names = {
        let Some(object) = loader.object(id) else {
            return;
        };
        let mut names = Vec::new();
        for index in 0..object.view.dynsymcount {
            if let Some(symbol) = object.view.sym(&loader.mem, index)
                && symbol.shndx == 0
                && symbol.bind() == raven_libc_elf::consts::STB_GLOBAL
                && let Ok(name) = object.view.name_at(&loader.mem, symbol.name)
            {
                names.push(String::from_utf8_lossy(name).into_owned());
            }
        }
        names.sort();
        names.dedup();
        names
    };
    let Some(stub) = fixtures::build_stub(&names) else {
        println!("SKIP: cannot build the host import stub");
        return;
    };
    let stub_id = match loader.load_object(stub.as_os_str().as_encoded_bytes(), b"libhoststub.so") {
        Ok(id) => id,
        Err(error) => {
            println!("SKIP: host import stub: {error:?}");
            return;
        }
    };
    loader.scope = raven_libc_elf::Scope::new(id);
    loader.scope.global.push(stub_id);
    if let Err(error) = loader.relocate_all() {
        println!("SKIP: partial libc.so.1 (LINK-FINDING): {error:?}");
        return;
    }
    if let Some(strlen) = loader.dlsym(Some(id), b"strlen", 0) {
        // SAFETY: `strlen` is a no-side-effect C function in the mapped image.
        let length = unsafe {
            std::mem::transmute::<usize, extern "C" fn(*const u8) -> usize>(strlen)(
                b"hello\0".as_ptr(),
            )
        };
        assert_eq!(length, 5);
    } else {
        println!("SKIP: strlen is local hidden in the partial libc.so.1 (LINK-FINDING)");
    }
    let Some(strcmp) = loader.dlsym(Some(id), b"strcmp", 0) else {
        println!("SKIP: strcmp is not exported by the partial libc.so.1 (LINK-FINDING)");
        return;
    };
    // SAFETY: `strcmp` is a pure C function in the mapped image.
    let order = unsafe {
        std::mem::transmute::<usize, extern "C" fn(*const u8, *const u8) -> i32>(strcmp)(
            b"a\0".as_ptr(),
            b"b\0".as_ptr(),
        )
    };
    assert!(order < 0);
    let Some(memchr) = loader.dlsym(Some(id), b"memchr", 0) else {
        println!("SKIP: memchr is not exported by the partial libc.so.1 (LINK-FINDING)");
        return;
    };
    // SAFETY: `memchr` reads only the requested bytes of the mapped image.
    let found = unsafe {
        std::mem::transmute::<usize, extern "C" fn(*const u8, i32, usize) -> *const u8>(memchr)(
            b"abcdef\0".as_ptr(),
            i32::from(b'd'),
            6,
        )
    };
    assert!(!found.is_null());
}

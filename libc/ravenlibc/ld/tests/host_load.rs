// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! C62 host test: load real x86_64 shared objects and call their code.

#[path = "support/fixtures.rs"]
mod fixtures;
#[path = "support/host_mapper.rs"]
mod host_mapper;

use host_mapper::{Event, HostApplier, HostMapper, HostMem};
use raven_libc_elf::{MemView, ObjId};
use raven_libc_rtld::loadobj::{LoadError, Loader};
use raven_libc_rtld::main_flow::{StartCtx, rtld_main};

/// `HostMapper` maps at fixed addresses; serialise the tests so two mappings cannot race.
static HOST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn loader() -> Option<(Loader<HostMapper, HostMem, HostApplier>, std::path::PathBuf)> {
    let dir = fixtures::required()?;
    let (mapper, mem, applier) = HostMapper::new();
    let mut loader = Loader::new(mapper, mem, applier);
    let _ = loader.add_library_path(dir.as_os_str().as_encoded_bytes());
    Some((loader, dir))
}

fn load(
    loader: &mut Loader<HostMapper, HostMem, HostApplier>,
    dir: &std::path::Path,
    name: &str,
) -> ObjId {
    let path = dir.join(name);
    let id = loader
        .load_object(path.as_os_str().as_encoded_bytes(), name.as_bytes())
        .expect("load");
    loader.load_needed_objects(id).expect("needed");
    if let Some(object) = loader.object(id) {
        eprintln!(
            "{name}: rela={:?} plt={:?} relr={:?} segs={}",
            object.view.rela,
            object.view.plt_rela,
            object.view.relr,
            object.view.segments.len()
        );
    }
    if let Err(error) = loader.relocate_all() {
        let detail = String::from_utf8_lossy(loader.error.as_bytes()).to_string();
        eprintln!("relocate {name}: {error:?} ({detail})");
        panic!("relocate {name}: {error:?} ({detail})");
    }
    id
}

#[test]
fn host_load_libval_calls_code_and_resolves_relative_data() {
    let _guard = HOST_LOCK.lock().expect("lock");
    let Some((mut loader, dir)) = loader() else {
        return;
    };
    eprintln!("load libval");
    let id = load(&mut loader, &dir, "libval.so");
    eprintln!("loaded libval");
    let val = loader.dlsym(Some(id), b"val", 0).expect("val");
    eprintln!("dlsym val {val:x}");
    // SAFETY: `val` is a no-argument integer function in the mapped image.
    let result = unsafe { std::mem::transmute::<usize, extern "C" fn() -> i32>(val)() };
    assert_eq!(result, 42);
    let call_deep = loader.dlsym(Some(id), b"call_deep", 0).expect("call_deep");
    // SAFETY: `call_deep` is a no-argument integer function in the mapped image.
    let deep = unsafe { std::mem::transmute::<usize, extern "C" fn() -> i32>(call_deep)() };
    assert_eq!(deep, 7);
    let table = loader.dlsym(Some(id), b"val_table", 0).expect("val_table");
    let bytes = loader.mem.read(table as u64, 16).expect("table bytes");
    let first = u64::from_le_bytes(bytes.get(..8).expect("first").try_into().expect("word"));
    let second = u64::from_le_bytes(bytes.get(8..16).expect("second").try_into().expect("word"));
    assert_eq!(first, u64::try_from(val).expect("address"));
    assert_eq!(second, u64::try_from(table).expect("table address"));
}

#[test]
fn host_load_versions_and_weak_symbols() {
    let _guard = HOST_LOCK.lock().expect("lock");
    let Some((mut loader, dir)) = loader() else {
        return;
    };
    let id = load(&mut loader, &dir, "libver.so");
    let old = loader.dlvsym(Some(id), b"ver", b"LIBV_0.9").expect("old");
    // SAFETY: `ver` is a no-argument integer function.
    let value = unsafe { std::mem::transmute::<usize, extern "C" fn() -> i32>(old)() };
    assert_eq!(value, 2);
    let default = loader.dlsym(Some(id), b"ver", 0).expect("default");
    // SAFETY: see above.
    let value = unsafe { std::mem::transmute::<usize, extern "C" fn() -> i32>(default)() };
    assert_eq!(value, 1);

    let weak = load(&mut loader, &dir, "libweak.so");
    assert!(loader.dlsym(Some(weak), b"missing_sym", 0).is_none());
}

#[test]
fn a_hidden_definition_is_not_exported() {
    let _guard = HOST_LOCK.lock().expect("lock");
    let Some((mut loader, dir)) = loader() else {
        return;
    };
    let id = load(&mut loader, &dir, "libhidden.so");
    assert!(loader.dlsym(Some(id), b"hidden_fn", 0).is_none());
}

#[test]
fn host_load_ifunc_and_dependency_cycle() {
    let _guard = HOST_LOCK.lock().expect("lock");
    let Some((mut loader, dir)) = loader() else {
        return;
    };
    eprintln!("load libifunc");
    let id = load(&mut loader, &dir, "libifunc.so");
    eprintln!("loaded libifunc");
    let pick = loader.dlsym(Some(id), b"pick", 0).expect("pick");
    // SAFETY: `pick` returns the implementation address.
    let implementation = unsafe { std::mem::transmute::<usize, extern "C" fn() -> usize>(pick)() };
    // SAFETY: the implementation is a no-argument integer function.
    let value = unsafe { std::mem::transmute::<usize, extern "C" fn() -> i32>(implementation)() };
    assert_eq!(value, 1);

    eprintln!("load libcycle_a");
    let a = load(&mut loader, &dir, "libcycle_a.so");
    eprintln!("loaded libcycle_a");
    assert!(loader.dlsym(Some(a), b"a_fn", 0).is_some());
    assert!(loader.dlsym(Some(a), b"b_fn", 0).is_some());
}

#[test]
fn ifunc_pass_precedes_relro() {
    let _guard = HOST_LOCK.lock().expect("lock");
    let Some((mut loader, dir)) = loader() else {
        return;
    };
    let _ = load(&mut loader, &dir, "libifunc.so");
    let events = loader.platform.events();
    let last_ifunc = events
        .iter()
        .rposition(|event| matches!(event, Event::Ifunc(_)))
        .expect("ifunc event");
    let first_protect = events
        .iter()
        .position(|event| matches!(event, Event::Protect(_)))
        .expect("protect event");
    assert!(
        last_ifunc < first_protect,
        "IFUNC must run before RELRO: {events:?}"
    );
}

#[test]
fn host_start_flow_loads_and_relocates() {
    let _guard = HOST_LOCK.lock().expect("lock");
    let Some((mut loader, dir)) = loader() else {
        return;
    };
    let path = dir.join("libval.so");
    let paths = [dir.as_os_str().as_encoded_bytes()];
    let ctx = StartCtx {
        main_path: path.as_os_str().as_encoded_bytes(),
        main_name: b"libval.so",
        library_path: &paths,
        arch: raven_libc_elf::Arch::X86_64,
        trust: true,
    };
    let result = rtld_main(&mut loader, ctx).expect("start");
    let _ = result.entry;
    assert!(loader.dlsym(Some(loader.scope.main), b"val", 0).is_some());
}

#[test]
fn a_missing_needed_library_reports_not_found() {
    let _guard = HOST_LOCK.lock().expect("lock");
    let Some((mut loader, dir)) = loader() else {
        return;
    };
    let path = dir.join("does-not-exist.so");
    let error = loader
        .load_object(path.as_os_str().as_encoded_bytes(), b"nope.so")
        .expect_err("missing");
    assert_eq!(error, LoadError::Io(2));
}

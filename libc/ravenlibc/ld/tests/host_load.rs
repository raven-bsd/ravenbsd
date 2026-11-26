// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! C62 host test: load real x86_64 shared objects and call their code.

#[path = "support/fixtures.rs"]
mod fixtures;
#[path = "support/host_mapper.rs"]
mod host_mapper;

use host_mapper::{Event, HostApplier, HostMapper, HostMem};
use raven_libc_elf::{DigestError, Ehdr, ElfError, MemView, ObjId, Phdr};
use raven_libc_rtld::dlfcn::{RTLD_GLOBAL, RTLD_NEXT, RTLD_NODELETE, RTLD_NOLOAD};
use raven_libc_rtld::loadobj::{LoadError, Loader};
use raven_libc_rtld::main_flow::{StartCtx, register_premapped, rtld_main};
use raven_libc_rtld::table::RtldTable;

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
        args: &[],
        preload: &[],
        debug: b"",
        arch: raven_libc_elf::Arch::X86_64,
        trust: true,
    };
    let result = rtld_main(&mut loader, ctx).expect("start");
    let _ = result.entry;
    assert!(loader.dlsym(Some(loader.scope.main), b"val", 0).is_some());
}

#[test]
fn dt_rel_is_rejected_by_the_loader() {
    let _guard = HOST_LOCK.lock().expect("lock");
    let Some((mut loader, dir)) = loader() else {
        return;
    };
    let source = std::fs::read(dir.join("libdeep.so")).expect("fixture");
    let header = Ehdr::parse(source.get(..64).expect("header")).expect("ehdr");
    let mut at = usize::try_from(header.e_phoff).expect("phoff");
    let mut dynamic = None;
    for _ in 0..header.e_phnum {
        let phdr = Phdr::parse(source.get(at..at + 56).expect("phdr")).expect("phdr");
        if phdr.p_type == 2 {
            dynamic = Some((
                usize::try_from(phdr.p_offset).expect("offset"),
                usize::try_from(phdr.p_filesz).expect("size"),
            ));
        }
        at = at.saturating_add(56);
    }
    let (start, size) = dynamic.expect("PT_DYNAMIC");
    let mut patched = source.clone();
    for entry in (start..start.saturating_add(size)).step_by(16) {
        let tag = u64::from_le_bytes(
            patched
                .get(entry..entry + 8)
                .expect("tag")
                .try_into()
                .expect("tag"),
        );
        if tag == 0 {
            patched
                .get_mut(entry..entry + 8)
                .expect("tag")
                .copy_from_slice(&17u64.to_le_bytes());
            patched
                .get_mut(entry + 8..entry + 16)
                .expect("value")
                .fill(0);
            break;
        }
    }
    let path = std::env::temp_dir().join(format!("c62-librel-{}.so", std::process::id()));
    std::fs::write(&path, patched).expect("write");
    let error = loader
        .load_object(path.as_os_str().as_encoded_bytes(), b"librel.so")
        .expect_err("DT_REL");
    assert_eq!(error, LoadError::Digest(DigestError::RelNotSupported));
}

#[test]
fn direct_exec_wires_the_program_and_list_mode() {
    let _guard = HOST_LOCK.lock().expect("lock");
    let Some(dir) = fixtures::required() else {
        return;
    };
    let (mapper, mem, applier) = HostMapper::new();
    let mut loader = Loader::new(mapper, mem, applier);
    let program = dir.join("libdeep.so");
    let program_bytes = program.as_os_str().as_encoded_bytes();
    let args: [&[u8]; 2] = [b"--list", program_bytes];
    let ctx = StartCtx {
        main_path: b"",
        main_name: b"",
        library_path: &[],
        args: &args,
        preload: &[],
        debug: b"files",
        arch: raven_libc_elf::Arch::X86_64,
        trust: true,
    };
    let result = rtld_main(&mut loader, ctx).expect("direct list");
    assert_eq!(result.entry, 0);
    assert!(loader.object(loader.scope.main).is_some());
    assert!(
        String::from_utf8_lossy(&loader.platform.debug_bytes()).contains("=>"),
        "missing trace output"
    );
}

#[test]
fn preload_objects_load_before_main_and_debug_is_emitted() {
    let _guard = HOST_LOCK.lock().expect("lock");
    let Some(dir) = fixtures::required() else {
        return;
    };
    let (mapper, mem, applier) = HostMapper::new();
    let mut loader = Loader::new(mapper, mem, applier);
    let program = dir.join("libdeep.so");
    let paths = [dir.as_os_str().as_encoded_bytes()];
    let preload: [&[u8]; 1] = [b"libval.so"];
    let ctx = StartCtx {
        main_path: program.as_os_str().as_encoded_bytes(),
        main_name: b"libdeep.so",
        library_path: &paths,
        args: &[],
        preload: &preload,
        debug: b"files",
        arch: raven_libc_elf::Arch::X86_64,
        trust: true,
    };
    let _ = rtld_main(&mut loader, ctx).expect("preload start");
    assert_eq!(loader.scope.preload.len(), 1);
    assert!(loader.by_name(b"libval.so").is_some());
    let output = String::from_utf8_lossy(&loader.platform.debug_bytes()).to_string();
    assert!(output.contains("preload"), "{output:?}");
    assert!(output.contains("load main"), "{output:?}");
}

#[test]
fn the_static_ldd_case_reports_not_a_dynamic_executable() {
    let _guard = HOST_LOCK.lock().expect("lock");
    let Some(dir) = fixtures::required() else {
        return;
    };
    let (mapper, mem, applier) = HostMapper::new();
    let mut loader = Loader::new(mapper, mem, applier);
    let program = dir.join("staticprog");
    let program_bytes = program.as_os_str().as_encoded_bytes();
    let args: [&[u8]; 2] = [b"--list", program_bytes];
    let ctx = StartCtx {
        main_path: b"",
        main_name: b"",
        library_path: &[],
        args: &args,
        preload: &[],
        debug: b"",
        arch: raven_libc_elf::Arch::X86_64,
        trust: true,
    };
    assert_eq!(rtld_main(&mut loader, ctx), Err(LoadError::NotDynamic));
    assert!(
        String::from_utf8_lossy(&loader.platform.debug_bytes())
            .contains("not a dynamic executable")
    );
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

#[test]
fn writable_executable_and_textrel_objects_are_rejected() {
    let _guard = HOST_LOCK.lock().expect("lock");
    let Some((mut loader, dir)) = loader() else {
        return;
    };
    let badwx = dir.join("libbadwx.so");
    let error = loader
        .load_object(badwx.as_os_str().as_encoded_bytes(), b"libbadwx.so")
        .expect_err("W|X");
    assert_eq!(error, LoadError::Elf(ElfError::BadSegment));

    let textrel = dir.join("libtextrel.so");
    let error = loader
        .load_object(textrel.as_os_str().as_encoded_bytes(), b"libtextrel.so")
        .expect_err("TEXTREL");
    assert_eq!(error, LoadError::Digest(DigestError::TextRel));
}

#[test]
fn rpath_runpath_and_environment_order_are_kept() {
    let _guard = HOST_LOCK.lock().expect("lock");
    let Some(dir) = fixtures::required() else {
        return;
    };
    let (mapper, mem, applier) = HostMapper::new();
    let mut loader = Loader::new(mapper, mem, applier);
    let rpath = dir.join("libsearch_rpath.so");
    let rpath_id = loader
        .load_object(rpath.as_os_str().as_encoded_bytes(), b"libsearch_rpath.so")
        .expect("rpath object");
    let runpath = dir.join("libsearch_runpath.so");
    let runpath_id = loader
        .load_object(
            runpath.as_os_str().as_encoded_bytes(),
            b"libsearch_runpath.so",
        )
        .expect("runpath object");

    let found = loader
        .find_library_for(b"libdeep.so", Some(rpath_id))
        .expect("rpath")
        .expect("found");
    assert!(
        found.as_bytes().ends_with(b"/rpath/libdeep.so"),
        "{found:?}"
    );

    let found = loader
        .find_library_for(b"libdeep.so", Some(runpath_id))
        .expect("runpath")
        .expect("found");
    assert!(
        found.as_bytes().ends_with(b"/runpath/libdeep.so"),
        "{found:?}"
    );

    let ldpath = dir.join("ldpath");
    let _ = loader.add_library_path(ldpath.as_os_str().as_encoded_bytes());
    let found = loader
        .find_library_for(b"libdeep.so", Some(rpath_id))
        .expect("rpath with ld path")
        .expect("found");
    assert!(
        found.as_bytes().ends_with(b"/rpath/libdeep.so"),
        "{found:?}"
    );
    let found = loader
        .find_library_for(b"libdeep.so", Some(runpath_id))
        .expect("runpath with ld path")
        .expect("found");
    assert!(
        found.as_bytes().ends_with(b"/ldpath/libdeep.so"),
        "{found:?}"
    );
}

#[test]
fn static_tls_offsets_are_assigned_before_relocation() {
    let _guard = HOST_LOCK.lock().expect("lock");
    let Some((mut loader, dir)) = loader() else {
        return;
    };
    let path = dir.join("libtls.so");
    let id = loader
        .load_object(path.as_os_str().as_encoded_bytes(), b"libtls.so")
        .expect("tls object");
    loader.assign_tls_offsets().expect("tls offsets");
    let object = loader.object(id).expect("object");
    assert_eq!(object.tls_size, 16);
    assert_eq!(loader.tls.count, 1);
}

extern "C" fn null_dlopen(_name: *const u8, _flags: i32) -> *mut u8 {
    core::ptr::null_mut()
}

extern "C" fn null_dlclose(_handle: *mut u8) -> i32 {
    -1
}

extern "C" fn null_dlsym(_handle: *mut u8, _name: *const u8, _caller: usize) -> *mut u8 {
    core::ptr::null_mut()
}

extern "C" fn null_dlvsym(
    _handle: *mut u8,
    _name: *const u8,
    _version: *const u8,
    _caller: usize,
) -> *mut u8 {
    core::ptr::null_mut()
}

extern "C" fn null_dlerror() -> *const u8 {
    core::ptr::null()
}

extern "C" fn null_dladdr(_address: usize, _info: *mut u8) -> i32 {
    0
}

extern "C" fn null_dlinfo(_handle: *mut u8, _request: i32, _argument: *mut u8) -> i32 {
    -1
}

extern "C" fn null_iterate(
    _callback: Option<extern "C" fn(*mut u8, usize, *mut u8) -> i32>,
    _data: *mut u8,
) -> i32 {
    0
}

fn test_table() -> RtldTable {
    RtldTable {
        version: RtldTable::VERSION,
        size: RtldTable::SIZE,
        dlopen: null_dlopen,
        dlclose: null_dlclose,
        dlsym: null_dlsym,
        dlvsym: null_dlvsym,
        dlerror: null_dlerror,
        dladdr: null_dladdr,
        dlinfo: null_dlinfo,
        dl_iterate_phdr: null_iterate,
    }
}

#[test]
fn the_rtld_table_is_installed_once_into_the_shell() {
    let _guard = HOST_LOCK.lock().expect("lock");
    let Some((mut loader, dir)) = loader() else {
        return;
    };
    let path = dir.join("libinstall.so");
    let id = loader
        .load_object(path.as_os_str().as_encoded_bytes(), b"libinstall.so")
        .expect("install object");
    loader.relocate_all().expect("relocate");
    let table = test_table();
    assert!(loader.install_table(&table));
    assert!(!loader.install_table(&table));
    let captured = loader
        .dlsym(Some(id), b"captured", 0)
        .expect("captured symbol");
    let bytes = loader.mem.read(captured as u64, 8).expect("captured word");
    let word = u64::from_le_bytes(bytes.try_into().expect("word"));
    assert_eq!(
        word,
        u64::try_from(&table as *const RtldTable as usize).expect("address")
    );
}

#[test]
fn a_missing_install_symbol_is_not_an_error() {
    let _guard = HOST_LOCK.lock().expect("lock");
    let Some((mut loader, dir)) = loader() else {
        return;
    };
    let _ = load(&mut loader, &dir, "libval.so");
    let table = test_table();
    assert!(!loader.install_table(&table));
    assert_eq!(loader.table_address, 0);
}

#[test]
fn secure_mode_refuses_relative_paths_and_ignores_preload() {
    let _guard = HOST_LOCK.lock().expect("lock");
    let Some((mut loader, dir)) = loader() else {
        return;
    };
    loader.trust = false;
    assert!(!loader.add_preload(b"libval.so"));
    assert_eq!(
        loader.find_library_for(b"./libdeep.so", None),
        Err(LoadError::SecurePath)
    );
    assert_eq!(
        loader.find_library_for(b"$ORIGIN/libdeep.so", None),
        Err(LoadError::SecurePath)
    );
    let relative = std::path::Path::new("./libdeep.so");
    assert_eq!(
        loader.load_object(relative.as_os_str().as_encoded_bytes(), b"libdeep.so"),
        Err(LoadError::SecurePath)
    );
    loader.trust = true;
    assert!(loader.add_preload(b"libval.so"));
    assert!(loader.find_library_for(b"$ORIGIN/libdeep.so", None).is_ok());
    let _ = dir;
}

#[test]
fn an_untrusted_start_ignores_the_library_path() {
    let _guard = HOST_LOCK.lock().expect("lock");
    let Some(dir) = fixtures::required() else {
        return;
    };
    let (mapper, mem, applier) = HostMapper::new();
    let mut loader = Loader::new(mapper, mem, applier);
    let path = dir.join("libdeep.so");
    let paths = [dir.as_os_str().as_encoded_bytes()];
    let ctx = StartCtx {
        main_path: path.as_os_str().as_encoded_bytes(),
        main_name: b"libdeep.so",
        library_path: &paths,
        args: &[],
        preload: &[],
        debug: b"",
        arch: raven_libc_elf::Arch::X86_64,
        trust: false,
    };
    let _ = rtld_main(&mut loader, ctx).expect("untrusted start");
    assert_eq!(loader.library_path_count, 0);
}

#[test]
fn a_ring_two_alias_reuses_the_loaded_object() {
    let _guard = HOST_LOCK.lock().expect("lock");
    let Some((mut loader, dir)) = loader() else {
        return;
    };
    let path = dir.join("libval.so");
    let id = loader
        .load_object(path.as_os_str().as_encoded_bytes(), b"libc.so.1")
        .expect("load");
    let alias = loader.dlopen(b"libm.so.6", RTLD_NOLOAD).expect("alias");
    assert_eq!(alias, id);
}

#[test]
fn dlclose_uses_reference_counts_and_honours_nodelete() {
    let _guard = HOST_LOCK.lock().expect("lock");
    let Some((mut loader, _dir)) = loader() else {
        return;
    };
    let first = loader.dlopen(b"libval.so", 0).expect("first");
    let second = loader.dlopen(b"libval.so", 0).expect("second");
    assert_eq!(first, second);
    assert_eq!(loader.object(first).expect("object").refcount, 2);
    assert_eq!(loader.dlclose(first), 0);
    assert!(loader.object(first).is_some());
    assert_eq!(loader.dlclose(second), 0);
    assert!(loader.object(first).is_none());

    let pinned = match loader.dlopen(b"libval.so", RTLD_NODELETE) {
        Ok(id) => id,
        Err(error) => {
            panic!(
                "nodelete: {error:?} ({})",
                String::from_utf8_lossy(loader.error.as_bytes())
            );
        }
    };
    assert!(loader.object(pinned).expect("object").nodelete);
    assert_eq!(loader.dlclose(pinned), 0);
    assert!(loader.object(pinned).is_some());
}

#[test]
fn dlopen_assigns_static_tls_offsets_before_relocation() {
    let _guard = HOST_LOCK.lock().expect("lock");
    let Some((mut loader, _dir)) = loader() else {
        return;
    };
    let id = loader.dlopen(b"libtls.so", 0).expect("load");
    let object = loader.object(id).expect("object");
    assert!(object.tls_size > 0);
    assert_eq!(loader.applier.tls_offset(id), Some(object.tls_offset));
}

#[test]
fn dlopen_initialises_the_current_thread_static_tls_once() {
    let _guard = HOST_LOCK.lock().expect("lock");
    let Some((mut loader, _dir)) = loader() else {
        return;
    };
    let id = loader.dlopen(b"libtls.so", 0).expect("load");
    let object = loader.object(id).expect("object");
    let tls = object.view.tls.expect("tls header");
    let (image, filesz) = loader.applier.tls_init(id.0).expect("tls init");
    assert_ne!(image, 0);
    assert_eq!(filesz, tls.filesz);
    assert_eq!(loader.applier.tls_init_count(id.0), 1);
    // A second open of the same path takes the by-name path and must not overwrite values the
    // program may have changed in the module's TLS block.
    let again = loader.dlopen(b"libtls.so", 0).expect("second open");
    assert_eq!(again, id);
    assert_eq!(loader.applier.tls_init_count(id.0), 1);
}

#[test]
fn dlclose_keeps_a_tls_object_after_one_handle_closes() {
    let _guard = HOST_LOCK.lock().expect("lock");
    let Some((mut loader, _dir)) = loader() else {
        return;
    };
    let local = loader.dlopen(b"libtls.so", 0).expect("local");
    let global = loader.dlopen(b"libtls.so", RTLD_GLOBAL).expect("global");
    assert_eq!(local, global);
    assert_eq!(loader.object(local).expect("object").refcount, 2);
    assert_eq!(loader.dlclose(global), 0);
    let object = loader.object(local).expect("object stays after one close");
    assert_eq!(object.refcount, 1);
    assert!(object.has_tls());
    assert!(loader.dlsym(Some(local), b"tls_marker", 0).is_some());
    assert_eq!(loader.dlclose(local), 0);
    // A static-TLS object stays mapped after its last close; the thread may still hold a
    // descriptor offset into its block.
    let object = loader.object(local).expect("TLS object stays mapped");
    assert!(object.has_tls());
}

#[test]
fn dlclose_runs_fini_array_in_reverse_order() {
    let _guard = HOST_LOCK.lock().expect("lock");
    let Some((mut loader, _dir)) = loader() else {
        return;
    };
    let id = loader.dlopen(b"libfini.so", 0).expect("load");
    let fini_a = loader.dlsym(Some(id), b"fini_a", 0).expect("fini_a") as u64;
    let fini_b = loader.dlsym(Some(id), b"fini_b", 0).expect("fini_b") as u64;
    assert_eq!(loader.dlclose(id), 0);
    assert!(loader.object(id).is_none());
    let calls: Vec<u64> = loader
        .platform
        .events()
        .into_iter()
        .filter_map(|event| match event {
            Event::Call(address) => Some(address),
            _ => None,
        })
        .collect();
    assert_eq!(calls, vec![fini_b, fini_a]);
}

#[test]
fn dlerror_is_returned_once_and_cleared_by_success() {
    let _guard = HOST_LOCK.lock().expect("lock");
    let Some((mut loader, _dir)) = loader() else {
        return;
    };
    assert!(loader.dlopen(b"missing.so", 0).is_err());
    assert!(loader.dlerror().is_some());
    assert!(loader.dlerror().is_none());
    let _ = loader.dlopen(b"libval.so", 0).expect("success");
    assert!(loader.dlerror().is_none());
}

#[test]
fn rtdl_next_searches_after_the_caller_object() {
    let _guard = HOST_LOCK.lock().expect("lock");
    let Some((mut loader, dir)) = loader() else {
        return;
    };
    let val = load(&mut loader, &dir, "libval.so");
    let caller = loader.dlsym(Some(val), b"val", 0).expect("val");
    let deep = loader
        .dlsym(Some(RTLD_NEXT), b"deep_fn", caller)
        .expect("next");
    assert_ne!(deep, 0);
}

#[test]
fn dl_phdr_info_has_the_c_shape_and_main_first_order() {
    let _guard = HOST_LOCK.lock().expect("lock");
    let Some((mut loader, dir)) = loader() else {
        return;
    };
    let main = load(&mut loader, &dir, "libval.so");
    loader.scope = raven_libc_elf::Scope::new(main);
    let info = loader.dl_phdr_info(main).expect("main info");
    assert_ne!(info.dlpi_addr, 0);
    assert_ne!(info.dlpi_phnum, 0);
    assert!(!info.dlpi_name.is_null());
    let mut count = 0usize;
    let result = loader.iterate_phdr_info(|_info| {
        count = count.saturating_add(1);
        if count == 1 { 0 } else { 7 }
    });
    assert_eq!(result, 7);
    assert_eq!(count, 2);
}

#[test]
fn a_premapped_library_keeps_its_own_dl_phdr_name() {
    let _guard = HOST_LOCK.lock().expect("lock");
    let Some((mut loader, dir)) = loader() else {
        return;
    };
    let mapped = load(&mut loader, &dir, "libval.so");
    let (phdr, phnum) = {
        let object = loader.object(mapped).expect("mapped");
        (object.view.phdr.0, object.view.phdr.1)
    };
    let id =
        register_premapped(&mut loader, phdr, phnum, b"libc.so.1").expect("register premapped");
    let info = loader.dl_phdr_info(id).expect("premapped info");
    // SAFETY: `dlpi_name` points at the object's stored NUL-terminated name bytes.
    let name = unsafe { std::ffi::CStr::from_ptr(info.dlpi_name.cast()) };
    assert_eq!(name.to_bytes(), b"libc.so.1");
}

#[test]
fn dl_iterate_phdr_keeps_main_first_and_counts_adds_and_subs() {
    let _guard = HOST_LOCK.lock().expect("lock");
    let Some((mut loader, dir)) = loader() else {
        return;
    };
    let main = load(&mut loader, &dir, "libval.so");
    loader.scope = raven_libc_elf::Scope::new(main);
    let ids: Vec<_> = loader.iterate_ids().into_iter().flatten().collect();
    assert_eq!(ids.first(), Some(&main));
    assert_eq!(ids.len(), 2);
    assert_eq!(loader.adds, 2);
    let mut visited = 0usize;
    let walked = loader.iterate_phdr(|_| {
        visited = visited.saturating_add(1);
        false
    });
    assert_eq!(walked, 1);
    assert_eq!(visited, 1);
    assert_eq!(
        loader.initial_order().first().copied().flatten(),
        ids.get(1).copied()
    );
    let _ = loader.dlclose(main);
    assert!(loader.subs > 0);
}

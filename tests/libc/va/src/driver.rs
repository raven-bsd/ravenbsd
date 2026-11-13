// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The generated-C differential driver: one C adapter per argument SHAPE.
//!
//! Stable Rust can CALL a C variadic function only with a compile-time argument list, and
//! only a C compiler lowers a call exactly as a real caller would. The ticket's mechanism is
//! "one C function per case"; this module emits one C function per argument *shape* and passes
//! the format string and the values at run time, which exercises the same calling convention
//! (the shape fixes where every argument travels) with a driver that compiles in seconds
//! instead of minutes. The deviation is recorded as `RD-new 9` in the report.
//!
//! Every shape has two functions: `ref_<shape>` calls the host glibc `snprintf` and
//! `ours_<shape>` calls our trampoline through a `raven_ours_fn` function pointer. Both get
//! the same format and the same values; the test compares the return value and every byte of
//! the buffer.

#![allow(dead_code)] // included by several test binaries, each using a subset

use std::ffi::{CString, c_char, c_int, c_void};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::vec::Vec;

/// Every build gets its own file names: two tests may load a driver at the same time, and a
/// shared object must never be rewritten while another handle maps it.
static BUILD_SERIAL: AtomicU64 = AtomicU64::new(0);

/// The function pointer type of our trampoline as the C driver sees it.
pub type OursFn = usize;

unsafe extern "C" {
    fn dlopen(filename: *const c_char, flags: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    fn dlerror() -> *const c_char;
}

const RTLD_NOW: c_int = 2;

/// The C source of the driver: the prelude plus one reference and one "ours" function per
/// shape.
#[must_use]
pub fn generate_c() -> String {
    let mut source = String::new();
    source.push_str("#include \"driver_prelude.h\"\n\n");

    // Integer shapes: k int arguments, 0..=12.
    source.push_str("int ref_i32(const char *f, char *b, size_t n, const int *v, int k) {\n");
    source.push_str("  switch (k) {\n");
    for k in 0..=12 {
        source.push_str(&format!(
            "  case {k}: return snprintf(b, n, f{});\n",
            int_args(k, "")
        ));
    }
    source.push_str("  }\n  return -1;\n}\n\n");
    source.push_str(
        "int ours_i32(unsigned long fn, const char *f, char *b, size_t n, const int *v, int k) {\n",
    );
    source.push_str("  raven_ours_fn p = (raven_ours_fn)fn;\n  switch (k) {\n");
    for k in 0..=12 {
        source.push_str(&format!(
            "  case {k}: return p(b, n, f{});\n",
            int_args(k, "")
        ));
    }
    source.push_str("  }\n  return -1;\n}\n\n");

    // Long shapes: k long arguments, 1..=12 (0 is covered by i32).
    source.push_str("int ref_i64(const char *f, char *b, size_t n, const long *v, int k) {\n");
    source.push_str("  switch (k) {\n");
    for k in 1..=12 {
        source.push_str(&format!(
            "  case {k}: return snprintf(b, n, f{});\n",
            int_args(k, "l")
        ));
    }
    source.push_str("  }\n  return -1;\n}\n\n");
    source.push_str(
        "int ours_i64(unsigned long fn, const char *f, char *b, size_t n, const long *v, int k) {\n",
    );
    source.push_str("  raven_ours_fn p = (raven_ours_fn)fn;\n  switch (k) {\n");
    for k in 1..=12 {
        source.push_str(&format!(
            "  case {k}: return p(b, n, f{});\n",
            int_args(k, "l")
        ));
    }
    source.push_str("  }\n  return -1;\n}\n\n");

    // Double shapes: k doubles from their bit images, 0..=12.
    source.push_str(
        "int ref_dbl(const char *f, char *b, size_t n, const unsigned long long *v, int k) {\n",
    );
    source.push_str("  switch (k) {\n");
    for k in 0..=12 {
        source.push_str(&format!(
            "  case {k}: return snprintf(b, n, f{});\n",
            dbl_args(k, "r")
        ));
    }
    source.push_str("  }\n  return -1;\n}\n\n");
    source.push_str("int ours_dbl(unsigned long fn, const char *f, char *b, size_t n, const unsigned long long *v, int k) {\n");
    source.push_str("  raven_ours_fn p = (raven_ours_fn)fn;\n  switch (k) {\n");
    for k in 0..=12 {
        source.push_str(&format!(
            "  case {k}: return p(b, n, f{});\n",
            dbl_args(k, "r")
        ));
    }
    source.push_str("  }\n  return -1;\n}\n\n");

    // Mixed shapes: the register/stack boundary cases of the ticket (6+6, 6+8, 7+9).
    for (name, ni, nd) in [("mix6", 6usize, 6usize), ("mix68", 6, 8), ("mix79", 7, 9)] {
        let mut args = String::new();
        for index in 0..ni {
            args.push_str(&format!(", (int)iv[{index}]"));
        }
        for index in 0..nd {
            args.push_str(&format!(", raven_double_from_bits(dv[{index}])"));
        }
        source.push_str(&format!(
            "int ref_{name}(const char *f, char *b, size_t n, const long *iv, const unsigned long long *dv) {{\n  return snprintf(b, n, f{args});\n}}\n"
        ));
        source.push_str(&format!(
            "int ours_{name}(unsigned long fn, const char *f, char *b, size_t n, const long *iv, const unsigned long long *dv) {{\n  raven_ours_fn p = (raven_ours_fn)fn;\n  return p(b, n, f{args});\n}}\n\n"
        ));
    }

    // Long double at every position among six integers.
    source.push_str("int ref_ld(const char *f, char *b, size_t n, const long *iv, int pos, const unsigned char *ld) {\n");
    source.push_str("  long double v = raven_ld_from_bits(*(const unsigned long long *)ld, *(const unsigned short *)(ld + 8));\n");
    source.push_str("  switch (pos) {\n");
    for position in 0..=6usize {
        let mut args = String::new();
        for index in 0..6usize {
            if index == position {
                args.push_str(", v");
            }
            args.push_str(&format!(", (int)iv[{index}]"));
        }
        if position == 6 {
            args.push_str(", v");
        }
        source.push_str(&format!(
            "  case {position}: return snprintf(b, n, f{args});\n"
        ));
    }
    source.push_str("  }\n  return -1;\n}\n\n");
    source.push_str("int ours_ld(unsigned long fn, const char *f, char *b, size_t n, const long *iv, int pos, const unsigned char *ld) {\n");
    source.push_str("  raven_ours_fn p = (raven_ours_fn)fn;\n");
    source.push_str("  long double v = raven_ld_from_bits(*(const unsigned long long *)ld, *(const unsigned short *)(ld + 8));\n");
    source.push_str("  switch (pos) {\n");
    for position in 0..=6usize {
        let mut args = String::new();
        for index in 0..6usize {
            if index == position {
                args.push_str(", v");
            }
            args.push_str(&format!(", (int)iv[{index}]"));
        }
        if position == 6 {
            args.push_str(", v");
        }
        source.push_str(&format!("  case {position}: return p(b, n, f{args});\n"));
    }
    source.push_str("  }\n  return -1;\n}\n\n");

    // One pointer argument (`%p`, `%s`).
    source.push_str("int ref_ptr(const char *f, char *b, size_t n, const void *p) {\n  return snprintf(b, n, f, p);\n}\n");
    source.push_str("int ours_ptr(unsigned long fn, const char *f, char *b, size_t n, const void *p) {\n  raven_ours_fn q = (raven_ours_fn)fn;\n  return q(b, n, f, p);\n}\n\n");

    // `%n`: one int argument, then the `%n` destination.
    source.push_str("int ref_pctn(const char *f, char *b, size_t n, int value, int *out) {\n  return snprintf(b, n, f, value, out);\n}\n");
    source.push_str("int ours_pctn(unsigned long fn, const char *f, char *b, size_t n, int value, int *out) {\n  raven_ours_fn q = (raven_ours_fn)fn;\n  return q(b, n, f, value, out);\n}\n\n");

    // The integer-class struct-by-value subset of C59 section 4.5. `t_rec` takes the size as
    // its one named argument and reads the aggregate through the reader.
    for size in [8usize, 12, 16, 24, 32] {
        let members = struct_members(size);
        source.push_str(&format!(
            "struct raven_s{size} {{ {members} }};\nint ours_agg{size}(unsigned long fn, const unsigned char *v) {{\n  raven_rec_fn q = (raven_rec_fn)fn;\n  struct raven_s{size} s;\n  memcpy(&s, v, sizeof s);\n  return q({size}, s);\n}}\n\n"
        ));
    }

    source
}

/// `, (int)v[0], (int)v[1], ...` for the integer shapes.
fn int_args(count: usize, suffix: &str) -> String {
    let mut args = String::new();
    for index in 0..count {
        if suffix.is_empty() {
            args.push_str(&format!(", (int)v[{index}]"));
        } else {
            args.push_str(&format!(", v[{index}]"));
        }
    }
    args
}

/// `, raven_double_from_bits(v[0]), ...` for the double shapes.
fn dbl_args(count: usize, _suffix: &str) -> String {
    let mut args = String::new();
    for index in 0..count {
        args.push_str(&format!(", raven_double_from_bits(v[{index}])"));
    }
    args
}

/// The members of the aggregate of `size` bytes: all integer, 8-byte aligned.
fn struct_members(size: usize) -> String {
    match size {
        8 => "long a;".to_string(),
        12 => "int a; int b; int c;".to_string(),
        16 => "long a; long b;".to_string(),
        24 => "long a; long b; long c;".to_string(),
        _ => "long a; long b; long c; long d;".to_string(),
    }
}

/// The C driver, loaded through `dlopen`/`dlsym` (declared here, no `libloading`).
pub struct Driver {
    handle: *mut c_void,
}

// SAFETY: the shared object is immutable after `dlopen` and the handle is only used for
// `dlsym` lookups and the calls the tests make.
unsafe impl Send for Driver {}
// SAFETY: as above.
unsafe impl Sync for Driver {}

impl Driver {
    /// Compile the generated driver and load it.
    ///
    /// Returns `Ok(None)` when the system C compiler is absent (the ticket's documented
    /// SKIP), and `Err` when the compiler or the loader fails.
    pub fn build() -> Result<Option<Self>, String> {
        Self::build_source(&generate_c())
    }

    /// Compile a given driver source and load it.
    ///
    /// Returns `Ok(None)` when the system C compiler is absent (the ticket's documented
    /// SKIP), and `Err` when the compiler or the loader fails.
    pub fn build_source(source: &str) -> Result<Option<Self>, String> {
        Self::build_source_with(source, &[])
    }

    /// Compile a given driver source with extra compiler flags and load it.
    pub fn build_source_with(source: &str, flags: &[&str]) -> Result<Option<Self>, String> {
        let directory = workdir()?;
        std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
        let serial = BUILD_SERIAL.fetch_add(1, Ordering::Relaxed);
        let hash = hash64(source.as_bytes()) ^ serial.wrapping_mul(0x9E37_79B9_7F4A_7C15);
        let path = directory.join(format!("driver_{hash:016x}.c"));
        std::fs::write(&path, source.as_bytes()).map_err(|error| error.to_string())?;
        let library = directory.join(format!("libdriver_{hash:016x}.so"));
        let compiler = std::env::var("CC").unwrap_or_else(|_| "clang".to_string());
        let include = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("c");
        let mut command = Command::new(&compiler);
        command.arg("-O1").arg("-fPIC").arg("-shared").arg("-w");
        for flag in flags {
            command.arg(flag);
        }
        let output = command
            .arg("-I")
            .arg(&include)
            .arg(&path)
            .arg("-o")
            .arg(&library)
            .output();
        let output = match output {
            Ok(output) => output,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(format!("cannot run {compiler}: {error}")),
        };
        if !output.status.success() {
            return Err(format!(
                "{compiler} failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        let name = CString::new(library.to_string_lossy().as_bytes()).map_err(|e| e.to_string())?;
        // SAFETY: `name` is a valid NUL-terminated path.
        let handle = unsafe { dlopen(name.as_ptr(), RTLD_NOW) };
        if handle.is_null() {
            // SAFETY: `dlerror` returns a NUL-terminated string or NULL.
            let message = unsafe {
                let pointer = dlerror();
                if pointer.is_null() {
                    String::from("dlopen failed")
                } else {
                    std::ffi::CStr::from_ptr(pointer)
                        .to_string_lossy()
                        .into_owned()
                }
            };
            return Err(message);
        }
        Ok(Some(Self { handle }))
    }

    /// Look up one symbol.
    #[must_use]
    pub fn sym(&self, name: &str) -> *mut c_void {
        let Ok(name) = CString::new(name) else {
            return std::ptr::null_mut();
        };
        // SAFETY: the handle is live and the name is a NUL-terminated string.
        unsafe { dlsym(self.handle, name.as_ptr()) }
    }

    /// Transmute one symbol to a function pointer type.
    ///
    /// # Safety
    ///
    /// `T` must be the exact signature of the C function `name`.
    #[must_use]
    pub unsafe fn func<T: Copy>(&self, name: &str) -> Option<T> {
        let pointer = self.sym(name);
        if pointer.is_null() {
            return None;
        }
        // SAFETY: the caller guarantees that `T` matches the C symbol.
        Some(unsafe { core::mem::transmute_copy(&pointer) })
    }
}

fn workdir() -> Result<std::path::PathBuf, String> {
    if let Ok(directory) = std::env::var("CARGO_TARGET_TMPDIR") {
        return Ok(std::path::PathBuf::from(directory).join("va-driver"));
    }
    // The workspace `target/` directory: generated C and shared objects are never inside the
    // scanned source tree (they are git-ignored and `libc-lint` does not see them).
    let manifest = std::env::var("CARGO_MANIFEST_DIR").map_err(|error| error.to_string())?;
    let root = std::path::Path::new(&manifest)
        .ancestors()
        .nth(3)
        .ok_or_else(|| "cannot find the workspace root".to_string())?;
    Ok(root.join("target/va-driver"))
}

fn hash64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[cfg(test)]
#[path = "driver_ut.rs"]
mod tests;

// ---------------------------------------------------------------------------------------
// One C function per case (the K03 corpus and other heterogeneous lists)
// ---------------------------------------------------------------------------------------

/// One argument of a per-case driver function.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CaseArg {
    /// `int`
    I32(i32),
    /// `unsigned int`
    U32(u32),
    /// `long`
    I64(i64),
    /// `unsigned long long`
    U64(u64),
    /// A `double` given by its bit image.
    F64(u64),
    /// A `long double` given by its 128-bit little-endian image (x86_64: 80-bit).
    LD(u128),
    /// A C string literal.
    Str(String),
    /// A NULL `const char *`.
    Null,
    /// A pointer value.
    Ptr(usize),
}

impl CaseArg {
    /// The C expression that passes this argument.
    #[must_use]
    pub fn c_expr(&self) -> String {
        match self {
            Self::I32(value) => {
                if *value == i32::MIN {
                    "(-2147483647-1)".to_string()
                } else {
                    format!("{value}")
                }
            }
            Self::U32(value) => format!("{value}U"),
            Self::I64(value) => {
                if *value == i64::MIN {
                    "(-9223372036854775807L-1L)".to_string()
                } else {
                    format!("{value}L")
                }
            }
            Self::U64(value) => format!("{value}ULL"),
            Self::F64(bits) => format!("raven_double_from_bits(0x{bits:016x}ULL)"),
            Self::LD(bits) => format!(
                "raven_ld_from_bits(0x{:016x}ULL, 0x{:04x})",
                (*bits as u64),
                ((*bits >> 64) as u16)
            ),
            Self::Str(text) => {
                let escaped = text.replace('\\', "\\\\").replace('"', "\\\"");
                format!("\"{escaped}\"")
            }
            Self::Null => "(const char *)0".to_string(),
            Self::Ptr(value) => format!("(const void *)0x{value:x}UL"),
        }
    }
}

/// One differential case with a C-level argument list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Case {
    /// The `printf` format.
    pub fmt: String,
    /// The variadic arguments, in order.
    pub args: Vec<CaseArg>,
}

impl Case {
    /// Build a case.
    #[must_use]
    pub fn new(fmt: &str, args: Vec<CaseArg>) -> Self {
        Self {
            fmt: fmt.to_string(),
            args,
        }
    }

    fn c_args(&self) -> String {
        let mut out = String::new();
        for argument in &self.args {
            out.push_str(", ");
            out.push_str(&argument.c_expr());
        }
        out
    }

    fn c_format(&self) -> String {
        format!(
            "\"{}\"",
            self.fmt.replace('\\', "\\\\").replace('"', "\\\"")
        )
    }
}

/// The C source of one reference and one "ours" function per case.
#[must_use]
pub fn generate_cases(cases: &[Case]) -> String {
    let mut source = String::from("#include \"driver_prelude.h\"\n\n");
    for (index, case) in cases.iter().enumerate() {
        let format = case.c_format();
        let args = case.c_args();
        source.push_str(&format!(
            "int kref_{index}(char *b, size_t n) {{\n  return snprintf(b, n, {format}{args});\n}}\n"
        ));
        source.push_str(&format!(
            "int kours_{index}(unsigned long fn, char *b, size_t n) {{\n  raven_ours_fn p = (raven_ours_fn)fn;\n  return p(b, n, {format}{args});\n}}\n\n"
        ));
    }
    source
}

/// Type of a generated reference function.
pub type RefCaseFn = unsafe extern "C" fn(*mut u8, usize) -> i32;
/// Type of a generated "ours" function.
pub type OursCaseFn = unsafe extern "C" fn(usize, *mut u8, usize) -> i32;

/// A driver built from a case list.
pub struct CaseDriver {
    driver: Driver,
    references: Vec<RefCaseFn>,
    ours: Vec<OursCaseFn>,
}

impl CaseDriver {
    /// Compile and load the per-case driver.
    ///
    /// Returns `Ok(None)` when the system C compiler is absent.
    pub fn build(cases: &[Case]) -> Result<Option<Self>, String> {
        let source = generate_cases(cases);
        let Some(driver) = Driver::build_source(&source)? else {
            return Ok(None);
        };
        let mut references = Vec::with_capacity(cases.len());
        let mut ours = Vec::with_capacity(cases.len());
        for index in 0..cases.len() {
            // SAFETY: `kref_N` and `kours_N` are generated with exactly these signatures.
            let reference: RefCaseFn = unsafe { driver.func(&format!("kref_{index}")) }
                .ok_or_else(|| format!("missing kref_{index}"))?;
            // SAFETY: as above.
            let call: OursCaseFn = unsafe { driver.func(&format!("kours_{index}")) }
                .ok_or_else(|| format!("missing kours_{index}"))?;
            references.push(reference);
            ours.push(call);
        }
        Ok(Some(Self {
            driver,
            references,
            ours,
        }))
    }

    /// Run the glibc reference of case `index`.
    pub fn reference(&self, index: usize, buffer: &mut [u8], capacity: usize) -> i32 {
        // SAFETY: the generated reference function takes a writable buffer and a size.
        unsafe { (self.references[index])(buffer.as_mut_ptr(), capacity) }
    }

    /// Run case `index` twice: the glibc reference and our trampoline.
    pub fn compare(
        &self,
        index: usize,
        ours_fn: usize,
        buffer: &mut [u8],
        capacity: usize,
    ) -> (i32, i32) {
        let mut reference_buffer = vec![0u8; buffer.len()];
        // SAFETY: the generated functions take a writable buffer and a size; the case's
        // arguments are baked into the C function.
        let reference =
            unsafe { (self.references[index])(reference_buffer.as_mut_ptr(), capacity) };
        // SAFETY: as above, through the trampoline's function pointer.
        let ours = unsafe { (self.ours[index])(ours_fn, buffer.as_mut_ptr(), capacity) };
        for (left, right) in reference_buffer.iter().zip(buffer.iter()) {
            assert_eq!(left, right, "case {index}: buffer bytes");
        }
        let _ = &self.driver;
        (reference, ours)
    }
}

// ---------------------------------------------------------------------------------------
// One C function per scanf case
// ---------------------------------------------------------------------------------------

/// One `sscanf` differential case: an input, a format and a conversion count.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScanCase {
    /// The input text.
    pub input: String,
    /// The format; it holds exactly `ints` integer conversions plus an optional `%n`.
    pub fmt: String,
    /// The number of `int` destinations the format consumes.
    pub ints: usize,
    /// True when the format ends with `%n`.
    pub count: bool,
}

impl ScanCase {
    /// The `sscanf` argument list (destinations) for this case.
    fn destinations(&self) -> String {
        let mut out = String::new();
        for index in 0..self.ints {
            out.push_str(&format!("&v{index}, "));
        }
        if self.count {
            out.push_str("&n, ");
        }
        out.trim_end_matches(' ').trim_end_matches(',').to_string()
    }

    fn result_slots(&self) -> String {
        let mut out = String::from("out[0] = r; out[1] = n;");
        for index in 0..3 {
            out.push_str(&format!(" out[{}] = v{index};", 2 + index));
        }
        out
    }
}

/// The C source of the scanf case driver.
#[must_use]
pub fn generate_scan_cases(cases: &[ScanCase]) -> String {
    let mut source = String::from("#include \"driver_prelude.h\"\n\n");
    for (index, case) in cases.iter().enumerate() {
        let input = case.input.replace('\\', "\\\\").replace('"', "\\\"");
        let fmt = case.fmt.replace('\\', "\\\\").replace('"', "\\\"");
        let destinations = case.destinations();
        let slots = case.result_slots();
        source.push_str(&format!(
            "int sref_{index}(const char *in, int *out) {{\n  int v0 = 0, v1 = 0, v2 = 0, n = -1;\n  int r = sscanf(in, \"{fmt}\", {destinations});\n  {slots}\n  (void)v0; (void)v1; (void)v2; (void)n;\n  return r;\n}}\n"
        ));
        source.push_str(&format!(
            "int sours_{index}(unsigned long fn, const char *in, int *out) {{\n  typedef int (*scan_fn)(const char *, const char *, ...);\n  scan_fn p = (scan_fn)fn;\n  int v0 = 0, v1 = 0, v2 = 0, n = -1;\n  int r = p(in, \"{fmt}\", {destinations});\n  {slots}\n  (void)v0; (void)v1; (void)v2; (void)n;\n  return r;\n}}\n\n"
        ));
        let _ = input;
    }
    source
}

/// Type of a generated scanf reference function.
pub type ScanRefFn = unsafe extern "C" fn(*const u8, *mut i32) -> i32;
/// Type of a generated scanf "ours" function.
pub type ScanOursFn = unsafe extern "C" fn(usize, *const u8, *mut i32) -> i32;

/// A scanf driver built from a case list.
pub struct ScanDriver {
    references: Vec<ScanRefFn>,
    ours: Vec<ScanOursFn>,
}

impl ScanDriver {
    /// Compile and load the per-case scanf driver.
    pub fn build(cases: &[ScanCase]) -> Result<Option<Self>, String> {
        let source = generate_scan_cases(cases);
        let Some(driver) = Driver::build_source(&source)? else {
            return Ok(None);
        };
        let mut references = Vec::with_capacity(cases.len());
        let mut ours = Vec::with_capacity(cases.len());
        for index in 0..cases.len() {
            // SAFETY: `sref_N` and `sours_N` are generated with exactly these signatures.
            let reference: ScanRefFn = unsafe { driver.func(&format!("sref_{index}")) }
                .ok_or_else(|| format!("missing sref_{index}"))?;
            // SAFETY: as above.
            let call: ScanOursFn = unsafe { driver.func(&format!("sours_{index}")) }
                .ok_or_else(|| format!("missing sours_{index}"))?;
            references.push(reference);
            ours.push(call);
        }
        Ok(Some(Self { references, ours }))
    }

    /// Run case `index` twice: glibc `sscanf` and our trampoline; compare five result slots.
    pub fn compare(&self, index: usize, ours_fn: usize, input: &[u8], label: &str) -> (i32, i32) {
        let mut reference_out = [0i32; 5];
        let mut ours_out = [0i32; 5];
        // SAFETY: the generated functions take a NUL-terminated input and five int slots.
        let reference =
            unsafe { (self.references[index])(input.as_ptr(), reference_out.as_mut_ptr()) };
        // SAFETY: as above, through the trampoline.
        let ours = unsafe { (self.ours[index])(ours_fn, input.as_ptr(), ours_out.as_mut_ptr()) };
        assert_eq!(
            ours_out, reference_out,
            "scan case {index} [{label}]: stored values"
        );
        (reference, ours)
    }
}

/// A driver compiled with `-msoft-float -mno-sse -mno-mmx`: a soft-float caller of our
/// trampolines (C59 section 7.1, the soft-float lane).
pub struct SoftDriver {
    ours: Vec<OursCaseFn>,
}

impl SoftDriver {
    /// Compile the same per-case source as a soft-float shared object.
    pub fn build(cases: &[Case]) -> Result<Option<Self>, String> {
        let source = generate_cases(cases);
        let flags = [
            "--target=x86_64-unknown-linux-gnu",
            "-msoft-float",
            "-mno-sse",
            "-mno-mmx",
        ];
        let Some(driver) = Driver::build_source_with(&source, &flags)? else {
            return Ok(None);
        };
        let mut ours = Vec::with_capacity(cases.len());
        for index in 0..cases.len() {
            // SAFETY: `kours_N` is generated with exactly this signature.
            let call: OursCaseFn = unsafe { driver.func(&format!("kours_{index}")) }
                .ok_or_else(|| format!("missing kours_{index}"))?;
            ours.push(call);
        }
        Ok(Some(Self { ours }))
    }

    /// Run case `index` through the soft-float caller.
    pub fn run(&self, index: usize, ours_fn: usize, buffer: &mut [u8], capacity: usize) -> i32 {
        // SAFETY: the generated function takes the trampoline, a writable buffer and a size.
        unsafe { (self.ours[index])(ours_fn, buffer.as_mut_ptr(), capacity) }
    }
}

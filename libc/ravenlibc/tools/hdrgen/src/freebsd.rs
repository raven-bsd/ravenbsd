// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The fail-closed FreeBSD profile oracle (DR-69).
//!
//! Expected values are measured from the pinned FreeBSD donor headers, never from Raven's
//! TOML. Actual values are measured from Raven's public C header tree and read from the
//! generated Rust ABI. Oracle integrity (inputs, compiler, non-empty coverage) is reported
//! separately from Raven conformance (the named red difference inventory).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};

use crate::model::{ConstDef, RecordDef};
use crate::probe::{Report, copy_tree, parse_assembly, run_command, same_mod_64};
use crate::validate::{Tables, Values};

/// Architectures the FreeBSD profile covers (DR-69).
pub const FREEBSD_ARCHES: [&str; 2] = ["x86_64", "aarch64"];

/// Every selectable family: the fourteen constant groups plus types and record layouts.
pub const FAMILIES: [&str; 16] = [
    "errno", "signal", "fcntl", "mman", "stat", "socket", "netinet", "poll", "time", "resource",
    "limits", "sysconf", "pthread", "misc", "types", "records",
];

/// Constants compared through a compiled runtime accessor instead of a literal value.
const RUNTIME_ACCESSORS: [&str; 1] = ["CLK_TCK"];

/// Oracle inputs.
pub struct Options<'a> {
    /// FreeBSD source checkout or a prepared include root.
    pub donor: &'a Path,
    /// Revision pinned by `donors.lock`.
    pub revision: &'a str,
    /// Product clone root.
    pub root: &'a Path,
    /// `abi/` directory.
    pub abi: &'a Path,
    /// `all`, `x86_64` or `aarch64`.
    pub target: &'a str,
    /// `all` or a comma-separated family list.
    pub families: &'a str,
    /// C compiler.
    pub clang: &'a str,
    /// Optional deterministic inventory output.
    pub report: Option<&'a Path>,
}

/// A family-selected conformance requirement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    /// Families that gate the exit status.
    pub families: BTreeSet<String>,
    /// Whether every family was selected explicitly or by `all`.
    pub all: bool,
}

impl Selection {
    /// Parse `all` or a comma-separated family list; empty or misspelled input fails.
    pub fn parse(text: &str) -> Result<Self> {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            bail!("--families must name at least one family");
        }
        if trimmed == "all" {
            return Ok(Self {
                families: FAMILIES.iter().map(|name| (*name).to_string()).collect(),
                all: true,
            });
        }
        let mut families = BTreeSet::new();
        for name in trimmed.split(',') {
            let name = name.trim();
            if name.is_empty() {
                bail!("--families contains an empty entry");
            }
            if !FAMILIES.contains(&name) {
                bail!(
                    "unknown family {name:?}; expected one of {} or all",
                    FAMILIES.join(", ")
                );
            }
            families.insert(name.to_string());
        }
        Ok(Self {
            families,
            all: false,
        })
    }

    fn wants(&self, family: &str) -> bool {
        self.families.contains(family)
    }
}

/// The target architectures of one run; `all` means both FreeBSD arches.
pub fn targets(text: &str) -> Result<Vec<&'static str>> {
    match text {
        "all" => Ok(FREEBSD_ARCHES.to_vec()),
        "x86_64" => Ok(vec!["x86_64"]),
        "aarch64" => Ok(vec!["aarch64"]),
        other => bail!("unknown FreeBSD --target {other:?}; expected x86_64, aarch64 or all"),
    }
}

/// One measured record field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldFacts {
    /// Field name.
    pub name: String,
    /// Byte offset.
    pub offset: i64,
    /// Donor C spelling of the field type.
    pub ty: String,
    /// `sizeof` of the field type when measured.
    pub size: Option<i64>,
    /// Signedness when the field type is arithmetic.
    pub signed: Option<bool>,
    /// C bitfield; `offsetof` and `sizeof` do not apply.
    pub bitfield: bool,
}

/// One measured record layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordFacts {
    /// Size in bytes.
    pub size: i64,
    /// Alignment in bytes.
    pub align: i64,
    /// Named fields in declaration order.
    pub fields: Vec<FieldFacts>,
}

/// One measured scalar type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TypeFacts {
    /// Size in bytes.
    pub size: i64,
    /// Alignment in bytes.
    pub align: i64,
    /// Whether the type is signed.
    pub signed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Value {
    Int(i128),
    Signed(bool),
    Text(String),
}

impl Value {
    fn render(&self) -> String {
        match self {
            Self::Int(value) => value.to_string(),
            Self::Signed(signed) => signed.to_string(),
            Self::Text(text) => text.clone(),
        }
    }
}

/// One named red (or approved) difference.
#[derive(Debug, Clone)]
struct Diff {
    /// Gating family.
    pub family: String,
    /// Difference kind.
    pub kind: &'static str,
    /// Constant, type or record name (`record.field` for a field).
    pub name: String,
    /// Architecture.
    pub arch: String,
    /// Donor-derived expectation.
    pub expected: Value,
    /// Raven actual.
    pub actual: Value,
    /// Detail for the inventory.
    pub detail: String,
    /// Whether `deviations.toml` approves this exact row.
    pub approved: bool,
}

/// The oracle result.
#[derive(Debug, Default)]
struct Inventory {
    /// Oracle integrity failures: missing inputs, compiler failure, empty coverage.
    pub integrity: Vec<String>,
    /// Named differences.
    pub diffs: Vec<Diff>,
    /// Measured coverage counters.
    pub coverage: BTreeMap<String, usize>,
    /// Public types whose field-level comparison is not yet wired to a donor tag.
    pub uncovered: Vec<String>,
    /// Compiler identity (`clang --version` first line).
    pub compiler: String,
    /// Feature macros used for the donor probes.
    pub features: Vec<String>,
    /// Every include root read for the run.
    pub include_roots: Vec<String>,
}

#[derive(Debug, Default)]
struct Side {
    constants: BTreeMap<String, i128>,
    absent: BTreeSet<String>,
    unevaluated: BTreeMap<String, String>,
    records: BTreeMap<String, RecordFacts>,
    record_errors: BTreeMap<String, String>,
    types: BTreeMap<String, TypeFacts>,
    type_errors: BTreeMap<String, String>,
    runtime: BTreeMap<String, Result<(), String>>,
}

struct Includes {
    clang: String,
    target: String,
    resource: PathBuf,
    dirs: Vec<PathBuf>,
    defines: Vec<String>,
}

impl Includes {
    fn new(clang: &str, target: &str, dirs: Vec<PathBuf>, defines: Vec<String>) -> Result<Self> {
        let resource = crate::probe::resource_include(clang)?;
        Ok(Self {
            clang: clang.to_string(),
            target: target.to_string(),
            resource,
            dirs,
            defines,
        })
    }

    fn compile(&self, source: &Path, assembly: bool) -> Result<String> {
        let mut args = vec![
            format!("--target={}", self.target),
            "-nostdinc".to_string(),
            "-isystem".to_string(),
            self.resource.display().to_string(),
        ];
        for dir in &self.dirs {
            args.push("-isystem".to_string());
            args.push(dir.display().to_string());
        }
        for define in &self.defines {
            args.push(format!("-D{define}"));
        }
        if assembly {
            args.push("-S".to_string());
            args.push("-o".to_string());
            args.push("-".to_string());
        } else {
            args.push("-Xclang".to_string());
            args.push("-fdump-record-layouts".to_string());
            args.push("-c".to_string());
            args.push("-o".to_string());
            args.push("/dev/null".to_string());
        }
        args.push(source.display().to_string());
        run_command(&self.clang, &args)
    }
}

fn git(donor: &Path, args: &[&str]) -> Result<String> {
    let donor = donor.to_str().context("donor path is not UTF-8")?;
    let mut full = vec!["-C", donor, "-c", "gc.auto=0"];
    full.extend(args.iter().copied());
    run_command(
        "git",
        &full
            .iter()
            .map(|value| (*value).to_string())
            .collect::<Vec<_>>(),
    )
}

fn donor_head(donor: &Path) -> Result<String> {
    Ok(git(donor, &["rev-parse", "HEAD"])?.trim().to_string())
}

const TREE_PATHS: [&str; 9] = [
    "include",
    "sys/sys",
    "sys/netinet",
    "sys/netinet6",
    "sys/net",
    "sys/amd64/include",
    "sys/x86/include",
    "sys/arm64/include",
    "sys/arm/include",
];

fn git_list(donor: &Path) -> Result<Vec<String>> {
    let mut args = vec!["ls-tree", "-r", "--name-only", "-z", "HEAD", "--"];
    args.extend(TREE_PATHS.iter().copied());
    let output = git(donor, &args)?;
    Ok(output
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(|path| path.to_string())
        .collect())
}

/// Read missing sparse files through one `git cat-file --batch` process.
fn git_batch(donor: &Path, revision: &str, paths: &[String]) -> Result<BTreeMap<String, Vec<u8>>> {
    let mut child = Command::new("git")
        .args([
            "-C",
            donor.to_str().context("donor path is not UTF-8")?,
            "-c",
            "gc.auto=0",
            "cat-file",
            "--batch",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("cannot start git cat-file")?;
    {
        let stdin = child.stdin.as_mut().context("git cat-file has no stdin")?;
        for path in paths {
            writeln!(stdin, "{revision}:{path}").context("cannot write git cat-file query")?;
        }
    }
    let output = child
        .wait_with_output()
        .context("cannot finish git cat-file")?;
    if !output.status.success() {
        bail!(
            "git cat-file failed ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let mut cursor = std::io::Cursor::new(output.stdout);
    let mut files = BTreeMap::new();
    for path in paths {
        let mut header = Vec::new();
        loop {
            let mut byte = [0u8; 1];
            cursor
                .read_exact(&mut byte)
                .with_context(|| format!("git cat-file ended before {path}"))?;
            if byte[0] == b'\n' {
                break;
            }
            header.push(byte[0]);
        }
        let header = String::from_utf8_lossy(&header).into_owned();
        let mut fields = header.split_whitespace();
        let _object = fields.next();
        let kind = fields.next();
        let size: usize = fields
            .next()
            .with_context(|| format!("bad git cat-file header {header:?}"))?
            .parse()
            .with_context(|| format!("bad git cat-file size in {header:?}"))?;
        if kind != Some("blob") {
            bail!("git cat-file cannot read {revision}:{path}: {header}");
        }
        let mut bytes = vec![0u8; size];
        cursor
            .read_exact(&mut bytes)
            .with_context(|| format!("git cat-file truncated {path}"))?;
        let mut newline = [0u8; 1];
        cursor
            .read_exact(&mut newline)
            .with_context(|| format!("git cat-file missing newline after {path}"))?;
        files.insert(path.clone(), bytes);
    }
    Ok(files)
}

/// A constructed include tree, labelled as constructed (never an installed FreeBSD system).
struct Tree {
    include: PathBuf,
    label: String,
    checkout_files: usize,
    git_files: usize,
}

fn copy_if_dir(from: &Path, to: &Path) -> Result<bool> {
    if from.is_dir() {
        copy_tree(from, to)?;
        return Ok(true);
    }
    Ok(false)
}

fn materialize(donor: &Path, arch: &str, temp: &Path) -> Result<Tree> {
    if donor.join("include/errno.h").is_file() && donor.join("include/sys/sys").is_dir() {
        return Ok(Tree {
            include: donor.to_path_buf(),
            label: "provided include root (not verified as an installed FreeBSD system)"
                .to_string(),
            checkout_files: 0,
            git_files: 0,
        });
    }
    if !donor.join("sys/sys").is_dir() {
        bail!(
            "FreeBSD donor {} has neither sys/sys nor a prepared include root",
            donor.display()
        );
    }
    let raw = temp.join("donor-raw");
    std::fs::create_dir_all(&raw)?;
    let files = git_list(donor).unwrap_or_default();
    let mut checkout_files = 0usize;
    let mut git_files = 0usize;
    if files.is_empty() {
        for name in TREE_PATHS {
            if copy_if_dir(&donor.join(name), &raw.join(name))? {
                checkout_files += 1;
            }
        }
    } else {
        let mut missing = Vec::new();
        for path in &files {
            let from = donor.join(path);
            let to = raw.join(path);
            if from.is_file() {
                if let Some(parent) = to.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::copy(&from, &to)
                    .with_context(|| format!("cannot copy donor {}", from.display()))?;
                checkout_files += 1;
            } else {
                missing.push(path.clone());
            }
        }
        for (path, bytes) in git_batch(donor, "HEAD", &missing)? {
            let to = raw.join(&path);
            if let Some(parent) = to.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&to, bytes)
                .with_context(|| format!("cannot write donor {}", to.display()))?;
            git_files += 1;
        }
    }

    let include = temp.join(format!("donor-{arch}"));
    std::fs::create_dir_all(&include)?;
    // The installed /usr/include layout: top-level include, then sys/*.h at the root and
    // under sys/, machine/ from the architecture and x86/ for amd64.
    copy_tree(&raw.join("include"), &include)?;
    if raw.join("sys/sys").is_dir() {
        copy_tree(&raw.join("sys/sys"), &include)?;
        copy_tree(&raw.join("sys/sys"), &include.join("sys"))?;
    }
    for name in ["netinet", "netinet6", "net"] {
        let from = raw.join("sys").join(name);
        if from.is_dir() {
            copy_tree(&from, &include.join(name))?;
        }
    }
    let machine = if arch == "x86_64" {
        raw.join("sys/amd64/include")
    } else {
        raw.join("sys/arm64/include")
    };
    if !machine.is_dir() {
        bail!("FreeBSD donor has no machine include directory for {arch}");
    }
    copy_tree(&machine, &include.join("machine"))?;
    if arch == "x86_64" && raw.join("sys/x86/include").is_dir() {
        copy_tree(&raw.join("sys/x86/include"), &include.join("x86"))?;
    }
    Ok(Tree {
        include,
        label: "constructed from the pinned FreeBSD source (not an installed FreeBSD system)"
            .to_string(),
        checkout_files,
        git_files,
    })
}

fn freebsd_version(text: &str) -> Option<u32> {
    for line in text.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("#define") else {
            continue;
        };
        let mut fields = rest.split_whitespace();
        if fields.next() == Some("__FreeBSD_version") {
            return fields.next().and_then(|value| value.parse().ok());
        }
    }
    None
}

fn read_donor_file(donor: &Path, path: &str) -> Result<String> {
    let local = donor.join(path);
    if local.is_file() {
        return std::fs::read_to_string(&local)
            .with_context(|| format!("cannot read {}", local.display()));
    }
    git(donor, &["show", &format!("HEAD:{path}")])
        .with_context(|| format!("FreeBSD donor has no {path}"))
}

fn family_headers(stem: &str) -> &'static [&'static str] {
    match stem {
        "errno" => &["errno.h"],
        "signal" => &["signal.h"],
        "fcntl" => &["fcntl.h"],
        "mman" => &["sys/mman.h"],
        "stat" => &["sys/stat.h", "dirent.h"],
        "socket" => &[
            "sys/socket.h",
            "netinet/in.h",
            "netinet/tcp.h",
            "netinet/udp.h",
        ],
        "netinet" => &["netinet/in.h", "netinet/tcp.h"],
        "poll" => &["poll.h"],
        "time" => &["time.h", "sys/time.h"],
        "resource" => &["sys/resource.h"],
        "limits" => &["limits.h"],
        "sysconf" => &["unistd.h"],
        "pthread" => &["pthread.h"],
        "misc" => &[
            "locale.h",
            "dlfcn.h",
            "sys/wait.h",
            "sys/file.h",
            "netdb.h",
            "sched.h",
            "sys/select.h",
            "net/if.h",
        ],
        _ => &[],
    }
}

fn record_headers(record: &RecordDef) -> &'static [&'static str] {
    match record.name.as_str() {
        "stat" | "statvfs" => &["sys/stat.h", "sys/statvfs.h"],
        "dirent" => &["dirent.h"],
        "sockaddr" | "sockaddr_in" | "sockaddr_in6" | "sockaddr_un" | "sockaddr_storage"
        | "msghdr" | "cmsghdr" | "addrinfo" => {
            &["sys/socket.h", "netinet/in.h", "sys/un.h", "netdb.h"]
        }
        "sigaction" | "siginfo_t" | "stack_t" => &["signal.h"],
        "termios" => &["termios.h"],
        "rusage" | "rlimit" => &["sys/resource.h"],
        "pollfd" => &["poll.h"],
        "timespec" | "itimerspec" | "tm" => &["time.h"],
        "timeval" => &["sys/time.h"],
        "passwd" => &["pwd.h"],
        "group" => &["grp.h"],
        "utsname" => &["sys/utsname.h"],
        "iovec" => &["sys/uio.h"],
        "flock" => &["sys/file.h"],
        "sched_param" => &["sched.h"],
        "lconv" => &["locale.h"],
        "sigval" => &["signal.h"],
        "mq_attr" | "aiocb" | "sigevent" => &["mqueue.h", "aio.h", "signal.h"],
        _ => &["sys/types.h", "sys/time.h", "signal.h", "sys/socket.h"],
    }
}

fn include_text(headers: &[&str]) -> String {
    let mut out = String::new();
    for header in headers {
        let _ = writeln!(out, "#include <{header}>");
    }
    out
}

#[allow(clippy::too_many_arguments)] // explicit probe inputs, no shared state object
fn compile_chunk(
    includes: &Includes,
    temp: &Path,
    prefix: &str,
    header: &str,
    names: &[String],
    body: &dyn Fn(&str) -> String,
    values: &mut BTreeMap<String, i128>,
    failures: &mut BTreeMap<String, String>,
) {
    if names.is_empty() {
        return;
    }
    let mut source = header.to_string();
    for name in names {
        source.push_str(&body(name));
    }
    let path = temp.join(format!("{prefix}.c"));
    if let Err(error) = std::fs::write(&path, &source) {
        for name in names {
            failures.insert(name.clone(), format!("cannot write probe: {error}"));
        }
        return;
    }
    match includes.compile(&path, true) {
        Ok(assembly) => {
            let labels: BTreeSet<String> = names.iter().map(|name| format!("v_{name}")).collect();
            let measured = parse_assembly(&assembly, &labels);
            for name in names {
                match measured.get(&format!("v_{name}")) {
                    Some(value) => {
                        values.insert(name.clone(), *value);
                    }
                    None => {
                        failures.insert(name.clone(), "probe printed no value".to_string());
                    }
                }
            }
        }
        Err(error) if names.len() == 1 => {
            failures.insert(names[0].clone(), format!("{error:#}"));
        }
        Err(_) => {
            let (left, right) = names.split_at(names.len() / 2);
            compile_chunk(includes, temp, prefix, header, left, body, values, failures);
            compile_chunk(
                includes, temp, prefix, header, right, body, values, failures,
            );
        }
    }
}

fn measure_constants(
    includes: &Includes,
    temp: &Path,
    stem: &str,
    consts: &[ConstDef],
    side: &mut Side,
    integrity: &mut Vec<String>,
) {
    let header = include_text(family_headers(stem));
    let header_path = temp.join(format!("header-{stem}.c"));
    if let Err(error) = std::fs::write(&header_path, &header)
        .map_err(anyhow::Error::from)
        .and_then(|()| includes.compile(&header_path, true))
    {
        integrity.push(format!(
            "{stem}: cannot compile the headers {:?}: {error:#}",
            family_headers(stem)
        ));
        for constant in consts {
            if !RUNTIME_ACCESSORS.contains(&constant.name.as_str()) {
                side.unevaluated
                    .insert(constant.name.clone(), "header probe failed".to_string());
            }
        }
        return;
    }
    let missing: Vec<String> = consts
        .iter()
        .map(|constant| constant.name.clone())
        .filter(|name| !RUNTIME_ACCESSORS.contains(&name.as_str()))
        .collect();
    let body = |name: &str| {
        format!(
            "#ifdef {name}\nconst long long v_{name} = (long long)({name});\n#else\nconst int absent_{name} = 1;\n#endif\n"
        )
    };
    let mut found = BTreeMap::new();
    let mut failures = BTreeMap::new();
    compile_chunk(
        includes,
        temp,
        &format!("const-{stem}"),
        &header,
        &missing,
        &body,
        &mut found,
        &mut failures,
    );
    side.constants.extend(found);
    side.unevaluated.extend(failures);

    // Distinguish names the donor does not define at all from defined-but-unevaluated ones.
    let absent_body = |name: &str| {
        format!("#ifdef {name}\nconst int v_{name} = 1;\n#else\nconst int w_{name} = 1;\n#endif\n")
    };
    let present = |name: &str| format!("v_{name}");
    let mut absent_values = BTreeMap::new();
    let mut absent_failures = BTreeMap::new();
    compile_chunk(
        includes,
        temp,
        &format!("absent-{stem}"),
        &header,
        &missing,
        &absent_body,
        &mut absent_values,
        &mut absent_failures,
    );
    let _ = (present, absent_failures);
    for name in &missing {
        if absent_values.contains_key(name) {
            continue;
        }
        side.absent.insert(name.clone());
        side.unevaluated.remove(name);
    }
}

fn type_header() -> String {
    include_text(&[
        "sys/types.h",
        "stdint.h",
        "sys/socket.h",
        "sys/stat.h",
        "time.h",
        "wchar.h",
        "stddef.h",
        "unistd.h",
        "signal.h",
        "pthread.h",
    ])
}

fn measure_types(
    includes: &Includes,
    temp: &Path,
    tables: &Tables,
    side: &mut Side,
    integrity: &mut Vec<String>,
) {
    let names: Vec<String> = tables
        .types
        .types
        .iter()
        .map(|ty| ty.name.clone())
        .collect();
    let header = type_header();
    let path = temp.join("types-header.c");
    if let Err(error) = std::fs::write(&path, &header)
        .map_err(anyhow::Error::from)
        .and_then(|()| includes.compile(&path, true))
    {
        integrity.push(format!("types: cannot compile headers: {error:#}"));
    }
    let sizes_body =
        |name: &str| format!("const long long v_{name} = (long long)sizeof({name});\n");
    let mut sizes = BTreeMap::new();
    let mut size_failures = BTreeMap::new();
    compile_chunk(
        includes,
        temp,
        "type-size",
        &header,
        &names,
        &sizes_body,
        &mut sizes,
        &mut size_failures,
    );
    let align_body =
        |name: &str| format!("const long long v_{name} = (long long)_Alignof({name});\n");
    let mut aligns = BTreeMap::new();
    let mut align_failures = BTreeMap::new();
    compile_chunk(
        includes,
        temp,
        "type-align",
        &header,
        &names,
        &align_body,
        &mut aligns,
        &mut align_failures,
    );
    let signed_body = |name: &str| format!("const int v_{name} = ((__typeof__({name}))-1) < 0;\n");
    let mut signed = BTreeMap::new();
    let mut signed_failures = BTreeMap::new();
    compile_chunk(
        includes,
        temp,
        "type-sign",
        &header,
        &names,
        &signed_body,
        &mut signed,
        &mut signed_failures,
    );
    for name in &names {
        let (Some(size), Some(align), Some(sign)) =
            (sizes.get(name), aligns.get(name), signed.get(name))
        else {
            let mut errors = Vec::new();
            for error in [
                size_failures.get(name),
                align_failures.get(name),
                signed_failures.get(name),
            ]
            .into_iter()
            .flatten()
            {
                errors.push(error.as_str());
            }
            side.type_errors.insert(
                name.clone(),
                if errors.is_empty() {
                    "type probe printed no value".to_string()
                } else {
                    errors.join("; ")
                },
            );
            continue;
        };
        side.types.insert(
            name.clone(),
            TypeFacts {
                size: i64::try_from(*size).unwrap_or(-1),
                align: i64::try_from(*align).unwrap_or(-1),
                signed: *sign != 0,
            },
        );
    }
}

/// Parse clang `-fdump-record-layouts` output for the requested record spellings.
#[must_use]
pub fn parse_record_layouts(
    text: &str,
    wanted: &BTreeSet<String>,
) -> BTreeMap<String, RecordFacts> {
    let mut layouts = BTreeMap::new();
    let mut current: Option<String> = None;
    let mut size: Option<i64> = None;
    let mut align: Option<i64> = None;
    let mut fields: Vec<FieldFacts> = Vec::new();
    for line in text.lines() {
        if line.contains("*** Dumping AST Record Layout") {
            if let (Some(name), Some(size), Some(align)) =
                (current.take(), size.take(), align.take())
            {
                layouts.insert(
                    name,
                    RecordFacts {
                        size,
                        align,
                        fields: std::mem::take(&mut fields),
                    },
                );
            } else {
                current = None;
                size = None;
                align = None;
                fields.clear();
            }
            continue;
        }
        let Some((left, right)) = line.split_once(" | ") else {
            continue;
        };
        let right = right.to_string();
        let trimmed = right.trim_start();
        let indent = right.len() - trimmed.len();
        if left.trim() == "0" && indent == 0 {
            let spelling = trimmed.trim();
            if spelling.starts_with("struct ") || spelling.starts_with("union ") {
                if let (Some(name), Some(size), Some(align)) =
                    (current.take(), size.take(), align.take())
                {
                    layouts.insert(
                        name,
                        RecordFacts {
                            size,
                            align,
                            fields: std::mem::take(&mut fields),
                        },
                    );
                } else {
                    fields.clear();
                }
                if wanted.contains(spelling) {
                    current = Some(spelling.to_string());
                }
            }
            continue;
        }
        if let Some(meta) = trimmed.trim().strip_prefix("[sizeof=") {
            let (size_text, align_text) = meta.split_once(", align=").unwrap_or((meta, ""));
            size = size_text.trim().parse().ok();
            align = align_text.trim().trim_end_matches(']').parse().ok();
            if let (Some(name), Some(size), Some(align)) =
                (current.take(), size.take(), align.take())
            {
                layouts.insert(
                    name,
                    RecordFacts {
                        size,
                        align,
                        fields: std::mem::take(&mut fields),
                    },
                );
            }
            continue;
        }
        if current.is_none() || indent != 2 || right.ends_with(' ') {
            // A trailing blank marks an anonymous (named padding) bitfield, not a field.
            continue;
        }
        let Some(offset_text) = left.split(':').next() else {
            continue;
        };
        let Ok(offset) = offset_text.trim().parse::<i64>() else {
            continue;
        };
        let mut tokens: Vec<&str> = trimmed.split_whitespace().collect();
        let Some(name) = tokens.pop() else {
            continue;
        };
        if tokens.is_empty() || name.is_empty() || name == ":" {
            // Anonymous padding (`int` with no declarator) is not a public field.
            continue;
        }
        fields.push(FieldFacts {
            name: name.to_string(),
            offset,
            ty: tokens.join(" "),
            size: None,
            signed: None,
            bitfield: left.contains(':'),
        });
    }
    if let (Some(name), Some(size), Some(align)) = (current, size, align) {
        layouts.insert(
            name,
            RecordFacts {
                size,
                align,
                fields,
            },
        );
    }
    layouts
}

fn measure_records(
    includes: &Includes,
    temp: &Path,
    tables: &Tables,
    side: &mut Side,
    integrity: &mut Vec<String>,
) {
    for record in &tables.structs.record {
        let header = include_text(record_headers(record));
        if !record.c.starts_with("struct ") && !record.c.starts_with("union ") {
            measure_typedef_record(includes, temp, record, &header, side);
            continue;
        }
        let source = format!("{header}const int __oracle_size = sizeof({});\n", record.c);
        let path = temp.join(format!("record-{}.c", record.name));
        if let Err(error) = std::fs::write(&path, &source) {
            integrity.push(format!(
                "record {}: cannot write probe: {error}",
                record.name
            ));
            continue;
        }
        match includes.compile(&path, false) {
            Ok(dump) => {
                let wanted: BTreeSet<String> = [record.c.clone()].into_iter().collect();
                match parse_record_layouts(&dump, &wanted).remove(&record.c) {
                    Some(mut facts) => {
                        measure_fields(includes, temp, record, &header, &mut facts, integrity);
                        side.records.insert(record.name.clone(), facts);
                    }
                    None => {
                        side.record_errors.insert(
                            record.name.clone(),
                            format!("compiler emitted no layout for {}", record.c),
                        );
                    }
                }
            }
            Err(error) => {
                side.record_errors
                    .insert(record.name.clone(), format!("{error:#}"));
            }
        }
    }
}

fn measure_typedef_record(
    includes: &Includes,
    temp: &Path,
    record: &RecordDef,
    header: &str,
    side: &mut Side,
) {
    let mut source = header.to_string();
    let _ = writeln!(
        source,
        "const long long v_{} = (long long)sizeof({});",
        record.name, record.c
    );
    let path = temp.join(format!("typedef-{}.c", record.name));
    if std::fs::write(&path, &source).is_err() {
        side.record_errors.insert(
            record.name.clone(),
            "cannot write typedef probe".to_string(),
        );
        return;
    }
    let size = includes.compile(&path, true).ok().and_then(|assembly| {
        let labels = BTreeSet::from([format!("v_{}", record.name)]);
        parse_assembly(&assembly, &labels)
            .get(&format!("v_{}", record.name))
            .and_then(|value| i64::try_from(*value).ok())
    });
    let mut align_source = header.to_string();
    let _ = writeln!(
        align_source,
        "const long long v_{} = (long long)_Alignof({});",
        record.name, record.c
    );
    let align_path = temp.join(format!("typedef-align-{}.c", record.name));
    if std::fs::write(&align_path, &align_source).is_err() {
        side.record_errors.insert(
            record.name.clone(),
            "cannot write typedef align probe".to_string(),
        );
        return;
    }
    let align = includes
        .compile(&align_path, true)
        .ok()
        .and_then(|assembly| {
            let labels = BTreeSet::from([format!("v_{}", record.name)]);
            parse_assembly(&assembly, &labels)
                .get(&format!("v_{}", record.name))
                .and_then(|value| i64::try_from(*value).ok())
        });
    match (size, align) {
        (Some(size), Some(align)) => {
            side.records.insert(
                record.name.clone(),
                RecordFacts {
                    size,
                    align,
                    fields: Vec::new(),
                },
            );
        }
        _ => {
            side.record_errors
                .insert(record.name.clone(), "typedef probe failed".to_string());
        }
    }
}

fn measure_fields(
    includes: &Includes,
    temp: &Path,
    record: &RecordDef,
    header: &str,
    facts: &mut RecordFacts,
    integrity: &mut Vec<String>,
) {
    for (index, field) in facts.fields.iter_mut().enumerate() {
        if field.bitfield {
            continue;
        }
        let c = &record.c;
        let name = &field.name;
        let mut source = header.to_string();
        // Neutralise attribute-like macros (`__unused`) that some headers leave defined.
        let _ = writeln!(source, "#pragma push_macro(\"{name}\")");
        let _ = writeln!(source, "#undef {name}");
        let _ = writeln!(
            source,
            "const long long v_size_{index} = (long long)sizeof((({c} *)0)->{name});"
        );
        let arithmetic = !field.ty.contains('*')
            && !field.ty.contains('[')
            && !field.ty.starts_with("struct ")
            && !field.ty.starts_with("union ");
        if arithmetic {
            let _ = writeln!(
                source,
                "const int v_sign_{index} = ((__typeof__((({c} *)0)->{name}))-1) < 0;"
            );
        }
        let _ = writeln!(source, "#pragma pop_macro(\"{name}\")");
        let path = temp.join(format!("field-{}-{index}.c", record.name));
        if let Err(error) = std::fs::write(&path, &source) {
            integrity.push(format!(
                "record {} field {}: cannot write probe: {error}",
                record.name, field.name
            ));
            continue;
        }
        match includes.compile(&path, true) {
            Ok(assembly) => {
                let mut labels = BTreeSet::from([format!("v_size_{index}")]);
                if arithmetic {
                    labels.insert(format!("v_sign_{index}"));
                }
                let measured = parse_assembly(&assembly, &labels);
                field.size = measured
                    .get(&format!("v_size_{index}"))
                    .and_then(|value| i64::try_from(*value).ok());
                field.signed = measured
                    .get(&format!("v_sign_{index}"))
                    .map(|value| *value != 0);
            }
            Err(error) => {
                integrity.push(format!(
                    "record {} field {}: field probe failed: {error:#}",
                    record.name, field.name
                ));
            }
        }
    }
}

fn measure_opaque(includes: &Includes, temp: &Path, tables: &Tables, side: &mut Side) {
    let header = include_text(&[
        "sys/types.h",
        "signal.h",
        "pthread.h",
        "setjmp.h",
        "fenv.h",
        "glob.h",
        "regex.h",
        "semaphore.h",
        "ucontext.h",
        "sys/select.h",
        "sched.h",
        "spawn.h",
        "mqueue.h",
        "aio.h",
    ]);
    for opaque in &tables.structs.opaque {
        let mut source = header.clone();
        let _ = writeln!(
            source,
            "const long long v_{} = (long long)sizeof({});",
            opaque.name, opaque.c
        );
        let path = temp.join(format!("opaque-{}.c", opaque.name));
        if std::fs::write(&path, &source).is_err() {
            side.type_errors
                .insert(opaque.name.clone(), "cannot write opaque probe".to_string());
            continue;
        }
        match includes.compile(&path, true) {
            Ok(assembly) => {
                let labels = BTreeSet::from([format!("v_{}", opaque.name)]);
                if let Some(value) =
                    parse_assembly(&assembly, &labels).get(&format!("v_{}", opaque.name))
                {
                    side.types.insert(
                        opaque.name.clone(),
                        TypeFacts {
                            size: i64::try_from(*value).unwrap_or(-1),
                            align: 0,
                            signed: false,
                        },
                    );
                }
            }
            Err(error) => {
                side.type_errors
                    .insert(opaque.name.clone(), format!("{error:#}"));
            }
        }
    }
}

fn measure_runtime(includes: &Includes, temp: &Path, side: &mut Side) {
    for name in RUNTIME_ACCESSORS {
        let source = format!(
            "#include <time.h>\n#include <limits.h>\n#include <unistd.h>\nlong long raven_oracle_{name}(void) {{ return (long long)({name}); }}\n"
        );
        let path = temp.join(format!("runtime-{name}.c"));
        if std::fs::write(&path, source).is_err() {
            side.runtime.insert(
                (*name).to_string(),
                Err("cannot write runtime probe".to_string()),
            );
            continue;
        }
        match includes.compile(&path, true) {
            Ok(_) => {
                side.runtime.insert((*name).to_string(), Ok(()));
            }
            Err(error) => {
                side.runtime
                    .insert((*name).to_string(), Err(format!("{error:#}")));
            }
        }
    }
}

fn measure_side(
    tables: &Tables,
    includes: &Includes,
    temp: &Path,
    side: &mut Side,
    integrity: &mut Vec<String>,
) {
    if let Err(error) = std::fs::create_dir_all(temp) {
        integrity.push(format!(
            "cannot create probe directory {}: {error}",
            temp.display()
        ));
        return;
    }
    for group in &tables.groups {
        measure_constants(
            includes,
            temp,
            &group.stem,
            &group.def.consts,
            side,
            integrity,
        );
    }
    measure_types(includes, temp, tables, side, integrity);
    measure_records(includes, temp, tables, side, integrity);
    measure_opaque(includes, temp, tables, side);
    measure_runtime(includes, temp, side);
}

/// Read `pub const NAME: TYPE = VALUE;` from a generated Rust file.
#[must_use]
pub fn parse_rust_constants(text: &str) -> BTreeMap<String, i128> {
    let mut values = BTreeMap::new();
    for line in text.lines() {
        let Some(rest) = line.trim().strip_prefix("pub const ") else {
            continue;
        };
        let Some((name, rest)) = rest.split_once(':') else {
            continue;
        };
        let Some((_ty, value)) = rest.split_once('=') else {
            continue;
        };
        let value = value.trim().trim_end_matches(';').trim();
        if let Ok(parsed) = crate::values::parse_int_literal(value) {
            values.insert(name.trim().to_string(), parsed);
        }
    }
    values
}

fn raven_includes(clang: &str, arch: &str, root: &Path, abi: &Path) -> Result<Includes> {
    let dirs = vec![
        root.join("libc/ravenlibc/include"),
        abi.join("generated/include"),
        root.join(format!("libc/ravenlibc/arch/{arch}/include")),
    ];
    for dir in &dirs {
        if !dir.is_dir() {
            bail!("Raven include root is missing: {}", dir.display());
        }
    }
    Includes::new(clang, &format!("{arch}-unknown-ravenbsd"), dirs, Vec::new())
}

fn donor_includes(clang: &str, arch: &str, tree: &Tree) -> Result<Includes> {
    Includes::new(
        clang,
        &format!("{arch}-unknown-freebsd14.0"),
        vec![tree.include.clone()],
        vec!["__BSD_VISIBLE=1".to_string()],
    )
}

fn approved(tables: &Tables, diff: &Diff) -> bool {
    tables.deviations.deviation.iter().any(|row| {
        if row.against != "freebsd" || row.name != diff.name {
            return false;
        }
        match (&diff.expected, &diff.actual) {
            (Value::Int(expected), Value::Int(actual)) => {
                row.theirs_int(&diff.arch) == Some(*expected)
                    && row.ours_int(&diff.arch) == Some(*actual)
            }
            _ => false,
        }
    })
}

#[allow(clippy::too_many_arguments)] // one explicit row of measured data
fn push_diff(
    tables: &Tables,
    inventory: &mut Inventory,
    family: &str,
    kind: &'static str,
    name: &str,
    arch: &str,
    expected: Value,
    actual: Value,
    detail: String,
) {
    let mut diff = Diff {
        family: family.to_string(),
        kind,
        name: name.to_string(),
        arch: arch.to_string(),
        expected,
        actual,
        detail,
        approved: false,
    };
    diff.approved = approved(tables, &diff);
    inventory.diffs.push(diff);
}

fn compare(
    tables: &Tables,
    arch: &str,
    donor: &Side,
    raven: &Side,
    rust: &BTreeMap<String, i128>,
    rust_layouts: &BTreeMap<String, RustRecord>,
    inventory: &mut Inventory,
) {
    for group in &tables.groups {
        let family = &group.stem;
        for constant in &group.def.consts {
            let name = &constant.name;
            if RUNTIME_ACCESSORS.contains(&name.as_str()) {
                continue;
            }
            let expected = donor.constants.get(name);
            let actual = raven.constants.get(name);
            match (expected, actual) {
                (Some(expected), Some(actual)) if !same_mod_64(*expected, *actual) => push_diff(
                    tables,
                    inventory,
                    family,
                    "constant",
                    name,
                    arch,
                    Value::Int(*expected),
                    Value::Int(*actual),
                    "Raven public header value differs from the donor header".to_string(),
                ),
                (Some(_), Some(_)) => {}
                (Some(expected), None) => {
                    let error =
                        raven.unevaluated.get(name).cloned().unwrap_or_else(|| {
                            "Raven header did not define the constant".to_string()
                        });
                    push_diff(
                        tables,
                        inventory,
                        family,
                        "constant-missing",
                        name,
                        arch,
                        Value::Int(*expected),
                        Value::Text(error),
                        "donor defines the constant, Raven does not evaluate it".to_string(),
                    );
                }
                (None, Some(_)) => {
                    if donor.unevaluated.contains_key(name) && !donor.absent.contains(name) {
                        push_diff(
                            tables,
                            inventory,
                            family,
                            "unevaluated",
                            name,
                            arch,
                            Value::Text("donor constant expression".to_string()),
                            Value::Text(donor.unevaluated.get(name).cloned().unwrap_or_default()),
                            "donor defines a non-constant expression that must be classified"
                                .to_string(),
                        );
                    } else {
                        push_diff(
                            tables,
                            inventory,
                            family,
                            "donor-absent",
                            name,
                            arch,
                            Value::Text("absent".to_string()),
                            Value::Int(raven.constants.get(name).copied().unwrap_or_default()),
                            "constant is not defined by the donor headers".to_string(),
                        );
                    }
                }
                (None, None) => {
                    if !donor.absent.contains(name) && !donor.unevaluated.contains_key(name) {
                        push_diff(
                            tables,
                            inventory,
                            family,
                            "unevaluated",
                            name,
                            arch,
                            Value::Text("missing".to_string()),
                            Value::Text("not measured".to_string()),
                            "constant was neither defined nor classified".to_string(),
                        );
                    }
                }
            }
            if let (Some(expected), Some(rust)) = (donor.constants.get(name), rust.get(name))
                && !same_mod_64(*expected, *rust)
                && raven.constants.get(name) == Some(expected)
            {
                push_diff(
                    tables,
                    inventory,
                    family,
                    "rust-constant",
                    name,
                    arch,
                    Value::Int(*expected),
                    Value::Int(*rust),
                    "generated Rust ABI differs from the donor header".to_string(),
                );
            }
            if donor.constants.contains_key(name)
                && raven.constants.contains_key(name)
                && !rust.contains_key(name)
            {
                push_diff(
                    tables,
                    inventory,
                    family,
                    "rust-missing",
                    name,
                    arch,
                    Value::Int(*donor.constants.get(name).unwrap_or(&0)),
                    Value::Text("absent".to_string()),
                    "constant is missing from the generated Rust ABI".to_string(),
                );
            }
            if let Some(alias) = &constant.alias {
                let donor_pair = (
                    donor.constants.get(name).copied(),
                    donor.constants.get(alias).copied(),
                );
                let raven_pair = (
                    raven.constants.get(name).copied(),
                    raven.constants.get(alias).copied(),
                );
                match (donor_pair, raven_pair) {
                    ((Some(left), Some(right)), (Some(rleft), Some(rright))) => {
                        if same_mod_64(left, right) != same_mod_64(rleft, rright) {
                            push_diff(
                                tables,
                                inventory,
                                family,
                                "alias-relation",
                                name,
                                arch,
                                Value::Text(format!(
                                    "{name} {} {alias}",
                                    if same_mod_64(left, right) { "==" } else { "!=" }
                                )),
                                Value::Text(format!(
                                    "{name} {} {alias}",
                                    if same_mod_64(rleft, rright) {
                                        "=="
                                    } else {
                                        "!="
                                    }
                                )),
                                "alias relation differs from the donor".to_string(),
                            );
                        }
                    }
                    ((Some(_), _), _) => {
                        push_diff(
                            tables,
                            inventory,
                            family,
                            "alias-target-missing",
                            name,
                            arch,
                            Value::Text(format!("{alias} defined")),
                            Value::Text("missing".to_string()),
                            "alias target is absent from one side".to_string(),
                        );
                    }
                    _ => {}
                }
            }
        }
    }

    for ty in &tables.types.types {
        match (donor.types.get(&ty.name), raven.types.get(&ty.name)) {
            (Some(expected), Some(actual)) => {
                if expected.size != actual.size {
                    push_diff(
                        tables,
                        inventory,
                        "types",
                        "type-size",
                        &ty.name,
                        arch,
                        Value::Int(expected.size.into()),
                        Value::Int(actual.size.into()),
                        "scalar width differs from the donor".to_string(),
                    );
                }
                if expected.signed != actual.signed {
                    push_diff(
                        tables,
                        inventory,
                        "types",
                        "type-signed",
                        &ty.name,
                        arch,
                        Value::Signed(expected.signed),
                        Value::Signed(actual.signed),
                        "scalar signedness differs from the donor".to_string(),
                    );
                }
            }
            (Some(_), None) => push_diff(
                tables,
                inventory,
                "types",
                "type-missing",
                &ty.name,
                arch,
                Value::Text("defined".to_string()),
                Value::Text(
                    raven
                        .type_errors
                        .get(&ty.name)
                        .cloned()
                        .unwrap_or_else(|| "not measured".to_string()),
                ),
                "donor defines the scalar type, Raven does not".to_string(),
            ),
            (None, _) => {
                let reason = donor
                    .type_errors
                    .get(&ty.name)
                    .cloned()
                    .unwrap_or_else(|| "donor does not define the type".to_string());
                push_diff(
                    tables,
                    inventory,
                    "types",
                    "donor-type-absent",
                    &ty.name,
                    arch,
                    Value::Text(reason),
                    Value::Text("not measured".to_string()),
                    "requested scalar type is absent from the donor headers".to_string(),
                );
            }
        }
    }

    for record in &tables.structs.record {
        match (
            donor.records.get(&record.name),
            raven.records.get(&record.name),
        ) {
            (Some(expected), Some(actual)) => {
                if expected.size != actual.size {
                    push_diff(
                        tables,
                        inventory,
                        "records",
                        "record-size",
                        &record.name,
                        arch,
                        Value::Int(expected.size.into()),
                        Value::Int(actual.size.into()),
                        "record size differs from the donor".to_string(),
                    );
                }
                if expected.align != actual.align {
                    push_diff(
                        tables,
                        inventory,
                        "records",
                        "record-align",
                        &record.name,
                        arch,
                        Value::Int(expected.align.into()),
                        Value::Int(actual.align.into()),
                        "record alignment differs from the donor".to_string(),
                    );
                }
                compare_fields(tables, arch, record, expected, actual, inventory);
                compare_rust_records(tables, arch, record, expected, rust_layouts, inventory);
            }
            (Some(_), None) => push_diff(
                tables,
                inventory,
                "records",
                "record-missing",
                &record.name,
                arch,
                Value::Text("measured".to_string()),
                Value::Text(
                    raven
                        .record_errors
                        .get(&record.name)
                        .cloned()
                        .unwrap_or_else(|| "not measured".to_string()),
                ),
                "donor record cannot be measured in Raven".to_string(),
            ),
            (None, _) => {
                let reason = donor
                    .record_errors
                    .get(&record.name)
                    .cloned()
                    .unwrap_or_else(|| "donor does not define the record".to_string());
                push_diff(
                    tables,
                    inventory,
                    "records",
                    "donor-record-absent",
                    &record.name,
                    arch,
                    Value::Text(reason),
                    Value::Text("not measured".to_string()),
                    "requested record is absent from the donor headers".to_string(),
                );
            }
        }
    }

    for opaque in &tables.structs.opaque {
        match (donor.types.get(&opaque.name), raven.types.get(&opaque.name)) {
            (Some(expected), Some(actual)) if expected.size != actual.size => push_diff(
                tables,
                inventory,
                "records",
                "opaque-size",
                &opaque.name,
                arch,
                Value::Int(expected.size.into()),
                Value::Int(actual.size.into()),
                "opaque size differs from the donor".to_string(),
            ),
            (Some(_), Some(_)) => {}
            _ => push_diff(
                tables,
                inventory,
                "records",
                "opaque-missing",
                &opaque.name,
                arch,
                Value::Text("measured".to_string()),
                Value::Text(
                    raven
                        .type_errors
                        .get(&opaque.name)
                        .cloned()
                        .unwrap_or_else(|| "not measured".to_string()),
                ),
                "opaque donor/Raven measurement is incomplete".to_string(),
            ),
        }
    }

    for name in RUNTIME_ACCESSORS {
        match (donor.runtime.get(name), raven.runtime.get(name)) {
            (Some(Ok(())), Some(Ok(()))) => {}
            (Some(Err(error)), _) | (_, Some(Err(error))) => push_diff(
                tables,
                inventory,
                "time",
                "runtime-accessor",
                name,
                arch,
                Value::Text("accessor compiles".to_string()),
                Value::Text(error.clone()),
                "runtime accessor must compile on both sides".to_string(),
            ),
            _ => push_diff(
                tables,
                inventory,
                "time",
                "runtime-accessor",
                name,
                arch,
                Value::Text("accessor compiles".to_string()),
                Value::Text("not measured".to_string()),
                "runtime accessor was not compiled".to_string(),
            ),
        }
    }
}

fn compare_fields(
    tables: &Tables,
    arch: &str,
    record: &RecordDef,
    donor: &RecordFacts,
    raven: &RecordFacts,
    inventory: &mut Inventory,
) {
    let donor_fields: BTreeMap<&str, &FieldFacts> = donor
        .fields
        .iter()
        .map(|field| (field.name.as_str(), field))
        .collect();
    let raven_fields: BTreeMap<&str, &FieldFacts> = raven
        .fields
        .iter()
        .map(|field| (field.name.as_str(), field))
        .collect();
    for (name, expected) in &donor_fields {
        let qualified = format!("{}.{}", record.name, name);
        match raven_fields.get(name) {
            Some(actual) => {
                if expected.offset != actual.offset {
                    push_diff(
                        tables,
                        inventory,
                        "records",
                        "field-offset",
                        &qualified,
                        arch,
                        Value::Int(expected.offset.into()),
                        Value::Int(actual.offset.into()),
                        format!("{} field offset differs", record.name),
                    );
                }
                if let (Some(expected_size), Some(actual_size)) = (expected.size, actual.size)
                    && expected_size != actual_size
                {
                    push_diff(
                        tables,
                        inventory,
                        "records",
                        "field-size",
                        &qualified,
                        arch,
                        Value::Int(expected_size.into()),
                        Value::Int(actual_size.into()),
                        format!("{} field type width differs", record.name),
                    );
                }
                if let (Some(expected_signed), Some(actual_signed)) =
                    (expected.signed, actual.signed)
                    && expected_signed != actual_signed
                {
                    push_diff(
                        tables,
                        inventory,
                        "records",
                        "field-signed",
                        &qualified,
                        arch,
                        Value::Signed(expected_signed),
                        Value::Signed(actual_signed),
                        format!("{} field signedness differs", record.name),
                    );
                }
            }
            None => push_diff(
                tables,
                inventory,
                "records",
                "field-missing",
                &qualified,
                arch,
                Value::Text(format!(
                    "{} {} at {}",
                    expected.ty, expected.name, expected.offset
                )),
                Value::Text("absent".to_string()),
                format!("{} field is absent from Raven", record.name),
            ),
        }
    }
    for (name, actual) in &raven_fields {
        if !donor_fields.contains_key(name) {
            push_diff(
                tables,
                inventory,
                "records",
                "field-extra",
                &format!("{}.{}", record.name, name),
                arch,
                Value::Text("absent".to_string()),
                Value::Text(format!("{} at {}", actual.ty, actual.offset)),
                format!("{} field does not exist in the donor record", record.name),
            );
        }
    }
}

/// One generated Rust record layout.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RustRecord {
    /// `size_of::<T>()`.
    pub size: Option<i64>,
    /// `align_of::<T>()`.
    pub align: Option<i64>,
    /// `offset_of!(T, field)` values.
    pub fields: BTreeMap<String, i64>,
}

/// Read the generated Rust layout assertions from `structs.rs`.
#[must_use]
pub fn parse_rust_layouts(text: &str) -> BTreeMap<String, RustRecord> {
    let mut records: BTreeMap<String, RustRecord> = BTreeMap::new();
    for line in text.lines() {
        if let Some(start) = line.find("size_of::<")
            && let Some(end) = line[start + 10..].find(">")
        {
            let name = &line[start + 10..start + 10 + end];
            let entry = records.entry(name.to_string()).or_default();
            if let Some(rest) = line.split("size_of::<").nth(1)
                && let Some((_ty, value)) = rest.split_once(">() == ")
                && let Some(number) = value.split_whitespace().next()
                && let Ok(parsed) = number.trim_end_matches(';').parse()
            {
                entry.size = Some(parsed);
            }
            if let Some(rest) = line.split("align_of::<").nth(1)
                && let Some((_ty, value)) = rest.split_once(">() == ")
                && let Some(number) = value.split_whitespace().next()
                && let Ok(parsed) = number.trim_end_matches([';', ')']).parse()
            {
                entry.align = Some(parsed);
            }
        }
        if let Some(start) = line.find("offset_of!(")
            && let Some(end) = line[start + 11..].find(')')
        {
            let inner = &line[start + 11..start + 11 + end];
            let mut parts = inner.split(',');
            let (Some(name), Some(field)) = (parts.next(), parts.next()) else {
                continue;
            };
            let Some((_rest, value)) = line[start + 11 + end..].split_once("== ") else {
                continue;
            };
            let Some(number) = value.split_whitespace().next() else {
                continue;
            };
            if let Ok(offset) = number.trim_end_matches([';', ')']).parse() {
                records
                    .entry(name.trim().to_string())
                    .or_default()
                    .fields
                    .insert(field.trim().to_string(), offset);
            }
        }
    }
    records
}

fn read_rust_layouts(root: &Path) -> BTreeMap<String, RustRecord> {
    std::fs::read_to_string(
        root.join("libc/ravenlibc/crates/raven-libc-abi/src/generated/structs.rs"),
    )
    .map(|text| parse_rust_layouts(&text))
    .unwrap_or_default()
}

fn compare_rust_records(
    tables: &Tables,
    arch: &str,
    record: &RecordDef,
    donor: &RecordFacts,
    rust: &BTreeMap<String, RustRecord>,
    inventory: &mut Inventory,
) {
    let Some(generated) = rust.get(&record.rust) else {
        return;
    };
    if let Some(size) = generated.size
        && size != donor.size
    {
        push_diff(
            tables,
            inventory,
            "records",
            "rust-record-size",
            &record.name,
            arch,
            Value::Int(donor.size.into()),
            Value::Int(size.into()),
            "generated Rust ABI record size differs from the donor".to_string(),
        );
    }
    if let Some(align) = generated.align
        && align != donor.align
    {
        push_diff(
            tables,
            inventory,
            "records",
            "rust-record-align",
            &record.name,
            arch,
            Value::Int(donor.align.into()),
            Value::Int(align.into()),
            "generated Rust ABI record alignment differs from the donor".to_string(),
        );
    }
    for field in &donor.fields {
        let qualified = format!("{}.{}", record.name, field.name);
        match generated.fields.get(&field.name) {
            Some(offset) if *offset != field.offset => push_diff(
                tables,
                inventory,
                "records",
                "rust-field-offset",
                &qualified,
                arch,
                Value::Int(field.offset.into()),
                Value::Int((*offset).into()),
                "generated Rust ABI field offset differs from the donor".to_string(),
            ),
            Some(_) => {}
            None if !field.bitfield => push_diff(
                tables,
                inventory,
                "records",
                "rust-field-missing",
                &qualified,
                arch,
                Value::Text(format!("offset {}", field.offset)),
                Value::Text("absent".to_string()),
                "generated Rust ABI omits a donor field".to_string(),
            ),
            None => {}
        }
    }
}

fn read_rust_constants(root: &Path, stem: &str) -> BTreeMap<String, i128> {
    let path = root.join(format!(
        "libc/ravenlibc/crates/raven-libc-abi/src/generated/{stem}.rs"
    ));
    std::fs::read_to_string(path)
        .map(|text| parse_rust_constants(&text))
        .unwrap_or_default()
}

fn json_escape(text: &str) -> String {
    let mut out = String::new();
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other if other < ' ' => {
                let _ = write!(out, "\\u{:04x}", other as u32);
            }
            other => out.push(other),
        }
    }
    out
}

fn write_inventory(
    path: &Path,
    donor_revision: &str,
    version: Option<u32>,
    tree_label: &str,
    targets: &[&str],
    selection: &Selection,
    inventory: &Inventory,
) -> Result<()> {
    let mut out = String::new();
    out.push_str("{\n");
    let _ = writeln!(out, "  \"oracle\": \"freebsd\",");
    let _ = writeln!(
        out,
        "  \"donor\": {{\"revision\": \"{}\", \"version\": {}, \"tree\": \"{}\"}},",
        json_escape(donor_revision),
        version.map_or("null".to_string(), |value| value.to_string()),
        json_escape(tree_label)
    );
    let target_list: Vec<String> = targets
        .iter()
        .map(|target| format!("\"{}\"", json_escape(target)))
        .collect();
    let _ = writeln!(out, "  \"targets\": [{}],", target_list.join(", "));
    let _ = writeln!(
        out,
        "  \"compiler\": \"{}\",",
        json_escape(&inventory.compiler)
    );
    let features: Vec<String> = inventory
        .features
        .iter()
        .map(|feature| format!("\"{}\"", json_escape(feature)))
        .collect();
    let _ = writeln!(out, "  \"features\": [{}],", features.join(", "));
    let roots: Vec<String> = inventory
        .include_roots
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|root| format!("\"{}\"", json_escape(&root)))
        .collect();
    let _ = writeln!(out, "  \"include_roots\": [{}],", roots.join(", "));
    let families: Vec<String> = selection
        .families
        .iter()
        .map(|family| format!("\"{}\"", json_escape(family)))
        .collect();
    let _ = writeln!(out, "  \"families\": [{}],", families.join(", "));
    let _ = writeln!(
        out,
        "  \"integrity\": {{\"ok\": {}, \"failures\": [{}]}},",
        inventory.integrity.is_empty(),
        inventory
            .integrity
            .iter()
            .map(|failure| format!("\"{}\"", json_escape(failure)))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let unapproved = inventory.diffs.iter().filter(|diff| !diff.approved).count();
    let _ = writeln!(
        out,
        "  \"conformance\": {{\"ok\": {}, \"unapproved\": {}, \"approved\": {}}},",
        unapproved == 0,
        unapproved,
        inventory.diffs.len() - unapproved
    );
    let coverage: Vec<String> = inventory
        .coverage
        .iter()
        .map(|(key, value)| format!("\"{}\": {}", json_escape(key), value))
        .collect();
    let _ = writeln!(out, "  \"coverage\": {{{}}},", coverage.join(", "));
    let uncovered: Vec<String> = inventory
        .uncovered
        .iter()
        .map(|name| format!("\"{}\"", json_escape(name)))
        .collect();
    let _ = writeln!(out, "  \"uncovered\": [{}],", uncovered.join(", "));
    out.push_str("  \"differences\": [\n");
    for (index, diff) in inventory.diffs.iter().enumerate() {
        let _ = write!(
            out,
            "    {{\"family\": \"{}\", \"kind\": \"{}\", \"name\": \"{}\", \"arch\": \"{}\", \
             \"expected\": \"{}\", \"actual\": \"{}\", \"approved\": {}, \"detail\": \"{}\"}}",
            json_escape(&diff.family),
            diff.kind,
            json_escape(&diff.name),
            json_escape(&diff.arch),
            json_escape(&diff.expected.render()),
            json_escape(&diff.actual.render()),
            diff.approved,
            json_escape(&diff.detail)
        );
        if index + 1 != inventory.diffs.len() {
            out.push(',');
        }
        out.push('\n');
    }
    out.push_str("  ]\n}\n");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("cannot create {}", parent.display()))?;
    }
    std::fs::write(path, out).with_context(|| format!("cannot write {}", path.display()))?;
    Ok(())
}

/// Run the FreeBSD profile oracle for every selected target.
pub fn verify(tables: &Tables, values: &Values, options: &Options<'_>) -> Result<Report> {
    let selection = Selection::parse(options.families)?;
    let targets = targets(options.target)?;
    let mut inventory = Inventory::default();
    let mut report = Report::default();
    inventory.compiler = run_command(options.clang, &["--version".to_string()])
        .ok()
        .and_then(|text| text.lines().next().map(|line| line.to_string()))
        .unwrap_or_else(|| "unknown".to_string());
    inventory.features.push("__BSD_VISIBLE=1".to_string());

    let mut donor_revision = options.revision.trim().to_string();
    if donor_revision.is_empty() {
        inventory
            .integrity
            .push("donors.lock has no freebsd revision".to_string());
    }
    if options.donor.join(".git").exists() {
        match donor_head(options.donor) {
            Ok(head) if head == donor_revision => {}
            Ok(head) => inventory.integrity.push(format!(
                "FreeBSD donor HEAD is {head} but donors.lock pins {donor_revision}"
            )),
            Err(error) => inventory
                .integrity
                .push(format!("cannot read the FreeBSD donor revision: {error:#}")),
        }
    }
    let version = read_donor_file(options.donor, "sys/sys/param.h")
        .ok()
        .and_then(|text| freebsd_version(&text));
    if version.is_none() {
        donor_revision.clear();
        inventory.integrity.push(
            "FreeBSD donor sys/sys/param.h is missing or has no __FreeBSD_version".to_string(),
        );
    }
    for record in &tables.structs.record {
        if !record.c.starts_with("struct ") && !record.c.starts_with("union ") {
            inventory.uncovered.push(format!(
                "{}.fields (experiment: add the donor tag spelling to structs.toml before field-level comparison; size/align are measured)",
                record.name
            ));
        }
    }

    let temp =
        std::env::temp_dir().join(format!("raven-libc-hdrgen-freebsd-{}", std::process::id()));
    std::fs::create_dir_all(&temp)?;
    let mut total_constants = 0usize;
    let mut total_records = 0usize;
    let mut tree_label = String::new();
    let mut tree_stats = (0usize, 0usize);
    for arch in &targets {
        let tree = match materialize(options.donor, arch, &temp) {
            Ok(tree) => tree,
            Err(error) => {
                inventory.integrity.push(format!(
                    "{arch}: cannot construct the donor include tree: {error:#}"
                ));
                continue;
            }
        };
        tree_label = tree.label.clone();
        tree_stats = (tree.checkout_files, tree.git_files);
        inventory
            .include_roots
            .push(tree.include.display().to_string());
        let mut donor = Side::default();
        match donor_includes(options.clang, arch, &tree) {
            Ok(includes) => measure_side(
                tables,
                &includes,
                &temp.join(format!("measure-{arch}")),
                &mut donor,
                &mut inventory.integrity,
            ),
            Err(error) => {
                inventory
                    .integrity
                    .push(format!("{arch}: donor includes unavailable: {error:#}"));
                continue;
            }
        }
        let mut raven = Side::default();
        match raven_includes(options.clang, arch, options.root, options.abi) {
            Ok(includes) => measure_side(
                tables,
                &includes,
                &temp.join(format!("measure-raven-{arch}")),
                &mut raven,
                &mut inventory.integrity,
            ),
            Err(error) => {
                inventory
                    .integrity
                    .push(format!("{arch}: Raven includes unavailable: {error:#}"));
                continue;
            }
        }
        let mut rust = BTreeMap::new();
        for group in &tables.groups {
            rust.extend(read_rust_constants(options.root, &group.stem));
        }
        for dir in [
            "libc/ravenlibc/include",
            "libc/ravenlibc/abi/generated/include",
        ] {
            inventory
                .include_roots
                .push(options.root.join(dir).display().to_string());
        }
        inventory.include_roots.push(
            options
                .root
                .join(format!("libc/ravenlibc/arch/{arch}/include"))
                .display()
                .to_string(),
        );
        let rust_layouts = read_rust_layouts(options.root);
        compare(
            tables,
            arch,
            &donor,
            &raven,
            &rust,
            &rust_layouts,
            &mut inventory,
        );
        total_constants += donor.constants.len();
        total_records += donor.records.len();
        report.lines.push(format!(
            "{arch}: donor constants {}, donor records {}, donor types {}",
            donor.constants.len(),
            donor.records.len(),
            donor.types.len()
        ));
    }
    let _ = std::fs::remove_dir_all(&temp);

    if total_constants == 0 {
        inventory
            .integrity
            .push("zero constants were measured from the donor headers".to_string());
    }
    if total_records == 0 {
        inventory
            .integrity
            .push("zero record layouts were measured from the donor headers".to_string());
    }
    inventory
        .coverage
        .insert("constants".to_string(), total_constants);
    inventory
        .coverage
        .insert("records".to_string(), total_records);
    inventory
        .coverage
        .insert("differences".to_string(), inventory.diffs.len());
    inventory
        .coverage
        .insert("checkout_files".to_string(), tree_stats.0);
    inventory
        .coverage
        .insert("git_files".to_string(), tree_stats.1);

    report.lines.push(format!(
        "oracle: FreeBSD {} (__FreeBSD_version {})",
        donor_revision,
        version.map_or("unknown".to_string(), |value| value.to_string())
    ));
    report
        .lines
        .push(format!("compiler: {}", inventory.compiler));
    report
        .lines
        .push(format!("features: {}", inventory.features.join(", ")));
    report.lines.push(format!(
        "include roots: {}",
        inventory
            .include_roots
            .iter()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
            .join("; ")
    ));
    report.lines.push(format!(
        "donor source: {tree_label}; checkout files {}, git-object files {}",
        tree_stats.0, tree_stats.1
    ));
    report
        .lines
        .push(format!("targets: {}", targets.join(", ")));
    report.lines.push(format!(
        "families: {}",
        selection
            .families
            .iter()
            .cloned()
            .collect::<Vec<_>>()
            .join(", ")
    ));
    if inventory.integrity.is_empty() {
        report.lines.push(format!(
            "oracle integrity: PASS ({} constants, {} records)",
            total_constants, total_records
        ));
    } else {
        report.lines.push(format!(
            "oracle integrity: FAIL ({} problem(s))",
            inventory.integrity.len()
        ));
        for failure in &inventory.integrity {
            report.failures.push(format!("oracle integrity: {failure}"));
        }
    }

    inventory.diffs.sort_by(|left, right| {
        (&left.family, &left.kind, &left.name, &left.arch).cmp(&(
            &right.family,
            &right.kind,
            &right.name,
            &right.arch,
        ))
    });
    let mut family_counts: BTreeMap<String, usize> = BTreeMap::new();
    for diff in &inventory.diffs {
        if !diff.approved {
            *family_counts.entry(diff.family.clone()).or_default() += 1;
        }
    }
    let mut selected_differences = 0usize;
    for family in FAMILIES {
        let count = family_counts.get(family).copied().unwrap_or(0);
        let state = if count == 0 { "PASS" } else { "FAIL" };
        let selected = selection.wants(family);
        let marker = if selected { "gated" } else { "inventory" };
        report.lines.push(format!(
            "family {family:<9} {state} ({count} unapproved difference(s), {marker})"
        ));
        if selected {
            selected_differences += count;
        }
    }
    if inventory.integrity.is_empty() && selected_differences == 0 {
        report
            .lines
            .push("Raven conformance: PASS for the selected families".to_string());
    } else {
        report.lines.push(format!(
            "Raven conformance: FAIL ({} unapproved difference(s) in the gated families)",
            selected_differences
        ));
    }
    report
        .lines
        .push("\ndifference inventory (expected donor -> actual Raven):".to_string());
    for diff in &inventory.diffs {
        let approval = if diff.approved { "APPROVED" } else { "RED" };
        report.lines.push(format!(
            "{approval} {:<9} {:<18} {} [{}]: {} -> {} ({})",
            diff.family,
            diff.kind,
            diff.name,
            diff.arch,
            diff.expected.render(),
            diff.actual.render(),
            diff.detail
        ));
    }
    let red_total = inventory.diffs.iter().filter(|diff| !diff.approved).count();
    report.lines.push(format!(
        "\ncoverage: {} constants x {} targets, {} record layouts x {} targets, {} named difference(s), {} approved",
        total_constants,
        targets.len(),
        total_records,
        targets.len(),
        red_total,
        inventory.diffs.len() - red_total
    ));
    if !inventory.uncovered.is_empty() {
        report.lines.push("uncovered public types:".to_string());
        for gap in &inventory.uncovered {
            report.lines.push(format!("  {gap}"));
        }
    }

    for diff in &inventory.diffs {
        if diff.approved || !selection.wants(&diff.family) {
            continue;
        }
        report.failures.push(format!(
            "{} {} on {}: donor {} != Raven {} ({})",
            diff.kind,
            diff.name,
            diff.arch,
            diff.expected.render(),
            diff.actual.render(),
            diff.detail
        ));
    }

    if let Some(path) = options.report {
        write_inventory(
            path,
            &donor_revision,
            version,
            &tree_label,
            &targets,
            &selection,
            &inventory,
        )?;
        report
            .lines
            .push(format!("inventory written: {}", path.display()));
    }
    report.lines.push(format!(
        "table values: {} evaluated constants",
        values.by_name.len()
    ));
    Ok(report)
}

#[cfg(test)]
#[path = "freebsd_ut.rs"]
mod tests;

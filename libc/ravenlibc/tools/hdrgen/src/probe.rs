// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The clang/musl probes of doc 14 4.7: L2 layout, H1-H3 constants, the musl measurement,
//! the compiler identity check and the generated-header compile.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::layout::{FieldType, Layouts, c_member, c_opaque_blob};
use crate::model::{ARCHES, ConstDef};
use crate::validate::{Kind, Tables, Values, expected_header, type_kinds};
use crate::values::DefineSet;

const SELF: &str = "raven-libc-hdrgen";

/// One measured `size/align` pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout {
    /// Size in bytes.
    pub size: i64,
    /// Alignment in bytes.
    pub align: i64,
}

impl Layout {
    fn label(self) -> String {
        format!("{}/{}", self.size, self.align)
    }
}

/// Everything a verification run produced.
#[derive(Debug, Default)]
pub struct Report {
    /// Human-readable evidence lines.
    pub lines: Vec<String>,
    /// Failures; empty means success.
    pub failures: Vec<String>,
    /// `Some(reason)` means the run was skipped and the CLI should exit 77.
    pub skip: Option<String>,
}

impl Report {
    fn line(&mut self, text: impl Into<String>) {
        self.lines.push(text.into());
    }

    fn fail(&mut self, text: impl Into<String>) {
        self.failures.push(text.into());
    }
}

/// A temporary directory removed on drop.
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(tag: &str) -> Result<Self> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let serial = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("{SELF}-{tag}-{}-{serial}", std::process::id()));
        std::fs::create_dir_all(&path)
            .with_context(|| format!("cannot create {}", path.display()))?;
        Ok(Self { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Whether the run is required to succeed (tests set `RL_ABI_REQUIRED=1`).
#[must_use]
pub fn required() -> bool {
    std::env::var("RL_ABI_REQUIRED").is_ok_and(|value| value == "1")
}

fn tool_exists(tool: &str) -> bool {
    Command::new(tool)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// Check clang and sed; `Ok(None)` means "skip with this reason".
pub fn tools(clang: &str, sed: &str) -> Result<Option<(String, String)>> {
    let mut missing = Vec::new();
    if !tool_exists(clang) {
        missing.push(format!("clang ({clang})"));
    }
    if !tool_exists(sed) {
        missing.push(format!("sed ({sed})"));
    }
    if missing.is_empty() {
        return Ok(Some((clang.to_string(), sed.to_string())));
    }
    let reason = format!("missing required tool: {}", missing.join(", "));
    if required() {
        bail!("{reason} and RL_ABI_REQUIRED=1");
    }
    eprintln!("SKIP: {reason}");
    Ok(None)
}

fn run_command(program: &str, args: &[String]) -> Result<String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .with_context(|| format!("cannot run {program}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "{program} {} failed ({}): {}",
            args.join(" "),
            output.status,
            stderr.trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn resource_include(clang: &str) -> Result<PathBuf> {
    let output = run_command(clang, &["-print-resource-dir".to_string()])?;
    let root = PathBuf::from(output.trim());
    Ok(root.join("include"))
}

/// Copy a directory tree recursively.
fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to).with_context(|| format!("cannot create {}", to.display()))?;
    let mut entries: Vec<PathBuf> = std::fs::read_dir(from)
        .with_context(|| format!("cannot read {}", from.display()))?
        .flatten()
        .map(|entry| entry.path())
        .collect();
    entries.sort();
    for entry in entries {
        let name = entry
            .file_name()
            .with_context(|| format!("bad entry {}", entry.display()))?
            .to_os_string();
        let destination = to.join(name);
        if entry.is_dir() {
            copy_tree(&entry, &destination)?;
        } else {
            std::fs::copy(&entry, &destination)
                .with_context(|| format!("cannot copy {}", entry.display()))?;
        }
    }
    Ok(())
}

/// Build the musl include tree of one architecture under `temp`.
fn musl_tree(musl: &Path, sed: &str, arch: &str, temp: &Path) -> Result<PathBuf> {
    let tree = temp.join(arch);
    if tree.join("bits/alltypes.h").is_file() {
        return Ok(tree);
    }
    copy_tree(&musl.join("include"), &tree)?;
    let bits = tree.join("bits");
    std::fs::create_dir_all(&bits)?;
    copy_tree(&musl.join("arch/generic/bits"), &bits)?;
    copy_tree(&musl.join(format!("arch/{arch}/bits")), &bits)?;
    let script = musl.join("tools/mkalltypes.sed");
    let generic = musl.join("include/alltypes.h.in");
    let specific = musl.join(format!("arch/{arch}/bits/alltypes.h.in"));
    let output = run_command(
        sed,
        &[
            "-f".to_string(),
            script.display().to_string(),
            specific.display().to_string(),
            generic.display().to_string(),
        ],
    )?;
    std::fs::write(bits.join("alltypes.h"), output)?;
    Ok(tree)
}

/// Parse `label:` -> value from clang `-S` output.
fn parse_assembly(output: &str, labels: &BTreeSet<String>) -> BTreeMap<String, i128> {
    let mut values = BTreeMap::new();
    let mut wanted: Option<String> = None;
    for line in output.lines() {
        let trimmed = line.trim();
        if let Some(label) = trimmed.strip_suffix(':')
            && labels.contains(label)
        {
            wanted = Some(label.to_string());
            continue;
        }
        if let Some(label) = &wanted {
            let mut fields = trimmed.split_whitespace();
            let Some(directive) = fields.next() else {
                continue;
            };
            if !directive.starts_with('.') {
                continue;
            }
            let Some(token) = fields.next() else {
                continue;
            };
            if let Ok(value) = token.parse::<i128>() {
                values.insert(label.clone(), value);
                wanted = None;
            }
        }
    }
    values
}

/// Run clang on `source` and return stdout.
fn clang_run(
    clang: &str,
    target: &str,
    resource: &Path,
    tree: Option<&Path>,
    source: &Path,
    extra: &[String],
    assembly: bool,
) -> Result<String> {
    let mut args = vec![format!("--target={target}"), "-nostdinc".to_string()];
    args.push("-isystem".to_string());
    args.push(resource.display().to_string());
    if let Some(tree) = tree {
        args.push("-isystem".to_string());
        args.push(tree.display().to_string());
    }
    for flag in extra {
        args.push(flag.clone());
    }
    if assembly {
        args.push("-S".to_string());
        args.push("-o".to_string());
        args.push("-".to_string());
    } else {
        args.push("-fsyntax-only".to_string());
    }
    args.push(source.display().to_string());
    run_command(clang, &args)
}

/// The L2 probe: local C definitions matching the TOML field lists, with static assertions.
pub fn layout_probe(tables: &Tables, arch: &str) -> Result<String> {
    let layouts: Layouts = crate::validate::layouts(tables, arch)?;
    let mut out = String::new();
    out.push_str("#include <stddef.h>\n\n");
    for opaque in &tables.structs.opaque {
        let layout = crate::layout::opaque_layout(opaque, arch)?;
        let _ = writeln!(
            out,
            "{}",
            c_opaque_blob(
                opaque,
                crate::layout::OpaqueLayout {
                    size: layout.size,
                    align: layout.align
                }
            )
        );
    }
    out.push('\n');
    let ordered = record_order(tables)?;
    for record in &ordered {
        let _ = writeln!(out, "struct __ab_{} {{", record.name);
        for field in &record.fields {
            let ty = crate::layout::parse_field_type(&field.ty)?;
            let declaration = c_member(&ty, &field.name, &|name| {
                tables
                    .structs
                    .record
                    .iter()
                    .any(|record| record.name == name)
            });
            let _ = writeln!(out, "    {declaration};");
        }
        let _ = writeln!(out, "}};");
    }
    out.push('\n');
    for opaque in &tables.structs.opaque {
        let layout = crate::layout::opaque_layout(opaque, arch)?;
        let _ = writeln!(
            out,
            "_Static_assert(sizeof(__ab_{}) == {}, \"size {}\");",
            opaque.name, layout.size, opaque.name
        );
        let _ = writeln!(
            out,
            "_Static_assert(_Alignof(__ab_{}) == {}, \"align {}\");",
            opaque.name, layout.align, opaque.name
        );
    }
    for record in &ordered {
        let computed = layouts
            .computed
            .get(&record.name)
            .with_context(|| format!("record {} was not computed", record.name))?;
        let _ = writeln!(
            out,
            "_Static_assert(sizeof(struct __ab_{}) == {}, \"size {}\");",
            record.name, computed.size, record.name
        );
        let _ = writeln!(
            out,
            "_Static_assert(_Alignof(struct __ab_{}) == {}, \"align {}\");",
            record.name, computed.align, record.name
        );
        for field in &computed.fields {
            let _ = writeln!(
                out,
                "_Static_assert(offsetof(struct __ab_{}, {}) == {}, \"offset {}.{}\");",
                record.name, field.name, field.offset, record.name, field.name
            );
        }
    }
    Ok(out)
}

fn record_order(tables: &Tables) -> Result<Vec<&crate::model::RecordDef>> {
    let by_name: BTreeMap<&str, &crate::model::RecordDef> = tables
        .structs
        .record
        .iter()
        .map(|record| (record.name.as_str(), record))
        .collect();
    let mut ordered = Vec::new();
    let mut done = BTreeSet::new();
    let mut visiting = BTreeSet::new();
    for record in &tables.structs.record {
        visit_record(record, &by_name, &mut ordered, &mut done, &mut visiting)?;
    }
    Ok(ordered)
}

fn visit_record<'a>(
    record: &'a crate::model::RecordDef,
    by_name: &BTreeMap<&str, &'a crate::model::RecordDef>,
    ordered: &mut Vec<&'a crate::model::RecordDef>,
    done: &mut BTreeSet<String>,
    visiting: &mut BTreeSet<String>,
) -> Result<()> {
    if done.contains(&record.name) {
        return Ok(());
    }
    if !visiting.insert(record.name.clone()) {
        bail!("record {} embeds itself", record.name);
    }
    for field in &record.fields {
        if let FieldType::Named(name) = crate::layout::parse_field_type(&field.ty)?
            && let Some(nested) = by_name.get(name.as_str())
        {
            visit_record(nested, by_name, ordered, done, visiting)?;
        }
    }
    visiting.remove(&record.name);
    done.insert(record.name.clone());
    ordered.push(record);
    Ok(())
}

/// Build the musl measurement probe of doc 14 3.A.3.
#[must_use]
pub fn musl_probe_source(tables: &Tables) -> String {
    let mut out = String::new();
    out.push_str("#define _GNU_SOURCE\n#include <stddef.h>\n");
    for header in [
        "setjmp.h",
        "signal.h",
        "ucontext.h",
        "fenv.h",
        "sys/stat.h",
        "pthread.h",
        "sys/socket.h",
        "netinet/in.h",
        "sys/un.h",
        "netdb.h",
        "sys/resource.h",
        "sys/utsname.h",
        "time.h",
        "dirent.h",
        "poll.h",
        "sched.h",
        "semaphore.h",
        "stdio.h",
        "wchar.h",
        "sys/select.h",
        "termios.h",
        "spawn.h",
        "glob.h",
        "regex.h",
        "locale.h",
        "aio.h",
        "mqueue.h",
        "sys/statvfs.h",
        "sys/epoll.h",
        "fcntl.h",
        "pwd.h",
        "grp.h",
        "sys/file.h",
        "sys/time.h",
    ] {
        let _ = writeln!(out, "#include <{header}>");
    }
    out.push('\n');
    for ty in &tables.types.types {
        let Some(probe) = &ty.ctype_probe else {
            continue;
        };
        let _ = writeln!(out, "const int sz_{probe} = sizeof({probe});");
        let _ = writeln!(out, "const int al_{probe} = _Alignof({probe});");
    }
    for record in &tables.structs.record {
        let Some(musl) = &record.musl else {
            continue;
        };
        let _ = writeln!(out, "const int sz_{} = sizeof({musl});", record.name);
        let _ = writeln!(out, "const int al_{} = _Alignof({musl});", record.name);
        if !record.deviates.is_empty() {
            // The documented musl difference makes the field set a different record; the
            // size/align comparison above is the meaningful one.
            continue;
        }
        for field in &record.fields {
            if field.musl_field.as_deref() == Some("") {
                // A named padding field with no musl counterpart.
                continue;
            }
            let name = field.musl_field.as_deref().unwrap_or(&field.name);
            let _ = writeln!(
                out,
                "const int off_{}_{} = offsetof({musl}, {name});",
                record.name, field.name
            );
        }
    }
    for opaque in &tables.structs.opaque {
        let Some(musl) = &opaque.musl else {
            continue;
        };
        let _ = writeln!(out, "const int sz_{} = sizeof({musl});", opaque.name);
        let _ = writeln!(out, "const int al_{} = _Alignof({musl});", opaque.name);
    }
    out
}

/// Measure the musl rows on `arch`.
fn measure_musl(
    tables: &Tables,
    arch: &str,
    clang: &str,
    sed: &str,
    musl: &Path,
    resource: &Path,
    temp: &TempDir,
) -> Result<BTreeMap<String, Layout>> {
    let tree = musl_tree(musl, sed, arch, temp.path())?;
    let probe = temp.path().join(format!("musl-probe-{arch}.c"));
    std::fs::write(&probe, musl_probe_source(tables))?;
    let mut labels = BTreeSet::new();
    for ty in &tables.types.types {
        if let Some(name) = &ty.ctype_probe {
            labels.insert(format!("sz_{name}"));
            labels.insert(format!("al_{name}"));
        }
    }
    for record in &tables.structs.record {
        if record.musl.is_some() {
            labels.insert(format!("sz_{}", record.name));
            labels.insert(format!("al_{}", record.name));
            for field in &record.fields {
                labels.insert(format!("off_{}_{}", record.name, field.name));
            }
        }
    }
    for opaque in &tables.structs.opaque {
        if opaque.musl.is_some() {
            labels.insert(format!("sz_{}", opaque.name));
            labels.insert(format!("al_{}", opaque.name));
        }
    }
    let output = clang_run(
        clang,
        &format!("{arch}-unknown-linux-gnu"),
        resource,
        Some(&tree),
        &probe,
        &[],
        true,
    )
    .with_context(|| format!("musl layout probe on {arch}"))?;
    let raw = parse_assembly(&output, &labels);
    let mut measured = BTreeMap::new();
    let mut insert = |name: &str, size: i128, align: i128| -> Result<()> {
        measured.insert(
            name.to_string(),
            Layout {
                size: i64::try_from(size)?,
                align: i64::try_from(align)?,
            },
        );
        Ok(())
    };
    for ty in &tables.types.types {
        if let Some(name) = &ty.ctype_probe {
            let size = raw
                .get(&format!("sz_{name}"))
                .copied()
                .with_context(|| format!("musl {arch} did not print sizeof({name})"))?;
            let align = raw
                .get(&format!("al_{name}"))
                .copied()
                .with_context(|| format!("musl {arch} did not print _Alignof({name})"))?;
            insert(name, size, align)?;
        }
    }
    for record in &tables.structs.record {
        if record.musl.is_none() {
            continue;
        }
        let size = raw
            .get(&format!("sz_{}", record.name))
            .copied()
            .with_context(|| format!("musl {arch} did not print sizeof({})", record.name))?;
        let align = raw
            .get(&format!("al_{}", record.name))
            .copied()
            .with_context(|| format!("musl {arch} did not print _Alignof({})", record.name))?;
        insert(&record.name, size, align)?;
    }
    for opaque in &tables.structs.opaque {
        if opaque.musl.is_none() {
            continue;
        }
        let size = raw
            .get(&format!("sz_{}", opaque.name))
            .copied()
            .with_context(|| format!("musl {arch} did not print sizeof({})", opaque.name))?;
        let align = raw
            .get(&format!("al_{}", opaque.name))
            .copied()
            .with_context(|| format!("musl {arch} did not print _Alignof({})", opaque.name))?;
        insert(&opaque.name, size, align)?;
    }
    Ok(measured)
}

/// Ours for one measured name on `arch`.
fn ours_layout(tables: &Tables, name: &str, arch: &str) -> Result<Layout> {
    if let Some(ty) = tables.types.types.iter().find(|ty| ty.name == name) {
        let size = i64::from(ty.bits / 8);
        return Ok(Layout { size, align: size });
    }
    if let Some(opaque) = tables
        .structs
        .opaque
        .iter()
        .find(|opaque| opaque.name == name)
    {
        let layout = crate::layout::opaque_layout(opaque, arch)?;
        return Ok(Layout {
            size: layout.size,
            align: layout.align,
        });
    }
    let layouts = crate::validate::layouts(tables, arch)?;
    let record = layouts
        .computed
        .get(name)
        .with_context(|| format!("{name} is not a record"))?;
    Ok(Layout {
        size: record.size,
        align: record.align,
    })
}

/// Public headers that define one group's constants, for the H1/H3 probe.
fn probe_includes(stem: &str) -> &'static [&'static str] {
    match stem {
        "errno" => &["errno.h"],
        "signal" => &["signal.h"],
        "fcntl" => &["fcntl.h"],
        "mman" => &["sys/mman.h"],
        "stat" => &["sys/stat.h", "dirent.h"],
        "socket" => &["sys/socket.h", "netinet/tcp.h", "netinet/udp.h"],
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

/// The constant probe of H1/H3: every group entry whose musl macro is a constant expression.
fn constant_probe(stem: &str, defs: &[ConstDef], define_set: &DefineSet) -> String {
    let mut out = String::from("#define _GNU_SOURCE\n");
    for header in probe_includes(stem) {
        let _ = writeln!(out, "#include <{header}>");
    }
    out.push('\n');
    for constant in defs {
        if define_set.constant_value(&constant.name).is_none() {
            continue;
        }
        let _ = writeln!(out, "#ifdef {}", constant.name);
        let _ = writeln!(
            out,
            "const long long m_{} = (long long)({});",
            constant.name, constant.name
        );
        let _ = writeln!(out, "#else");
        let _ = writeln!(out, "const int missing_{} = 1;", constant.name);
        let _ = writeln!(out, "#endif");
    }
    out
}

#[allow(clippy::too_many_arguments)] // explicit probe inputs; the host tool has no state object
fn probe_constants(
    stem: &str,
    defs: &[ConstDef],
    define_set: &DefineSet,
    arch: &str,
    clang: &str,
    sed: &str,
    musl: &Path,
    resource: &Path,
    temp: &TempDir,
) -> Result<BTreeMap<String, i128>> {
    let tree = musl_tree(musl, sed, arch, temp.path())?;
    let source = constant_probe(stem, defs, define_set);
    let probe = temp.path().join(format!("const-probe-{arch}.c"));
    std::fs::write(&probe, source)?;
    let labels: BTreeSet<String> = defs
        .iter()
        .map(|constant| format!("m_{}", constant.name))
        .collect();
    let output = clang_run(
        clang,
        &format!("{arch}-unknown-linux-gnu"),
        resource,
        Some(&tree),
        &probe,
        &[],
        true,
    )
    .with_context(|| format!("constant probe on {arch}"))?;
    Ok(parse_assembly(&output, &labels))
}

fn same_mod_64(left: i128, right: i128) -> bool {
    (left as u128 as u64) == (right as u128 as u64)
}

fn musl_source_files(musl: &Path, sources: &[String]) -> Vec<PathBuf> {
    sources.iter().map(|source| musl.join(source)).collect()
}

fn parse_macro_map(text: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for line in text.lines() {
        let Some(rest) = line.strip_prefix("#define ") else {
            continue;
        };
        let mut fields = rest.splitn(2, char::is_whitespace);
        let (Some(name), Some(value)) = (fields.next(), fields.next()) else {
            continue;
        };
        map.insert(name.to_string(), value.trim().to_string());
    }
    map
}

/// Verify everything for the selected targets.
#[allow(clippy::too_many_arguments)] // the host tool passes explicit paths and tools
pub fn verify(
    tables: &Tables,
    values: &Values,
    target: &str,
    abi_dir: &Path,
    resources: &Path,
    clang: &str,
    sed: &str,
) -> Result<Report> {
    let mut report = Report::default();
    let musl = resources.join("musl");
    if !musl.is_dir() {
        if required() {
            bail!("musl checkout not found at {}", musl.display());
        }
        report.skip = Some(format!(
            "SKIP: musl checkout not found at {}",
            musl.display()
        ));
        report.line("SKIP: musl checkout not found");
        return Ok(report);
    }
    let resource = resource_include(clang)?;
    let targets: Vec<&str> = match target {
        "all" => ARCHES.to_vec(),
        other if ARCHES.contains(&other) => vec![other],
        other => bail!("unknown --target {other:?}; expected x86_64, aarch64, riscv64 or all"),
    };
    let temp = TempDir::new("verify")?;
    report.line(format!(
        "clang: {}",
        run_command(clang, &["--version".to_string()])?
            .lines()
            .next()
            .unwrap_or("")
    ));

    // L2: the local C definitions must match every stated offset, size and alignment.
    for arch in &targets {
        let probe = temp.path().join(format!("layout-{arch}.c"));
        std::fs::write(&probe, layout_probe(tables, arch)?)?;
        clang_run(
            clang,
            &format!("{arch}-unknown-linux-gnu"),
            &resource,
            None,
            &probe,
            &["-Werror".to_string()],
            false,
        )
        .with_context(|| format!("L2 layout probe on {arch}"))?;
        report.line(format!("L2 layout: {arch} ok"));
    }

    // The musl measurement of doc 14 3.A.3, reproduced on all three architectures.
    let mut measured: BTreeMap<String, BTreeMap<String, Layout>> = BTreeMap::new();
    for arch in ARCHES {
        measured.insert(
            arch.to_string(),
            measure_musl(tables, arch, clang, sed, &musl, &resource, &temp)?,
        );
    }

    let mut layout_differences: Vec<(String, String, Layout, Layout)> = Vec::new();
    // The recorded musl values in structs.toml must equal the measurement.
    for record in &tables.structs.record {
        for (arch, recorded) in &record.deviates {
            if let Some(measured) = measured
                .get(arch)
                .and_then(|rows| rows.get(&record.name))
                .copied()
                && (measured.size != recorded.size || measured.align != recorded.align)
            {
                report.fail(format!(
                    "structs.toml: {} deviates on {arch} says {}/{}, musl measures {}/{}",
                    record.name, recorded.size, recorded.align, measured.size, measured.align
                ));
            }
        }
    }
    report.line("\nmusl probe (size/align):");
    report.line(format!(
        "{:<26} {:>12} {:>12} {:>12} {:>12}  status",
        "type", "musl x86_64", "musl aarch64", "musl riscv64", "ours"
    ));
    let mut probe_names: Vec<String> = measured
        .get("x86_64")
        .map(|rows| rows.keys().cloned().collect())
        .unwrap_or_default();
    probe_names.sort();
    for name in probe_names {
        let mut columns = Vec::new();
        let mut ours_column = String::new();
        let mut differs = false;
        for arch in ARCHES {
            let musl_layout = measured.get(arch).and_then(|rows| rows.get(&name)).copied();
            columns.push(musl_layout.map_or_else(|| "-".to_string(), Layout::label));
            let ours = ours_layout(tables, &name, arch)?;
            if ours_column.is_empty() {
                ours_column = ours.label();
            }
            if let Some(musl_layout) = musl_layout
                && musl_layout != ours
            {
                differs = true;
                layout_differences.push((name.clone(), arch.to_string(), ours, musl_layout));
            }
        }
        report.line(format!(
            "{:<26} {:>12} {:>12} {:>12} {:>12}  {}",
            name,
            columns.first().cloned().unwrap_or_default(),
            columns.get(1).cloned().unwrap_or_default(),
            columns.get(2).cloned().unwrap_or_default(),
            ours_column,
            if differs { "differs" } else { "equal" }
        ));
    }

    // H1 (x86_64) and H3 (aarch64 fcntl) constant probes.
    let mut constant_differences: Vec<(String, String, i128, i128)> = Vec::new();
    for group in &tables.groups {
        if group.def.source.is_empty() {
            continue;
        }
        let files = musl_source_files(&musl, &group.def.source);
        let define_set = DefineSet::from_files(&files)?;
        let arches: &[&str] = if group.stem == "fcntl" {
            &["x86_64", "aarch64"]
        } else {
            &["x86_64"]
        };
        for arch in arches {
            let probed = probe_constants(
                &group.stem,
                &group.def.consts,
                &define_set,
                arch,
                clang,
                sed,
                &musl,
                &resource,
                &temp,
            )?;
            for constant in &group.def.consts {
                if define_set.constant_value(&constant.name).is_none() {
                    continue;
                }
                let Some(ours) = values.by_name.get(&constant.name) else {
                    continue;
                };
                if let Some(&their) = probed.get(&format!("m_{}", constant.name))
                    && !same_mod_64(ours.value, their)
                {
                    constant_differences.push((
                        constant.name.clone(),
                        (*arch).to_string(),
                        ours.value,
                        their,
                    ));
                }
            }
        }
        // H2: completeness of the group's source prefixes.
        let mut missing = Vec::new();
        for name in define_set.names_with_prefixes(&group.def.prefixes) {
            if define_set.constant_value(name).is_none() {
                continue;
            }
            let listed = group
                .def
                .consts
                .iter()
                .any(|constant| constant.name == name)
                || group.def.omit.iter().any(|omit| omit.name == name);
            if !listed {
                missing.push(name.to_string());
            }
        }
        if !missing.is_empty() {
            report.fail(format!(
                "H2 {}: {} musl constant(s) neither in the table nor omitted: {}",
                group.stem,
                missing.len(),
                missing.join(", ")
            ));
        }
        for omit in &group.def.omit {
            if define_set.constant_value(&omit.name).is_some()
                && group
                    .def
                    .consts
                    .iter()
                    .any(|constant| constant.name == omit.name)
            {
                report.fail(format!(
                    "H2 {}: {} is both present and omitted",
                    group.stem, omit.name
                ));
            }
        }
    }

    // H3: the aarch64 O_* difference set is exactly the five documented flags.
    let expected: BTreeSet<&str> = [
        "O_DIRECTORY",
        "O_NOFOLLOW",
        "O_DIRECT",
        "O_LARGEFILE",
        "O_TMPFILE",
    ]
    .into_iter()
    .collect();
    let actual: BTreeSet<&str> = constant_differences
        .iter()
        .filter(|(_, arch, _, _)| arch == "aarch64")
        .map(|(name, _, _, _)| name.as_str())
        .collect();
    if actual != expected {
        report.fail(format!(
            "H3: aarch64 fcntl difference set is {:?}, expected {:?}",
            actual, expected
        ));
    } else {
        report.line("\nH3: aarch64 fcntl difference set exactly {O_DIRECTORY, O_NOFOLLOW, O_DIRECT, O_LARGEFILE, O_TMPFILE}");
    }

    // Compare the measured difference set with deviations.toml.
    let kinds = type_kinds(tables);
    let mut used_musl_rows = BTreeSet::new();
    for (name, arch, ours, theirs) in &layout_differences {
        let row = tables.deviations.deviation.iter().find(|row| {
            row.against == "musl"
                && row.name == *name
                && kinds.get(name) == Some(&Kind::Layout)
                && row.ours_layout(arch) == Some((ours.size, ours.align))
                && row.theirs_layout(arch) == Some((theirs.size, theirs.align))
        });
        match row {
            Some(row) => {
                used_musl_rows.insert(row.name.clone());
            }
            None => report.fail(format!(
                "undocumented musl layout difference: {name} on {arch}: ours {}, musl {}",
                ours.label(),
                theirs.label()
            )),
        }
    }
    for (name, arch, ours, theirs) in &constant_differences {
        let row = tables.deviations.deviation.iter().find(|row| {
            row.against == "musl"
                && row.name == *name
                && kinds.get(name) == Some(&Kind::Constant)
                && row.ours_int(arch) == Some(*ours)
                && row
                    .theirs_int(arch)
                    .is_some_and(|value| same_mod_64(value, *theirs))
        });
        match row {
            Some(row) => {
                used_musl_rows.insert(row.name.clone());
            }
            None => report.fail(format!(
                "undocumented musl constant difference: {name} on {arch}: ours {ours}, musl {theirs}"
            )),
        }
    }
    for row in &tables.deviations.deviation {
        if row.against != "musl" || used_musl_rows.contains(&row.name) {
            continue;
        }
        let covered = layout_differences
            .iter()
            .any(|(name, _, _, _)| *name == row.name)
            || constant_differences
                .iter()
                .any(|(name, _, _, _)| *name == row.name);
        if !covered {
            report.fail(format!(
                "stale deviations.toml row: {} ({}) is not an actual difference",
                row.name, row.against
            ));
        }
    }

    // Compiler identity (D2-D4).
    let mut identity_failures = Vec::new();
    for (index, arch) in ARCHES.iter().enumerate() {
        let macros = parse_macro_map(&run_command(
            clang,
            &[
                format!("--target={arch}-unknown-ravenbsd"),
                "-E".to_string(),
                "-dM".to_string(),
                "-x".to_string(),
                "c".to_string(),
                "/dev/null".to_string(),
            ],
        )?);
        let char = tables
            .types
            .types
            .iter()
            .find(|ty| ty.name == "char")
            .context("types.toml has no char row")?;
        let expected_unsigned = !char.per_arch.get(*arch).map_or(char.signed, |ty| ty.signed);
        let char_unsigned = macros.contains_key("__CHAR_UNSIGNED__");
        if char_unsigned != expected_unsigned {
            identity_failures.push(format!(
                "{arch}: __CHAR_UNSIGNED__ is {char_unsigned}, types.toml says {expected_unsigned}"
            ));
        }
        let wchar = tables
            .types
            .types
            .iter()
            .find(|ty| ty.name == "wchar_t")
            .context("types.toml has no wchar_t row")?;
        let expected_wchar = wchar
            .per_arch
            .get(*arch)
            .map_or(wchar.c.as_str(), |ty| ty.c.as_str());
        if macros.get("__WCHAR_TYPE__").map(String::as_str) != Some(expected_wchar) {
            identity_failures.push(format!(
                "{arch}: __WCHAR_TYPE__ is {:?}, types.toml says {expected_wchar:?}",
                macros.get("__WCHAR_TYPE__")
            ));
        }
        let wint = ["int", "int", "unsigned int"];
        if macros.get("__WINT_TYPE__").map(String::as_str) != Some(wint[index]) {
            identity_failures.push(format!(
                "{arch}: __WINT_TYPE__ is {:?}, doc 14 D4 says {:?}",
                macros.get("__WINT_TYPE__"),
                wint[index]
            ));
        }
        for (macro_name, expected) in [
            ("__SIZE_TYPE__", "long unsigned int"),
            ("__INT64_TYPE__", "long int"),
        ] {
            if macros.get(macro_name).map(String::as_str) != Some(expected) {
                identity_failures.push(format!(
                    "{arch}: {macro_name} is {:?}, expected {expected:?}",
                    macros.get(macro_name)
                ));
            }
        }
        let intmax = macros.get("__INTMAX_TYPE__").map(String::as_str);
        if intmax != Some("long int") && intmax != Some("long") {
            identity_failures.push(format!(
                "{arch}: __INTMAX_TYPE__ is {:?}, expected long",
                macros.get("__INTMAX_TYPE__")
            ));
        }
        let mantissa = if *arch == "x86_64" { "64" } else { "113" };
        if macros.get("__LDBL_MANT_DIG__").map(String::as_str) != Some(mantissa) {
            identity_failures.push(format!(
                "{arch}: __LDBL_MANT_DIG__ is {:?}, expected {mantissa}",
                macros.get("__LDBL_MANT_DIG__")
            ));
        }
        if macros.get("__SIZEOF_LONG_DOUBLE__").map(String::as_str) != Some("16") {
            identity_failures.push(format!(
                "{arch}: __SIZEOF_LONG_DOUBLE__ is {:?}, expected 16",
                macros.get("__SIZEOF_LONG_DOUBLE__")
            ));
        }
    }
    if identity_failures.is_empty() {
        report.line("\ncompiler identity: -dM matches types.toml and doc 14 D2-D4");
    } else {
        for failure in identity_failures {
            report.fail(failure);
        }
    }

    // The C-header leg: every generated header compiles standalone per target, in the default
    // mode and with every visibility class switched on.
    let include = abi_dir.join("generated/include");
    let headers: Vec<String> = tables
        .groups
        .iter()
        .map(|group| expected_header(&group.stem))
        .collect();
    for arch in &targets {
        for header in &headers {
            let probe = temp.path().join("header.c");
            std::fs::write(&probe, format!("#include <{header}>\n"))?;
            let args = vec![
                "-Werror".to_string(),
                "-I".to_string(),
                include.display().to_string(),
            ];
            clang_run(
                clang,
                &format!("{arch}-unknown-ravenbsd"),
                &resource,
                None,
                &probe,
                &args,
                false,
            )
            .with_context(|| format!("header {header} on {arch}"))?;
            let mut visible = args;
            for class in [
                "__BSD_VISIBLE",
                "__GNU_VISIBLE",
                "__LINUX_VISIBLE",
                "__SOLARIS_VISIBLE",
            ] {
                visible.push(format!("-D{class}=1"));
            }
            clang_run(
                clang,
                &format!("{arch}-unknown-ravenbsd"),
                &resource,
                None,
                &probe,
                &visible,
                false,
            )
            .with_context(|| format!("header {header} on {arch} with all visibility classes"))?;
        }
    }
    report.line(format!(
        "\nC headers: {} headers x {} target(s) compile standalone (-Werror, default and all-visible)",
        headers.len(),
        targets.len()
    ));

    let constants: usize = tables
        .groups
        .iter()
        .map(|group| group.def.consts.len())
        .sum();
    report.line(format!(
        "\ncounts: {} constants in {} groups, {} records, {} opaque types, {} musl deviations, {} linux-abi deviations, {} differences measured",
        constants,
        tables.groups.len(),
        tables.structs.record.len(),
        tables.structs.opaque.len(),
        tables.deviations.deviation.iter().filter(|row| row.against == "musl").count(),
        tables.deviations.deviation.iter().filter(|row| row.against == "linux-abi").count(),
        constant_differences.len() + layout_differences.len()
    ));
    Ok(report)
}

/// The constant probe source of one group, for tests.
#[cfg(test)]
#[must_use]
pub fn constant_probe_source(stem: &str, group: &crate::model::ConstFile) -> String {
    let mut set = DefineSet::default();
    for constant in &group.consts {
        set.definitions
            .insert(constant.name.clone(), "1".to_string());
    }
    constant_probe(stem, &group.consts, &set)
}

/// The value of an `IntOrTable`, for tests.
#[cfg(test)]
#[must_use]
pub fn int_or_table(value: &crate::model::IntOrTable, arch: &str) -> Option<i64> {
    value.get(arch)
}

#[cfg(test)]
#[path = "probe_ut.rs"]
mod tests;

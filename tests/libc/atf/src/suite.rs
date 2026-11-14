// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Suite descriptions, generation from vendored trees, and the per-suite runner.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::build::{self, Builder};
use crate::exclude::ExcludeFile;
use crate::gen_make;
use crate::gen_meson;
use crate::owners::Owners;
use crate::result::parse_result;
use crate::runner::{self, RunOptions};
use crate::table;
use crate::{
    BuildStatus, Case, Framework, Mode, Outcome, RunBackend, SuiteRun, TpCase, Verdict, glob_match,
};

/// A suite description (`tests/libc-oracle/suites/<suite>.toml`).
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SuiteFile {
    /// Schema version; C71 writes 1.
    pub schema: u32,
    /// Suite name, `[a-z0-9-]+`.
    pub suite: String,
    /// Donor key of `donors.lock`.
    pub donor: String,
    /// Test framework.
    pub framework: Framework,
    /// Root under `tests/libc-oracle`.
    pub root: String,
    /// Pass `-s <shadow source dir>` for ATF tests that read data files.
    #[serde(default)]
    pub srcdir_for_tests: bool,
    /// Shell tests (`TESTS_SH+=`): listed, never dropped.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shell_tests: Vec<String>,
    /// `external` for LTP; empty otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build: Option<String>,
    /// A prebuilt directory supplied at run time (LTP only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prebuilt: Option<String>,
    /// The programs.
    #[serde(default, rename = "program")]
    pub programs: Vec<Program>,
}

/// One program in a suite.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Program {
    /// Path relative to the suite root, without `.c`; unique in the suite.
    pub name: String,
    /// The program's `.c` files, relative to the suite root.
    pub sources: Vec<String>,
    /// Whitelisted compiler flags.
    #[serde(default)]
    pub cflags: Vec<String>,
    /// Whitelisted link flags.
    #[serde(default)]
    pub ldadd: Vec<String>,
    /// `ok`, `unparsed`, `needs-dynamic`, `needs-pg:<n>`, `needs-root`, `needs-fs`, `needs-sh`.
    #[serde(default = "ok_status")]
    pub status: String,
    /// Why `status` is not `ok`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_reason: Option<String>,
    /// Exit status that means pass (exit-status framework).
    #[serde(default)]
    pub expect_exit: i32,
    /// Arguments for an exit-status program.
    #[serde(default)]
    pub args: Vec<String>,
}

fn ok_status() -> String {
    "ok".to_string()
}

impl Program {
    /// The status as a build status.
    #[must_use]
    pub fn build_status(&self) -> BuildStatus {
        match self.status.as_str() {
            "ok" => BuildStatus::Built,
            "unparsed" => BuildStatus::Unparsed(
                self.status_reason
                    .clone()
                    .unwrap_or_else(|| "unparsed".to_string()),
            ),
            other => BuildStatus::Needs(other.to_string()),
        }
    }
}

/// One suite generation spec.
struct Spec {
    suite: &'static str,
    donor: &'static str,
    framework: Framework,
    root: &'static str,
    kind: Kind,
}

enum Kind {
    Make,
    Mlibc,
    Picolibc,
    Pjdfstest,
    ExternalLtp,
}

const SPECS: &[Spec] = &[
    Spec {
        suite: "netbsd-libc",
        donor: "netbsd",
        framework: Framework::Atf,
        root: "vendor/netbsd/libc",
        kind: Kind::Make,
    },
    Spec {
        suite: "netbsd-libm",
        donor: "netbsd",
        framework: Framework::Atf,
        root: "vendor/netbsd/libm",
        kind: Kind::Make,
    },
    Spec {
        suite: "netbsd-libpthread",
        donor: "netbsd",
        framework: Framework::Atf,
        root: "vendor/netbsd/libpthread",
        kind: Kind::Make,
    },
    Spec {
        suite: "netbsd-libposix",
        donor: "netbsd",
        framework: Framework::Atf,
        root: "vendor/netbsd/libposix",
        kind: Kind::Make,
    },
    Spec {
        suite: "netbsd-libcrypt",
        donor: "netbsd",
        framework: Framework::Atf,
        root: "vendor/netbsd/libcrypt",
        kind: Kind::Make,
    },
    Spec {
        suite: "openbsd-libc",
        donor: "openbsd",
        framework: Framework::ExitStatus,
        root: "vendor/openbsd/libc",
        kind: Kind::Make,
    },
    Spec {
        suite: "openbsd-libm",
        donor: "openbsd",
        framework: Framework::ExitStatus,
        root: "vendor/openbsd/libm",
        kind: Kind::Make,
    },
    Spec {
        suite: "openbsd-libpthread",
        donor: "openbsd",
        framework: Framework::ExitStatus,
        root: "vendor/openbsd/libpthread",
        kind: Kind::Make,
    },
    Spec {
        suite: "freebsd-libc",
        donor: "freebsd",
        framework: Framework::Atf,
        root: "vendor/freebsd/libc",
        kind: Kind::Make,
    },
    Spec {
        suite: "freebsd-msun",
        donor: "freebsd",
        framework: Framework::Atf,
        root: "vendor/freebsd/msun",
        kind: Kind::Make,
    },
    Spec {
        suite: "freebsd-libthr",
        donor: "freebsd",
        framework: Framework::Atf,
        root: "vendor/freebsd/libthr",
        kind: Kind::Make,
    },
    Spec {
        suite: "picolibc",
        donor: "picolibc",
        framework: Framework::ExitStatus,
        root: "vendor/picolibc/test",
        kind: Kind::Picolibc,
    },
    Spec {
        suite: "mlibc",
        donor: "mlibc",
        framework: Framework::ExitStatus,
        root: "vendor/mlibc",
        kind: Kind::Mlibc,
    },
    Spec {
        suite: "pjdfstest",
        donor: "pjdfstest",
        framework: Framework::Tap,
        root: "vendor/pjdfstest/tests",
        kind: Kind::Pjdfstest,
    },
    Spec {
        suite: "ltp",
        donor: "ltp",
        framework: Framework::ExitStatus,
        root: "",
        kind: Kind::ExternalLtp,
    },
];

/// Generate one or all suite files from the vendored trees.
///
/// `oracle` is `tests/libc-oracle` (the directory containing `vendor/` and `suites/`).
pub fn generate(oracle: &Path, out: &Path, only: Option<&str>, date: &str) -> Result<Vec<String>> {
    let mut written = Vec::new();
    for spec in SPECS {
        if only.is_some_and(|name| name != spec.suite) {
            continue;
        }
        let suite = match &spec.kind {
            Kind::ExternalLtp => external_ltp(spec),
            _ => {
                let root = oracle.join(spec.root);
                if !root.is_dir() {
                    if only.is_some() {
                        bail!(
                            "suite {}: vendor root {} does not exist (run the C35 vendor command first)",
                            spec.suite,
                            root.display()
                        );
                    }
                    eprintln!("gen-suite: skipped {} (no {})", spec.suite, root.display());
                    continue;
                }
                let programs = match &spec.kind {
                    Kind::Make => make_programs(&root, spec.framework)?,
                    Kind::Mlibc => gen_meson::scan_mlibc(&root)?,
                    Kind::Picolibc => gen_meson::scan_picolibc(&root)?,
                    Kind::Pjdfstest => gen_meson::scan_pjdfstest(&root)?,
                    Kind::ExternalLtp => unreachable!(),
                };
                let shell_tests = match &spec.kind {
                    Kind::Make => gen_make::scan(&root)?.shell_tests,
                    _ => Vec::new(),
                };
                SuiteFile {
                    schema: 1,
                    suite: spec.suite.to_string(),
                    donor: spec.donor.to_string(),
                    framework: spec.framework,
                    root: spec.root.to_string(),
                    srcdir_for_tests: true,
                    shell_tests,
                    build: None,
                    prebuilt: None,
                    programs: programs
                        .into_iter()
                        .map(|program| Program {
                            name: program.name,
                            sources: program.sources,
                            cflags: program.cflags,
                            ldadd: program.ldadd,
                            status: program.status,
                            status_reason: program.status_reason,
                            expect_exit: 0,
                            args: Vec::new(),
                        })
                        .collect(),
                }
            }
        };
        let text = render_suite(&suite, date)?;
        let destination = out.join(format!("{}.toml", suite.suite));
        fs::create_dir_all(out).with_context(|| format!("cannot create {}", out.display()))?;
        fs::write(&destination, text)
            .with_context(|| format!("cannot write {}", destination.display()))?;
        println!(
            "gen-suite: {} programs={} shell-tests={}",
            suite.suite,
            suite.programs.len(),
            suite.shell_tests.len()
        );
        written.push(suite.suite.clone());
    }
    Ok(written)
}

fn make_programs(root: &Path, _framework: Framework) -> Result<Vec<gen_make::MakeProgram>> {
    let scan = gen_make::scan(root)?;
    Ok(scan.programs)
}

fn external_ltp(spec: &Spec) -> SuiteFile {
    SuiteFile {
        schema: 1,
        suite: spec.suite.to_string(),
        donor: spec.donor.to_string(),
        framework: spec.framework,
        root: String::new(),
        srcdir_for_tests: false,
        shell_tests: Vec::new(),
        build: Some("external".to_string()),
        prebuilt: Some("<dir>".to_string()),
        programs: Vec::new(),
    }
}

fn render_suite(suite: &SuiteFile, date: &str) -> Result<String> {
    let body = toml::to_string_pretty(suite).context("cannot serialise suite")?;
    Ok(format!(
        "# SPDX-License-Identifier: BSD-3-Clause\n# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors\n# Generated by raven-atf-run gen-suite on {date}; reviewed by hand.\n\n{body}"
    ))
}

/// Read a committed suite file.
pub fn load(path: &Path) -> Result<SuiteFile> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("cannot read suite {}", path.display()))?;
    let suite: SuiteFile =
        toml::from_str(&text).with_context(|| format!("cannot parse suite {}", path.display()))?;
    validate(&suite)?;
    Ok(suite)
}

/// Validate names and schema.
pub fn validate(suite: &SuiteFile) -> Result<()> {
    if suite.schema != 1 {
        bail!(
            "suite {} has schema {}, expected 1",
            suite.suite,
            suite.schema
        );
    }
    if suite.suite.is_empty()
        || !suite.suite.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
    {
        bail!("suite name `{}` is not [a-z0-9-]+", suite.suite);
    }
    let mut seen = BTreeSet::new();
    for program in &suite.programs {
        if program.name.is_empty() {
            bail!("suite {} has an empty program name", suite.suite);
        }
        if !seen.insert(&program.name) {
            bail!("suite {} repeats program `{}`", suite.suite, program.name);
        }
        for source in &program.sources {
            if source.starts_with('/') || source.contains("..") {
                bail!(
                    "suite {} program {}: source `{source}` must be vendor-relative",
                    suite.suite,
                    program.name
                );
            }
        }
    }
    Ok(())
}

/// Find a suite file by name under `tests/libc-oracle/suites`.
pub fn find(oracle: &Path, name: &str) -> Result<SuiteFile> {
    let path = oracle.join("suites").join(format!("{name}.toml"));
    if !path.is_file() {
        bail!("unknown suite `{name}` (no {})", path.display());
    }
    load(&path)
}

/// Options for `raven-atf-run suite`.
#[derive(Clone, Debug)]
pub struct RunConfig {
    /// Repository root.
    pub root: PathBuf,
    /// Suite description.
    pub suite: SuiteFile,
    /// Oracle or target.
    pub mode: Mode,
    /// What executes target binaries.
    pub backend: RunBackend,
    /// C compiler command.
    pub cc: String,
    /// Output root.
    pub out: PathBuf,
    /// Program filter.
    pub filter: Option<String>,
    /// Parallel programs.
    pub jobs: usize,
    /// Maximum timeout per case.
    pub max_timeout: u64,
    /// Keep work directories.
    pub keep: bool,
    /// ATF config variables.
    pub vars: BTreeMap<String, String>,
    /// Target sysroot.
    pub sysroot: Option<PathBuf>,
}

/// Run one suite in one mode.
pub fn run(config: &RunConfig) -> Result<SuiteRun> {
    if config.mode == Mode::Target {
        match config.backend {
            RunBackend::HostLinux => {
                bail!("host-linux: raven-libc-sys has no Linux backend (O-B5)");
            }
            RunBackend::Qemu => {
                bail!(
                    "not implemented here: the on-target ATF run is a later ticket (needs raven-atf-run on target and PG-35/44/45/49, C73 covers hello..spawn only)"
                );
            }
            RunBackend::None => {}
        }
        let Some(_sysroot) = &config.sysroot else {
            bail!("target mode needs --sysroot DIR");
        };
        // A missing tree is not a usage error: every program is `not-run` with
        // `sysroot-incomplete:<path>` (ticket C71, 4.6). The runner never creates a sysroot.
    }
    let oracle = config.root.join("tests/libc-oracle");
    let exclusions = ExcludeFile::load(&oracle.join("excluded.toml"))?;
    let owners = Owners::load(&oracle.join("owners.toml"))?;
    let overrides = Overrides::load(&oracle.join("overrides.toml"))?;
    let mut suite = config.suite.clone();
    overrides.apply(&mut suite);
    if let Some(filter) = &config.filter {
        suite
            .programs
            .retain(|program| glob_match(filter, &program.name));
    }

    let incomplete = (config.mode == Mode::Target)
        .then(|| config.sysroot.as_deref().and_then(build::sysroot_missing))
        .flatten();
    let mut run = SuiteRun {
        suite: suite.suite.clone(),
        mode: config.mode,
        outcomes: Vec::new(),
        builds: BTreeMap::new(),
    };
    let mut program_jobs = Vec::new();
    for program in &suite.programs {
        let manual = exclusions.matched(&suite.suite, &program.name, None);
        let status = program.build_status();
        if let Some(exclusion) = manual {
            let case = Case::new(&suite.suite, &program.name, "unknown-bodies");
            run.outcomes.push(Outcome::new(
                case,
                Verdict::Excluded,
                format!("excluded:{}", exclusion.reason),
                "-",
            ));
            run.builds.insert(
                program.name.clone(),
                BuildStatus::Needs("excluded".to_string()),
            );
            continue;
        }
        match &status {
            BuildStatus::Unparsed(reason) => {
                let case = Case::new(&suite.suite, &program.name, "unknown-bodies");
                run.outcomes.push(Outcome::new(
                    case,
                    Verdict::Excluded,
                    format!("excluded:refused by vendor gate (REVIEW.txt): {reason}"),
                    "-",
                ));
                run.builds.insert(program.name.clone(), status);
                continue;
            }
            BuildStatus::Needs(token) if token.starts_with("needs-") => {
                let case = Case::new(&suite.suite, &program.name, "unknown-bodies");
                run.outcomes.push(Outcome::new(
                    case,
                    Verdict::Skip,
                    token.clone(),
                    owners.ticket(&suite.suite, &program.name),
                ));
                run.builds.insert(program.name.clone(), status);
                continue;
            }
            _ => {}
        }
        program_jobs.push(program.clone());
    }
    for shell in &suite.shell_tests {
        let case = Case::new(&suite.suite, shell, "shell-test");
        run.outcomes.push(Outcome::new(
            case,
            Verdict::Skip,
            "needs sh (O-B2)",
            owners.ticket(&suite.suite, shell),
        ));
        run.builds
            .insert(shell.clone(), BuildStatus::Needs("needs-sh".to_string()));
    }

    let builder = Builder {
        root: config.root.clone(),
        vendor: oracle.clone(),
        out: config.out.clone(),
        mode: config.mode,
        cc: config.cc.clone(),
        sysroot: config.sysroot.clone(),
    };
    if config.mode == Mode::Oracle && suite.framework == Framework::Atf {
        check_tool(&config.cc)?;
        builder.build_atf()?;
    }
    if config.mode == Mode::Target && incomplete.is_none() {
        check_tool(&config.cc)?;
        if suite.framework == Framework::Atf {
            builder.build_atf()?;
        }
    }

    let results = process_programs(
        &builder,
        &suite,
        &program_jobs,
        config,
        &owners,
        incomplete.as_deref(),
    )?;
    for result in results {
        let mut result = result?;
        run.outcomes.append(&mut result.outcomes);
        run.builds
            .insert(result.program.clone(), result.status.clone());
        write_program_log(config, &suite, &result)?;
        write_missing(config, &suite, &result)?;
        if let Some(cases) = result.cases {
            write_case_cache(config, &suite, &result.program, &cases)?;
        }
    }
    run.sort();
    write_run(config, &run, &suite)?;
    Ok(run)
}

fn process_programs(
    builder: &Builder,
    suite: &SuiteFile,
    programs: &[Program],
    config: &RunConfig,
    owners: &Owners,
    incomplete: Option<&Path>,
) -> Result<Vec<Result<ProgramResult>>> {
    let jobs = config.jobs.max(1).min(programs.len().max(1));
    let results: Mutex<Vec<Option<Result<ProgramResult>>>> =
        Mutex::new((0..programs.len()).map(|_| None).collect());
    let next = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..jobs {
            scope.spawn(|| {
                loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    if index >= programs.len() {
                        break;
                    }
                    let program = &programs[index];
                    let result =
                        process_program(builder, suite, config, owners, program, incomplete);
                    let mut slots = results.lock().expect("result slots poisoned");
                    slots[index] = Some(result);
                }
            });
        }
    });
    let slots = results.into_inner().expect("result slots poisoned");
    Ok(slots.into_iter().flatten().collect())
}

struct ProgramResult {
    program: String,
    outcomes: Vec<Outcome>,
    status: BuildStatus,
    log: String,
    missing_headers: BTreeSet<String>,
    missing_symbols: BTreeSet<String>,
    cases: Option<Vec<TpCase>>,
}

fn process_program(
    builder: &Builder,
    suite: &SuiteFile,
    config: &RunConfig,
    owners: &Owners,
    program: &Program,
    incomplete: Option<&Path>,
) -> Result<ProgramResult> {
    let ticket = owners.ticket(&suite.suite, &program.name);
    let build = program.build_status();
    if matches!(build, BuildStatus::Unparsed(_) | BuildStatus::Needs(_)) {
        return sync_skipped(suite, program, build, ticket);
    }
    if let Some(missing) = incomplete {
        let cases = cached_cases(config, suite, program);
        let reason = format!("sysroot-incomplete:{}", missing.display());
        return Ok(ProgramResult {
            program: program.name.clone(),
            outcomes: cases
                .iter()
                .map(|case| {
                    Outcome::new(
                        Case::new(&suite.suite, &program.name, &case.ident),
                        Verdict::NotRun,
                        reason.clone(),
                        &ticket,
                    )
                })
                .collect(),
            status: BuildStatus::Failed(reason),
            log: String::new(),
            missing_headers: BTreeSet::new(),
            missing_symbols: BTreeSet::new(),
            cases: Some(cases),
        });
    }
    let report = builder.build(&suite.suite, &suite.root, suite.framework, program)?;
    match &report.status {
        BuildStatus::Failed(_) => {
            let cases = cached_cases(config, suite, program);
            let reason = report.status.reason().to_string();
            let verdict = if config.mode == Mode::Oracle {
                Verdict::OracleFail
            } else {
                Verdict::Fail
            };
            let row_ticket = if config.mode == Mode::Oracle {
                "-"
            } else {
                ticket.as_str()
            };
            return Ok(ProgramResult {
                program: program.name.clone(),
                outcomes: cases
                    .iter()
                    .map(|case| {
                        Outcome::new(
                            Case::new(&suite.suite, &program.name, &case.ident),
                            verdict,
                            if config.mode == Mode::Oracle {
                                format!("oracle-build:{reason}")
                            } else {
                                reason.clone()
                            },
                            row_ticket,
                        )
                    })
                    .collect(),
                status: report.status,
                log: report.log,
                missing_headers: report.missing_headers,
                missing_symbols: report.missing_symbols,
                cases: Some(cases),
            });
        }
        BuildStatus::Needs(_) | BuildStatus::Unparsed(_) => unreachable!(),
        BuildStatus::Built => {}
    }

    let source_dir = builder
        .vendor
        .join(&suite.root)
        .join(Path::new(&program.name).parent().unwrap_or(Path::new("")));
    let mut cases = Vec::new();
    let mut outcomes = Vec::new();
    match suite.framework {
        Framework::Atf => {
            let cases_list = match runner::list(
                report
                    .binary
                    .as_deref()
                    .expect("built program has a binary"),
                Some(&source_dir),
                config.max_timeout,
            ) {
                Ok(cases) => cases,
                Err(error) => {
                    let case = Case::new(&suite.suite, &program.name, "unknown-bodies");
                    outcomes.push(Outcome::new(
                        case,
                        if config.mode == Mode::Oracle {
                            Verdict::OracleFail
                        } else {
                            Verdict::Fail
                        },
                        format!("oracle-behaviour:list failed: {error}"),
                        if config.mode == Mode::Oracle {
                            "-"
                        } else {
                            ticket.as_str()
                        },
                    ));
                    return Ok(ProgramResult {
                        program: program.name.clone(),
                        outcomes,
                        status: BuildStatus::Built,
                        log: report.log,
                        missing_headers: report.missing_headers,
                        missing_symbols: report.missing_symbols,
                        cases: None,
                    });
                }
            };
            for case in &cases_list {
                let observation = run_case(
                    config,
                    suite,
                    program,
                    report.binary.as_deref(),
                    case,
                    &source_dir,
                )?;
                let outcome = if config.mode == Mode::Oracle {
                    match observation.verdict {
                        Verdict::Fail => Outcome::new(
                            Case::new(&suite.suite, &program.name, &case.ident),
                            Verdict::OracleFail,
                            format!(
                                "oracle-behaviour:{}",
                                observation
                                    .reason
                                    .strip_prefix("behaviour:")
                                    .unwrap_or(&observation.reason)
                            ),
                            "-",
                        ),
                        other => Outcome::new(
                            Case::new(&suite.suite, &program.name, &case.ident),
                            other,
                            observation.reason,
                            &ticket,
                        ),
                    }
                } else {
                    Outcome::new(
                        Case::new(&suite.suite, &program.name, &case.ident),
                        observation.verdict,
                        observation.reason,
                        &ticket,
                    )
                };
                outcomes.push(outcome);
            }
            cases = cases_list;
        }
        Framework::ExitStatus => {
            let case = TpCase {
                ident: "run".to_string(),
                meta: BTreeMap::new(),
            };
            let observation = run_exit(config, suite, program, report.binary.as_deref())?;
            let outcome = if config.mode == Mode::Oracle && observation.verdict == Verdict::Fail {
                Outcome::new(
                    Case::new(&suite.suite, &program.name, &case.ident),
                    Verdict::OracleFail,
                    format!(
                        "oracle-behaviour:{}",
                        observation
                            .reason
                            .strip_prefix("behaviour:")
                            .unwrap_or(&observation.reason)
                    ),
                    "-",
                )
            } else {
                Outcome::new(
                    Case::new(&suite.suite, &program.name, &case.ident),
                    observation.verdict,
                    observation.reason,
                    &ticket,
                )
            };
            outcomes.push(outcome);
            cases.push(case);
        }
        Framework::Tap => {
            let case = TpCase {
                ident: "run".to_string(),
                meta: BTreeMap::new(),
            };
            outcomes.push(Outcome::new(
                Case::new(&suite.suite, &program.name, &case.ident),
                Verdict::Skip,
                "needs-fs",
                &ticket,
            ));
            cases.push(case);
        }
    }
    Ok(ProgramResult {
        program: program.name.clone(),
        outcomes,
        status: BuildStatus::Built,
        log: report.log,
        missing_headers: report.missing_headers,
        missing_symbols: report.missing_symbols,
        cases: Some(cases),
    })
}

fn run_case(
    config: &RunConfig,
    suite: &SuiteFile,
    program: &Program,
    binary: Option<&Path>,
    case: &TpCase,
    source_dir: &Path,
) -> Result<runner::Observation> {
    let Some(binary) = binary else {
        bail!("program {} has no binary", program.name);
    };
    if config.mode == Mode::Target {
        return Ok(runner::Observation {
            verdict: Verdict::NotRun,
            reason: "no-run-backend".to_string(),
        });
    }
    let work_root = config
        .out
        .join("work")
        .join(&suite.suite)
        .join(&program.name);
    fs::create_dir_all(&work_root)
        .with_context(|| format!("cannot create {}", work_root.display()))?;
    let options = RunOptions {
        timeout_secs: config.max_timeout,
        vars: config.vars.clone(),
        keep: config.keep,
    };
    runner::run_case(binary, case, Some(source_dir), &work_root, &options)
}

fn run_exit(
    config: &RunConfig,
    suite: &SuiteFile,
    program: &Program,
    binary: Option<&Path>,
) -> Result<runner::Observation> {
    let Some(binary) = binary else {
        bail!("program {} has no binary", program.name);
    };
    if config.mode == Mode::Target {
        return Ok(runner::Observation {
            verdict: Verdict::NotRun,
            reason: "no-run-backend".to_string(),
        });
    }
    let work_root = config
        .out
        .join("work")
        .join(&suite.suite)
        .join(&program.name);
    fs::create_dir_all(&work_root)
        .with_context(|| format!("cannot create {}", work_root.display()))?;
    let options = RunOptions {
        timeout_secs: config.max_timeout,
        vars: BTreeMap::new(),
        keep: config.keep,
    };
    runner::run_exit_status(
        binary,
        &program.args,
        program.expect_exit,
        &work_root,
        &options,
    )
}

fn sync_skipped(
    suite: &SuiteFile,
    program: &Program,
    build: BuildStatus,
    ticket: String,
) -> Result<ProgramResult> {
    let reason = build.reason().to_string();
    let case = Case::new(&suite.suite, &program.name, "unknown-bodies");
    Ok(ProgramResult {
        program: program.name.clone(),
        outcomes: vec![Outcome::new(case, Verdict::Skip, reason, ticket)],
        status: build,
        log: String::new(),
        missing_headers: BTreeSet::new(),
        missing_symbols: BTreeSet::new(),
        cases: None,
    })
}

fn cached_cases(config: &RunConfig, suite: &SuiteFile, program: &Program) -> Vec<TpCase> {
    let path = config
        .out
        .join("oracle")
        .join(&suite.suite)
        .join(format!("{}.list.tsv", program.name));
    let Ok(text) = fs::read_to_string(path) else {
        return vec![TpCase {
            ident: "unknown-bodies".to_string(),
            meta: BTreeMap::new(),
        }];
    };
    // An existing but empty cache is a real empty listing (some ATF programs add no cases on a
    // host); only a missing cache means the bodies are unknown.
    text.lines()
        .filter(|line| !line.is_empty())
        .map(|line| TpCase {
            ident: line.to_string(),
            meta: BTreeMap::new(),
        })
        .collect()
}

fn write_case_cache(
    config: &RunConfig,
    suite: &SuiteFile,
    program: &str,
    cases: &[TpCase],
) -> Result<()> {
    if config.mode != Mode::Oracle {
        return Ok(());
    }
    let path = config
        .out
        .join("oracle")
        .join(&suite.suite)
        .join(format!("{program}.list.tsv"));
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("cannot create {}", parent.display()))?;
    }
    let text: String = cases
        .iter()
        .map(|case| format!("{}\n", case.ident))
        .collect();
    fs::write(&path, text).with_context(|| format!("cannot write {}", path.display()))?;
    Ok(())
}

fn write_program_log(config: &RunConfig, suite: &SuiteFile, result: &ProgramResult) -> Result<()> {
    if result.log.is_empty() {
        return Ok(());
    }
    let path = config
        .out
        .join("build-logs")
        .join(config.mode.as_str())
        .join(&suite.suite)
        .join(format!("{}.log", result.program.replace('/', "_")));
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("cannot create {}", parent.display()))?;
    }
    fs::write(&path, &result.log).with_context(|| format!("cannot write {}", path.display()))?;
    Ok(())
}

fn write_missing(config: &RunConfig, suite: &SuiteFile, result: &ProgramResult) -> Result<()> {
    if result.missing_headers.is_empty() && result.missing_symbols.is_empty() {
        return Ok(());
    }
    let directory = config.out.join("missing");
    fs::create_dir_all(&directory)
        .with_context(|| format!("cannot create {}", directory.display()))?;
    let path = directory.join(format!("{}.tsv", suite.suite));
    let mut headers: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut symbols: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    if let Ok(text) = fs::read_to_string(&path) {
        for line in text.lines() {
            let fields: Vec<&str> = line.split('\t').collect();
            if fields.len() == 3 {
                match fields[0] {
                    "header" => {
                        headers
                            .entry(fields[1].to_string())
                            .or_default()
                            .insert(fields[2].to_string());
                    }
                    "symbol" => {
                        symbols
                            .entry(fields[1].to_string())
                            .or_default()
                            .insert(fields[2].to_string());
                    }
                    _ => {}
                }
            }
        }
    }
    for header in &result.missing_headers {
        headers
            .entry(header.clone())
            .or_default()
            .insert(result.program.clone());
    }
    for symbol in &result.missing_symbols {
        symbols
            .entry(symbol.clone())
            .or_default()
            .insert(result.program.clone());
    }
    let mut text = String::new();
    for (header, programs) in headers {
        for program in programs {
            text.push_str(&format!("header\t{header}\t{program}\n"));
        }
    }
    for (symbol, programs) in symbols {
        for program in programs {
            text.push_str(&format!("symbol\t{symbol}\t{program}\n"));
        }
    }
    fs::write(&path, text).with_context(|| format!("cannot write {}", path.display()))?;
    Ok(())
}

fn write_run(config: &RunConfig, run: &SuiteRun, suite: &SuiteFile) -> Result<()> {
    let directory = config.out.join(config.mode.as_str());
    fs::create_dir_all(&directory)
        .with_context(|| format!("cannot create {}", directory.display()))?;
    let tsv = directory.join(format!("{}.tsv", suite.suite));
    table::write_outcomes(&tsv, &run.outcomes)?;
    let builds = directory.join(format!("{}.builds.tsv", suite.suite));
    table::write_builds(&builds, &run.builds)?;
    println!(
        "suite {} {}: {} bodies, {} programs",
        suite.suite,
        config.mode.as_str(),
        run.outcomes.len(),
        run.builds.len()
    );
    Ok(())
}

/// Per-suite override of compiler flags (`overrides.toml`).
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Overrides {
    /// The override rows.
    #[serde(default, rename = "override")]
    pub entries: Vec<Override>,
}

/// One override row.
#[derive(Clone, Debug, Deserialize)]
pub struct Override {
    /// Suite name.
    pub suite: String,
    /// Program name.
    pub program: String,
    /// Flags to add.
    #[serde(default)]
    pub cflags: Vec<String>,
    /// Link flags to add.
    #[serde(default)]
    pub ldadd: Vec<String>,
    /// Extra sources to add (picolibc `test_extra_src`).
    #[serde(default)]
    pub sources: Vec<String>,
    /// Replace the exit status that means pass.
    #[serde(default)]
    pub expect_exit: Option<i32>,
    /// Replace the program arguments.
    #[serde(default)]
    pub args: Option<Vec<String>>,
}

impl Overrides {
    /// Load `overrides.toml`; a missing file is an empty table.
    pub fn load(path: &Path) -> Result<Self> {
        if !path.is_file() {
            return Ok(Self::default());
        }
        let text =
            fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
        toml::from_str(&text).with_context(|| format!("cannot parse {}", path.display()))
    }

    /// Apply matching rows to a suite.
    pub fn apply(&self, suite: &mut SuiteFile) {
        for row in &self.entries {
            if row.suite != suite.suite {
                continue;
            }
            if let Some(program) = suite
                .programs
                .iter_mut()
                .find(|program| program.name == row.program)
            {
                program.cflags.extend(row.cflags.iter().cloned());
                program.ldadd.extend(row.ldadd.iter().cloned());
                program.sources.extend(row.sources.iter().cloned());
                if let Some(expect_exit) = row.expect_exit {
                    program.expect_exit = expect_exit;
                }
                if let Some(args) = &row.args {
                    program.args = args.clone();
                }
            }
        }
    }
}

fn check_tool(cc: &str) -> Result<()> {
    let program = cc.split_whitespace().next().unwrap_or("cc");
    let ok = std::process::Command::new(program)
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success());
    if !ok {
        bail!("required host tool `{program}` is missing");
    }
    Ok(())
}

/// Read the first line of a result file for tests and reports.
#[must_use]
pub fn first_result_line(path: &Path) -> String {
    fs::read(path)
        .ok()
        .and_then(|bytes| parse_result(&String::from_utf8_lossy(&bytes)).ok())
        .map_or_else(String::new, |expect| expect.reason().to_string())
}

#[cfg(test)]
#[path = "suite_ut.rs"]
mod tests;

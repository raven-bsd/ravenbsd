// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the `raven` command contract.

use raven_cli::{Exit, Io};

fn run(args: &[&str]) -> (Exit, String, String) {
    run_with(args, false, true)
}

fn run_with(args: &[&str], is_tty: bool, no_color: bool) -> (Exit, String, String) {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let exit = {
        let mut io = Io {
            stdout: &mut stdout,
            stderr: &mut stderr,
            is_tty,
            no_color,
        };
        raven_cli::run(args.iter().copied(), &mut io)
    };
    (
        exit,
        String::from_utf8(stdout).unwrap(),
        String::from_utf8(stderr).unwrap(),
    )
}

fn json(text: &str) -> serde_json::Value {
    serde_json::from_str(text.trim()).unwrap()
}

const GENERAL_HELP: &str = concat!(
    "raven - RavenBSD administration client\n",
    "\n",
    "Usage: raven [OPTIONS] <COMMAND> [ARGUMENTS]\n",
    "\n",
    "Commands:\n",
    "  version       print version, schema and target information\n",
    "  help          show this help or help for a command\n",
    "  completion    print a shell completion script\n",
    "  status        show host health, services, load and memory\n",
    "  doctor        run ordered diagnostics\n",
    "  svc           list, inspect and control services\n",
    "  config        validate and show the effective TOML configuration\n",
    "  theme         preview, set, apply or export the Raven terminal theme\n",
    "  net           show network interfaces and IPv4 routes\n",
    "  log           reserved: log access\n",
    "  pkg           reserved: package access\n",
    "  user          reserved: user identities\n",
    "  image         reserved: image management\n",
    "\n",
    "Global options:\n",
    "  --json             emit NDJSON records\n",
    "  --quiet            suppress normal output\n",
    "  --verbose          extra diagnostics on stderr\n",
    "  --color=<policy>   auto, always or never (human output only)\n",
    "  --dry-run          print a plan and change nothing\n",
    "  --yes              assume yes for a destructive action\n",
    "  -h, --help         show this help\n",
    "  -V, --version      print the version\n",
);

const RESERVED: &[&str] = &["log", "pkg", "user", "image"];

const ALL_COMMANDS: &[&str] = &[
    "version",
    "help",
    "completion",
    "status",
    "doctor",
    "svc",
    "config",
    "theme",
    "net",
    "log",
    "pkg",
    "user",
    "image",
];

#[test]
fn json_version_has_the_contract_envelope() {
    let (exit, stdout, stderr) = run(&["--json", "version"]);
    assert_eq!(exit, Exit::Success);
    assert!(stderr.is_empty());
    let value = json(&stdout);
    assert_eq!(value["schema"], 1);
    assert_eq!(value["kind"], "version");
    assert_eq!(value["version"], "0.1.0");
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["target"], "x86_64-unknown-ravenbsd");
    assert!(value["build"].is_null());
    assert!(value["boot_id"].is_null());
    assert_eq!(stdout.lines().count(), 1);
}

#[test]
fn json_is_accepted_after_the_command() {
    let (exit, stdout, _) = run(&["version", "--json"]);
    assert_eq!(exit, Exit::Success);
    assert_eq!(json(&stdout)["kind"], "version");
}

#[test]
fn quiet_version_prints_nothing() {
    let (exit, stdout, stderr) = run(&["--quiet", "version"]);
    assert_eq!(exit, Exit::Success);
    assert!(stdout.is_empty());
    assert!(stderr.is_empty());
}

#[test]
fn json_and_quiet_are_a_usage_error() {
    let (exit, stdout, stderr) = run(&["--json", "--quiet", "version"]);
    assert_eq!(exit, Exit::Usage);
    assert!(stdout.is_empty());
    let value = json(&stderr);
    assert_eq!(value["kind"], "error");
    assert_eq!(value["code"], "EUSAGE");
}

#[test]
fn an_unknown_command_is_usage() {
    let (exit, stdout, stderr) = run(&["--json", "does-not-exist"]);
    assert_eq!(exit, Exit::Usage);
    assert!(stdout.is_empty());
    let value = json(&stderr);
    assert_eq!(value["code"], "EUSAGE");
    assert_eq!(value["context"]["command"], "does-not-exist");
}

#[test]
fn an_unknown_flag_is_usage() {
    let (exit, stdout, stderr) = run(&["--json", "--wat", "version"]);
    assert_eq!(exit, Exit::Usage);
    assert!(stdout.is_empty());
    assert_eq!(json(&stderr)["code"], "EUSAGE");
}

#[test]
fn host_status_doctor_and_svc_report_the_missing_backend() {
    for (args, command) in [
        (vec!["--json", "status"], "status"),
        (vec!["--json", "doctor"], "doctor"),
        (vec!["--json", "svc", "list"], "svc"),
    ] {
        let (exit, stdout, stderr) = run(&args);
        assert_eq!(exit, Exit::Failure, "{command}");
        assert!(stdout.is_empty(), "{command}");
        let value = json(&stderr);
        assert_eq!(value["code"], "EUNAVAILABLE", "{command}");
        assert_eq!(value["context"]["command"], command, "{command}");
    }
}

#[test]
fn reserved_commands_are_recognized_and_not_faked() {
    for name in RESERVED {
        let (exit, stdout, stderr) = run(&["--json", name]);
        assert_eq!(exit, Exit::Failure, "{name}");
        assert!(stdout.is_empty(), "{name}");
        let value = json(&stderr);
        assert_eq!(value["kind"], "error", "{name}");
        assert_eq!(value["code"], "EUNAVAILABLE", "{name}");
        assert_eq!(value["context"]["command"], *name, "{name}");
    }
}

#[test]
fn reserved_commands_have_a_human_error() {
    let (exit, stdout, stderr) = run(&["log"]);
    assert_eq!(exit, Exit::Failure);
    assert!(stdout.is_empty());
    assert!(stderr.contains("not available"));
}

#[test]
fn reserved_commands_reject_unknown_flags() {
    let (exit, stdout, stderr) = run(&["--json", "log", "--wat"]);
    assert_eq!(exit, Exit::Usage);
    assert!(stdout.is_empty());
    assert_eq!(json(&stderr)["code"], "EUSAGE");
}

#[test]
fn general_help_matches_the_golden_text() {
    let (exit, stdout, stderr) = run(&["help"]);
    assert_eq!(exit, Exit::Success);
    assert!(stderr.is_empty());
    assert_eq!(stdout, GENERAL_HELP);
}

#[test]
fn no_arguments_print_general_help() {
    let (exit, stdout, _) = run(&[]);
    assert_eq!(exit, Exit::Success);
    assert_eq!(stdout, GENERAL_HELP);
}

#[test]
fn help_for_a_command_shows_its_usage() {
    let (exit, stdout, _) = run(&["help", "version"]);
    assert_eq!(exit, Exit::Success);
    assert!(stdout.starts_with("Usage: raven version [--json]\n"));
    assert!(stdout.contains("print version, schema and target information"));
}

#[test]
fn help_flag_after_a_command_works() {
    let (exit, stdout, _) = run(&["version", "--help"]);
    assert_eq!(exit, Exit::Success);
    assert!(stdout.starts_with("Usage: raven version [--json]\n"));
}

#[test]
fn help_marks_reserved_commands() {
    let (exit, stdout, _) = run(&["help", "log"]);
    assert_eq!(exit, Exit::Success);
    assert!(stdout.contains("reserved"));
    assert!(stdout.contains("EUNAVAILABLE"));
}

#[test]
fn help_for_net_shows_the_read_only_usage() {
    let (exit, stdout, _) = run(&["help", "net"]);
    assert_eq!(exit, Exit::Success);
    assert!(stdout.contains("Usage: raven net <if|route|status> [--json]"));
    assert!(!stdout.contains("reserved"));
}

#[test]
fn help_for_an_unknown_command_is_usage() {
    let (exit, stdout, stderr) = run(&["help", "nope"]);
    assert_eq!(exit, Exit::Usage);
    assert!(stdout.is_empty());
    assert!(stderr.contains("unknown command"));
}

#[test]
fn completion_covers_all_three_shells_and_all_commands() {
    for (shell, marker) in [
        ("bash", "complete -F _raven raven"),
        ("zsh", "#compdef raven"),
        ("fish", "complete -c raven"),
    ] {
        let (exit, stdout, stderr) = run(&["completion", shell]);
        assert_eq!(exit, Exit::Success, "{shell}");
        assert!(stderr.is_empty(), "{shell}");
        assert!(stdout.contains(marker), "{shell}");
        for name in ALL_COMMANDS {
            assert!(stdout.contains(name), "{shell} misses {name}");
        }
        assert!(!stdout.contains("eval "), "{shell} evaluates shell text");
    }
}

#[test]
fn completion_needs_a_valid_shell() {
    let (exit, _, _) = run(&["completion"]);
    assert_eq!(exit, Exit::Usage);
    let (exit, _, _) = run(&["completion", "tcsh"]);
    assert_eq!(exit, Exit::Usage);
}

#[test]
fn the_double_dash_terminates_global_options() {
    let (exit, stdout, _) = run(&["--", "version"]);
    assert_eq!(exit, Exit::Success);
    assert!(stdout.starts_with("raven 0.1.0"));
}

#[test]
fn command_arguments_after_double_dash_are_positional() {
    let (exit, _, stderr) = run(&["version", "--", "--json"]);
    assert_eq!(exit, Exit::Usage);
    assert!(stderr.contains("version takes no arguments"));
}

#[test]
fn global_flags_are_accepted_after_the_command() {
    let (exit, _, _) = run(&[
        "version",
        "--verbose",
        "--color=never",
        "--dry-run",
        "--yes",
    ]);
    assert_eq!(exit, Exit::Success);
}

#[test]
fn an_invalid_colour_is_usage() {
    let (exit, _, stderr) = run(&["--color=purple", "version"]);
    assert_eq!(exit, Exit::Usage);
    assert!(stderr.contains("invalid --color"));
}

#[test]
fn json_without_a_command_is_usage() {
    let (exit, stdout, stderr) = run(&["--json"]);
    assert_eq!(exit, Exit::Usage);
    assert!(stdout.is_empty());
    assert_eq!(json(&stderr)["code"], "EUSAGE");
}

#[test]
fn human_version_names_the_target() {
    let (exit, stdout, _) = run(&["version"]);
    assert_eq!(exit, Exit::Success);
    assert_eq!(stdout, "raven 0.1.0 (x86_64-unknown-ravenbsd)\n");
}

#[test]
fn theme_json_preview_has_the_contract_envelope() {
    let (exit, stdout, stderr) = run(&["theme", "preview", "--json"]);
    assert_eq!(exit, Exit::Success);
    assert!(stderr.is_empty());
    assert_eq!(stdout.lines().count(), 1);
    assert!(!stdout.contains('\x1b'));
    let value = json(&stdout);
    assert_eq!(value["schema"], 1);
    assert_eq!(value["kind"], "theme");
    assert!(value["name"].is_string());
    let roles = value["roles"].as_object().expect("roles object");
    assert_eq!(roles.len(), 11);
    for (role, hex) in roles {
        assert!(role.chars().all(|c| c.is_ascii_lowercase()), "{role}");
        let hex = hex.as_str().expect("hex string");
        assert!(hex.starts_with('#'), "{role}");
        assert_eq!(hex.len(), 7, "{role}");
    }
}

#[test]
fn theme_preview_is_plain_when_piped() {
    let (exit, stdout, stderr) = run(&["theme", "preview"]);
    assert_eq!(exit, Exit::Success);
    assert!(stderr.is_empty());
    assert!(!stdout.contains('\x1b'));
    assert_eq!(stdout.lines().count(), 11);
    let first = stdout.lines().next().unwrap();
    assert!(first.starts_with("accent  #"), "{first}");
    assert_eq!(first.len(), "accent  #rrggbb".len(), "{first}");
}

#[test]
fn theme_preview_colours_a_terminal() {
    let (exit, stdout, _) = run_with(&["theme", "preview"], true, false);
    assert_eq!(exit, Exit::Success);
    assert!(stdout.contains('\x1b'));
    assert!(stdout.contains("accent"));
    assert!(stdout.contains('#'));
}

#[test]
fn theme_preview_respects_no_color_on_a_terminal() {
    let (exit, stdout, _) = run_with(&["theme", "preview"], true, true);
    assert_eq!(exit, Exit::Success);
    assert!(!stdout.contains('\x1b'));
}

#[test]
fn theme_preview_never_colours_json() {
    let (exit, stdout, _) = run_with(&["--json", "theme", "preview"], true, false);
    assert_eq!(exit, Exit::Success);
    assert!(!stdout.contains('\x1b'));
}

#[test]
fn color_always_forces_colour_into_a_pipe() {
    let (exit, stdout, _) = run_with(&["--color=always", "theme", "preview"], false, true);
    assert_eq!(exit, Exit::Success);
    assert!(stdout.contains('\x1b'));
}

#[test]
fn color_never_suppresses_a_terminal() {
    let (exit, stdout, _) = run_with(&["--color=never", "theme", "preview"], true, false);
    assert_eq!(exit, Exit::Success);
    assert!(!stdout.contains('\x1b'));
}

#[test]
fn theme_apply_dry_run_lists_the_paths() {
    let (exit, stdout, stderr) = run(&["--dry-run", "theme", "apply"]);
    assert_eq!(exit, Exit::Success);
    assert!(stderr.is_empty());
    assert!(stdout.starts_with("would write "));
    assert!(stdout.contains("theme.env"));
    assert!(!stdout.contains('\x1b'));
}

#[test]
fn theme_apply_dry_run_json_is_one_theme_record() {
    let (exit, stdout, stderr) = run(&["--json", "--dry-run", "theme", "apply"]);
    assert_eq!(exit, Exit::Success);
    assert!(stderr.is_empty());
    assert_eq!(stdout.lines().count(), 1);
    let value = json(&stdout);
    assert_eq!(value["schema"], 1);
    assert_eq!(value["kind"], "theme");
    assert_eq!(value["action"], "apply");
    assert!(value["dry_run"].as_bool().unwrap());
    assert!(value["path"].as_str().unwrap().ends_with("theme.env"));
    assert!(value["backup"].is_null() || value["backup"].is_string());
}

#[test]
fn theme_set_without_a_value_is_usage() {
    let (exit, stdout, stderr) = run(&["--json", "theme", "set"]);
    assert_eq!(exit, Exit::Usage);
    assert!(stdout.is_empty());
    assert_eq!(json(&stderr)["code"], "EUSAGE");
}

#[test]
fn theme_set_rejects_an_invalid_file_without_writing() {
    let path = std::env::temp_dir().join(format!(
        "raven-cli-invalid-theme-{}.toml",
        std::process::id()
    ));
    std::fs::write(&path, "[colors]\naccent = \"violet\"\n").unwrap();
    let (exit, stdout, stderr) = run(&["--json", "theme", "set", path.to_str().unwrap()]);
    let _ = std::fs::remove_file(&path);
    assert_eq!(exit, Exit::Failure);
    assert!(stdout.is_empty());
    assert_eq!(json(&stderr)["code"], "ECONFIG");
}

#[test]
fn theme_export_prints_the_env_spec() {
    let (exit, stdout, stderr) = run(&["theme", "export"]);
    assert_eq!(exit, Exit::Success);
    assert!(stderr.is_empty());
    assert_eq!(stdout.lines().count(), 1);
    assert!(!stdout.contains('\x1b'));
    let values: Vec<&str> = stdout.trim().split(':').collect();
    assert_eq!(values.len(), 11);
    for value in values {
        assert!(value.starts_with('#'));
        assert_eq!(value.len(), 7);
    }
}

#[test]
fn theme_export_shell_wraps_the_spec() {
    let (exit, stdout, stderr) = run(&["theme", "export", "--shell"]);
    assert_eq!(exit, Exit::Success);
    assert!(stderr.is_empty());
    assert!(stdout.starts_with("export RAVEN_PALETTE='#"));
    assert!(stdout.trim_end().ends_with('\''));
    assert!(!stdout.contains('\x1b'));
}

#[test]
fn theme_export_json_has_the_theme_record() {
    let (exit, stdout, stderr) = run(&["--json", "theme", "export"]);
    assert_eq!(exit, Exit::Success);
    assert!(stderr.is_empty());
    let value = json(&stdout);
    assert_eq!(value["schema"], 1);
    assert_eq!(value["kind"], "theme");
    assert!(value["name"].is_string());
    let palette = value["palette"].as_str().expect("palette");
    assert_eq!(palette.split(':').count(), 11);
    assert!(!stdout.contains('\x1b'));
}

#[test]
fn theme_export_rejects_extra_arguments() {
    let (exit, stdout, stderr) = run(&["--json", "theme", "export", "wat"]);
    assert_eq!(exit, Exit::Usage);
    assert!(stdout.is_empty());
    assert_eq!(json(&stderr)["code"], "EUSAGE");
}

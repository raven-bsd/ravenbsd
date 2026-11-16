// SPDX-License-Identifier: MIT
// Copyright (c) 2024 reuben olinsky

//! Host tests for the brush entry point.

use super::*;

    use anyhow::Result;
    use pretty_assertions::{assert_eq, assert_matches};

    fn args(strs: &[&str]) -> Vec<String> {
        strs.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn plus_options_are_options_only_before_operands() -> Result<()> {
        let parsed = CommandLineArgs::parse_shell_args(args(&[
            "brush", "+o", "errexit", "-o", "nounset", "+O", "extglob", "script", "+o", "+Oz",
        ]))?;
        assert_eq!(parsed.disabled_options, ["errexit"]);
        assert_eq!(parsed.enabled_options, ["nounset"]);
        assert_eq!(parsed.disabled_shopt_options, ["extglob"]);
        assert_eq!(parsed.script_args, ["script", "+o", "+Oz"]);

        let parsed = CommandLineArgs::parse_shell_args(args(&[
            "brush", "-xc", "echo", "arg0", "+o", "--", "+o",
        ]))?;
        assert_eq!(parsed.command.as_deref(), Some("echo"));
        assert_eq!(parsed.script_args, ["arg0", "+o", "--", "+o"]);
        Ok(())
    }

    #[test]
    fn parse_empty_args() -> Result<()> {
        let parsed_args = CommandLineArgs::parse_shell_args(args(&["brush"]))?;
        assert_matches!(parsed_args.script_args.as_slice(), []);
        Ok(())
    }

    #[test]
    fn parse_script_and_args() -> Result<()> {
        let parsed_args = CommandLineArgs::parse_shell_args(args(&[
            "brush",
            "some-script",
            "-x",
            "1",
            "--option",
        ]))?;
        assert_eq!(
            parsed_args.script_args,
            ["some-script", "-x", "1", "--option"]
        );
        Ok(())
    }

    #[test]
    fn parse_script_and_args_with_double_dash_in_script_args() -> Result<()> {
        let parsed_args = CommandLineArgs::parse_shell_args(args(&["brush", "some-script", "--"]))?;
        assert_eq!(parsed_args.script_args, ["some-script", "--"]);
        Ok(())
    }

    #[test]
    fn parse_unknown_args() {
        let result = CommandLineArgs::parse_shell_args(args(&["brush", "--unknown-option"]));
        assert!(result.is_err());
    }

    #[test]
    fn parse_c_with_double_dash_separator() -> Result<()> {
        let parsed_args =
            CommandLineArgs::parse_shell_args(args(&["brush", "-c", "--", "echo hello", "arg0"]))?;
        assert_eq!(parsed_args.command, Some("echo hello".to_string()));
        assert_eq!(parsed_args.script_args, ["arg0"]);
        Ok(())
    }

    #[test]
    fn parse_c_with_double_dash_no_command() {
        assert!(CommandLineArgs::parse_shell_args(args(&["brush", "-c", "--"])).is_err());
    }

    #[test]
    fn parse_c_with_double_dash_command_is_double_dash() -> Result<()> {
        let parsed_args =
            CommandLineArgs::parse_shell_args(args(&["brush", "-c", "--", "--", "echo", "hi"]))?;
        assert_eq!(parsed_args.command, Some("--".to_string()));
        assert_eq!(parsed_args.script_args, ["echo", "hi"]);
        Ok(())
    }

    #[test]
    fn parse_ec_with_double_dash_separator() -> Result<()> {
        let parsed_args =
            CommandLineArgs::parse_shell_args(args(&["brush", "-ec", "--", "echo hello", "arg0"]))?;
        assert_eq!(parsed_args.command, Some("echo hello".to_string()));
        assert!(parsed_args.exit_on_nonzero_command_exit);
        assert_eq!(parsed_args.script_args, ["arg0"]);
        Ok(())
    }

    #[test]
    fn parse_c_with_value_before_double_dash_unchanged() -> Result<()> {
        let parsed_args =
            CommandLineArgs::parse_shell_args(args(&["brush", "-c", "echo hi", "--", "arg0"]))?;
        assert_eq!(parsed_args.command, Some("echo hi".to_string()));
        assert_eq!(parsed_args.script_args, ["--", "arg0"]);
        Ok(())
    }

    #[test]
    fn parse_c_with_option_before_command() -> Result<()> {
        // bash takes -c's command string from the first operand, so options may
        // sit between the two: `bash -c -l 'echo hello' arg0`.
        let parsed_args =
            CommandLineArgs::parse_shell_args(args(&["brush", "-c", "-l", "echo hello", "arg0"]))?;
        assert_eq!(parsed_args.command, Some("echo hello".to_string()));
        assert!(parsed_args.login);
        assert_eq!(parsed_args.script_args, ["arg0"]);
        Ok(())
    }

    #[test]
    fn parse_c_with_value_taking_option_before_command() -> Result<()> {
        // -o consumes its own value, so the first operand is still the command.
        let parsed_args = CommandLineArgs::parse_shell_args(args(&[
            "brush",
            "-c",
            "-o",
            "errexit",
            "echo hello",
        ]))?;
        assert_eq!(parsed_args.command, Some("echo hello".to_string()));
        assert_eq!(parsed_args.enabled_options, ["errexit"]);
        assert_eq!(parsed_args.script_args, Vec::<String>::new());
        Ok(())
    }

    #[test]
    fn parse_ec_with_option_before_command() -> Result<()> {
        // Only the `c` leaves the combined group; -e still applies.
        let parsed_args =
            CommandLineArgs::parse_shell_args(args(&["brush", "-ec", "-l", "echo hello"]))?;
        assert_eq!(parsed_args.command, Some("echo hello".to_string()));
        assert!(parsed_args.exit_on_nonzero_command_exit);
        assert!(parsed_args.login);
        Ok(())
    }

    #[test]
    fn parse_c_with_several_options_before_command() -> Result<()> {
        let parsed_args =
            CommandLineArgs::parse_shell_args(args(&["brush", "-c", "-l", "-x", "echo hello"]))?;
        assert_eq!(parsed_args.command, Some("echo hello".to_string()));
        assert!(parsed_args.login);
        assert!(parsed_args.print_commands_and_arguments);
        assert_eq!(parsed_args.script_args, Vec::<String>::new());
        Ok(())
    }

    #[test]
    fn parse_c_after_value_taking_option() -> Result<()> {
        // "errexit" is -o's value rather than an operand, so option parsing
        // continues and the command is still the first operand.
        let parsed_args = CommandLineArgs::parse_shell_args(args(&[
            "brush",
            "-o",
            "errexit",
            "-c",
            "-l",
            "echo hello",
        ]))?;
        assert_eq!(parsed_args.command, Some("echo hello".to_string()));
        assert_eq!(parsed_args.enabled_options, ["errexit"]);
        assert!(parsed_args.login);
        Ok(())
    }

    #[test]
    fn parse_c_after_script_stays_a_script_argument() -> Result<()> {
        // Option parsing stops at the first operand, so these reach the script:
        // `bash announce.sh -c -l` passes "-c" and "-l" as $1 and $2.
        let parsed_args =
            CommandLineArgs::parse_shell_args(args(&["brush", "announce.sh", "-c", "-l"]))?;
        assert!(parsed_args.command.is_none());
        assert!(!parsed_args.login);
        assert_eq!(parsed_args.script_args, ["announce.sh", "-c", "-l"]);
        Ok(())
    }

    #[test]
    fn parse_c_with_double_dash_and_option_like_command() -> Result<()> {
        // After `--`, the next token is the command even when it looks like an
        // option: `bash -c -- -l 'echo zero'` runs `-l` with $0 of "echo zero".
        let parsed_args =
            CommandLineArgs::parse_shell_args(args(&["brush", "-c", "--", "-l", "echo zero"]))?;
        assert_eq!(parsed_args.command, Some("-l".to_string()));
        assert!(!parsed_args.login);
        assert_eq!(parsed_args.script_args, ["echo zero"]);
        Ok(())
    }

    #[test]
    fn parse_c_with_option_and_no_command() {
        // bash: "-c: option requires an argument", exit 2.
        assert!(CommandLineArgs::parse_shell_args(args(&["brush", "-c", "-l"])).is_err());
    }

    #[test]
    fn parse_c_with_option_then_double_dash() -> Result<()> {
        let parsed_args =
            CommandLineArgs::parse_shell_args(args(&["brush", "-c", "-l", "--", "echo hello"]))?;
        assert_eq!(parsed_args.command, Some("echo hello".to_string()));
        assert!(parsed_args.login);
        assert_eq!(parsed_args.script_args, Vec::<String>::new());
        Ok(())
    }

    #[test]
    fn parse_c_with_option_and_doubled_double_dash() -> Result<()> {
        // Only the first `--` terminates options; the second is the command
        // operand, matching `bash -c -l -- -- echo hi`.
        let parsed_args = CommandLineArgs::parse_shell_args(args(&[
            "brush", "-c", "-l", "--", "--", "echo", "hi",
        ]))?;
        assert_eq!(parsed_args.command, Some("--".to_string()));
        assert_eq!(parsed_args.script_args, ["echo", "hi"]);
        Ok(())
    }

    #[test]
    fn parse_c_combined_with_other_flags() -> Result<()> {
        // `-c` groups with other short flags in any order, as in `brush -cl "echo hi"`.
        for argv in [
            &["brush", "-cl", "echo hi", "name"][..],
            &["brush", "-lc", "echo hi", "name"][..],
            &["brush", "-c", "-l", "echo hi", "name"][..],
        ] {
            let parsed_args = CommandLineArgs::parse_shell_args(args(argv))?;
            assert!(parsed_args.login, "for {argv:?}");
            assert_eq!(
                parsed_args.command,
                Some("echo hi".to_string()),
                "for {argv:?}"
            );
            assert_eq!(parsed_args.script_args, ["name"], "for {argv:?}");
        }
        Ok(())
    }

    #[test]
    fn parse_c_without_command_string() {
        assert!(CommandLineArgs::parse_shell_args(args(&["brush", "-c"])).is_err());
    }

    #[test]
    fn parse_c_with_empty_command_string() -> Result<()> {
        let parsed_args = CommandLineArgs::parse_shell_args(args(&["brush", "-c", ""]))?;
        assert_eq!(parsed_args.command, Some(String::new()));
        Ok(())
    }

    #[test]
    fn parse_o_with_double_dash_is_not_transformed() {
        // Unlike -c, bash's -o consumes -- as its literal value (invalid option
        // name), not as an option terminator.
        let result = CommandLineArgs::parse_shell_args(args(&["brush", "-o", "--"]));
        // try_parse_known splits at --, so -o ends up
        // without a value and parsing correctly fails. The key assertion is
        // that we MUST NOT reinterpret -- as an option terminator for -o and
        // then take any later argument as its value.
        assert!(result.is_err());
    }

    #[test]
    fn parse_oc_is_o_with_an_attached_value() -> Result<()> {
        // To brush's parser, -oc is -o with attached value "c", not -o flag + -c
        // flag, so the -- is an ordinary option terminator here. NOTE: bash instead
        // takes the *next* word as -o's option name; see the known-failure compat
        // case "-oc with -- takes its option name from the next word".
        let parsed_args = CommandLineArgs::parse_shell_args(args(&["brush", "-oc", "--", "echo"]))?;
        // -o consumed "c" as its value; -- ended the options; no -c command.
        assert!(parsed_args.command.is_none());
        assert_eq!(parsed_args.script_args, ["echo"]);
        Ok(())
    }

    #[test]
    fn parse_bool_flag_before_double_dash_not_transformed() -> Result<()> {
        // -e is a boolean flag, not -c. The -- ends the options, so
        // everything after it is positional (including -c).
        let parsed_args =
            CommandLineArgs::parse_shell_args(args(&["brush", "-e", "--", "-c", "echo"]))?;
        assert!(parsed_args.command.is_none());
        assert!(parsed_args.exit_on_nonzero_command_exit);
        assert_eq!(parsed_args.script_args, ["-c", "echo"]);
        Ok(())
    }

    #[test]
    fn parse_s_with_double_dash_separator() -> Result<()> {
        let parsed_args =
            CommandLineArgs::parse_shell_args(args(&["brush", "-s", "--", "--", "a"]))?;
        assert!(parsed_args.read_commands_from_stdin);
        assert_eq!(parsed_args.script_args, ["--", "a"]);
        Ok(())
    }

    #[test]
    fn parse_double_dash_before_script() -> Result<()> {
        let parsed_args =
            CommandLineArgs::parse_shell_args(args(&["brush", "--", "script.sh", "--", "x"]))?;
        assert_eq!(parsed_args.script_args, ["script.sh", "--", "x"]);
        Ok(())
    }

    #[test]
    fn parse_c_with_double_dash_and_later_double_dash() -> Result<()> {
        // After removing the first --, -c gets "echo". The second -- is
        // handled by try_parse_known and appears in script_args.
        let parsed_args =
            CommandLineArgs::parse_shell_args(args(&["brush", "-c", "--", "echo", "--", "more"]))?;
        assert_eq!(parsed_args.command, Some("echo".to_string()));
        assert_eq!(parsed_args.script_args, ["--", "more"]);
        Ok(())
    }

    #[test]
    fn parse_c_flag_group_edge_cases() -> Result<()> {
        // `-C` is a different flag, and `-oc` is `-o` with the value "c", so
        // neither puts the shell in command mode.
        let parsed_args = CommandLineArgs::parse_shell_args(args(&["brush", "-C", "script.sh"]))?;
        assert!(!parsed_args.command_string_mode);
        assert!(parsed_args.command.is_none());

        let parsed_args = CommandLineArgs::parse_shell_args(args(&["brush", "-oc", "script.sh"]))?;
        assert!(!parsed_args.command_string_mode);
        assert_eq!(parsed_args.enabled_options, ["c"]);
        Ok(())
    }

    #[test]
    fn defaults_to_an_available_input_backend() {
        for at_terminal in [true, false] {
            let backend = default_input_backend_type(at_terminal);
            assert!(is_input_backend_available(backend), "{backend:?}");
        }
    }

    #[cfg(reedline_backend)]
    #[test]
    fn defaults_to_reedline_at_a_terminal() {
        assert_eq!(default_input_backend_type(true), InputBackendType::Reedline);
    }

    #[cfg(all(not(reedline_backend), vt_backend))]
    #[test]
    fn defaults_to_vt_at_a_terminal_without_reedline() {
        assert_eq!(default_input_backend_type(true), InputBackendType::Vt);
    }

    #[cfg(minimal_backend)]
    #[test]
    fn defaults_to_minimal_away_from_a_terminal() {
        assert_eq!(default_input_backend_type(false), InputBackendType::Minimal);
    }

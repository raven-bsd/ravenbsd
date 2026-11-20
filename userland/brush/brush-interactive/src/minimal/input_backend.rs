// SPDX-License-Identifier: MIT
// Copyright (c) 2024 reuben olinsky

use std::io::{BufRead, IsTerminal, Write};

use crate::{
    InputBackend, ShellError,
    input_backend::{InteractivePrompt, ReadResult},
};

/// Reads the console descriptor without std's thread-local stdin buffer.
#[cfg(target_os = "ravenbsd")]
struct RawStdin;

#[cfg(target_os = "ravenbsd")]
impl std::io::Read for RawStdin {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        use std::os::fd::AsFd;
        let stdin = std::io::stdin();
        nix::unistd::read(stdin.as_fd(), buffer)
            .map_err(|error| std::io::Error::from_raw_os_error(error as i32))
    }
}

/// Represents a minimal shell input backend, capable of taking commands from standard input.
#[derive(Default)]
pub struct MinimalInputBackend;

impl InputBackend for MinimalInputBackend {
    #[allow(
        clippy::unused_async_trait_impl,
        reason = "this backend reads synchronously; the trait is async for those that don't"
    )]
    async fn read_line(
        &mut self,
        shell_ref: &crate::ShellRef<impl brush_core::ShellExtensions>,
        prompt: InteractivePrompt,
    ) -> Result<ReadResult, ShellError> {
        #[cfg(target_os = "ravenbsd")]
        {
            // The login console reports as a terminal (`isatty` maps to the tty
            // GET_MODE call), but the default RavenBSD backend is `Minimal`. Use the
            // VT line editor so TAB reaches the completion menu and the semantic
            // highlighter. The plain switches keep drawn input free of escapes.
            #[cfg(feature = "vt")]
            if std::io::stdout().is_terminal() || std::io::stdin().is_terminal() {
                let plain = std::env::var_os("NO_COLOR").is_some_and(|value| !value.is_empty())
                    || std::env::var("TERM").is_ok_and(|term| term == "dumb")
                    || std::env::var("RAVEN_PLAIN_PROMPT").is_ok_and(|value| value == "1");
                let options = crate::UIOptions {
                    disable_color: plain,
                    disable_highlighting: plain,
                    ..Default::default()
                };
                let mut backend = crate::VtInputBackend::default().with_ui_options(&options);
                if plain {
                    backend = backend.with_basic_style();
                }
                return Box::pin(backend.read_line(shell_ref, prompt)).await;
            }

            // Fallback: the console scheme is not a POSIX tty and std's stdin buffer
            // lives in a thread-local that the current-thread runtime tears down
            // early. Read fd 0 directly and always show the prompt: the login shell
            // is interactive.
            let mut reader = std::io::BufReader::new(RawStdin);
            return Self::read_program_from(
                shell_ref,
                Some(&prompt),
                &mut reader,
                &mut std::io::stderr(),
            );
        }
        #[cfg(not(target_os = "ravenbsd"))]
        {
            let stdin = std::io::stdin();
            let prompt = stdin.is_terminal().then_some(&prompt);

            Self::read_program_from(shell_ref, prompt, &mut stdin.lock(), &mut std::io::stderr())
        }
    }
}

impl MinimalInputBackend {
    /// Reads lines from the given reader until they form a complete shell program.
    /// Compound commands, here documents, and backslash continuations all span lines,
    /// so a single line is often not a program on its own.
    ///
    /// # Arguments
    ///
    /// * `shell_ref` - The shell whose parsing options decide when input is complete.
    /// * `prompt` - The prompt to display, or `None` to display nothing.
    /// * `reader` - The source to read lines from.
    /// * `writer` - Where prompts are written.
    fn read_program_from<R: BufRead, W: Write>(
        shell_ref: &crate::ShellRef<impl brush_core::ShellExtensions>,
        prompt: Option<&InteractivePrompt>,
        reader: &mut R,
        writer: &mut W,
    ) -> Result<ReadResult, ShellError> {
        let mut result = String::new();

        loop {
            if let Some(prompt) = prompt {
                let to_display = if result.is_empty() {
                    &prompt.prompt
                } else {
                    &prompt.continuation_prompt
                };

                write!(writer, "{to_display}")?;
                writer.flush()?;
            }

            // Nothing more is coming; run what we have and let the parser complain if
            // it's still incomplete.
            let Some(line) = Self::read_input_line(reader)? else {
                break;
            };

            result.push_str(line.as_str());

            if !crate::completeness::needs_more_input(shell_ref, result.as_str()) {
                break;
            }
        }

        if result.is_empty() {
            Ok(ReadResult::Eof)
        } else {
            Ok(ReadResult::Input(result))
        }
    }

    /// Reads a single line, returning `None` at end of input.
    fn read_input_line<R: BufRead>(reader: &mut R) -> Result<Option<String>, ShellError> {
        let mut input = String::new();
        let bytes_read = reader
            .read_line(&mut input)
            .map_err(ShellError::InputError)?;

        Ok((bytes_read > 0).then_some(input))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::Cursor, sync::Arc};

    async fn test_shell_ref() -> crate::ShellRef {
        Arc::new(tokio::sync::Mutex::new(
            brush_core::Shell::builder().build().await.unwrap(),
        ))
    }

    fn test_prompt() -> InteractivePrompt {
        InteractivePrompt {
            prompt: String::from("$ "),
            alt_side_prompt: String::new(),
            continuation_prompt: String::from("> "),
        }
    }

    /// Reads one program from `input`, returning what the backend accumulated.
    async fn read_program(input: &str) -> ReadResult {
        let shell_ref = test_shell_ref().await;
        let mut reader = Cursor::new(input.as_bytes());

        MinimalInputBackend::read_program_from(&shell_ref, None, &mut reader, &mut Vec::new())
            .unwrap()
    }

    async fn assert_reads(input: &str, expected: &str) {
        let actual = match read_program(input).await {
            ReadResult::Input(s) => format!("Input({s:?})"),
            ReadResult::BoundCommand(s) => format!("BoundCommand({s:?})"),
            ReadResult::Eof => String::from("Eof"),
            ReadResult::Interrupted => String::from("Interrupted"),
        };

        assert_eq!(
            actual,
            format!("Input({expected:?})"),
            "for input {input:?}"
        );
    }

    #[tokio::test]
    async fn stops_at_a_complete_program() {
        assert_reads("echo one\necho two\n", "echo one\n").await;
    }

    #[tokio::test]
    async fn accumulates_until_complete() {
        assert_reads("true &&\necho yes\n", "true &&\necho yes\n").await;
        assert_reads(
            "if true\nthen\necho yes\nfi\necho after\n",
            "if true\nthen\necho yes\nfi\n",
        )
        .await;
        assert_reads("cat <<EOF\nbody\nEOF\n", "cat <<EOF\nbody\nEOF\n").await;
        assert_reads("echo a\\\nb\n", "echo a\\\nb\n").await;
    }

    #[tokio::test]
    async fn returns_incomplete_input_at_end_of_input() {
        // Truncated mid-construct: hand it over anyway so the parser reports the error.
        assert_reads("if true\nthen\n", "if true\nthen\n").await;
    }

    #[tokio::test]
    async fn reports_end_of_input_when_there_is_nothing_to_read() {
        assert!(matches!(read_program("").await, ReadResult::Eof));
    }

    #[tokio::test]
    async fn displays_a_continuation_prompt_for_each_extra_line() {
        let shell_ref = test_shell_ref().await;
        let prompt = test_prompt();
        let mut prompts = Vec::new();

        MinimalInputBackend::read_program_from(
            &shell_ref,
            Some(&prompt),
            &mut Cursor::new(b"if true\nthen\necho hi\nfi\n".as_slice()),
            &mut prompts,
        )
        .unwrap();

        assert_eq!(String::from_utf8(prompts).unwrap(), "$ > > > ");
    }

    #[tokio::test]
    async fn displays_no_prompts_without_one_to_display() {
        let shell_ref = test_shell_ref().await;
        let mut prompts = Vec::new();

        MinimalInputBackend::read_program_from(
            &shell_ref,
            None,
            &mut Cursor::new(b"if true\nthen\necho hi\nfi\n".as_slice()),
            &mut prompts,
        )
        .unwrap();

        assert_eq!(prompts, Vec::<u8>::new());
    }
}

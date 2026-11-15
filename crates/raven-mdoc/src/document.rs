// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Parse and lay out the mdoc subset used by our pages.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use core::fmt;

use raven_theme::{Depth, Palette, Role, paint};

/// The default wrapping width.
pub const DEFAULT_WIDTH: usize = 80;

/// One styled piece of a rendered line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    /// Semantic role, or `None` for ordinary text.
    pub role: Option<Role>,
    /// The text, without a trailing space.
    pub text: String,
}

/// One rendered output line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    /// Leading spaces.
    pub indent: usize,
    /// Styled pieces in order.
    pub spans: Vec<Span>,
}

impl Line {
    /// Render the line as plain text, without a newline.
    #[must_use]
    pub fn text(&self) -> String {
        let mut out = String::new();
        for _ in 0..self.indent {
            out.push(' ');
        }
        for span in &self.spans {
            out.push_str(&span.text);
        }
        out
    }

    /// Render the line with the semantic roles painted when `enabled`.
    #[must_use]
    pub fn painted(&self, palette: &Palette, depth: Depth, enabled: bool) -> String {
        let mut out = String::new();
        for _ in 0..self.indent {
            out.push(' ');
        }
        for span in &self.spans {
            match span.role {
                Some(role) if enabled => {
                    out.push_str(&paint(role, &span.text, palette, depth, true));
                }
                _ => out.push_str(&span.text),
            }
        }
        out
    }
}

/// A parsed manual page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Document {
    /// Uppercase manual title (`Dt`).
    pub title: String,
    /// Manual section (`Dt`).
    pub section: u8,
    /// Operating system (`Os`).
    pub os: Option<String>,
    /// Date string (`Dd`).
    pub date: Option<String>,
    body: Vec<String>,
}

/// Why a page could not be parsed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// The source has no content.
    Empty,
    /// The source has no valid `.Dt` line.
    MissingTitle,
    /// The `.Dt` section is not 1..=9.
    InvalidSection,
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Empty => "page is empty",
            Self::MissingTitle => "page has no .Dt title",
            Self::InvalidSection => "page has an invalid section",
        })
    }
}

impl Document {
    /// Parse the metadata and keep the body for layout.
    ///
    /// # Errors
    /// [`Error::Empty`], [`Error::MissingTitle`] or [`Error::InvalidSection`].
    pub fn parse(source: &str) -> Result<Self, Error> {
        let body: Vec<String> = source.lines().map(ToString::to_string).collect();
        if body.iter().all(|line| {
            let line = line.trim();
            line.is_empty() || line.starts_with(".\\\"") || line.starts_with("'\\\"")
        }) {
            return Err(Error::Empty);
        }
        let mut title = None;
        let mut section = None;
        let mut os = None;
        let mut date = None;
        for line in &body {
            let line = line.trim_start();
            let Some(rest) = macro_rest(line) else {
                continue;
            };
            let mut words = rest.splitn(2, char::is_whitespace);
            let name = words.next().unwrap_or("");
            let args = split_args(words.next().unwrap_or(""));
            match name {
                "Dt" => {
                    let candidate = args.first().cloned().unwrap_or_default();
                    if candidate.is_empty() {
                        continue;
                    }
                    let parsed = args
                        .get(1)
                        .and_then(|value| value.parse::<u8>().ok())
                        .filter(|value| (1..=9).contains(value));
                    title = Some(candidate.to_ascii_uppercase());
                    section = parsed;
                }
                "Os" => os = Some(args.join(" ")),
                "Dd" => date = Some(args.join(" ")),
                _ => {}
            }
        }
        let Some(title) = title else {
            return Err(Error::MissingTitle);
        };
        let Some(section) = section else {
            return Err(Error::InvalidSection);
        };
        Ok(Self {
            title,
            section,
            os: os.filter(|value| !value.is_empty()),
            date: date.filter(|value| !value.is_empty()),
            body,
        })
    }

    /// Lay the page out at `width` columns.
    #[must_use]
    pub fn render(&self, width: usize) -> Vec<Line> {
        let width = width.max(20);
        let mut renderer = Renderer::new(width, self.title.clone());
        renderer.run(&self.body);
        renderer.finish(self)
    }

    /// Render the page as plain text, with a trailing newline for each line.
    #[must_use]
    pub fn render_plain(&self, width: usize) -> String {
        let mut out = String::new();
        for line in self.render(width) {
            out.push_str(&line.text());
            out.push('\n');
        }
        out
    }
}

/// Parse and lay out one page.
///
/// # Errors
/// The same errors as [`Document::parse`].
pub fn render(source: &str, width: usize) -> Result<Vec<Line>, Error> {
    Ok(Document::parse(source)?.render(width))
}

fn macro_rest(line: &str) -> Option<&str> {
    let rest = line.strip_prefix('.').or_else(|| line.strip_prefix('\''))?;
    if rest.starts_with("\\\"") {
        return None;
    }
    Some(rest.trim_start())
}

fn split_args(text: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut started = false;
    for character in text.chars() {
        match character {
            '"' => {
                quoted = !quoted;
                started = true;
            }
            character if character.is_whitespace() && !quoted => {
                if started {
                    args.push(core::mem::take(&mut current));
                    started = false;
                }
            }
            character => {
                current.push(character);
                started = true;
            }
        }
    }
    if started {
        args.push(current);
    }
    args
}

fn decode(text: &str) -> String {
    let mut out = String::new();
    let mut characters = text.chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            out.push(character);
            continue;
        }
        match characters.next() {
            Some('-') => out.push('-'),
            Some('&') => {}
            Some('e') => out.push('\\'),
            Some(' ') | Some('~') => out.push(' '),
            Some('(') => {
                let mut name = String::new();
                for character in characters.by_ref() {
                    if character == ')' {
                        break;
                    }
                    name.push(character);
                }
                match name.as_str() {
                    "em" => out.push_str("--"),
                    "en" => out.push('-'),
                    "bu" => out.push('*'),
                    "aq" => out.push('\''),
                    "dq" => out.push('"'),
                    "co" => out.push_str("(c)"),
                    "ti" => out.push('~'),
                    "ga" => out.push('`'),
                    _ => out.push_str(&name),
                }
            }
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ListKind {
    Tag,
    Bullet,
    Enum,
    Item,
    Column,
}

#[derive(Clone, Debug)]
struct List {
    kind: ListKind,
    indent: usize,
    number: u32,
}

struct Renderer {
    lines: Vec<Line>,
    current: Vec<Span>,
    indent: usize,
    width: usize,
    no_space: bool,
    lists: Vec<List>,
    literal: Option<usize>,
    block_restore: Option<usize>,
    in_name: bool,
    pending_name: Option<String>,
    command: Option<String>,
    function: Option<(String, Vec<String>)>,
    explicit_title: String,
}

impl Renderer {
    fn new(width: usize, title: String) -> Self {
        Self {
            lines: Vec::new(),
            current: Vec::new(),
            indent: 4,
            width,
            no_space: false,
            lists: Vec::new(),
            literal: None,
            block_restore: None,
            in_name: false,
            pending_name: None,
            command: None,
            function: None,
            explicit_title: title,
        }
    }

    fn run(&mut self, body: &[String]) {
        for raw in body {
            let line = raw.trim_end();
            if let Some(indent) = self.literal {
                if line.trim() == ".Ed" {
                    self.literal = None;
                    self.flush();
                    continue;
                }
                if line.is_empty() {
                    self.blank();
                } else {
                    self.raw_line(indent, line);
                }
                continue;
            }
            let trimmed = line.trim_start();
            if trimmed.is_empty() {
                continue;
            }
            if trimmed.starts_with(".\\\"") || trimmed.starts_with("'\\\"") {
                continue;
            }
            let Some(rest) = macro_rest(trimmed) else {
                self.text(trimmed);
                continue;
            };
            let mut words = rest.splitn(2, char::is_whitespace);
            let name = words.next().unwrap_or("");
            if name.is_empty() {
                continue;
            }
            let args = split_args(words.next().unwrap_or(""));
            self.macro_call(name, &args);
        }
        self.flush();
    }

    fn finish(mut self, document: &Document) -> Vec<Line> {
        self.flush_pending_name();
        while self.lines.last().is_some_and(is_blank) {
            self.lines.pop();
        }
        let mut header = format!("{}({})", document.title, document.section);
        if let Some(os) = &document.os {
            header.push_str("  ");
            header.push_str(os);
        }
        if let Some(date) = &document.date {
            header.push_str("  ");
            header.push_str(date);
        }
        let mut out = Vec::with_capacity(self.lines.len() + 2);
        out.push(Line {
            indent: 0,
            spans: vec![Span {
                role: Some(Role::Accent),
                text: header,
            }],
        });
        out.push(Line {
            indent: 0,
            spans: Vec::new(),
        });
        out.append(&mut self.lines);
        while out.last().is_some_and(is_blank) {
            out.pop();
        }
        out
    }

    fn macro_call(&mut self, name: &str, args: &[String]) {
        if name != "Nd" && name != "Nm" {
            self.flush_pending_name();
        }
        match name {
            "Dd" | "Dt" | "Os" => {}
            "Sh" => self.section_heading(&args.join(" ")),
            "Ss" => self.subheading(&args.join(" ")),
            "Nm" => self.name_macro(args),
            "Nd" => self.description(args),
            "Pp" => {
                self.flush();
                self.blank();
            }
            "Bl" => self.list_begin(args),
            "El" => self.list_end(),
            "It" => self.item(args),
            "Bd" => self.block_begin(args),
            "Ed" => self.block_end(),
            "D1" => self.display(args, None),
            "Dl" => self.display(args, Some(Role::Muted)),
            "Ex" => {
                self.flush();
                self.paragraph(
                    "The utility exits 0 on success, and >0 if an error occurs.",
                    Some(4),
                );
                self.flush();
            }
            "Rv" => {
                self.flush();
                self.paragraph(
                    "The utility returns 0 on success, and >0 if an error occurs.",
                    Some(4),
                );
                self.flush();
            }
            "Fo" => {
                self.flush();
                let function = args.first().cloned().unwrap_or_default();
                self.function = Some((function, args.get(1..).unwrap_or(&[]).to_vec()));
            }
            "Fc" => {
                self.flush_function();
            }
            "Fn" => self.function_call(args),
            _ => {
                let mut tokens = Vec::with_capacity(args.len() + 1);
                if is_inline_macro(name) {
                    tokens.push(name.to_string());
                }
                tokens.extend_from_slice(args);
                self.parse_inline(&tokens, None);
            }
        }
    }

    fn section_heading(&mut self, heading: &str) {
        self.flush_pending_name();
        self.flush();
        self.ensure_blank();
        let heading = decode(heading);
        self.in_name = heading == "NAME";
        if !heading.is_empty() {
            self.lines.push(Line {
                indent: 0,
                spans: vec![Span {
                    role: Some(Role::Accent),
                    text: heading,
                }],
            });
        }
        self.blank();
    }

    fn subheading(&mut self, heading: &str) {
        self.flush();
        self.ensure_blank();
        let heading = decode(heading);
        if !heading.is_empty() {
            self.lines.push(Line {
                indent: 4,
                spans: vec![Span {
                    role: Some(Role::Accent),
                    text: heading,
                }],
            });
        }
        self.blank();
    }

    fn name_macro(&mut self, args: &[String]) {
        let name = args.first().cloned().unwrap_or_else(|| {
            self.command
                .clone()
                .unwrap_or_else(|| self.explicit_title.to_ascii_lowercase())
        });
        if !args.is_empty() {
            self.command = Some(name.clone());
        }
        if self.in_name {
            self.flush();
            self.pending_name = Some(decode(&name));
        } else {
            self.append_word(&name, Some(Role::Accent));
        }
    }

    fn description(&mut self, args: &[String]) {
        let description = decode(&args.join(" "));
        if let Some(name) = self.pending_name.take() {
            self.flush();
            self.lines.push(Line {
                indent: 4,
                spans: vec![
                    Span {
                        role: Some(Role::Accent),
                        text: name,
                    },
                    Span {
                        role: None,
                        text: " - ".to_string(),
                    },
                    Span {
                        role: None,
                        text: description,
                    },
                ],
            });
        } else {
            self.append_word(&description, None);
        }
    }

    fn list_begin(&mut self, args: &[String]) {
        self.flush();
        let kind = match args.first().map(String::as_str) {
            Some("-bullet") => ListKind::Bullet,
            Some("-enum") => ListKind::Enum,
            Some("-item") => ListKind::Item,
            Some("-column") => ListKind::Column,
            _ => ListKind::Tag,
        };
        self.lists.push(List {
            kind,
            indent: self.indent,
            number: 0,
        });
    }

    fn list_end(&mut self) {
        self.flush();
        if let Some(list) = self.lists.pop() {
            self.indent = list.indent;
        }
    }

    fn item(&mut self, args: &[String]) {
        self.flush();
        let Some(list) = self.lists.last_mut() else {
            self.parse_inline(args, None);
            self.flush();
            return;
        };
        let indent = list.indent;
        let kind = list.kind;
        match kind {
            ListKind::Bullet => {
                self.indent = indent;
                self.append_raw("-", None, false);
                self.flush();
            }
            ListKind::Enum => {
                list.number = list.number.saturating_add(1);
                let number = list.number;
                self.indent = indent;
                self.append_raw(&format!("{number}."), None, false);
                self.flush();
            }
            ListKind::Item => {
                self.indent = indent;
                self.parse_inline(args, None);
                self.flush();
            }
            ListKind::Tag | ListKind::Column => {
                self.indent = indent;
                if args.is_empty() {
                    self.flush();
                } else {
                    self.parse_inline(args, None);
                    self.flush();
                }
            }
        }
        self.indent = indent + 4;
    }

    fn block_begin(&mut self, args: &[String]) {
        self.flush();
        if args.iter().any(|arg| arg == "-literal") {
            self.literal = Some(self.indent + 4);
        } else {
            self.block_restore = Some(self.indent);
            self.indent += 4;
        }
    }

    fn block_end(&mut self) {
        self.flush();
        if self.literal.is_some() {
            self.literal = None;
        } else if let Some(indent) = self.block_restore.take() {
            self.indent = indent;
        }
    }

    fn display(&mut self, args: &[String], role: Option<Role>) {
        self.flush();
        let restore = self.indent;
        self.indent += 4;
        self.parse_inline(args, role);
        self.flush();
        self.indent = restore;
    }

    fn function_call(&mut self, args: &[String]) {
        let name = args.first().cloned().unwrap_or_default();
        let arguments = args.get(1..).unwrap_or(&[]);
        let mut call = String::from(&name);
        call.push('(');
        call.push_str(&arguments.join(", "));
        call.push(')');
        self.append_word(&call, Some(Role::Info));
    }

    fn flush_function(&mut self) {
        if let Some((name, arguments)) = self.function.take() {
            let mut call = name;
            call.push('(');
            call.push_str(&arguments.join(", "));
            call.push(')');
            self.append_word(&call, Some(Role::Info));
        }
    }

    fn parse_inline(&mut self, tokens: &[String], default: Option<Role>) {
        let mut index = 0usize;
        while index < tokens.len() {
            let token = tokens[index].as_str();
            match token {
                "Ns" => {
                    self.no_space = true;
                    index += 1;
                }
                "No" => {
                    if let Some(value) = tokens.get(index + 1) {
                        self.append_word(value, default);
                        index += 2;
                    } else {
                        index += 1;
                    }
                }
                "Nm" => {
                    let name = self
                        .command
                        .clone()
                        .unwrap_or_else(|| self.explicit_title.to_ascii_lowercase());
                    self.append_word(&name, Some(Role::Accent));
                    index += 1;
                }
                "Fl" => {
                    if let Some(value) = tokens.get(index + 1) {
                        let value = decode(value);
                        self.append_word(&format!("-{value}"), Some(Role::Accent));
                        index += 2;
                    } else {
                        index += 1;
                    }
                }
                "Ar" => {
                    if let Some(value) = tokens.get(index + 1) {
                        self.append_word(value, Some(Role::Warning));
                        index += 2;
                    } else {
                        index += 1;
                    }
                }
                "Pa" => {
                    if let Some(value) = tokens.get(index + 1) {
                        self.append_word(value, Some(Role::Muted));
                        index += 2;
                    } else {
                        index += 1;
                    }
                }
                "Li" | "Cd" => {
                    if let Some(value) = tokens.get(index + 1) {
                        self.append_word(value, Some(Role::Muted));
                        index += 2;
                    } else {
                        index += 1;
                    }
                }
                "Cm" | "Ic" | "Em" | "Ft" | "Vt" => {
                    if let Some(value) = tokens.get(index + 1) {
                        self.append_word(value, Some(Role::Info));
                        index += 2;
                    } else {
                        index += 1;
                    }
                }
                "Ev" | "Va" | "Dv" => {
                    if let Some(value) = tokens.get(index + 1) {
                        self.append_word(value, Some(Role::Keyword));
                        index += 2;
                    } else {
                        index += 1;
                    }
                }
                "Er" => {
                    if let Some(value) = tokens.get(index + 1) {
                        self.append_word(value, Some(Role::Error));
                        index += 2;
                    } else {
                        index += 1;
                    }
                }
                "Fa" => {
                    if let Some(value) = tokens.get(index + 1) {
                        self.append_word(value, Some(Role::Warning));
                        index += 2;
                    } else {
                        index += 1;
                    }
                }
                "Sy" | "B" => {
                    if let Some(value) = tokens.get(index + 1) {
                        self.append_word(value, Some(Role::Accent));
                        index += 2;
                    } else {
                        index += 1;
                    }
                }
                "In" => {
                    if let Some(value) = tokens.get(index + 1) {
                        self.append_word(&format!("#include <{value}>"), Some(Role::Muted));
                        index += 2;
                    } else {
                        index += 1;
                    }
                }
                "Lb" => {
                    if let Some(value) = tokens.get(index + 1) {
                        self.append_word(&format!("-l{value}"), Some(Role::Muted));
                        index += 2;
                    } else {
                        index += 1;
                    }
                }
                "Lk" | "Mt" => {
                    if let Some(value) = tokens.get(index + 1) {
                        self.append_word(value, Some(Role::Info));
                        index += 2;
                    } else {
                        index += 1;
                    }
                }
                "Xr" => {
                    let name = tokens.get(index + 1).cloned().unwrap_or_default();
                    let section = tokens.get(index + 2).cloned();
                    let text = match section {
                        Some(section) if section.chars().all(|ch| ch.is_ascii_digit()) => {
                            format!("{name}({section})")
                        }
                        _ => name,
                    };
                    self.append_word(&text, Some(Role::Info));
                    index += 2;
                    if tokens
                        .get(index)
                        .is_some_and(|value| value.chars().all(|ch| ch.is_ascii_digit()))
                    {
                        index += 1;
                    }
                }
                "Fn" => {
                    let args = &tokens[index + 1..];
                    self.function_call(args);
                    index = tokens.len();
                }
                "Op" | "Pq" | "Dq" | "Sq" | "Qq" | "Ql" | "Bq" => {
                    let (open, close, role) = match token {
                        "Op" | "Bq" => ("[", "]", default),
                        "Pq" => ("(", ")", default),
                        "Dq" | "Qq" => ("\"", "\"", default),
                        "Sq" => ("'", "'", default),
                        _ => ("'", "'", Some(Role::Muted)),
                    };
                    self.append_raw(open, default, false);
                    self.no_space = true;
                    self.parse_inline(&tokens[index + 1..], role);
                    self.append_raw(close, default, true);
                    self.no_space = false;
                    return;
                }
                "St" => {
                    if let Some(value) = tokens.get(index + 1) {
                        let text = match value.as_str() {
                            "-isoC" => "ISO C".to_string(),
                            "-p1003.1" => "POSIX.1".to_string(),
                            other => other.trim_start_matches('-').to_string(),
                        };
                        self.append_word(&text, Some(Role::Muted));
                        index += 2;
                    } else {
                        index += 1;
                    }
                }
                _ => {
                    self.append_word(token, default);
                    index += 1;
                }
            }
        }
    }

    fn text(&mut self, line: &str) {
        self.flush_pending_name();
        for word in line.split_whitespace() {
            self.append_word(word, None);
        }
    }

    fn append_word(&mut self, text: &str, role: Option<Role>) {
        let text = decode(text);
        if text.is_empty() {
            return;
        }
        let mut characters = text.chars();
        let closing = characters.next().is_some_and(|ch| {
            matches!(ch, ',' | '.' | ';' | ':' | '!' | '?' | ')' | ']' | '}')
                && characters.next().is_none()
        });
        let space = !self.no_space && !self.current.is_empty() && !closing;
        let width = self.current_width() + usize::from(space) + text.chars().count();
        if !self.current.is_empty() && width > self.width && !closing && !self.no_space {
            self.flush();
        }
        if !self.no_space && !self.current.is_empty() && !closing {
            self.current.push(Span {
                role: None,
                text: " ".to_string(),
            });
        }
        self.current.push(Span { role, text });
        self.no_space = false;
    }

    fn append_raw(&mut self, text: &str, role: Option<Role>, attach: bool) {
        if !attach && !self.no_space && !self.current.is_empty() {
            self.current.push(Span {
                role: None,
                text: " ".to_string(),
            });
        }
        self.current.push(Span {
            role,
            text: text.to_string(),
        });
    }

    fn current_width(&self) -> usize {
        self.indent
            + self
                .current
                .iter()
                .map(|span| span.text.chars().count())
                .sum::<usize>()
    }

    fn paragraph(&mut self, text: &str, indent: Option<usize>) {
        if let Some(indent) = indent {
            self.indent = indent;
        }
        for word in text.split_whitespace() {
            self.append_word(word, None);
        }
    }

    fn raw_line(&mut self, indent: usize, text: &str) {
        self.flush();
        self.lines.push(Line {
            indent,
            spans: vec![Span {
                role: None,
                text: text.to_string(),
            }],
        });
    }

    fn flush(&mut self) {
        if self.current.is_empty() {
            return;
        }
        self.lines.push(Line {
            indent: self.indent,
            spans: core::mem::take(&mut self.current),
        });
        self.no_space = false;
    }

    fn blank(&mut self) {
        if self.lines.last().is_some_and(is_blank) {
            return;
        }
        self.lines.push(Line {
            indent: 0,
            spans: Vec::new(),
        });
    }

    fn ensure_blank(&mut self) {
        if self.lines.is_empty() || self.lines.last().is_some_and(is_blank) {
            return;
        }
        self.blank();
    }

    fn flush_pending_name(&mut self) {
        if let Some(name) = self.pending_name.take() {
            self.flush();
            self.lines.push(Line {
                indent: 4,
                spans: vec![Span {
                    role: Some(Role::Accent),
                    text: name,
                }],
            });
        }
    }
}

fn is_inline_macro(name: &str) -> bool {
    matches!(
        name,
        "Ns" | "No"
            | "Nm"
            | "Fl"
            | "Ar"
            | "Pa"
            | "Li"
            | "Cd"
            | "Cm"
            | "Ic"
            | "Em"
            | "Ft"
            | "Vt"
            | "Ev"
            | "Va"
            | "Dv"
            | "Er"
            | "Fa"
            | "Sy"
            | "B"
            | "In"
            | "Lb"
            | "Lk"
            | "Mt"
            | "Xr"
            | "Fn"
            | "Op"
            | "Pq"
            | "Dq"
            | "Sq"
            | "Qq"
            | "Ql"
            | "Bq"
            | "St"
    )
}

fn is_blank(line: &Line) -> bool {
    line.spans.is_empty()
}

#[cfg(test)]
#[path = "document_ut.rs"]
mod tests;

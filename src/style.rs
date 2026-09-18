//! How `rt` dresses its output: ripgrep's colours for a terminal, and plain
//! compact text for a coding agent.

use std::borrow::Cow;
use std::fmt::{self, Display, Formatter};
use std::io::IsTerminal;

use anstyle::{AnsiColor, Color, Effects, Style as Ansi};
use clap::ColorChoice;
use serde::Serialize;

/// ripgrep's defaults: magenta paths, green line numbers, bold red matches.
pub const PATH: Ansi = Ansi::new().fg_color(Some(Color::Ansi(AnsiColor::Magenta)));
pub const LINE: Ansi = Ansi::new().fg_color(Some(Color::Ansi(AnsiColor::Green)));
pub const MATCH: Ansi = Ansi::new()
    .fg_color(Some(Color::Ansi(AnsiColor::Red)))
    .effects(Effects::BOLD);
pub const HEADING: Ansi = Ansi::new().effects(Effects::BOLD);
pub const KIND: Ansi = Ansi::new().fg_color(Some(Color::Ansi(AnsiColor::Cyan)));
pub const CONTEXT: Ansi = Ansi::new().effects(Effects::DIMMED);

/// The environment variables a coding agent harness sets.
const HARNESS_VARS: &[&str] = &[
    "AGENT",
    "CLAUDECODE",
    "CODEX_SANDBOX",
    "GEMINI_CLI",
    "OMPCODE",
    "OPENCODE",
];

/// The shape of a line of output.
#[derive(Copy, Clone)]
pub struct Style {
    color: bool,
    /// Spend no tokens on padding, indentation or long words.
    pub compact: bool,
}

impl Style {
    pub fn new(choice: ColorChoice) -> Self {
        let compact = Self::agent();
        let color = match choice {
            ColorChoice::Always => true,
            ColorChoice::Never => false,
            ColorChoice::Auto => {
                !compact
                    && !anstyle_query::no_color()
                    && anstyle_query::term_supports_color()
                    && std::io::stdout().is_terminal()
            }
        };
        Style { color, compact }
    }

    /// Whether a coding agent is reading this output.
    pub fn agent() -> bool {
        HARNESS_VARS
            .iter()
            .any(|key| std::env::var_os(key).is_some())
    }

    /// `label (N)`, or how many of N survived `--limit`.
    pub fn heading(&self, label: &str, total: usize, shown: usize) -> String {
        let label = self.paint(HEADING, label);
        match (self.compact, shown < total) {
            (true, true) => format!("{label} {shown}/{total} (--limit)"),
            (true, false) => format!("{label} {total}"),
            (false, true) => format!("{label} ({total} total, showing {shown} - raise --limit)"),
            (false, false) => format!("{label} ({total})"),
        }
    }

    /// JSON: indented for a human, one line for an agent.
    pub fn json<T: Serialize>(&self, value: &T) -> serde_json::Result<String> {
        match self.compact {
            true => serde_json::to_string(value),
            false => serde_json::to_string_pretty(value),
        }
    }

    /// `value` in `style`, or bare when colour is off. Width and alignment
    /// reach the value itself, so the escapes never count towards a column.
    pub fn paint<T: Display>(&self, style: Ansi, value: T) -> Paint<T> {
        Paint {
            style: self.color.then_some(style),
            value,
        }
    }

    /// The indent before a result line.
    pub fn indent(&self) -> &'static str {
        if self.compact { "" } else { "  " }
    }

    /// The gap between the columns of a result line.
    pub fn gap(&self) -> &'static str {
        if self.compact { " " } else { "  " }
    }

    /// The source line at a site, trimmed, with the occurrence highlighted.
    ///
    /// `col` and `len` count characters of the untrimmed line, and a line
    /// edited since the index was built is printed as it stands.
    pub fn source<'a>(&self, line: &'a str, col: u32, len: u32) -> Cow<'a, str> {
        let text = line.trim();
        if !self.color {
            return Cow::Borrowed(text);
        }
        let base = line.len() - line.trim_start().len();
        match span(line, col, len) {
            Some((start, end)) if start >= base && end <= base + text.len() && start < end => {
                Cow::Owned(self.spliced(text, start - base, end - base))
            }
            _ => Cow::Borrowed(text),
        }
    }

    /// `text` with the first case-insensitive run of `needle` highlighted.
    pub fn found<'a>(&self, text: &'a str, needle: Option<&str>) -> Cow<'a, str> {
        let Some(needle) = needle.filter(|needle| self.color && !needle.is_empty()) else {
            return Cow::Borrowed(text);
        };
        match text.to_ascii_lowercase().find(&needle.to_ascii_lowercase()) {
            Some(start) => Cow::Owned(self.spliced(text, start, start + needle.len())),
            None => Cow::Borrowed(text),
        }
    }

    fn spliced(&self, text: &str, start: usize, end: usize) -> String {
        format!(
            "{}{}{}",
            &text[..start],
            self.paint(MATCH, &text[start..end]),
            &text[end..]
        )
    }
}

/// The byte range of the `len` characters starting at character `col`.
fn span(line: &str, col: u32, len: u32) -> Option<(usize, usize)> {
    let mut offsets = line.char_indices().map(|(at, _)| at).chain([line.len()]);
    let start = offsets.nth(col as usize)?;
    let end = match len {
        0 => start,
        len => offsets.nth(len as usize - 1)?,
    };
    Some((start, end))
}

/// A value in an ANSI style, or the value as it is.
pub struct Paint<T> {
    style: Option<Ansi>,
    value: T,
}

impl<T: Display> Display for Paint<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let Some(style) = self.style else {
            return self.value.fmt(f);
        };
        write!(f, "{style}")?;
        self.value.fmt(f)?;
        write!(f, "{style:#}")
    }
}

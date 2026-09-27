//! Compiler diagnostics: errors, warnings and pretty rendering.

use crate::span::{SourceFile, Span};
use std::fmt;
use std::io::IsTerminal;

/// How severe a diagnostic is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Note,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Severity::Error => write!(f, "error"),
            Severity::Warning => write!(f, "warning"),
            Severity::Note => write!(f, "note"),
        }
    }
}

/// A primary span + message pair attached to a diagnostic.
#[derive(Clone, Debug)]
pub struct Label {
    pub span: Span,
    pub message: String,
}

/// A single compiler diagnostic.
#[derive(Clone, Debug)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: &'static str,
    pub message: String,
    pub label: Option<Label>,
    pub notes: Vec<Label>,
}

impl Diagnostic {
    pub fn error(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Error,
            code,
            message: message.into(),
            label: None,
            notes: Vec::new(),
        }
    }

    pub fn warning(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Warning,
            code,
            message: message.into(),
            label: None,
            notes: Vec::new(),
        }
    }

    /// Attach a primary location to this diagnostic.
    pub fn at(mut self, span: Span, message: impl Into<String>) -> Self {
        self.label = Some(Label {
            span,
            message: message.into(),
        });
        self
    }

    /// Attach a secondary note with a location.
    pub fn note(mut self, span: Span, message: impl Into<String>) -> Self {
        self.notes.push(Label {
            span,
            message: message.into(),
        });
        self
    }
}

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const RED: &str = "\x1b[31m";
const YELLOW: &str = "\x1b[33m";
const CYAN: &str = "\x1b[36m";

fn color(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => RED,
        Severity::Warning => YELLOW,
        Severity::Note => CYAN,
    }
}

/// Whether diagnostics should contain ANSI escape codes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ColorChoice {
    #[default]
    Auto,
    Always,
    Never,
}

/// Collects diagnostics produced by any compiler stage.
///
/// Stages push into the engine; the driver decides whether the
/// accumulated errors abort compilation.
#[derive(Default)]
pub struct DiagnosticEngine {
    diagnostics: Vec<Diagnostic>,
    pub color: ColorChoice,
}

impl DiagnosticEngine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, d: Diagnostic) {
        self.diagnostics.push(d);
    }

    pub fn extend(&mut self, other: DiagnosticEngine) {
        self.diagnostics.extend(other.diagnostics);
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
    }

    pub fn error_count(&self) -> usize {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .count()
    }

    pub fn warning_count(&self) -> usize {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Warning)
            .count()
    }

    /// Render every diagnostic against `file` (which supplies source
    /// lines for caret drawing).
    pub fn render(&self, file: &SourceFile, out: &mut String) {
        for d in &self.diagnostics {
            self.render_one(d, file, out);
        }
        let (e, w) = (self.error_count(), self.warning_count());
        if e > 0 || w > 0 {
            out.push_str(&format!(
                "{BOLD}{}{RESET}: {} error(s), {} warning(s)\n",
                "summary",
                e,
                w,
                BOLD = BOLD,
                RESET = RESET
            ));
        }
    }

    fn render_one(&self, d: &Diagnostic, file: &SourceFile, out: &mut String) {
        let use_color = match self.color {
            ColorChoice::Always => true,
            ColorChoice::Never => false,
            ColorChoice::Auto => std::io::stderr().is_terminal(),
        };
        let (c, b, r) = if use_color {
            (color(d.severity), BOLD, RESET)
        } else {
            ("", "", "")
        };

        let (path, line_no) = file.resolve_line(d.label.as_ref().map(|l| l.span.line).unwrap_or(1));
        out.push_str(&format!(
            "{c}{b}{sev}{r}{cb}[{code}]{cr} {msg}\n{b}  --> {path}:{line}:{col}{r}\n",
            sev = d.severity,
            code = d.code,
            msg = d.message,
            path = path.display(),
            line = line_no,
            col = d.label.as_ref().map(|l| l.span.col).unwrap_or(1),
            c = c,
            cb = c,
            cr = r,
            b = b,
            r = r,
        ));

        if let Some(label) = &d.label {
            draw_source_line(
                file,
                label.span.line,
                Some((label.span.col, label.span.len, c)),
                out,
            );
        }
        for note in &d.notes {
            let (np, nl) = file.resolve_line(note.span.line);
            out.push_str(&format!(
                "  {b}note{r}: {} at {p}:{l}\n",
                note.message,
                p = np.display(),
                l = nl
            ));
            draw_source_line(
                file,
                note.span.line,
                Some((note.span.col, note.span.len, "")),
                out,
            );
        }
        out.push('\n');
    }
}

/// Draw one source line plus a caret row underneath.
fn draw_source_line(
    file: &SourceFile,
    physical_line: u32,
    caret: Option<(u32, u32, &str)>,
    out: &mut String,
) {
    let text = file
        .text
        .lines()
        .nth(physical_line.saturating_sub(1) as usize)
        .unwrap_or("");
    let gutter = physical_line.to_string();
    out.push_str(&format!("   {gutter} | {text}\n"));
    if let Some((col, len, color)) = caret {
        let pad = " ".repeat(col.saturating_sub(1) as usize);
        let carets = "^".repeat(len.max(1) as usize);
        out.push_str(&format!(
            "   {:width$} | {pad}{c}{carets}{r}\n",
            "",
            width = gutter.len(),
            c = color,
            r = RESET
        ));
    }
}

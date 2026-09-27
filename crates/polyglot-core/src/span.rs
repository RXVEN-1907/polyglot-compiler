//! Source positions and source files.

use std::fmt;
use std::path::{Path, PathBuf};

/// A half-open range of characters in a [`SourceFile`], resolved to a
/// line and column so diagnostics never need the raw text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Span {
    pub line: u32,
    /// Column is measured in characters (not bytes) starting at 1.
    pub col: u32,
    pub len: u32,
}

impl Span {
    pub fn new(line: u32, col: u32, len: u32) -> Self {
        Self { line, col, len }
    }

    /// A span that points at a single position (used when no better
    /// location is known).
    pub fn point(line: u32, col: u32) -> Self {
        Self::new(line, col, 1)
    }

    /// The smallest span that covers both `self` and `other`. Spans on
    /// different lines collapse to the first span (multi-line ranges
    /// are not needed for diagnostics).
    pub fn merge(self, other: Span) -> Span {
        if other.line != self.line {
            return self;
        }
        let end = (other.col + other.len).max(self.col + 1);
        Span {
            line: self.line,
            col: self.col,
            len: end.saturating_sub(self.col),
        }
    }
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.line, self.col)
    }
}

/// An in-memory source file with its logical name.
///
/// The preprocessor splices `#include "..."` files into a single token
/// stream; [`line_map`](SourceFile::line_map) records
/// virtual line-number resets (`#line`) so diagnostics still point at
/// the file the code was written in.
#[derive(Clone, Debug)]
pub struct SourceFile {
    pub name: PathBuf,
    /// Full text after preprocessing (comments stripped, includes
    /// spliced, macros expanded).
    pub text: String,
    /// Sorted list of `(physical_line, target_file_index, target_line)`
    /// markers inserted by the preprocessor as `#@line <target> <file>`
    /// pseudo-comments. The lexer consumes them.
    pub line_map: Vec<(u32, usize, u32)>,
    /// Names of every physical file contributing text, indexable by the
    /// `file` component of the line map.
    pub files: Vec<PathBuf>,
}

impl SourceFile {
    pub fn new(name: impl AsRef<Path>, text: String) -> Self {
        Self {
            name: name.as_ref().to_path_buf(),
            text,
            line_map: Vec::new(),
            files: vec![name.as_ref().to_path_buf()],
        }
    }

    /// Resolve a physical line to `(file, line)` using the line map.
    ///
    /// Each map entry `(at, file, target)` means "physical line `at` is
    /// a marker; the line *after* it is line `target` of `file`".
    /// Without any markers the whole file is `self.name`.
    pub fn resolve_line(&self, physical_line: u32) -> (&Path, u32) {
        let mut file = 0usize;
        let mut at = 0u32;
        let mut target = 1u32;
        for &(marker_at, marker_file, marker_target) in &self.line_map {
            if marker_at < physical_line {
                at = marker_at;
                file = marker_file;
                target = marker_target;
            } else {
                break;
            }
        }
        let logical = if physical_line > at {
            target + (physical_line - at - 1)
        } else {
            physical_line
        };
        (
            self.files
                .get(file)
                .map(|p| p.as_path())
                .unwrap_or(&self.name),
            logical,
        )
    }
}

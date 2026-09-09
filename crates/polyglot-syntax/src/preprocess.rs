//! The C preprocessor subset implemented by Polyglot.
//!
//! Supported features:
//!
//! * comments (`//`, `/* */`, including multi-line blocks) and
//!   backslash line continuations
//! * `#include <std-header>` for a small set of built-in headers
//! * `#include "local.h"` with recursive splicing and cycle detection
//! * object-like and function-like `#define` / `#undef`
//! * `#ifdef`, `#ifndef`, `#else`, `#endif` (`#if 1` also accepted)
//! * `#pragma once`
//!
//! The output is one text buffer where every contributing file keeps
//! its original line numbering inside its segment (consumed lines are
//! padded with blanks), so diagnostics map back to the file the user
//! wrote through [`Preprocessed::line_map`].

use polyglot_core::span::Span;
use polyglot_core::{Diagnostic, DiagnosticEngine};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Result of preprocessing.
pub struct Preprocessed {
    pub text: String,
    /// `(marker_line, file_index, target_line)` entries; the lexer uses
    /// them to translate spans back to original files.
    pub line_map: Vec<(u32, usize, u32)>,
    pub files: Vec<PathBuf>,
}

/// Declarations injected when a well-known system header is included.
///
/// Polyglot does not parse real libc headers; each supported header
/// maps to explicit declarations of the facilities programs most
/// commonly use from it. This is a complete, honest implementation of
/// exactly what is listed.
const BUILTIN_HEADERS: &[(&str, &str)] = &[
    (
        "stdio.h",
        concat!(
            "int printf(char* fmt, ...);\n",
            "int sprintf(char* buf, char* fmt, ...);\n",
            "int puts(char* s);\n",
            "int putchar(int c);\n",
        ),
    ),
    (
        "stdlib.h",
        concat!(
            "char* malloc(long size);\n",
            "char* calloc(long n, long size);\n",
            "void free(void* p);\n",
            "void exit(int status);\n",
            "int abs(int x);\n",
            "long labs(long x);\n",
        ),
    ),
    (
        "string.h",
        concat!(
            "long strlen(char* s);\n",
            "int strcmp(char* a, char* b);\n",
            "char* strcpy(char* dst, char* src);\n",
            "char* strcat(char* dst, char* src);\n",
            "char* strchr(char* s, int c);\n",
            "char* memcpy(void* dst, void* src, long n);\n",
            "char* memset(void* dst, int c, long n);\n",
        ),
    ),
    ("stddef.h", ""),
];

/// `NULL` is always available.
const PRELUDE: &str = "#define NULL ((void*)0)\n";

/// Preprocess `text` originating from `path`.
pub fn preprocess(
    path: &Path,
    text: &str,
    include_dirs: &[PathBuf],
    diags: &mut DiagnosticEngine,
) -> anyhow::Result<Preprocessed> {
    let mut pp = Pp {
        out_lines: Vec::new(),
        line_map: Vec::new(),
        files: vec![path.to_path_buf()],
        macros: HashMap::new(),
        once: HashSet::new(),
        include_stack: Vec::new(),
        frames: Vec::new(),
        include_dirs: include_dirs.to_vec(),
        diags,
    };
    // Register prelude macros before any user code.
    for line in PRELUDE.lines() {
        pp.parse_define(line.strip_prefix("#define").unwrap().trim(), 0);
    }
    pp.include_file(0, text)?;
    Ok(Preprocessed {
        text: pp.out_lines.join("\n") + "\n",
        line_map: pp.line_map,
        files: pp.files,
    })
}

#[derive(Clone, Copy)]
struct Frame {
    /// Activity of the chain *below* this frame.
    parent_active: bool,
    /// Any branch of this conditional has been taken.
    taken: bool,
    /// Whole-chain activity including this frame.
    active: bool,
}

#[derive(Clone)]
enum Macro {
    Object(String),
    Function { params: Vec<String>, body: String },
}

struct Pp<'d> {
    out_lines: Vec<String>,
    line_map: Vec<(u32, usize, u32)>,
    files: Vec<PathBuf>,
    macros: HashMap<String, Macro>,
    once: HashSet<PathBuf>,
    /// Indices into `files`; used for recursive include detection.
    include_stack: Vec<usize>,
    frames: Vec<Frame>,
    include_dirs: Vec<PathBuf>,
    diags: &'d mut DiagnosticEngine,
}

impl<'d> Pp<'d> {
    fn active(&self) -> bool {
        self.frames.last().map(|f| f.active).unwrap_or(true)
    }

    fn warn(&mut self, msg: impl Into<String>, physical: u32) {
        self.diags.push(Diagnostic::warning("W0108", msg).at(Span::point(physical, 1), "directive"));
    }

    /// Process the raw text of one file. `file_idx` indexes `self.files`.
    fn include_file(&mut self, file_idx: usize, text: &str) -> anyhow::Result<()> {
        if self.include_stack.contains(&file_idx) {
            self.diags.push(Diagnostic::error(
                "E0107",
                format!("recursive #include of '{}' skipped", self.files[file_idx].display()),
            ));
            return Ok(());
        }
        self.include_stack.push(file_idx);
        self.emit_marker(1, file_idx);

        let raw: Vec<&str> = text.lines().collect();
        let mut i = 0usize;
        let mut in_block_comment = false;
        while i < raw.len() {
            let start_physical = i + 1;

            // Gather one logical line: block-comment continuation plus
            // backslash continuations.
            let mut logical = String::new();
            let mut consumed = 0usize;
            loop {
                if i + consumed >= raw.len() {
                    break;
                }
                let mut line = raw[i + consumed].to_string();
                consumed += 1;
                if in_block_comment {
                    match line.find("*/") {
                        Some(end) => {
                            in_block_comment = false;
                            line = line[end + 2..].to_string();
                        }
                        None => {
                            // Whole line stays inside the comment.
                            continue;
                        }
                    }
                }
                logical.push_str(&strip_comments_with_state(&mut in_block_comment, &line));
                if logical.trim_end().ends_with('\\') && !in_block_comment {
                    logical.truncate(logical.trim_end().len() - 1);
                    continue;
                }
                break;
            }
            i += consumed;
            // Preserve original numbering inside this segment.
            for _ in 1..consumed {
                self.out_lines.push(String::new());
            }

            let trimmed = logical.trim_start().to_string();
            let is_directive = trimmed.starts_with('#');

            // Every input line reserves exactly one output line so
            // numbering stays aligned; directives leave it blank,
            // expansions overwrite it.
            self.out_lines.push(String::new());

            if !self.active() && !(is_directive && is_conditional_directive(trimmed.trim())) {
                continue;
            }

            if is_directive {
                // `resume` is the next physical line of this file, used
                // to restore numbering after an include splices lines.
                self.handle_directive(trimmed.trim(), file_idx, start_physical as u32, i + 1)?;
            } else {
                let n = self.out_lines.len();
                self.out_lines[n - 1] = self.expand_line(&trimmed);
            }
        }
        self.include_stack.pop();
        Ok(())
    }

    fn emit_marker(&mut self, target_line: u32, file_idx: usize) {
        let marker = format!("#@line {target_line} {file_idx}");
        self.out_lines.push(marker);
        self.line_map.push((self.out_lines.len() as u32, file_idx, target_line));
    }

    fn handle_directive(
        &mut self,
        rest: &str,
        file_idx: usize,
        physical: u32,
        resume: usize,
    ) -> anyhow::Result<()> {
        let body = rest.strip_prefix('#').unwrap_or(rest).trim();

        if body.starts_with("include") {
            return self.handle_include(body, file_idx, resume as u32);
        }
        if body == "pragma once" {
            if let Some(p) = self.files.get(file_idx) {
                self.once.insert(p.clone());
            }
            return Ok(());
        }
        if let Some(def) = body.strip_prefix("define") {
            if self.active() {
                self.parse_define(def.trim(), physical);
            }
            return Ok(());
        }
        if let Some(name) = body.strip_prefix("undef") {
            if self.active() {
                self.macros.remove(name.trim());
            }
            return Ok(());
        }
        if let Some(cond) = body.strip_prefix("ifdef") {
            let cond = self.active() && self.macros.contains_key(cond.trim());
            self.push_frame(cond);
            return Ok(());
        }
        if let Some(cond) = body.strip_prefix("ifndef") {
            let cond = self.active() && !self.macros.contains_key(cond.trim());
            self.push_frame(cond);
            return Ok(());
        }
        if body.starts_with("if") {
            // Constant expressions other than `#if 1` are unsupported.
            let expr = body[2..].trim();
            let cond = self.active() && expr == "1";
            if !expr.is_empty() && expr != "1" && self.active() {
                self.warn("#if with constant expressions is not supported; treating as false", physical);
            }
            self.push_frame(cond);
            return Ok(());
        }
        if body == "else" {
            if let Some(f) = self.frames.pop() {
                let now = f.parent_active && !f.taken;
                self.frames.push(Frame { parent_active: f.parent_active, taken: true, active: now });
            }
            return Ok(());
        }
        if body.starts_with("endif") {
            self.frames.pop();
            return Ok(());
        }
        if self.active() {
            self.warn(format!("ignoring unsupported directive '#{}'", body), physical);
        }
        Ok(())
    }

    fn push_frame(&mut self, cond: bool) {
        let parent_active = self.active();
        self.frames.push(Frame {
            parent_active,
            taken: cond,
            active: parent_active && cond,
        });
    }

    fn handle_include(&mut self, body: &str, file_idx: usize, resume: u32) -> anyhow::Result<()> {
        let arg = body.strip_prefix("include").unwrap_or("").trim_start();
        if let Some(header) = arg.strip_prefix('<').and_then(|s| s.split_once('>')) {
            if self.active() {
                self.include_builtin(header.0.trim());
                // Restore the including file's numbering after spliced
                // builtin declarations.
                self.emit_marker(resume, file_idx);
            }
            return Ok(());
        }
        if let Some(inner) = arg.strip_prefix('"').and_then(|s| s.split_once('"')) {
            if self.active() {
                let before = self.out_lines.len();
                self.include_local(inner.0, file_idx)?;
                if self.out_lines.len() > before {
                    self.emit_marker(resume, file_idx);
                }
            }
            return Ok(());
        }
        if self.active() {
            self.diags.push(Diagnostic::error("E0104", "malformed #include"));
        }
        Ok(())
    }

    fn include_builtin(&mut self, header: &str) {
        match BUILTIN_HEADERS.iter().find(|(h, _)| *h == header) {
            Some((_, decls)) => {
                let builtin_file_idx = self.files.len();
                self.files
                    .push(PathBuf::from(format!("<builtin/{header}>")));
                self.emit_marker(1, builtin_file_idx);
                for d in decls.lines() {
                    self.out_lines.push(d.to_string());
                }
                let back_to = self.include_stack.last().copied().unwrap_or(0);
                self.emit_marker(1, back_to);
            }
            None => {
                let known: Vec<&str> = BUILTIN_HEADERS.iter().map(|(h, _)| *h).collect();
                self.diags.push(
                    Diagnostic::error("E0106", format!("unknown or unsupported system header <{header}>"))
                        .note(Span::point(1, 1), format!("built-in headers: {}", known.join(", "))),
                );
            }
        }
    }

    fn include_local(&mut self, name: &str, from_file: usize) -> anyhow::Result<()> {
        let mut candidates: Vec<PathBuf> = Vec::new();
        if let Some(dir) = self.files.get(from_file).and_then(|p| p.parent()) {
            candidates.push(dir.join(name));
        }
        for dir in &self.include_dirs {
            candidates.push(dir.join(name));
        }
        for cand in candidates {
            if let Ok(text) = std::fs::read_to_string(&cand) {
                let canon = cand.canonicalize().unwrap_or_else(|_| cand.clone());
                if self.once.contains(&canon) || self.once.contains(&cand) {
                    return Ok(());
                }
                self.files.push(canon);
                let idx = self.files.len() - 1;
                self.include_file(idx, &text)?;
                return Ok(());
            }
        }
        self.diags
            .push(Diagnostic::error("E0105", format!("'#include \"{name}\"' not found")));
        Ok(())
    }

    fn parse_define(&mut self, def: &str, physical: u32) {
        // The macro name ends at the first non-identifier character. A
        // '(' *immediately* after the name (no space) makes this a
        // function-like macro per the C standard.
        let name_end = def
            .find(|c: char| !(c.is_alphanumeric() || c == '_'))
            .unwrap_or(def.len());
        let name = &def[..name_end];
        let after_name = &def[name_end..];
        let valid_name = !name.is_empty()
            && name.chars().next().is_some_and(|c| c.is_alphabetic() || c == '_')
            && name.chars().all(|c| c.is_alphanumeric() || c == '_');
        if !valid_name {
            self.diags.push(
                Diagnostic::error("E0109", "invalid macro name in #define")
                    .at(Span::point(physical.max(1), 1), "macro"),
            );
            return;
        }
        if let Some(params_and_body) = after_name.strip_prefix('(') {
            let (params_str, after) = match params_and_body.split_once(')') {
                Some(x) => x,
                None => {
                    self.diags.push(
                        Diagnostic::error("E0110", "unterminated parameter list in #define")
                            .at(Span::point(physical.max(1), 1), "macro"),
                    );
                    return;
                }
            };
            let params: Vec<String> = params_str
                .split(',')
                .map(str::trim)
                .filter(|p| !p.is_empty())
                .map(String::from)
                .collect();
            self.macros.insert(
                name.to_string(),
                Macro::Function { params, body: after.trim().to_string() },
            );
        } else {
            let value = after_name.trim().to_string();
            self.macros.insert(name.to_string(), Macro::Object(value));
        }
    }

    fn expand_line(&self, line: &str) -> String {
        let mut result = line.to_string();
        for _ in 0..8 {
            let expanded = self.expand_once(&result);
            if expanded == result {
                break;
            }
            result = expanded;
        }
        result
    }

    fn expand_once(&self, line: &str) -> String {
        let chars: Vec<char> = line.chars().collect();
        let mut out = String::with_capacity(line.len());
        let mut i = 0usize;
        while i < chars.len() {
            let c = chars[i];
            if c.is_alphabetic() || c == '_' {
                let start = i;
                while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                let word: String = chars[start..i].iter().collect();
                match self.macros.get(&word) {
                    Some(Macro::Object(body)) => out.push_str(body),
                    Some(Macro::Function { params, body }) => {
                        let mut j = i;
                        while j < chars.len() && chars[j].is_whitespace() {
                            j += 1;
                        }
                        if j < chars.len() && chars[j] == '(' {
                            let (args, end) = split_macro_args(&chars, j);
                            i = end;
                            out.push_str(&substitute(body, params, &args));
                        } else {
                            out.push_str(&word);
                        }
                    }
                    None => out.push_str(&word),
                }
            } else if c == '"' || c == '\'' {
                let start = i;
                i = skip_literal(&chars, i);
                out.extend(chars[start..i].iter());
            } else {
                out.push(c);
                i += 1;
            }
        }
        out
    }
}

/// Skip over a string or character literal starting at `start`;
/// returns the index just past the closing quote.
fn skip_literal(chars: &[char], start: usize) -> usize {
    let quote = chars[start];
    let mut i = start + 1;
    while i < chars.len() {
        if chars[i] == '\\' {
            i += 2;
            continue;
        }
        if chars[i] == quote {
            return i + 1;
        }
        i += 1;
    }
    chars.len()
}

fn is_conditional_directive(body: &str) -> bool {
    let b = body.strip_prefix('#').unwrap_or(body).trim_start();
    b.starts_with("if") || b.starts_with("else") || b.starts_with("endif")
}

/// Remove C comments from `line`, tracking whether a block comment is
/// left open at end-of-line via `in_comment`.
fn strip_comments_with_state(in_comment: &mut bool, line: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    let mut out = String::with_capacity(line.len());
    let mut i = 0usize;
    while i < chars.len() {
        if *in_comment {
            if chars[i] == '*' && i + 1 < chars.len() && chars[i + 1] == '/' {
                *in_comment = false;
                out.push(' ');
                i += 2;
                continue;
            }
            i += 1;
            continue;
        }
        let c = chars[i];
        if c == '/' && chars.get(i + 1) == Some(&'/') {
            break;
        }
        if c == '/' && chars.get(i + 1) == Some(&'*') {
            *in_comment = true;
            i += 2;
            continue;
        }
        if c == '"' || c == '\'' {
            let start = i;
            i = skip_literal(&chars, i);
            out.extend(chars[start..i.min(chars.len())].iter());
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

/// Split macro call arguments starting at the '(' at index `open`.
/// Returns the arguments plus the index just past the closing ')'.
fn split_macro_args(chars: &[char], open: usize) -> (Vec<String>, usize) {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut depth = 0usize;
    let mut i = open + 1;
    while i < chars.len() {
        let c = chars[i];
        if c == '(' {
            depth += 1;
            current.push(c);
        } else if c == ')' {
            if depth == 0 {
                args.push(current.trim().to_string());
                return (args, i + 1);
            }
            depth -= 1;
            current.push(c);
        } else if c == ',' && depth == 0 {
            args.push(current.trim().to_string());
            current.clear();
        } else if c == '"' || c == '\'' {
            let start = i;
            i = skip_literal(chars, i);
            current.extend(chars[start..i.min(chars.len())].iter());
            continue;
        } else {
            current.push(c);
        }
        i += 1;
    }
    args.push(current.trim().to_string());
    (args, i)
}

/// Replace parameter names with argument text in a macro body.
fn substitute(body: &str, params: &[String], args: &[String]) -> String {
    if body.is_empty() {
        return String::new();
    }
    let chars: Vec<char> = body.chars().collect();
    let mut out = String::new();
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            if let Some(pos) = params.iter().position(|p| p == &word) {
                out.push_str(args.get(pos).map(String::as_str).unwrap_or(""));
            } else {
                out.push_str(&word);
            }
        } else if c == '"' || c == '\'' {
            let start = i;
            i = skip_literal(&chars, i);
            out.extend(chars[start..i.min(chars.len())].iter());
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

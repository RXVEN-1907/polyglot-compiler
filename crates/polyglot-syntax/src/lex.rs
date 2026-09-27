//! Tokenizer for preprocessed C source.
//!
//! Tracks line/column incrementally so tokenization is O(n).

use polyglot_core::span::Span;
use polyglot_core::Diagnostic;
use std::fmt;

#[derive(Clone, Debug, PartialEq)]
pub enum TokenKind {
    /// Integer literal with optional suffix-derived kind.
    Int(i128, Suffix),
    /// String literal contents (escapes already decoded).
    Str(String),
    /// Character literal (decoded byte value).
    Char(u8),
    Ident(String),
    Punct(&'static str),
    Eof,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Suffix {
    None,
    Long,
    Unsigned,
    LongUnsigned,
}

impl Suffix {
    pub fn to_kind(self) -> polyglot_core::IntKind {
        use polyglot_core::IntKind::*;
        match self {
            Suffix::None | Suffix::Unsigned => Int,
            _ => Long,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl fmt::Display for TokenKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TokenKind::Int(v, _) => write!(f, "integer `{v}`"),
            TokenKind::Str(_) => write!(f, "string literal"),
            TokenKind::Char(c) => write!(f, "char literal `{c}`"),
            TokenKind::Ident(s) => write!(f, "identifier `{s}`"),
            TokenKind::Punct(p) => write!(f, "`{p}`"),
            TokenKind::Eof => write!(f, "end of file"),
        }
    }
}

/// All punctuation recognized by the lexer, longest first so greedy
/// matching picks multi-character operators.
const PUNCTS: &[&str] = &[
    "<<=", ">>=", "...", "->", "++", "--", "<<", ">>", "<=", ">=", "==", "!=", "&&", "||", "+=",
    "-=", "*=", "/=", "%=", "&=", "|=", "^=", "+", "-", "*", "/", "%", "=", "<", ">", "!", "~",
    "&", "|", "^", "?", ":", ";", ",", ".", "(", ")", "{", "}", "[", "]",
];

/// Tokenize preprocessed text. `#@line` markers have already been
/// consumed by the caller's line map; they are skipped as comments.
pub fn tokenize(text: &str) -> Vec<Token> {
    let mut lexer = Lexer {
        chars: text.chars().collect(),
        pos: 0,
        line: 1,
        col: 1,
    };
    let mut tokens = Vec::new();
    loop {
        let start = lexer.point();
        lexer.skip_trivia();
        if lexer.at_end() {
            tokens.push(Token {
                kind: TokenKind::Eof,
                span: Span::point(start.0, start.1),
            });
            break;
        }
        tokens.push(lexer.next_token(start));
    }
    tokens
}

struct Lexer {
    chars: Vec<char>,
    pos: usize,
    line: u32,
    col: u32,
}

impl Lexer {
    /// Current `(line, col)`.
    fn point(&self) -> (u32, u32) {
        (self.line, self.col)
    }

    fn at_end(&self) -> bool {
        self.pos >= self.chars.len()
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn peek2(&self) -> Option<char> {
        self.chars.get(self.pos + 1).copied()
    }

    fn peek_at(&self, off: usize) -> Option<char> {
        self.chars.get(self.pos + off).copied()
    }

    fn bump(&mut self) -> char {
        let c = self.chars[self.pos];
        self.pos += 1;
        if c == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        c
    }

    fn skip_trivia(&mut self) {
        loop {
            match self.peek() {
                Some(c) if c.is_whitespace() => {
                    self.bump();
                }
                Some('#') if self.peek2() == Some('@') => {
                    while let Some(c) = self.peek() {
                        if c == '\n' {
                            break;
                        }
                        self.bump();
                    }
                }
                _ => break,
            }
        }
    }

    fn span_from(&self, start: (u32, u32)) -> Span {
        Span::new(start.0, start.1, self.col.saturating_sub(start.1).max(1))
    }

    fn next_token(&mut self, start: (u32, u32)) -> Token {
        let c = self.bump();
        let mk = |kind: TokenKind, slf: &Self| Token {
            kind,
            span: slf.span_from(start),
        };

        if c.is_ascii_digit() {
            self.unbump();
            return self.number(start);
        }
        if c.is_alphabetic() || c == '_' {
            let mut s = String::new();
            s.push(c);
            while let Some(n) = self.peek() {
                if n.is_alphanumeric() || n == '_' {
                    s.push(self.bump());
                } else {
                    break;
                }
            }
            return mk(TokenKind::Ident(s), self);
        }
        if c == '"' {
            return self.string(start);
        }
        if c == '\'' {
            return self.char_literal(start);
        }
        // Greedy punctuation matching, longest-first.
        let mut best: Option<&'static str> = None;
        'outer: for p in PUNCTS {
            let pcs: Vec<char> = p.chars().collect();
            if pcs[0] != c {
                continue;
            }
            for (k, pc) in pcs.iter().enumerate().skip(1) {
                if self.peek_at(k - 1) != Some(*pc) {
                    continue 'outer;
                }
            }
            best = Some(p);
            break;
        }
        match best {
            Some(p) => {
                for _ in 1..p.chars().count() {
                    self.bump();
                }
                mk(TokenKind::Punct(p), self)
            }
            None => mk(
                TokenKind::Punct("?"), // unreachable for valid input
                self,
            ),
        }
    }

    /// Step back over a character already consumed (digits only, so no
    /// newline concerns).
    fn unbump(&mut self) {
        self.pos -= 1;
        self.col -= 1;
    }

    fn number(&mut self, start: (u32, u32)) -> Token {
        let mut text = String::new();
        let radix = if self.peek() == Some('0') && matches!(self.peek2(), Some('x') | Some('X')) {
            self.bump();
            self.bump();
            while self.peek().is_some_and(|n| n.is_ascii_hexdigit()) {
                text.push(self.bump());
            }
            16
        } else if self.peek() == Some('0') && matches!(self.peek2(), Some('b') | Some('B')) {
            self.bump();
            self.bump();
            while matches!(self.peek(), Some('0') | Some('1')) {
                text.push(self.bump());
            }
            2
        } else if self.peek() == Some('0') && self.peek2().is_some_and(|n| n.is_ascii_digit()) {
            self.bump(); // leading 0
            while self.peek().is_some_and(|n| n.is_ascii_digit()) {
                text.push(self.bump());
            }
            8
        } else {
            while self.peek().is_some_and(|n| n.is_ascii_digit()) {
                text.push(self.bump());
            }
            10
        };
        let value = if text.is_empty() {
            0
        } else {
            i128::from_str_radix(&text, radix).unwrap_or(0)
        };
        let suffix = self.read_suffix();
        Token {
            kind: TokenKind::Int(value, suffix),
            span: self.span_from(start),
        }
    }

    fn read_suffix(&mut self) -> Suffix {
        let mut has_l = false;
        let mut has_u = false;
        loop {
            match self.peek() {
                Some('l') | Some('L') if !has_l => has_l = true,
                Some('u') | Some('U') if !has_u => has_u = true,
                _ => break,
            }
            self.bump();
        }
        match (has_l, has_u) {
            (true, true) => Suffix::LongUnsigned,
            (true, false) => Suffix::Long,
            (false, true) => Suffix::Unsigned,
            (false, false) => Suffix::None,
        }
    }

    fn string(&mut self, start: (u32, u32)) -> Token {
        let mut s = Vec::new();
        while let Some(c) = self.peek() {
            match c {
                '"' => {
                    self.bump();
                    break;
                }
                '\n' => break,
                '\\' => {
                    self.bump();
                    s.push(self.escape());
                }
                _ => {
                    self.bump();
                    s.extend(c.to_string().as_bytes());
                }
            }
        }
        Token {
            kind: TokenKind::Str(String::from_utf8_lossy(&s).into_owned()),
            span: self.span_from(start),
        }
    }

    fn char_literal(&mut self, start: (u32, u32)) -> Token {
        let byte = match self.peek() {
            Some('\\') => {
                self.bump();
                self.escape()
            }
            Some(c) => {
                self.bump();
                *c.to_string().as_bytes().first().unwrap_or(&0)
            }
            None => b'\0',
        };
        while let Some(c) = self.peek() {
            if c == '\'' || c == '\n' {
                if c == '\'' {
                    self.bump();
                }
                break;
            }
            self.bump();
        }
        Token {
            kind: TokenKind::Char(byte),
            span: self.span_from(start),
        }
    }

    fn escape(&mut self) -> u8 {
        match self.peek() {
            Some('n') => self.take(b'\n'),
            Some('t') => self.take(b'\t'),
            Some('r') => self.take(b'\r'),
            Some('0') => self.take(b'\0'),
            Some('\\') => self.take(b'\\'),
            Some('\'') => self.take(b'\''),
            Some('"') => self.take(b'"'),
            Some('a') => self.take(0x07),
            Some('b') => self.take(0x08),
            Some('f') => self.take(0x0c),
            Some('v') => self.take(0x0b),
            Some('x') => {
                self.bump();
                let mut v = 0u32;
                while self.peek().is_some_and(|h| h.is_ascii_hexdigit()) {
                    v = v * 16 + self.peek().unwrap().to_digit(16).unwrap();
                    self.bump();
                }
                v as u8
            }
            _ => b'\0',
        }
    }

    fn take(&mut self, b: u8) -> u8 {
        self.bump();
        b
    }
}

/// Convenience for parser error construction.
pub fn unexpected(kind: &TokenKind, span: Span, expected: &str) -> Diagnostic {
    Diagnostic::error("E0201", format!("expected {expected}, found {kind}")).at(span, "here")
}

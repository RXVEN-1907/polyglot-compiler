//! Integration tests for diagnostic rendering (`polyglot-core::diag`).
//!
//! The engine must produce labeled, spanned output against a synthetic
//! source file: severity + code headers, `-->` locations, source lines
//! with carets, secondary notes, a summary footer, and controllable
//! colorization.

use polyglot_core::diag::ColorChoice;
use polyglot_core::{Diagnostic, DiagnosticEngine, Severity, SourceFile, Span};

const SYNTHETIC: &str = "int main() {\n    int x = ;\n    return 0;\n}\n";

fn engine_with_error() -> DiagnosticEngine {
    let mut engine = DiagnosticEngine::new();
    engine.color = ColorChoice::Never;
    engine.push(
        Diagnostic::error("E0999", "expected expression before `;`")
            .at(Span::new(2, 13, 1), "expected an expression here")
            .note(Span::new(2, 5, 3), "while parsing this declaration"),
    );
    engine
}

#[test]
fn renders_error_with_span_and_caret() {
    let file = SourceFile::new("synthetic.c", SYNTHETIC.to_string());
    let engine = engine_with_error();
    let mut out = String::new();
    engine.render(&file, &mut out);

    // Severity, code and message appear in the header.
    assert!(
        out.contains("error[E0999] expected expression before `;`"),
        "header missing in:\n{out}"
    );
    // The primary location resolves to file:line:col.
    assert!(
        out.contains("--> synthetic.c:2:13"),
        "location missing in:\n{out}"
    );
    // The offending source line is quoted with its gutter.
    assert!(
        out.contains("2 |     int x = ;"),
        "source line missing in:\n{out}"
    );
    // A caret row points exactly under the offending token.
    let lines: Vec<&str> = out.lines().collect();
    let src_line = lines
        .iter()
        .find(|l| l.contains("int x = ;"))
        .expect("source line");
    let caret_row = lines
        .iter()
        .find(|l| l.contains('^'))
        .expect("caret row present");
    assert_eq!(
        src_line.find(';'),
        caret_row.find('^'),
        "caret aligns under the `;`:\n{out}"
    );
    assert_eq!(
        caret_row.matches('^').count(),
        1,
        "one spanned token, one caret:\n{out}"
    );
}

#[test]
fn renders_notes_with_their_own_location() {
    let file = SourceFile::new("synthetic.c", SYNTHETIC.to_string());
    let engine = engine_with_error();
    let mut out = String::new();
    engine.render(&file, &mut out);

    assert!(
        out.contains("note: while parsing this declaration at synthetic.c:2"),
        "secondary note with location missing in:\n{out}"
    );
}

#[test]
fn renders_summary_footer() {
    let file = SourceFile::new("synthetic.c", SYNTHETIC.to_string());
    let mut engine = DiagnosticEngine::new();
    engine.color = ColorChoice::Never;
    engine.push(Diagnostic::error("E0999", "boom").at(Span::new(2, 13, 1), "here"));
    engine.push(
        Diagnostic::warning("W0999", "unused variable `y`").at(Span::new(3, 5, 1), "declared here"),
    );
    let mut out = String::new();
    engine.render(&file, &mut out);

    assert!(
        out.contains("warning[W0999] unused variable `y`"),
        "warning header:\n{out}"
    );
    assert!(
        out.contains("--> synthetic.c:3:5"),
        "warning location:\n{out}"
    );
    assert!(
        out.contains("3 |     return 0;"),
        "warning source line:\n{out}"
    );
    assert!(
        out.contains("1 error(s), 1 warning(s)"),
        "summary footer missing in:\n{out}"
    );
}

#[test]
fn no_summary_when_nothing_to_report() {
    let file = SourceFile::new("clean.c", "int ok = 1;\n".to_string());
    let engine = DiagnosticEngine::new();
    let mut out = String::new();
    engine.render(&file, &mut out);
    assert!(
        out.is_empty(),
        "an empty engine renders nothing, got:\n{out}"
    );
}

#[test]
fn color_choice_controls_ansi_output() {
    let file = SourceFile::new("synthetic.c", SYNTHETIC.to_string());

    let mut plain = engine_with_error(); // Never
    plain.color = ColorChoice::Never;
    let mut plain_out = String::new();
    plain.render(&file, &mut plain_out);
    let header = plain_out.lines().next().unwrap_or("");
    assert!(
        !header.contains('\x1b'),
        "the header carries no ANSI codes with ColorChoice::Never:\n{plain_out}"
    );

    let mut colored = engine_with_error();
    colored.color = ColorChoice::Always;
    let mut colored_out = String::new();
    colored.render(&file, &mut colored_out);
    assert!(
        colored_out.contains("\x1b[31m"),
        "errors render in red with ColorChoice::Always:\n{colored_out}"
    );
}

#[test]
fn counts_severities() {
    let mut engine = DiagnosticEngine::new();
    engine.push(Diagnostic::error("E1", "a").at(Span::point(1, 1), "here"));
    engine.push(Diagnostic::error("E2", "b").at(Span::point(1, 1), "here"));
    engine.push(Diagnostic::warning("W1", "c").at(Span::point(1, 1), "here"));
    assert!(engine.has_errors());
    assert_eq!(engine.error_count(), 2);
    assert_eq!(engine.warning_count(), 1);
    assert_eq!(engine.diagnostics().len(), 3);

    let warnings_only = {
        let mut e = DiagnosticEngine::new();
        e.push(Diagnostic::warning("W1", "c").at(Span::point(1, 1), "here"));
        e
    };
    assert!(!warnings_only.has_errors(), "warnings alone are not errors");
}

#[test]
fn severity_renders_as_lowercase_words() {
    assert_eq!(Severity::Error.to_string(), "error");
    assert_eq!(Severity::Warning.to_string(), "warning");
    assert_eq!(Severity::Note.to_string(), "note");
}

#[test]
fn spans_render_as_line_col() {
    assert_eq!(Span::new(4, 7, 2).to_string(), "4:7");
    assert_eq!(Span::point(2, 3).len, 1);
}

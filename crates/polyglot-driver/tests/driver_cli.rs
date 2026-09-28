//! CLI-level tests for `polyglot-driver`: multi-source `build` through
//! the real binary, its `--help` contract, and the usage-error paths.

use std::process::Command;

fn driver() -> Command {
    Command::new(env!("CARGO_BIN_EXE_polyglot-driver"))
}

/// Run the driver with `args` inside `dir`; returns (exit code, stdout, stderr).
fn run_in(dir: &std::path::Path, args: &[&str]) -> (i32, String, String) {
    let out = driver()
        .args(args)
        .current_dir(dir)
        .output()
        .expect("driver binary should run");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn help_documents_multi_source_and_drops_single_source_note() {
    let (code, stdout, _) = run_in(&std::env::temp_dir(), &["build", "--help"]);
    assert_eq!(code, 0);
    assert!(
        !stdout.contains("Single source file only"),
        "--help still carries the removed single-source note:\n{stdout}"
    );
    assert!(
        stdout.contains("PATH") && stdout.to_lowercase().contains("source file"),
        "--help should document source file arguments:\n{stdout}"
    );
}

#[test]
fn cli_builds_and_runs_two_sources_natively() {
    if std::process::Command::new("cc")
        .arg("--version")
        .output()
        .map(|o| !o.status.success())
        .unwrap_or(true)
    {
        eprintln!("skipping: no cc linker driver");
        return;
    }
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        dir.path().join("math.h"),
        "int add(int a, int b);\nint mul(int a, int b);\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("math.c"),
        "int add(int a, int b) { return a + b; }\nint mul(int a, int b) { return a * b; }\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("main.c"),
        "#include \"math.h\"\n#include <stdio.h>\nint main() {\n  printf(\"%d %d\\n\", add(2, 3), mul(2, 3));\n  return 0;\n}\n",
    )
    .unwrap();

    let exe = dir.path().join("app");
    let exe_str = exe.to_string_lossy().into_owned();
    let (code, stdout, stderr) = run_in(
        dir.path(),
        &[
            "build",
            "main.c",
            "math.c",
            "--backend",
            "native",
            "-o",
            &exe_str,
        ],
    );
    assert_eq!(code, 0, "build failed: {stderr}");
    assert!(
        stdout.contains("linked 2 object file(s)"),
        "unexpected output: {stdout}"
    );
    assert!(exe.is_file(), "no executable at {}", exe.display());

    let out = Command::new(&exe).output().expect("run compiled exe");
    assert!(out.status.success());
    assert_eq!(String::from_utf8_lossy(&out.stdout), "5 6\n");
}

#[test]
fn cli_build_ir_multi_source_writes_per_file_artifacts() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("a.c"), "int a(void) { return 1; }\n").unwrap();
    std::fs::write(dir.path().join("b.c"), "int b(void) { return 2; }\n").unwrap();

    let (code, stdout, stderr) = run_in(dir.path(), &["build", "a.c", "b.c"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(
        stdout.contains("a.c -> a.pgir"),
        "unexpected output: {stdout}"
    );
    assert!(
        stdout.contains("b.c -> b.pgir"),
        "unexpected output: {stdout}"
    );
    assert!(dir.path().join("a.pgir").is_file());
    assert!(dir.path().join("b.pgir").is_file());
}

#[test]
fn cli_build_ir_multi_source_rejects_output() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("a.c"), "int a(void) { return 1; }\n").unwrap();
    std::fs::write(dir.path().join("b.c"), "int b(void) { return 2; }\n").unwrap();

    let (code, _, stderr) = run_in(dir.path(), &["build", "a.c", "b.c", "-o", "combined.pgir"]);
    assert_eq!(code, 1);
    assert!(
        stderr.contains("cannot name a single artifact"),
        "unexpected error: {stderr}"
    );
}

#[test]
fn cli_build_requires_a_source() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (code, _, stderr) = run_in(dir.path(), &["build"]);
    assert_eq!(code, 1);
    assert!(
        stderr.contains("build requires a source file"),
        "unexpected error: {stderr}"
    );
}

#[test]
fn cli_build_rejects_missing_source() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (code, _, stderr) = run_in(dir.path(), &["build", "nope.c"]);
    assert_eq!(code, 1);
    assert!(
        stderr.contains("no such source file"),
        "unexpected error: {stderr}"
    );
}

/// Default output for a single-source native build stays
/// `<stem-without-extension>` (a.out for the multi-source default).
#[test]
fn cli_multi_source_native_defaults_to_a_out() {
    if std::process::Command::new("cc")
        .arg("--version")
        .output()
        .map(|o| !o.status.success())
        .unwrap_or(true)
    {
        eprintln!("skipping: no cc linker driver");
        return;
    }
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("p.c"), "int main() { return 4; }\n").unwrap();
    std::fs::write(dir.path().join("q.c"), "int four(void) { return 4; }\n").unwrap();
    let (code, stdout, stderr) =
        run_in(dir.path(), &["build", "p.c", "q.c", "--backend", "native"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(
        dir.path().join("a.out").is_file(),
        "default output missing; stdout: {stdout}"
    );
    let out = Command::new(dir.path().join("a.out"))
        .output()
        .expect("run");
    assert_eq!(out.status.code(), Some(4));
}

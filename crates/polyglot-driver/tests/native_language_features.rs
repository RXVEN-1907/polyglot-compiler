//! End-to-end native tests for the language features the frontend
//! gained (enums, unions, switch, bit-fields) plus the SysV x86-64
//! aggregate ABI hardening (struct arguments/returns by value, small
//! register classes and large MEMORY-class objects).
//!
//! Each test drives the full in-process pipeline per translation unit
//! (parse -> check -> lower -> optimize), emits a real object with
//! `llc`, links with the system `cc` and executes the result,
//! asserting the program's stdout. Tests skip (with a note) when
//! `llc` or a linker driver is unavailable.

use polyglot_codegen::emit_object;
use polyglot_driver::pipeline;
use std::path::PathBuf;
use std::process::Command;

/// Locate a linker driver the way the driver does (`cc`, then `gcc`).
fn find_cc() -> Option<PathBuf> {
    for name in ["cc", "gcc"] {
        if let Some(path_var) = std::env::var_os("PATH") {
            for dir in std::env::split_paths(&path_var) {
                let cand = dir.join(name);
                if cand.is_file() {
                    return Some(cand);
                }
            }
        }
    }
    None
}

/// A linked executable and its (live) scratch directory.
struct Exe {
    path: PathBuf,
    _dir: tempfile::TempDir,
}

/// Compile each `(name, source)` translation unit through the
/// in-process pipeline to a native object, then link them together.
fn build_native(sources: &[(&str, &str)]) -> Result<Exe, String> {
    if polyglot_codegen::find_llc().is_none() {
        return Err("llc not found".to_string());
    }
    let Some(cc) = find_cc() else {
        return Err("no linker driver (cc/gcc) on PATH".to_string());
    };
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let mut objects = Vec::new();
    for (name, src) in sources {
        let path = dir.path().join(name);
        std::fs::write(&path, src).map_err(|e| e.to_string())?;
        let compiled = pipeline::compile_to_ir(
            &path,
            &[dir.path().to_path_buf()],
            polyglot_ir::OptLevel::Default,
        )
        .map_err(|e| e.to_string())?;
        let stem = name.trim_end_matches(".c");
        let obj = dir.path().join(format!("{stem}.o"));
        emit_object(&compiled.module, &obj, name).map_err(|e| e.to_string())?;
        objects.push(obj);
    }
    let exe = dir.path().join("prog");
    let status = Command::new(&cc)
        .args(&objects)
        .arg("-o")
        .arg(&exe)
        .status()
        .map_err(|e| e.to_string())?;
    if !status.success() {
        return Err(format!("link failed with exit code {:?}", status.code()));
    }
    Ok(Exe {
        path: exe,
        _dir: dir,
    })
}

/// Compile, link and run a single-source program; returns
/// `(exit code, stdout)`.
fn run_native(src: &str) -> Result<(i32, String), String> {
    let exe = build_native(&[("main.c", src)])?;
    let out = Command::new(&exe.path)
        .output()
        .map_err(|e| e.to_string())?;
    Ok((
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    ))
}

fn require_native() -> bool {
    if polyglot_codegen::find_llc().is_none() || find_cc().is_none() {
        eprintln!("skipping: llc or cc/gcc unavailable");
        return false;
    }
    true
}

// ---- switch ---------------------------------------------------------------

#[test]
fn switch_dispatch_and_fallthrough() {
    if !require_native() {
        return;
    }
    let src = r#"
#include <stdio.h>

int classify(int v) {
    switch (v) {
        case 0:
        case 1:
            return 10;
        case 2: {
            int local = 100;
            return local + v;
        }
        case 3:
            v += 1; /* fall through */
        case 4:
            return v + 1000;
        default:
            return -1;
    }
}

int main() {
    for (int i = 0; i < 6; i++) {
        printf("%d ", classify(i));
    }
    printf("\n");
    return 0;
}
"#;
    let (code, out) = run_native(src).expect("build+run failed");
    assert_eq!(code, 0);
    assert_eq!(out, "10 10 102 1004 1004 -1 \n");
}

#[test]
fn switch_in_loop_with_break_and_default() {
    if !require_native() {
        return;
    }
    let src = r#"
#include <stdio.h>

int main() {
    int total = 0;
    for (int i = 0; i < 6; i++) {
        switch (i) {
            case 0:
                total += 1;
                break;
            case 1:
            case 2:
                total += 10;
                break;
            case 3: {
                total += 100;
                break;
            }
            default:
                total += 1000;
        }
    }
    printf("total=%d\n", total); /* 1 + 10 + 10 + 100 + 1000 + 1000 */
    return 0;
}
"#;
    let (code, out) = run_native(src).expect("build+run failed");
    assert_eq!(code, 0);
    assert_eq!(out, "total=2121\n");
}

// ---- enums ------------------------------------------------------------------

#[test]
fn enum_constants_params_and_switch() {
    if !require_native() {
        return;
    }
    let src = r#"
#include <stdio.h>

enum Color { RED, GREEN = 5, BLUE };

enum Color step(enum Color c) {
    switch (c) {
        case RED: return GREEN;
        case GREEN: return BLUE;
        case BLUE: return RED;
    }
    return RED;
}

int main() {
    printf("%d %d %d\n", RED, GREEN, BLUE);
    enum Color c = BLUE;
    printf("step=%d\n", step(c));
    printf("twice=%d\n", step(step(c)));
    if (c > GREEN) {
        printf("gt\n");
    }
    switch (c) {
        case RED:
        case GREEN:
            printf("low\n");
            break;
        case BLUE:
            printf("high\n");
            break;
    }
    return 0;
}
"#;
    let (code, out) = run_native(src).expect("build+run failed");
    assert_eq!(code, 0);
    assert_eq!(out, "0 5 6\nstep=0\ntwice=5\ngt\nhigh\n");
}

// ---- unions -------------------------------------------------------------------

#[test]
fn union_member_overlap() {
    if !require_native() {
        return;
    }
    let src = r#"
#include <stdio.h>

union Value { int i; long l; };
struct Holder { char tag; union Value v; };

int main() {
    union Value u;
    u.l = 0;
    u.i = 42;
    printf("%d\n", u.i);
    u.i = 0x01020304;
    printf("%ld\n", u.l); /* overlap: same bytes read as long */
    u.l = -1;
    printf("%d\n", u.i);
    struct Holder h;
    h.tag = 7;
    h.v.i = 1234;
    printf("%d %d\n", h.tag, h.v.i);
    return 0;
}
"#;
    let (code, out) = run_native(src).expect("build+run failed");
    assert_eq!(code, 0);
    assert_eq!(out, "42\n16909060\n-1\n7 1234\n");
}

// ---- bit-fields -----------------------------------------------------------------

#[test]
fn bitfield_masking_and_sign() {
    if !require_native() {
        return;
    }
    let src = r#"
#include <stdio.h>

struct Flags { unsigned a : 3; unsigned b : 5; unsigned c : 24; };
struct Signed { int s : 4; unsigned u : 4; int : 0; int wide : 20; };

int main() {
    struct Flags f;
    f.a = 5;
    f.b = 17;
    f.c = 1000000;
    printf("%u %u %u\n", f.a, f.b, f.c);
    f.a = 9; /* wraps to 3 bits */
    printf("%u\n", f.a);
    struct Signed g;
    g.s = -6; /* sign-extends on read */
    g.u = 14;
    g.wide = 400000;
    printf("%d %u %d\n", g.s, g.u, g.wide);
    return 0;
}
"#;
    let (code, out) = run_native(src).expect("build+run failed");
    assert_eq!(code, 0);
    assert_eq!(out, "5 17 1000000\n1\n-6 14 400000\n");
}

// ---- struct by value (SysV x86-64) -------------------------------------------------

#[test]
fn struct_by_value_register_classes() {
    if !require_native() {
        return;
    }
    let src = r#"
#include <stdio.h>

struct Pair { long a; long b; };   /* 16 bytes: two full eightbytes */
struct Tail { long a; char c; };   /* 16 bytes: partial trailing eightbyte */
struct Wide { int x; int y; int z; }; /* 12 bytes: partial second eightbyte */

struct Pair mkpair(long a, long b) { struct Pair p; p.a = a; p.b = b; return p; }
struct Tail mktail(long a, char c) { struct Tail t; t.a = a; t.c = c; return t; }
struct Wide mkwide(int x, int y, int z) { struct Wide w; w.x = x; w.y = y; w.z = z; return w; }

long usepair(struct Pair p) { return p.a * 100 + p.b; }
long usetail(struct Tail t) { return t.a * 10 + t.c; }
int usewide(struct Wide w) { return w.x + w.y + w.z; }

int main() {
    struct Pair p = mkpair(3, 4);
    struct Tail t = mktail(5, 6);
    struct Wide w = mkwide(7, 8, 9);
    printf("%ld %ld %ld\n", p.a, p.b, usepair(p));
    printf("%ld %d %ld\n", t.a, t.c, usetail(t));
    printf("%d %d %d %d\n", w.x, w.y, w.z, usewide(w));
    printf("chain=%ld\n", usepair(mkpair(7, 8)));
    return 0;
}
"#;
    let (code, out) = run_native(src).expect("build+run failed");
    assert_eq!(code, 0);
    assert_eq!(out, "3 4 304\n5 6 56\n7 8 9 24\nchain=708\n");
}

#[test]
fn struct_by_value_memory_class_and_mixed_args() {
    if !require_native() {
        return;
    }
    let src = r#"
#include <stdio.h>

struct Big { long a; long b; long c; }; /* 24 bytes: MEMORY class */
struct Pair { long a; long b; };

struct Big mkbig(long a, long b, long c) {
    struct Big g;
    g.a = a; g.b = b; g.c = c;
    return g;
}

long usebig(struct Big g) { return g.a + g.b + g.c; }

/* Interleaved structs and scalars keep their ABI order. */
long mix(struct Pair p, int k, struct Big g, short s) {
    return p.a + p.b + k + g.a + g.b + g.c + s;
}

int main() {
    struct Big g = mkbig(10, 20, 30);
    printf("%ld %ld %ld\n", g.a, g.b, g.c);
    printf("use=%ld\n", usebig(g));
    struct Pair p;
    p.a = 1;
    p.b = 2;
    printf("mix=%ld\n", mix(p, 100, g, 200)); /* 3 + 100 + 60 + 200 */
    struct Big copy;
    copy = g; /* aggregate assignment is a memcpy */
    printf("copy=%ld\n", usebig(copy));
    return 0;
}
"#;
    let (code, out) = run_native(src).expect("build+run failed");
    assert_eq!(code, 0);
    assert_eq!(out, "10 20 30\nuse=60\nmix=363\ncopy=60\n");
}

// ---- multi-source -----------------------------------------------------------------

#[test]
fn multi_source_native_pipeline() {
    if !require_native() {
        return;
    }
    let header = r#"
struct Point { int x; int y; };
struct Big { long a; long b; long c; };
/* Tentative definition: each TU emits a COMMON symbol and the linker
   merges them into one object. */
int counter;
int  sum(struct Point p);
struct Point mkpt(int x, int y);
struct Big scale(struct Big b, long k);
int  bump(void);
"#;
    let main_c = r#"
#include "geom.h"
#include <stdio.h>

int main() {
    struct Point p = mkpt(3, 4);
    struct Big b;
    b.a = 1; b.b = 2; b.c = 3;
    struct Big s = scale(b, 10);
    printf("sum=%d big=%ld\n", sum(p), s.a + s.b + s.c);
    printf("counter=%d\n", counter);
    int n = bump();
    printf("bump=%d again=%d\n", n, counter);
    return 0;
}
"#;
    let impl_c = r#"
#include "geom.h"

int sum(struct Point p) { return p.x + p.y; }

struct Point mkpt(int x, int y) {
    struct Point p;
    p.x = x;
    p.y = y;
    return p;
}

struct Big scale(struct Big b, long k) {
    struct Big r;
    r.a = b.a * k;
    r.b = b.b * k;
    r.c = b.c * k;
    return r;
}

int bump(void) {
    counter = counter + 5;
    return counter;
}
"#;
    let exe = build_native(&[("geom.h", header), ("main.c", main_c), ("impl.c", impl_c)])
        .expect("multi-source build failed");
    let out = Command::new(&exe.path).output().expect("run failed").stdout;
    let out = String::from_utf8_lossy(&out).into_owned();
    assert_eq!(out, "sum=7 big=60\ncounter=0\nbump=5 again=5\n");
}

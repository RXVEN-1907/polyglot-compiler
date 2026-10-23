# Example 9: Native Backend (Rust Pipeline)

This example demonstrates the driver's **native backend**: a fully Rust-driven
pipeline that compiles C with Polyglot's own frontend (parser, type checker,
Polyglot IR, IR optimizer), emits **LLVM IR as text**, and finishes the job
with `llc` and the system C compiler, **without ever using clang to parse C**.

Compare with `examples/08_ir_asm/`, which reaches LLVM IR through the *clang*
backend (`-S -emit-llvm`). Here clang only appears as the final linker.

## Pipeline

```
demo.c
  │
  ▼
┌──────────────────────────────────────────────────────────────┐
│  Rust frontend (polyglot-driver, pure Rust)                  │
│  lexer → parser → type check → Polyglot IR → IR optimizer    │
└──────────────────────────┬───────────────────────────────────┘
                           │  (textual)
                           ▼
              Polyglot IR        LLVM IR (.ll, textual)
                   │                    │
                   ▼                    ▼
        emit-ir --format pgir   llc demo.ll -o demo.o
                                        │
                                        ▼
                              cc demo.o -o demo   (link)
                                        │
                                        ▼
                                   ./demo  ← real executable
```

## The example

**File:** `demo.c`: recursion (`factorial`), a loop with arithmetic
(`sum_squares`), an if/return chain (`classify`), and `printf` calls.

## Verified commands

The binary used below was built with `cargo build --release -p polyglot-driver`
and lives at `target/release/polyglot-driver`. All output is captured from a
real run.

**Build and run a native executable:**

```bash
./target/release/polyglot-driver build examples/09_native_backend/demo.c -o /tmp/demo --backend native
```
```
compiled examples/09_native_backend/demo.c -> /tmp/demo (8 functions, 6 globals, 83 -> 79 IR instructions, opt removed 4)
```

```bash
/tmp/demo
```
```
factorial(6) = 720
sum_squares(10) = 385
-1 is negative
0 is zero
1 is positive
```

**Print the LLVM IR the native backend generates** (406 lines for this file):

```bash
./target/release/polyglot-driver emit-ir examples/09_native_backend/demo.c --format llvm
```
```
; ModuleID = 'examples/09_native_backend/demo.c'
source_filename = "examples/09_native_backend/demo.c"
target triple = "x86_64-unknown-linux-gnu"
@.LC0 = private unnamed_addr constant [19 x i8] c"factorial(6) = %d\0A\00", align 1
@.LC1 = private unnamed_addr constant [22 x i8] c"sum_squares(10) = %d\0A\00", align 1
@.LC2 = private unnamed_addr constant [9 x i8] c"negative\00", align 1
@.LC3 = private unnamed_addr constant [5 x i8] c"zero\00", align 1
@.LC4 = private unnamed_addr constant [9 x i8] c"positive\00", align 1
```

**Print Polyglot's own IR instead** (`--format pgir` is the default):

```bash
./target/release/polyglot-driver emit-ir examples/09_native_backend/demo.c
```
```
; Polyglot IR

; ---- globals ----
const @.LC0: char size=19 init="factorial(6) = %d\n" (19)
const @.LC1: char size=22 init="sum_squares(10) = %d\n" (22)
```

## Limitations

- **Single source file.** The native backend compiles one `.c` file at a time;
  use `examples/06_multi_file/` with the clang/gcc backends for multi-file builds.
- **Struct-by-value calls bail cleanly.** Passing a struct by value to a
  function fails with a clear error instead of miscompiling. For a file like:

  ```c
  int dot(struct Point p, struct Point q) {
      return p.x * q.x + p.y * q.y;
  }
  ```

  the driver reports:

  ```
  Error: native code generation failed for struct_val.c

  Caused by:
      StoreLocal of aggregate slot s0 is unsupported
  ```

  Struct *locals* and field access work fine (`a.x = 2; printf("%d", a.x);`);
  only calls that pass or return structs by value are rejected.
- **C subset.** The Rust frontend handles the same subset as the other
  examples (integers, chars, pointers, arrays, control flow, functions);
  see `examples/01`-`05`.
- `llc` and a system C compiler must be available (the driver finds LLVM 17 via
  its install prefix).

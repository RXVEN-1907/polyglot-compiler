# Examples

## Example Programs

The `examples/` directory contains working programs demonstrating various features.

---

## 1. Hello World

**File:** `examples/01_hello.c`

```c
#include <stdio.h>

int main() {
  printf("Hello, Polyglot!\n");
  return 0;
}
```

**Run:**
```bash
polyglot run examples/01_hello.c
# Hello, Polyglot!
```

---

## 2. Arithmetic Operations

**File:** `examples/02_arithmetic.c`

```c
#include <stdio.h>

int compute(int a, int b) {
    int sum = a + b;
    int diff = a - b;
    int prod = a * b;
    int quot = a / b;
    int rem = a % b;
    return sum + diff + prod + quot + rem;
}

int main() {
    int x = 100;
    int y = 7;
    int result = compute(x, y);
    printf("compute(%d, %d) = %d\n", x, y, result);
    return 0;
}
```

**Run:**
```bash
polyglot run examples/02_arithmetic.c
# compute(100, 7) = 916
```

---

## 3. Control Flow

**File:** `examples/03_control_flow.c`

```c
#include <stdio.h>

int fibonacci(int n) {
    if (n <= 1) return n;
    int a = 0, b = 1;
    for (int i = 2; i <= n; i++) {
        int c = a + b;
        a = b;
        b = c;
    }
    return b;
}

int classify(int x) {
    if (x < 0) return -1;
    else if (x == 0) return 0;
    else return 1;
}

int main() {
    printf("Fibonacci(10) = %d\n", fibonacci(10));

    for (int i = -2; i <= 2; i++) {
        int c = classify(i);
        const char *desc = c < 0 ? "negative" : (c == 0 ? "zero" : "positive");
        printf("%d is %s\n", i, desc);
    }

    return 0;
}
```

**Run:**
```bash
polyglot run examples/03_control_flow.c
# Fibonacci(10) = 55
# -2 is negative
# -1 is negative
# 0 is zero
# 1 is positive
# 2 is positive
```

---

## 4. Functions & Recursion

**File:** `examples/04_functions.c`

```c
#include <stdio.h>

int factorial(int n) {
    if (n <= 1) return 1;
    return n * factorial(n - 1);
}

int gcd(int a, int b) {
    while (b != 0) {
        int t = b;
        b = a % b;
        a = t;
    }
    return a;
}

int sum_range(int start, int end) {
    int sum = 0;
    for (int i = start; i <= end; i++) {
        sum += i;
    }
    return sum;
}

int main() {
    printf("factorial(10) = %d\n", factorial(10));
    printf("gcd(48, 18) = %d\n", gcd(48, 18));
    printf("sum_range(1, 100) = %d\n", sum_range(1, 100));
    return 0;
}
```

**Run:**
```bash
polyglot run examples/04_functions.c
# factorial(10) = 3628800
# gcd(48, 18) = 6
# sum_range(1, 100) = 5050
```

---

## 5. Structs & Pointers

**File:** `examples/05_structs.c`

```c
#include <stdio.h>
#include <string.h>

struct Point {
    int x, y;
};

struct Rectangle {
    struct Point top_left;
    struct Point bottom_right;
};

int rectangle_area(struct Rectangle *r) {
    int width = r->bottom_right.x - r->top_left.x;
    int height = r->bottom_right.y - r->top_left.y;
    return width * height;
}

void swap(int *a, int *b) {
    int tmp = *a;
    *a = *b;
    *b = tmp;
}

int main() {
    struct Rectangle r = {{0, 0}, {10, 5}};
    printf("Rectangle area: %d\n", rectangle_area(&r));

    int a = 10, b = 20;
    printf("Before swap: a=%d, b=%d\n", a, b);
    swap(&a, &b);
    printf("After swap: a=%d, b=%d\n", a, b);

    struct Point points[3] = {{1, 2}, {3, 4}, {5, 6}};
    for (int i = 0; i < 3; i++) {
        printf("Point %d: (%d, %d)\n", i, points[i].x, points[i].y);
    }

    return 0;
}
```

**Run:**
```bash
polyglot run examples/05_structs.c
# Rectangle area: 50
# Before swap: a=10, b=20
# After swap: a=20, b=10
# Point 0: (1, 2)
# Point 1: (3, 4)
# Point 2: (5, 6)
```

---

## 6. Multi-File Project

**Directory:** `examples/06_multi_file/`

**`math.h`:**
```c
#ifndef MATH_H
#define MATH_H

int add(int a, int b);
int multiply(int a, int b);
int power(int base, int exp);
int factorial(int n);

#endif
```

**`math.c`:**
```c
#include "math.h"

int add(int a, int b) {
    return a + b;
}

int multiply(int a, int b) {
    return a * b;
}

int power(int base, int exp) {
    int result = 1;
    for (int i = 0; i < exp; i++) {
        result *= base;
    }
    return result;
}

int factorial(int n) {
    if (n <= 1) return 1;
    return n * factorial(n - 1);
}
```

**`main.c`:**
```c
#include <stdio.h>
#include "math.h"

int main() {
    int a = 5, b = 3;
    printf("add(%d, %d) = %d\n", a, b, add(a, b));
    printf("multiply(%d, %d) = %d\n", a, b, multiply(a, b));
    printf("power(%d, %d) = %d\n", a, b, power(a, b));
    printf("factorial(%d) = %d\n", a, factorial(a));
    return 0;
}
```

**Build & Run:**
```bash
polyglot build examples/06_multi_file/main.c examples/06_multi_file/math.c -o mathapp
./mathapp
# add(5, 3) = 8
# multiply(5, 3) = 15
# power(5, 3) = 125
# factorial(5) = 120
```

---

## 7. Error Diagnostics

**Syntax Error:** `examples/07_diagnostics/bad_syntax.c`
```c
#include <stdio.h>

int main() {
    int x = 10
    // Missing semicolon - should cause a syntax error
    printf("This won't compile\n");
    return 0;
}
```

**Run:**
```bash
polyglot check examples/07_diagnostics/bad_syntax.c
# error: expected ';' at end of declaration
```

**Undefined Function:** `examples/07_diagnostics/undefined_func.c`
```c
#include <stdio.h>

int main() {
    int result = nonexistent_function(5);  // Not declared
    printf("Result: %d\n", result);
    return 0;
}
```

**Run:**
```bash
polyglot build examples/07_diagnostics/undefined_func.c -o test
# error: call to undeclared function 'nonexistent_function'; ISO C99 and
# later do not support implicit function declarations
```

---

## 8. IR & Assembly Demo

**File:** `examples/08_ir_asm/simple.c`

```c
#include <stdio.h>

int add(int a, int b) {
    return a + b;
}

int main() {
    int x = add(10, 20);
    printf("Result: %d\n", x);
    return 0;
}
```

**LLVM IR (clang backend; output is written to a file):**
```bash
polyglot --compiler clang emit-ir examples/08_ir_asm/simple.c -o simple.ll
head -30 simple.ll
```

On the GCC backend the same command emits GIMPLE instead (`simple.gimple` by default). Captured LLVM IR from clang 17:

```llvm
; ModuleID = 'simple.c'
source_filename = "simple.c"
target datalayout = "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-f80:128-n8:16:32:64-S128"
target triple = "x86_64-unknown-linux-gnu"

@.str = private unnamed_addr constant [12 x i8] c"Result: %d\0A\00", align 1

; Function Attrs: mustprogress nofree norecurse nosync nounwind willreturn memory(none) uwtable
define dso_local i32 @add(i32 noundef %a, i32 noundef %b) local_unnamed_addr #0 {
entry:
  %add = add nsw i32 %b, %a
  ret i32 %add
}

; Function Attrs: nofree nounwind uwtable
define dso_local i32 @main() local_unnamed_addr #1 {
entry:
  %call1 = tail call i32 (ptr, ...) @printf(ptr noundef nonnull dereferenceable(1) @.str, i32 noundef 30)
  ret i32 0
}

; Function Attrs: nofree nounwind
declare noundef i32 @printf(ptr nocapture noundef readonly, ...) local_unnamed_addr #2
```

**Assembly (written to `simple.s` in the working directory, or wherever `-o` points):**
```bash
polyglot emit-asm examples/08_ir_asm/simple.c -o simple.s
head -20 simple.s
```

Captured from the GCC 13 backend:

```asm
	.file	"simple.c"
	.text
	.p2align 4
	.globl	add
	.type	add, @function
add:
.LFB12:
	.cfi_startproc
	endbr64
	leal	(%rdi,%rsi), %eax
	ret
	.cfi_endproc
.LFE12:
	.size	add, .-add
	.section	.rodata.str1.1,"aMS",@progbits,1
.LC0:
	.string	"Result: %d\n"
```

---

## 9. Native Backend (Rust Pipeline)

**File:** `examples/09_native_backend/demo.c`

Unlike sections 1-8, which drive clang or gcc to parse C, this example uses
the driver's native backend: Polyglot's pure-Rust frontend (lexer → parser →
type check → Polyglot IR → IR optimizer) emits **LLVM IR as text**, then
`llc` assembles it and `cc` links the result. Clang never parses the C.

```
demo.c → Rust frontend → Polyglot IR → LLVM IR (.ll)
                                            │ llc
                                            ▼
                                          demo.o
                                            │ cc (link)
                                            ▼
                                         ./demo
```

`demo.c` exercises recursion, a loop, and an if/return chain:

```c
int factorial(int n) {
    if (n <= 1) return 1;
    return n * factorial(n - 1);
}

int sum_squares(int n) {
    int sum = 0;
    for (int i = 1; i <= n; i++) {
        sum += i * i;
    }
    return sum;
}
```

**Build & Run** (binary from `cargo build --release -p polyglot-driver`; all
output captured from a real run):

```bash
./target/release/polyglot-driver build examples/09_native_backend/demo.c -o /tmp/demo --backend native
# compiled examples/09_native_backend/demo.c -> /tmp/demo (8 functions, 6 globals, 83 -> 79 IR instructions, opt removed 4)

/tmp/demo
# factorial(6) = 720
# sum_squares(10) = 385
# -1 is negative
# 0 is zero
# 1 is positive
```

**Inspect the LLVM IR it feeds to `llc`:**

```bash
./target/release/polyglot-driver emit-ir examples/09_native_backend/demo.c --format llvm
```
```llvm
; ModuleID = 'examples/09_native_backend/demo.c'
source_filename = "examples/09_native_backend/demo.c"
target triple = "x86_64-unknown-linux-gnu"
@.LC0 = private unnamed_addr constant [19 x i8] c"factorial(6) = %d\0A\00", align 1
@.LC1 = private unnamed_addr constant [22 x i8] c"sum_squares(10) = %d\0A\00", align 1
```

Drop `--format llvm` (or pass `--format pgir`) to print Polyglot's own IR
instead. Limitations: single source file only, and calls that pass structs by
value fail cleanly with `StoreLocal of aggregate slot is unsupported` (struct
locals and field access work). See `examples/09_native_backend/README.md`.

---

## Complete Example: Calculator

A self-contained program. Save it as `calculator.c`:

```c
#include <stdio.h>
#include <stdlib.h>

typedef enum { ADD, SUB, MUL, DIV } Op;

double calculate(double a, Op op, double b) {
    switch (op) {
        case ADD: return a + b;
        case SUB: return a - b;
        case MUL: return a * b;
        case DIV: return b != 0 ? a / b : 0;
    }
    return 0;
}

int main(int argc, char *argv[]) {
    if (argc != 4) {
        printf("Usage: %s <num1> <op> <num2>\n", argv[0]);
        printf("Ops: + - * /\n");
        return 1;
    }
    
    double a = atof(argv[1]);
    char op = argv[2][0];
    double b = atof(argv[3]);
    
    Op operation;
    switch (op) {
        case '+': operation = ADD; break;
        case '-': operation = SUB; break;
        case '*': operation = MUL; break;
        case '/': operation = DIV; break;
        default:
            printf("Invalid operator: %c\n", op);
            return 1;
    }
    
    double result = calculate(a, operation, b);
    printf("Result: %.2f\n", result);
    return 0;
}
```

**Build & Run:**
```bash
polyglot build calculator.c -o calc
./calc 10 + 5
# Result: 15.00
```

---

## Next Steps

- [Project Workflow](project-workflow.md) - Full project setup
- [Optimization](optimization.md) - Optimize these examples
- [Cross Compilation](cross-compilation.md) - Build for other targets
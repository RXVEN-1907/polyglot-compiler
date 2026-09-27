/*
 * Example 9: Native Backend
 * Arithmetic, control flow, and a function call compiled by the
 * pure-Rust frontend through the native backend:
 * Polyglot IR -> LLVM IR (textual) -> llc -> .o -> cc link.
 *
 * Compare with examples/03_control_flow.c and examples/08_ir_asm/.
 * Build with:
 *   polyglot-driver build demo.c -o demo --backend native
 * Then run:
 *   ./demo
 */
#include <stdio.h>

/* Recursive function call. */
int factorial(int n) {
  if (n <= 1)
    return 1;
  return n * factorial(n - 1);
}

/* Control flow: if/return chain. */
int classify(int x) {
  if (x < 0)
    return -1;
  if (x == 0)
    return 0;
  return 1;
}

/* Loop with arithmetic. */
int sum_squares(int n) {
  int sum = 0;
  for (int i = 1; i <= n; i++) {
    sum += i * i;
  }
  return sum;
}

int main() {
  printf("factorial(6) = %d\n", factorial(6));
  printf("sum_squares(10) = %d\n", sum_squares(10));

  for (int i = -1; i <= 1; i++) {
    const char *desc =
        classify(i) < 0 ? "negative" : (classify(i) == 0 ? "zero" : "positive");
    printf("%d is %s\n", i, desc);
  }

  return 0;
}

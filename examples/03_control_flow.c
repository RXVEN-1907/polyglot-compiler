/*
 * Example 3: Control Flow
 * Demonstrates if/else, switch, loops, and their optimization.
 */
#include <stdio.h>

int fibonacci(int n) {
  if (n <= 1)
    return n;
  int a = 0, b = 1;
  for (int i = 2; i <= n; i++) {
    int c = a + b;
    a = b;
    b = c;
  }
  return b;
}

int classify(int x) {
  if (x < 0)
    return -1;
  else if (x == 0)
    return 0;
  else
    return 1;
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
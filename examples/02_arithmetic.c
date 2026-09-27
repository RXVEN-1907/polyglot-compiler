/*
 * Example 2: Arithmetic Operations
 * Demonstrates basic arithmetic operations and their optimization.
 */
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
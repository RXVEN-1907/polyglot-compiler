#include "math.h"

int add(int a, int b) { return a + b; }

int multiply(int a, int b) { return a * b; }

int power(int base, int exp) {
  int result = 1;
  for (int i = 0; i < exp; i++) {
    result *= base;
  }
  return result;
}

int factorial(int n) {
  if (n <= 1)
    return 1;
  return n * factorial(n - 1);
}
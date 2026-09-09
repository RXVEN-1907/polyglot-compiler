/*
 * Example 4: Functions and Recursion
 * Demonstrates function calls, recursion, and inlining optimization.
 */
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
#include <stdio.h>

int add(int a, int b) {
    return a + b;
}

int multiply(int a, int b) {
    return a * b;
}

int factorial(int n) {
    if (n <= 1) return 1;
    return n * factorial(n - 1);
}

int main() {
    int x = 5;
    int y = 3;
    printf("add(%d, %d) = %d\n", x, y, add(x, y));
    printf("multiply(%d, %d) = %d\n", x, y, multiply(x, y));
    printf("factorial(%d) = %d\n", 5, factorial(5));
    return 0;
}
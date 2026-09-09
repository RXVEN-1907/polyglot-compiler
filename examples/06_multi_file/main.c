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
#include <stdio.h>
#include "math_utils.h"

int main() {
    int a = 5;
    int b = 3;

    printf("add(%d, %d) = %d\n", a, b, add(a, b));
    printf("multiply(%d, %d) = %d\n", a, b, multiply(a, b));
    printf("power(%d, %d) = %d\n", a, b, power(a, b));

    return 0;
}
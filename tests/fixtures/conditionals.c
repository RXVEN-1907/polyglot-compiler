#include <stdio.h>

int main() {
    int x = 10;

    // if-else
    if (x > 5) {
        printf("x > 5\n");
    } else {
        printf("x <= 5\n");
    }

    // else-if chain
    if (x < 0) {
        printf("negative\n");
    } else if (x == 0) {
        printf("zero\n");
    } else {
        printf("positive\n");
    }

    // ternary
    const char *result = (x % 2 == 0) ? "even" : "odd";
    printf("x is %s\n", result);

    return 0;
}
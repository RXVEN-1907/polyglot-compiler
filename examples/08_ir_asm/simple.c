#include <stdio.h>

int add(int a, int b) {
    return a + b;
}

int main() {
    int x = add(10, 20);
    printf("Result: %d\n", x);
    return 0;
}
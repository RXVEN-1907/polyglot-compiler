#include <stdio.h>
#include <string.h>

void swap(int *a, int *b) {
    int temp = *a;
    *a = *b;
    *b = temp;
}

int main() {
    int a = 10;
    int b = 20;

    printf("Before swap: a=%d, b=%d\n", a, b);
    swap(&a, &b);
    printf("After swap: a=%d, b=%d\n", a, b);

    // Pointer arithmetic
    int arr[5] = {10, 20, 30, 40, 50};
    int *ptr = arr;
    printf("Array via pointer: ");
    for (int i = 0; i < 5; i++) {
        printf("%d ", *(ptr + i));
    }
    printf("\n");

    // String manipulation
    char str[] = "Hello, Polyglot!";
    char *p = str;
    while (*p) {
        *p = (*p >= 'a' && *p <= 'z') ? *p - 32 : *p;
        p++;
    }
    printf("Uppercase: %s\n", str);

    return 0;
}
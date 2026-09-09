#include <stdio.h>

int main() {
    // for loop
    int sum = 0;
    for (int i = 0; i < 10; i++) {
        sum += i;
    }
    printf("sum 0..9 = %d\n", sum);

    // while loop
    int n = 5;
    int factorial = 1;
    while (n > 0) {
        factorial *= n;
        n--;
    }
    printf("5! = %d\n", factorial);

    // do-while
    int count = 0;
    do {
        count++;
    } while (count < 3);
    printf("do-while count = %d\n", count);

    // nested loops
    printf("Multiplication table:\n");
    for (int i = 1; i <= 3; i++) {
        for (int j = 1; j <= 3; j++) {
            printf("%d ", i * j);
        }
        printf("\n");
    }

    return 0;
}
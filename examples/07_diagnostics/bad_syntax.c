#include <stdio.h>

int main() {
  int x = 10
      // Missing semicolon - should cause a syntax error
      printf("This won't compile\n");
  return 0;
}
/*
 * Example 5: Structs and Pointers
 * Demonstrates structs, pointers, and memory operations.
 */
#include <stdio.h>
#include <string.h>

struct Point {
  int x, y;
};

struct Rectangle {
  struct Point top_left;
  struct Point bottom_right;
};

int rectangle_area(struct Rectangle *r) {
  int width = r->bottom_right.x - r->top_left.x;
  int height = r->bottom_right.y - r->top_left.y;
  return width * height;
}

void swap(int *a, int *b) {
  int tmp = *a;
  *a = *b;
  *b = tmp;
}

int main() {
  struct Rectangle r = {{0, 0}, {10, 5}};
  printf("Rectangle area: %d\n", rectangle_area(&r));

  int a = 10, b = 20;
  printf("Before swap: a=%d, b=%d\n", a, b);
  swap(&a, &b);
  printf("After swap: a=%d, b=%d\n", a, b);

  struct Point points[3] = {{1, 2}, {3, 4}, {5, 6}};
  for (int i = 0; i < 3; i++) {
    printf("Point %d: (%d, %d)\n", i, points[i].x, points[i].y);
  }

  return 0;
}
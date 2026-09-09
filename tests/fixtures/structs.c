#include <stdio.h>
#include <string.h>

struct Point {
    int x;
    int y;
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

int main() {
    struct Point p1 = {0, 0};
    struct Point p2 = {10, 5};

    struct Rectangle rect = {p1, p2};

    int area = rectangle_area(&rect);
    printf("Rectangle area: %d\n", area);

    // Array of structs
    struct Point points[3] = {{1, 2}, {3, 4}, {5, 6}};
    for (int i = 0; i < 3; i++) {
        printf("Point %d: (%d, %d)\n", i, points[i].x, points[i].y);
    }

    return 0;
}
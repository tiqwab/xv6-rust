#include "../user.h"

void *memset(void *s, int c, size_t n) {
    char *ptr = (char *) s;
    size_t cnt = n * sizeof(void *);
    for (int i = 0; i < cnt; i++) {
        ptr[i] = c;
    }
    return s;
}

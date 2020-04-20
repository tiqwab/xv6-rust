#include "../user.h"

void *memmove(void *dest, const void *src, size_t n) {
    char *pd = (char *) dest;
    char *ps = (char *) src;

    char *buf = (char *) malloc(n);
    if (buf == (char *) -1) {
        printf("memmove: failed to allocate memory\n");
        return NULL;
    }

    for (int i = 0; i < n; i++) {
        buf[i] = *ps;
        ps++;
    }

    for (int i = 0; i < n; i++) {
        *pd = buf[i];
        pd++;
    }

    free(buf);
    return dest;
}

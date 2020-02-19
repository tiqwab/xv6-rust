#include "../user.h"

size_t strlen(const char *s) {
    size_t len = 0;
    const char *p = s;
    while (*p != '\0') {
        len++;
        p++;
    }
    return len;
}

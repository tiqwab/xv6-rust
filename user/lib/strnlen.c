#include "../user.h"

size_t strnlen(const char *s, size_t maxlen) {
    size_t len = 0;
    const char *p = s;
    while (*p != '\0' && len < maxlen) {
        len++;
        p++;
    }
    return len;
}

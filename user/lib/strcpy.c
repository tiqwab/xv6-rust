#include "../user.h"

char *strcpy(char *dest, const char *src) {
    char *pd = dest;
    char *ps = (char *) src;

    while (*ps != '\0') {
        *pd = *ps;
        pd++;
        ps++;
    }

    return dest;
}

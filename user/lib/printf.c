#include "../user.h"

int printf(const char *s, ...) {
    sys_cputs(s, strlen(s));
    return 0;
}

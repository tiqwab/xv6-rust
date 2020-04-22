#include "user.h"

void umain(int argc, char **argv) {
    char buf[DIR_SIZ + 1];
    if (sys_getcwd(buf, DIR_SIZ + 1) == NULL) {
        printf("pwd: cannot getcwd\n");
    } else {
        printf("%s\n", buf);
    }
}

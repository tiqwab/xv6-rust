#include "user.h"

void cd(char *path) {
    if (sys_chdir(path) != 0) {
        printf("cd: cannot cd to %s\n", path);
    }
}

void umain(int argc, char **argv) {
    if (argc < 2) {
        printf("cd: missing operand\n");
    } else {
        char *path = argv[1];
        cd(path);
    }
}

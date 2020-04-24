#include "user.h"

void mkdir(char *path) {
    if (sys_mkdir(path) != 0) {
        printf("mkdir: cannot create a directory %s\n", path);
    }
}

void umain(int argc, char **argv) {
    if (argc < 2) {
        printf("mkdir: missing operand\n");
    } else {
        char *path = argv[1];
        mkdir(path);
    }
}

#include "user.h"

#define BUF_LEN 128

void cat(int fd) {
    int n;
    char buf[BUF_LEN];

    while ((n = read(fd, buf, BUF_LEN)) > 0) {
        buf[n] = 0;
        printf("%s", buf);
    }
}

void umain(int argc, char **argv) {
    if (argc < 2) {
        cat(STDIN_FILENO);
    } else {
        for (int i = 1; i < argc; i++) {
            int fd;
            char *path = argv[i];
            if ((fd = open(path, O_RDONLY)) < 0) {
                printf("cat: cannot open %s\n", path);
                return;
            }
            cat(fd);
            close(fd);
        }
    }
}

#include "user.h"

#define BUF_LEN 128

void cat(char *path) {
    int fd, n;
    char buf[BUF_LEN];

    if ((fd = open(path, O_RDONLY)) < 0) {
        printf("cat: cannot open %s\n", path);
        return;
    }

    while ((n = read(fd, buf, BUF_LEN)) > 0) {
        buf[n] = 0;
        printf("%s", buf);
    }

    close(fd);
}

void umain(int argc, char **argv) {
    if (argc < 2) {
        printf("cat: require at least one argument\n");
    } else {
        for (int i = 1; i < argc; i++) {
            cat(argv[i]);
        }
    }
}

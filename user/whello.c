#include "user.h"

void whello() {
    int fd, n;
    char *path = "hello.txt";

    if ((fd = open(path, O_RDWR | O_CREAT)) < 0) {
        printf("whello: cannot open %s\n", path);
    }

    for (int i = 0; i < 3; i++) {
        char *msg = "Hello World\n";
        write(fd, msg, strlen(msg));
    }

    close(fd);
}

// Write "Hello World" to hello.txt
void umain(int argc, char **argv) {
    whello();
}

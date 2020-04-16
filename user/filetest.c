#include "user.h"

#define BUF_LEN 64

char buf[BUF_LEN];

void umain(int argc, char **argv) {
    // write
    {
        int fd = open("test.txt", O_CREAT | O_RDWR);
        printf("opened fd: %d\n", fd);

        char *msg = "hello, world";
        int count = strlen(msg);
        write(fd, msg, count);
        printf("wrote fd\n");

        close(fd);
        printf("closed fd: %d\n", fd);
    }

    // read
    {
        int fd = open("test.txt", O_CREAT | O_RDWR);
        printf("opened fd: %d\n", fd);

        read(fd, buf, BUF_LEN);
        printf("read message: %s\n", buf);

        close(fd);
        printf("closed fd: %d\n", fd);
    }

    // console
    {
        int count = read(STDIN_FILENO, buf, BUF_LEN);
        buf[count++] = '\n';
        write(STDOUT_FILENO, buf, count);
    }
}

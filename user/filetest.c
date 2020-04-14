#include "user.h"

#define BUF_LEN 64

char buf[BUF_LEN];

void umain(int argc, char **argv) {
    // write
    {
        int fd = sys_open("test.txt", O_CREAT | O_RDWR);
        printf("opened fd: %d\n", fd);

        char *msg = "hello, world";
        int count = strlen(msg);
        sys_write(fd, msg, count);
        printf("wrote fd\n");

        sys_close(fd);
        printf("closed fd: %d\n", fd);
    }

    // read
    {
        int fd = sys_open("test.txt", O_CREAT | O_RDWR);
        printf("opened fd: %d\n", fd);

        sys_read(fd, buf, BUF_LEN);
        printf("read message: %s\n", buf);

        sys_close(fd);
        printf("closed fd: %d\n", fd);
    }
}

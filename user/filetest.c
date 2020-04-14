#include "user.h"

void umain(int argc, char **argv) {
    int fd = sys_open("test.txt", O_CREAT | O_RDWR);
    printf("opened fd: %d\n", fd);
    sys_close(fd);
    printf("closed fd: %d\n", fd);
}
